//! Drawing. Reads [`App`] and paints one frame; keeps no state of its
//! own, so a frame is a pure function of the app and the terminal size.
//!
//! The shape is a page rather than a window: a header line and a rule,
//! a centred column of content no wider than [`COLUMN`], a rule and a
//! footer of key hints. Nothing is stretched to the terminal's width --
//! a 20-character field in a 200-column box looks broken, and the same
//! field in a 90-column column looks meant.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use serde_json::Value;

use msip::element::{Element, types};
use msip::msg::Outcome;
use msip::surface::Page;

use crate::app::{App, Finished, Input, Modal, is_list, row_enabled, rows_of};

/// The content column is never wider than this, whatever the terminal.
pub(crate) const COLUMN: u16 = 96;
/// A text input: room for a hostname or a password, not a paragraph.
const INPUT: u16 = 56;
/// Rows a list shows before it scrolls.
const LIST_ROWS: usize = 8;
const TABLE_ROWS: usize = 10;
/// A table column is capped here so one long model string cannot push
/// the size column off the edge.
const CELL: usize = 40;

/// What every element is drawn against.
struct Ctx {
    focused: Option<String>,
    /// The progress element in flight: the first that is not done.
    active: Option<String>,
}

/// Turn classes this surface understands. They are hints (§3.7): a page
/// carrying none is drawn as a form, which is right for most pages.
const MENU: &str = "menu";
const CONFIRM: &str = "confirm";

impl App {
    pub(crate) fn render(&self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Clear, area);
        let [header, rule_top, body, rule_bottom, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        self.header(f, header);
        self.rule(f, rule_top);
        self.rule(f, rule_bottom);
        self.footer(f, footer);

        let column = column(body);
        if let Some(fin) = &self.finished {
            self.finished_screen(f, column, fin);
        } else if let Some(page) = self.session.page() {
            self.page(f, column, page);
        } else {
            self.waiting(f, column);
        }
        if let Some(m) = &self.modal {
            self.modal(f, area, m);
        }
    }

    fn header(&self, f: &mut Frame, area: Rect) {
        let line = Line::from(Span::styled(
            format!(" {}", self.title),
            self.theme.heading(),
        ));
        f.render_widget(Paragraph::new(line), area);
    }

    fn rule(&self, f: &mut Frame, area: Rect) {
        let line = Line::from(Span::styled(
            self.theme.glyphs.rule.repeat(area.width as usize),
            self.theme.dim(),
        ));
        f.render_widget(Paragraph::new(line), area);
    }

    /// One line, in priority order: a protocol complaint; why the
    /// focused thing is disabled, or what the focused action does; the
    /// keys that apply to whatever is focused.
    fn footer(&self, f: &mut Frame, area: Rect) {
        let (text, style) = if let Some(s) = &self.status {
            (s.clone(), self.theme.error())
        } else if self.modal.is_some() {
            ("Enter: choose   Esc: back".to_string(), self.theme.dim())
        } else if self.finished.is_some() {
            ("Any key: exit".to_string(), self.theme.dim())
        } else if let Some(help) = self
            .focused_element()
            .filter(|e| !e.enabled || e.is_action())
            .and_then(|e| e.help)
        {
            (help, self.theme.dim())
        } else {
            (self.hints().to_string(), self.theme.dim())
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(format!(" {text}"), style))),
            area,
        );
    }

    fn hints(&self) -> &'static str {
        match self.focused_element() {
            Some(e) if is_list(&e) => {
                "Up/Down: choose   Tab: next   Enter: continue   Esc: leave   F1: help"
            }
            Some(e) if e.is_action() => {
                "Enter: press   Left/Right: other actions   Esc: leave   F1: help"
            }
            Some(e) if e.r#type == types::BOOLEAN => {
                "Space: toggle   Tab: next   Enter: continue   Esc: leave   F1: help"
            }
            Some(_) => "Tab: next field   Enter: continue   Esc: leave   F1: help",
            None => "Esc: leave   F1: help",
        }
    }

    fn waiting(&self, f: &mut Frame, column: Rect) {
        let g = self.theme.glyphs;
        let spin = g.spinner[(self.tick as usize) % g.spinner.len()];
        let line = Line::from(vec![
            Span::styled(spin.to_string(), self.theme.accent()),
            Span::raw(format!(" Connecting to {}…", self.daemon_hint)),
        ]);
        f.render_widget(Paragraph::new(line), column);
    }

    fn finished_screen(&self, f: &mut Frame, column: Rect, fin: &Finished) {
        let g = self.theme.glyphs;
        let (glyph, style) = match fin.outcome {
            Some(Outcome::Complete) => (g.ok, self.theme.ok()),
            Some(Outcome::Failed) => (g.fail, self.theme.error()),
            Some(Outcome::Cancelled) => ("-", self.theme.dim()),
            None => (g.warn, self.theme.error()),
        };
        let mut lines: Vec<Line> = Vec::new();
        for (i, text) in fin.message.split('\n').enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(format!("{glyph} "), style.add_modifier(Modifier::BOLD)),
                    Span::styled(
                        text.to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]));
            } else {
                lines.push(Line::raw(format!("  {text}")));
            }
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Press any key to exit.",
            self.theme.dim(),
        )));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), column);
    }

    fn page(&self, f: &mut Frame, column: Rect, page: &Page) {
        let menu = page.turn.class.iter().any(|c| c == MENU);
        let confirm = page.turn.class.iter().any(|c| c == CONFIRM);
        let ctx = Ctx {
            focused: self.focused_ref(),
            active: page
                .turn
                .elements
                .iter()
                .find(|e| e.r#type == types::PROGRESS && !progress_done(e))
                .map(|e| e.r#ref.clone()),
        };

        enum Part<'a> {
            Heading(&'a str),
            Element(&'a Element),
            Actions(Vec<&'a Element>),
        }
        let mut parts: Vec<Part> = Vec::new();
        let mut constraints: Vec<Constraint> = Vec::new();
        if let Some(name) = &page.turn.name {
            parts.push(Part::Heading(name));
            constraints.push(Constraint::Length(2));
        }
        let mut actions: Vec<&Element> = Vec::new();
        let mut fills = false;
        for e in &page.turn.elements {
            if e.is_action() && !menu {
                actions.push(e);
                continue;
            }
            let c = self.height_of(e, column.width);
            fills |= matches!(c, Constraint::Fill(_));
            parts.push(Part::Element(e));
            constraints.push(c);
        }
        if !actions.is_empty() {
            parts.push(Part::Actions(actions));
            constraints.push(Constraint::Length(2));
        }
        // Content sits at the top of the column unless something on the
        // page -- a log -- wants the room.
        if !fills {
            constraints.push(Constraint::Fill(1));
        }
        let rows = Layout::vertical(constraints).split(column);
        for (i, part) in parts.iter().enumerate() {
            match part {
                Part::Heading(name) => self.heading(f, rows[i], name, confirm),
                Part::Element(e) => self.draw_element(f, rows[i], e, &ctx),
                Part::Actions(a) => self.action_bar(f, rows[i], a, &ctx),
            }
        }
    }

    fn heading(&self, f: &mut Frame, area: Rect, name: &str, confirm: bool) {
        let mut spans = Vec::new();
        if confirm {
            spans.push(Span::styled(
                format!("{} ", self.theme.glyphs.warn),
                self.theme.error().add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::styled(name.to_string(), self.theme.heading()));
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    /// How tall an element is in a column this wide. Stable across
    /// focus changes: a field's note line is reserved whenever it has
    /// help or an error, so focus moving does not shift the page.
    fn height_of(&self, e: &Element, width: u16) -> Constraint {
        let note = (e.error.is_some() || e.help.is_some()) as u16;
        let n = rows_of(e).len();
        let h = match e.r#type.as_str() {
            types::TEXT => wrap_count(text_of(e), width) + 1,
            types::STRING => 1 + 3 + note + 1,
            types::BOOLEAN => 1 + note + 1,
            types::SELECT => 1 + 2 + n.clamp(1, LIST_ROWS) as u16 + note + 1,
            types::TABLE => 1 + 3 + n.clamp(1, TABLE_ROWS) as u16 + note + 1,
            types::PROGRESS => 1,
            types::LOG => return Constraint::Fill(1),
            types::ACTION => {
                // A menu item; a disabled one explains itself beneath.
                1 + match (&e.help, e.enabled) {
                    (Some(h), false) => wrap_count(h, width.saturating_sub(4)),
                    _ => 0,
                }
            }
            _ => 1,
        };
        Constraint::Length(h)
    }

    fn draw_element(&self, f: &mut Frame, area: Rect, e: &Element, ctx: &Ctx) {
        let focused = ctx.focused.as_deref() == Some(e.r#ref.as_str());
        match e.r#type.as_str() {
            types::TEXT => {
                f.render_widget(
                    Paragraph::new(text_of(e).to_string()).wrap(Wrap { trim: false }),
                    area,
                );
            }
            types::STRING => self.string(f, area, e, focused),
            types::BOOLEAN => self.boolean(f, area, e, focused),
            types::SELECT | types::TABLE => self.list(f, area, e, focused),
            types::PROGRESS => self.progress(f, area, e, ctx),
            types::LOG => self.log(f, area, e),
            types::ACTION => self.menu_item(f, area, e, focused),
            other => f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("({other}: this surface cannot draw it)"),
                    self.theme.dim(),
                ))),
                area,
            ),
        }
    }

    fn label(&self, f: &mut Frame, area: Rect, e: &Element, focused: bool) {
        let name = e.name.clone().unwrap_or_else(|| e.r#ref.clone());
        let style = if e.enabled {
            self.theme.label(focused)
        } else {
            self.theme.dim()
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(name, style))),
            row(area, 0, 1),
        );
    }

    /// The line under a field: its error, or its help while focused.
    fn note(&self, f: &mut Frame, area: Rect, e: &Element, focused: bool) {
        let line = if let Some(err) = &e.error {
            Line::from(Span::styled(err.clone(), self.theme.error()))
        } else if let (true, Some(h)) = (focused, &e.help) {
            Line::from(Span::styled(h.clone(), self.theme.dim()))
        } else {
            return;
        };
        f.render_widget(Paragraph::new(line), area);
    }

    fn boxed(&self, focused: bool, enabled: bool) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(self.theme.glyphs.border)
            .border_style(self.theme.frame(focused, enabled))
    }

    fn string(&self, f: &mut Frame, area: Rect, e: &Element, focused: bool) {
        let g = self.theme.glyphs;
        self.label(f, area, e, focused);
        let boxed = Rect {
            width: area.width.min(INPUT),
            ..row(area, 1, 3)
        };
        let block = self.boxed(focused, e.enabled);
        let inner = block.inner(boxed);
        f.render_widget(block, boxed);

        let (value, cursor) = match self.inputs.get(&e.r#ref) {
            Some(Input::Text { value, cursor }) => (value.as_str(), *cursor),
            _ => ("", 0),
        };
        let placeholder = e.state.get("placeholder").and_then(Value::as_str);
        let line = if value.is_empty() && !focused {
            Line::from(Span::styled(
                placeholder.unwrap_or("").to_string(),
                self.theme.dim(),
            ))
        } else {
            let shown: Vec<char> = if e.secret {
                std::iter::repeat_n(
                    g.secret.chars().next().unwrap_or('*'),
                    value.chars().count(),
                )
                .collect()
            } else {
                value.chars().collect()
            };
            let (before, after) = shown.split_at(cursor.min(shown.len()));
            // Keep the cursor in view: show the tail of a long value.
            let room = inner.width.saturating_sub(2) as usize;
            let before: String = before.iter().rev().take(room).rev().collect();
            let after: String = after.iter().collect();
            let style = if e.enabled {
                Style::default()
            } else {
                self.theme.dim()
            };
            let mut spans = vec![Span::styled(before, style)];
            if focused && e.enabled {
                spans.push(Span::styled(g.cursor.to_string(), self.theme.accent()));
            }
            spans.push(Span::styled(after, style));
            Line::from(spans)
        };
        f.render_widget(Paragraph::new(line), inner);
        self.note(f, row(area, 4, 1), e, focused);
    }

    fn boolean(&self, f: &mut Frame, area: Rect, e: &Element, focused: bool) {
        let g = self.theme.glyphs;
        let on = matches!(self.inputs.get(&e.r#ref), Some(Input::Flag(true)));
        let name = e.name.clone().unwrap_or_else(|| e.r#ref.clone());
        let mark_style = if !e.enabled {
            self.theme.dim()
        } else if focused {
            self.theme.accent().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let line = Line::from(vec![
            Span::styled(if on { g.checked } else { g.unchecked }, mark_style),
            Span::styled(
                format!(" {name}"),
                if e.enabled {
                    self.theme.label(focused)
                } else {
                    self.theme.dim()
                },
            ),
        ]);
        f.render_widget(Paragraph::new(line), row(area, 0, 1));
        self.note(f, row(area, 1, 1), e, focused);
    }

    /// A select or a table: a box of rows with one highlighted.
    fn list(&self, f: &mut Frame, area: Rect, e: &Element, focused: bool) {
        let g = self.theme.glyphs;
        let table = e.r#type == types::TABLE;
        self.label(f, area, e, focused);
        let rows = rows_of(e);
        let visible = rows
            .len()
            .clamp(1, if table { TABLE_ROWS } else { LIST_ROWS });
        let header = table as u16;
        let boxed = Rect {
            width: if table {
                area.width
            } else {
                area.width.min(INPUT)
            },
            ..row(area, 1, visible as u16 + 2 + header)
        };
        let block = self.boxed(focused, e.enabled);
        let inner = block.inner(boxed);
        f.render_widget(block, boxed);

        let sel = match self.inputs.get(&e.r#ref) {
            Some(Input::Row(s)) => *s,
            _ => None,
        };
        let mut lines: Vec<Line> = Vec::new();
        let width = inner.width as usize;

        if rows.is_empty() {
            let empty = e
                .state
                .get("empty")
                .and_then(Value::as_str)
                .unwrap_or(if e.enabled {
                    "Nothing to choose from."
                } else {
                    "Nothing to choose from yet."
                });
            lines.push(Line::from(Span::styled(
                format!("  {empty}"),
                self.theme.dim(),
            )));
        } else {
            let cols = if table {
                fit_columns(e, &rows, width.saturating_sub(2))
            } else {
                Vec::new()
            };
            if table {
                let mut text = String::from("  ");
                for (i, c) in cols.iter().enumerate() {
                    if i > 0 {
                        text.push_str("  ");
                    }
                    text.push_str(&pad(&c.name, c.width, c.right));
                }
                lines.push(Line::from(Span::styled(
                    text,
                    self.theme.dim().add_modifier(Modifier::BOLD),
                )));
            }
            // Scroll so the highlight is in view.
            let offset = match sel {
                Some(s) if s + 1 > visible => s + 1 - visible,
                _ => 0,
            };
            for (i, r) in rows.iter().enumerate().skip(offset).take(visible) {
                let enabled = e.enabled && row_enabled(r);
                let highlighted = sel == Some(i);
                let mut text = String::new();
                text.push_str(if highlighted { g.marker } else { " " });
                text.push(' ');
                if table {
                    for (ci, c) in cols.iter().enumerate() {
                        if ci > 0 {
                            text.push_str("  ");
                        }
                        let cell = r
                            .get("cells")
                            .and_then(|cells| cells.get(&c.key))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        text.push_str(&pad(cell, c.width, c.right));
                    }
                    if let Some(note) = r.get("note").and_then(Value::as_str) {
                        text.push_str("  ");
                        text.push_str(note);
                    }
                } else {
                    text.push_str(
                        r.get("name")
                            .and_then(Value::as_str)
                            .or_else(|| r.get("value").and_then(Value::as_str))
                            .unwrap_or("?"),
                    );
                }
                let style = match (highlighted, focused, enabled) {
                    (true, true, true) => self.theme.highlight(),
                    (true, true, false) => self.theme.dim().add_modifier(Modifier::REVERSED),
                    (true, false, true) => Style::default().add_modifier(Modifier::BOLD),
                    (_, _, false) => self.theme.dim(),
                    _ => Style::default(),
                };
                let text = if highlighted {
                    pad(&text, width, false)
                } else {
                    text
                };
                lines.push(Line::from(Span::styled(text, style)));
            }
        }
        f.render_widget(Paragraph::new(lines), inner);
        self.note(f, row(area, 1 + boxed.height, 1), e, focused);
    }

    fn progress(&self, f: &mut Frame, area: Rect, e: &Element, ctx: &Ctx) {
        let g = self.theme.glyphs;
        let name = e.name.clone().unwrap_or_else(|| e.r#ref.clone());
        let value = e.state.get("value").and_then(Value::as_u64).unwrap_or(0);
        let max = e.state.get("max").and_then(Value::as_u64);
        let done = progress_done(e);
        let active = ctx.active.as_deref() == Some(e.r#ref.as_str());

        let mut spans: Vec<Span> = Vec::new();
        if done {
            spans.push(Span::styled(format!("{} ", g.done), self.theme.ok()));
            spans.push(Span::styled(name, self.theme.dim()));
        } else if active {
            let glyph = match max {
                Some(_) => g.active.to_string(),
                None => g.spinner[(self.tick as usize) % g.spinner.len()].to_string(),
            };
            spans.push(Span::styled(format!("{glyph} "), self.theme.accent()));
            let name_len = name.chars().count() as u16;
            spans.push(Span::styled(
                name,
                Style::default().add_modifier(Modifier::BOLD),
            ));
            if let Some(max) = max.filter(|m| *m > 0) {
                let ratio = (value as f64 / max as f64).clamp(0.0, 1.0);
                let bar = area.width.saturating_sub(name_len + 12).min(32) as usize;
                let filled = (ratio * bar as f64).round() as usize;
                spans.push(Span::raw("  "));
                spans.push(Span::styled(g.fill.repeat(filled), self.theme.accent()));
                spans.push(Span::styled(g.track.repeat(bar - filled), self.theme.dim()));
                spans.push(Span::styled(
                    format!("  {:>3}%", (ratio * 100.0).round() as u64),
                    self.theme.dim(),
                ));
            }
        } else {
            spans.push(Span::styled(format!("{} ", g.pending), self.theme.dim()));
            spans.push(Span::styled(name, self.theme.dim()));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn log(&self, f: &mut Frame, area: Rect, e: &Element) {
        let name = e.name.clone().unwrap_or_else(|| "Details".into());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(self.theme.glyphs.border)
            .border_style(self.theme.dim())
            .title(Span::styled(format!(" {name} "), self.theme.dim()));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let empty = vec![];
        let all = e
            .state
            .get("lines")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        let keep = inner.height as usize;
        let lines: Vec<Line> = all
            .iter()
            .rev()
            .take(keep)
            .rev()
            .map(|l| {
                Line::from(Span::styled(
                    l.as_str().unwrap_or("").to_string(),
                    self.theme.dim(),
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    /// An action on a `menu` page: one row of a vertical list.
    fn menu_item(&self, f: &mut Frame, area: Rect, e: &Element, focused: bool) {
        let g = self.theme.glyphs;
        let name = e.name.clone().unwrap_or_else(|| e.r#ref.clone());
        let primary = e
            .state
            .get("primary")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut lines: Vec<Line> = Vec::new();
        let text = format!("{} {name}", if focused { g.marker } else { " " });
        let line = if focused && e.enabled {
            Line::from(Span::styled(
                pad(&text, area.width as usize, false),
                self.theme.highlight(),
            ))
        } else if focused {
            Line::from(Span::styled(
                text,
                self.theme.dim().add_modifier(Modifier::REVERSED),
            ))
        } else if !e.enabled {
            Line::from(Span::styled(text, self.theme.dim()))
        } else if primary {
            Line::from(Span::styled(
                text,
                self.theme.accent().add_modifier(Modifier::BOLD),
            ))
        } else {
            Line::raw(text)
        };
        lines.push(line);
        if let (Some(help), false) = (&e.help, e.enabled) {
            lines.push(Line::from(Span::styled(
                format!("    {help}"),
                self.theme.dim(),
            )));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    fn action_bar(&self, f: &mut Frame, area: Rect, actions: &[&Element], ctx: &Ctx) {
        let mut spans: Vec<Span> = Vec::new();
        for e in actions {
            let name = e.name.clone().unwrap_or_else(|| e.r#ref.clone());
            let focused = ctx.focused.as_deref() == Some(e.r#ref.as_str());
            let primary = e
                .state
                .get("primary")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let destructive = e
                .state
                .get("destructive")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let style = match (e.enabled, focused, destructive, primary) {
                (false, true, _, _) => self.theme.dim().add_modifier(Modifier::REVERSED),
                (false, false, _, _) => self.theme.dim(),
                (true, true, true, _) => self.theme.danger_highlight(),
                (true, true, false, _) => self.theme.highlight(),
                (true, false, true, _) => self.theme.error().add_modifier(Modifier::BOLD),
                (true, false, false, true) => self.theme.accent().add_modifier(Modifier::BOLD),
                _ => Style::default(),
            };
            spans.push(Span::styled(format!("[ {name} ]"), style));
            spans.push(Span::raw("  "));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
            area,
        );
    }

    fn modal(&self, f: &mut Frame, area: Rect, modal: &Modal) {
        let width = area.width.saturating_sub(4).clamp(20, 64);
        let inner_w = width.saturating_sub(4);
        type ModalParts<'a> = (&'a str, Vec<String>, Vec<(&'a str, bool, bool)>);
        let (title, body, buttons): ModalParts<'_> = match modal {
            Modal::Leave { leaving } => (
                "Leave?",
                vec![capitalise(self.leave_hint) + "."],
                vec![("Stay", !leaving, false), ("Leave", *leaving, true)],
            ),
            Modal::Help => (
                "Keys",
                HELP.iter().map(|(k, v)| format!("{k:<16}{v}")).collect(),
                vec![("Close", true, false)],
            ),
        };
        let body_h: u16 = body.iter().map(|l| wrap_count(l, inner_w)).sum();
        let height = body_h + 6;
        let rect = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height: height.min(area.height),
        };
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(self.theme.glyphs.border)
            .border_style(self.theme.accent())
            .title(Span::styled(format!(" {title} "), self.theme.heading()));
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let text = Rect {
            x: inner.x + 1,
            y: inner.y + 1,
            width: inner_w,
            height: body_h,
        };
        let lines: Vec<Line> = body.into_iter().map(Line::raw).collect();
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text);

        let mut spans: Vec<Span> = Vec::new();
        for (name, on, danger) in buttons {
            let style = match (on, danger) {
                (true, true) => self.theme.danger_highlight(),
                (true, false) => self.theme.highlight(),
                (false, true) => self.theme.error(),
                (false, false) => Style::default(),
            };
            spans.push(Span::styled(format!("[ {name} ]"), style));
            spans.push(Span::raw("  "));
        }
        let row = Rect {
            x: inner.x + 1,
            y: inner.y + inner.height.saturating_sub(2),
            width: inner_w,
            height: 1,
        };
        f.render_widget(Paragraph::new(Line::from(spans)), row);
    }
}

const HELP: &[(&str, &str)] = &[
    ("Tab, Shift+Tab", "move between fields"),
    ("Up, Down", "move within a list, or between fields"),
    ("Left, Right", "move within a line, or between actions"),
    ("Enter", "press the action, or move on"),
    ("Space", "toggle a checkbox"),
    ("Esc", "leave"),
    ("Ctrl+L", "repaint the screen"),
    ("F1", "this list"),
];

/// A column laid out in a table, after fitting.
struct Col {
    key: String,
    name: String,
    right: bool,
    width: usize,
}

/// Size the columns to their content and drop from the right until they
/// fit, as the spec asks (a2): the daemon's column order is its
/// priority order.
fn fit_columns(e: &Element, rows: &[Value], available: usize) -> Vec<Col> {
    let mut cols: Vec<Col> = e
        .state
        .get("columns")
        .and_then(Value::as_array)
        .map(|cs| {
            cs.iter()
                .map(|c| {
                    let key = c
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let name = c
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&key)
                        .to_string();
                    let right = c.get("align").and_then(Value::as_str) == Some("right");
                    let mut width = name.chars().count();
                    for r in rows {
                        if let Some(cell) = r
                            .get("cells")
                            .and_then(|c| c.get(&key))
                            .and_then(Value::as_str)
                        {
                            width = width.max(cell.chars().count());
                        }
                    }
                    Col {
                        key,
                        name,
                        right,
                        width: width.min(CELL),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let total = |cols: &[Col]| {
        cols.iter().map(|c| c.width).sum::<usize>() + 2 * cols.len().saturating_sub(1)
    };
    while cols.len() > 1 && total(&cols) > available {
        cols.pop();
    }
    if let Some(last) = cols.len().checked_sub(1) {
        let others = total(&cols) - cols[last].width;
        cols[last].width = cols[last]
            .width
            .min(available.saturating_sub(others).max(1));
    }
    cols
}

fn pad(s: &str, width: usize, right: bool) -> String {
    let s: String = s.chars().take(width).collect();
    let n = s.chars().count();
    let fill = " ".repeat(width.saturating_sub(n));
    if right {
        format!("{fill}{s}")
    } else {
        format!("{s}{fill}")
    }
}

fn text_of(e: &Element) -> &str {
    e.state.get("text").and_then(Value::as_str).unwrap_or("")
}

fn progress_done(e: &Element) -> bool {
    let value = e.state.get("value").and_then(Value::as_u64).unwrap_or(0);
    match e.state.get("max").and_then(Value::as_u64) {
        Some(max) if max > 0 => value >= max,
        _ => false,
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The `n`th row of an area, `h` tall.
fn row(area: Rect, n: u16, h: u16) -> Rect {
    Rect {
        x: area.x,
        y: area.y + n,
        width: area.width,
        height: h.min(area.height.saturating_sub(n)),
    }
}

/// The content column: centred, never wider than [`COLUMN`], with a
/// row's breathing space above.
pub(crate) fn column(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).clamp(1, COLUMN);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + 1,
        width,
        height: area.height.saturating_sub(1),
    }
}

/// How many rows `text` takes when word-wrapped to `width`. Greedy, as
/// ratatui's wrapping is, so the heights agree closely enough that the
/// layout does not jump.
pub(crate) fn wrap_count(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let mut lines = 0u16;
    for para in text.split('\n') {
        let mut count = 1u16;
        let mut used = 0usize;
        for word in para.split_whitespace() {
            let w = word.chars().count();
            if used == 0 {
                used = w;
            } else if used + 1 + w <= width {
                used += 1 + w;
            } else {
                count += 1;
                used = w;
            }
            while used > width {
                count += 1;
                used -= width;
            }
        }
        lines += count;
    }
    lines
}
