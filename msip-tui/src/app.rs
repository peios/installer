//! The surface's state and what a key does to it. Nothing here draws;
//! `render` reads this and paints it.

use std::collections::HashMap;
use std::os::unix::net::UnixStream;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Map, Value, json};

use msip::element::{Element, types};
use msip::frame::{MsgType, write_msg};
use msip::msg::{Outcome, RefusedReason};
use msip::surface::{Event, Session};

use crate::theme::Theme;

pub(crate) enum Input {
    Text {
        value: String,
        cursor: usize,
    },
    Flag(bool),
    /// The highlighted row of a select or table. The highlight *is* the
    /// choice: there is no separate act of picking, which is how a
    /// native list box behaves and what a person expects of one.
    Row(Option<usize>),
}

pub(crate) struct Finished {
    pub message: String,
    pub outcome: Option<Outcome>,
}

pub(crate) enum Modal {
    /// Esc was pressed; `leaving` is which button is highlighted.
    Leave {
        leaving: bool,
    },
    Help,
}

/// What the main loop does after a key.
pub(crate) enum After {
    Continue,
    Repaint,
    Quit,
}

pub(crate) struct App {
    pub session: Session,
    pub inputs: HashMap<String, Input>,
    /// Index into [`App::focusable`].
    pub focus: usize,
    /// A transient line for the footer: a protocol complaint, or why a
    /// disabled thing would not act.
    pub status: Option<String>,
    pub finished: Option<Finished>,
    pub modal: Option<Modal>,
    /// Advances on every idle poll; drives the spinner.
    pub tick: u64,
    pub daemon_hint: &'static str,
    pub leave_hint: &'static str,
    pub title: &'static str,
    pub theme: Theme,
}

/// The rows of a select (its `choices`) or a table (its `rows`). One
/// shape for both, since the surface treats them alike: a list with a
/// highlight, answered with the highlighted row's `value`.
pub(crate) fn rows_of(e: &Element) -> Vec<Value> {
    let key = if e.r#type == types::TABLE {
        "rows"
    } else {
        "choices"
    };
    e.state
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn row_enabled(row: &Value) -> bool {
    row.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

pub(crate) fn is_list(e: &Element) -> bool {
    e.r#type == types::SELECT || e.r#type == types::TABLE
}

/// Output-only types take no focus. Everything else does -- disabled
/// elements included, so that Tab reaches them and the footer can say
/// why they are disabled. A thing shown greyed out with no way to ask
/// why is worse than one not shown at all.
fn takes_focus(e: &Element) -> bool {
    !matches!(
        e.r#type.as_str(),
        types::TEXT | types::PROGRESS | types::LOG
    )
}

impl App {
    pub(crate) fn new(
        theme: Theme,
        title: &'static str,
        daemon_hint: &'static str,
        leave_hint: &'static str,
    ) -> App {
        App {
            session: Session::new(),
            inputs: HashMap::new(),
            focus: 0,
            status: None,
            finished: None,
            modal: None,
            tick: 0,
            daemon_hint,
            leave_hint,
            title,
            theme,
        }
    }

    pub(crate) fn elements(&self) -> Vec<Element> {
        self.session
            .page()
            .map(|p| p.turn.elements.clone())
            .unwrap_or_default()
    }

    pub(crate) fn element(&self, r#ref: &str) -> Option<Element> {
        self.session.page().and_then(|p| p.element(r#ref).cloned())
    }

    pub(crate) fn focusable(&self) -> Vec<String> {
        self.elements()
            .iter()
            .filter(|e| takes_focus(e))
            .map(|e| e.r#ref.clone())
            .collect()
    }

    pub(crate) fn focused_ref(&self) -> Option<String> {
        self.focusable().get(self.focus).cloned()
    }

    pub(crate) fn focused_element(&self) -> Option<Element> {
        self.focused_ref().and_then(|r| self.element(&r))
    }

    /// A message from the daemon. Returns whether the page changed
    /// shape and so wants a full repaint.
    pub(crate) fn handle(&mut self, t: MsgType, body: Value) -> bool {
        match self.session.handle(t, body) {
            Ok(Event::NewTurn) => {
                self.rebuild_inputs();
                true
            }
            Ok(Event::Updated) => {
                self.refresh_inputs();
                false
            }
            Ok(Event::Bound(_)) | Ok(Event::Welcome(_)) | Ok(Event::Listing(_)) => false,
            Ok(Event::Refused(r)) => {
                let daemon = self.daemon_hint;
                let why = match r.reason {
                    RefusedReason::AccessDenied => {
                        format!("{daemon} refused this session: access denied.")
                    }
                    RefusedReason::UnknownKind => {
                        format!("{daemon} does not offer this kind of conversation.")
                    }
                    RefusedReason::UnsupportedElements => {
                        format!("This surface cannot draw what {daemon} needs to show.")
                    }
                    RefusedReason::Unavailable => {
                        format!("{daemon} cannot start a conversation right now.")
                    }
                };
                let message = match r.message {
                    Some(m) => format!("{why}\n\n{m}"),
                    None => why,
                };
                self.finished = Some(Finished {
                    message,
                    outcome: None,
                });
                true
            }
            Ok(Event::Ended(end)) => {
                self.finished = Some(Finished {
                    message: end
                        .message
                        .unwrap_or_else(|| format!("Conversation ended: {:?}", end.outcome)),
                    outcome: Some(end.outcome),
                });
                true
            }
            Ok(Event::ProtocolError(e)) => {
                self.status = Some(format!(
                    "protocol error: {:?}{}",
                    e.code,
                    e.message.map(|m| format!(" -- {m}")).unwrap_or_default()
                ));
                false
            }
            Err(e) => {
                self.finished = Some(Finished {
                    message: format!("Protocol failure: {e}"),
                    outcome: None,
                });
                true
            }
        }
    }

    /// The reader thread ended, which means the socket did.
    pub(crate) fn disconnected(&mut self) -> bool {
        if self.finished.is_some() {
            return false;
        }
        self.finished = Some(Finished {
            message: format!("{} closed the connection.", self.daemon_hint),
            outcome: None,
        });
        true
    }

    fn rebuild_inputs(&mut self) {
        self.inputs.clear();
        self.status = None;
        self.modal = None;
        let elements = self.elements();
        for e in &elements {
            let input = match e.r#type.as_str() {
                types::STRING => {
                    let value = e
                        .default
                        .as_ref()
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let cursor = value.chars().count();
                    Input::Text { value, cursor }
                }
                types::BOOLEAN => {
                    Input::Flag(e.default.as_ref().and_then(Value::as_bool).unwrap_or(false))
                }
                types::SELECT | types::TABLE => Input::Row(None),
                _ => continue,
            };
            self.inputs.insert(e.r#ref.clone(), input);
        }
        // Start on the first thing that can be answered, not the first
        // thing that can be focused: a page that opens on two disabled
        // fields wants the cursor on Next.
        let focusable = self.focusable();
        let first_enabled = focusable
            .iter()
            .position(|r| elements.iter().any(|e| &e.r#ref == r && e.enabled))
            .unwrap_or(0);
        self.set_focus(first_enabled);
    }

    /// After an UPDATE: keep what the person entered, but a highlight
    /// on a row that no longer exists is dropped.
    fn refresh_inputs(&mut self) {
        for e in self.elements() {
            if is_list(&e) {
                let n = rows_of(&e).len();
                if let Some(Input::Row(Some(i))) = self.inputs.get_mut(&e.r#ref)
                    && *i >= n
                {
                    self.inputs.insert(e.r#ref.clone(), Input::Row(None));
                }
            }
        }
    }

    /// Move focus, and give a list that has no highlight yet its first
    /// enabled row -- the moment focus lands on a list is the moment a
    /// person expects to see something highlighted in it.
    fn set_focus(&mut self, i: usize) {
        self.focus = i;
        let Some(e) = self.focused_element() else {
            return;
        };
        if !is_list(&e) || !e.enabled {
            return;
        }
        if let Some(Input::Row(None)) = self.inputs.get(&e.r#ref) {
            let first = rows_of(&e).iter().position(row_enabled);
            self.inputs.insert(e.r#ref.clone(), Input::Row(first));
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, stream: &mut UnixStream) -> After {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return After::Quit;
        }
        if ctrl && key.code == KeyCode::Char('l') {
            return After::Repaint;
        }
        if self.finished.is_some() {
            return After::Quit;
        }
        if let Some(modal) = self.modal.take() {
            return self.modal_key(modal, key.code);
        }
        match key.code {
            KeyCode::Esc => {
                self.modal = Some(Modal::Leave { leaving: false });
                return After::Continue;
            }
            KeyCode::F(1) => {
                self.modal = Some(Modal::Help);
                return After::Continue;
            }
            _ => {}
        }

        let focusable = self.focusable();
        if focusable.is_empty() {
            return After::Continue;
        }
        let n = focusable.len();
        self.focus = self.focus.min(n - 1);
        let current = focusable[self.focus].clone();
        let Some(elem) = self.element(&current) else {
            return After::Continue;
        };
        self.status = None;

        let next = (self.focus + 1) % n;
        let prev = (self.focus + n - 1) % n;
        match key.code {
            KeyCode::Tab => self.set_focus(next),
            KeyCode::BackTab => self.set_focus(prev),
            KeyCode::Down | KeyCode::Up => {
                let down = key.code == KeyCode::Down;
                // In a list, the arrows move the highlight, and step out
                // of the list only at its ends.
                if is_list(&elem) && elem.enabled {
                    if !self.move_row(&elem, down) {
                        self.set_focus(if down { next } else { prev });
                    }
                } else {
                    self.set_focus(if down { next } else { prev });
                }
            }
            KeyCode::Left | KeyCode::Right => {
                let right = key.code == KeyCode::Right;
                match self.inputs.get_mut(&current) {
                    Some(Input::Text { value, cursor }) => {
                        let len = value.chars().count();
                        *cursor = if right {
                            (*cursor + 1).min(len)
                        } else {
                            cursor.saturating_sub(1)
                        };
                    }
                    Some(Input::Row(_)) if elem.enabled => {
                        self.move_row(&elem, right);
                    }
                    _ if elem.is_action() => self.set_focus(if right { next } else { prev }),
                    _ => {}
                }
            }
            KeyCode::Home | KeyCode::End => {
                if let Some(Input::Text { value, cursor }) = self.inputs.get_mut(&current) {
                    *cursor = if key.code == KeyCode::Home {
                        0
                    } else {
                        value.chars().count()
                    };
                }
            }
            KeyCode::Char(' ') => match self.inputs.get_mut(&current) {
                Some(Input::Flag(b)) if elem.enabled => *b = !*b,
                Some(Input::Text { .. }) if elem.enabled => self.insert(&current, ' '),
                _ => {}
            },
            KeyCode::Char(c) => {
                if elem.enabled && matches!(self.inputs.get(&current), Some(Input::Text { .. })) {
                    self.insert(&current, c);
                }
            }
            KeyCode::Backspace => {
                if let (true, Some(Input::Text { value, cursor })) =
                    (elem.enabled, self.inputs.get_mut(&current))
                    && *cursor > 0
                {
                    let at = byte_at(value, *cursor - 1);
                    value.remove(at);
                    *cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if let (true, Some(Input::Text { value, cursor })) =
                    (elem.enabled, self.inputs.get_mut(&current))
                    && *cursor < value.chars().count()
                {
                    let at = byte_at(value, *cursor);
                    value.remove(at);
                }
            }
            KeyCode::Enter => {
                if elem.is_action() {
                    if elem.enabled {
                        self.press(&current, stream);
                    } else {
                        self.status = elem.help.clone().or_else(|| Some("Not available.".into()));
                    }
                } else {
                    self.set_focus(next);
                }
            }
            _ => {}
        }
        After::Continue
    }

    fn modal_key(&mut self, modal: Modal, code: KeyCode) -> After {
        match modal {
            Modal::Help => After::Continue,
            Modal::Leave { leaving } => match code {
                KeyCode::Esc => After::Continue,
                KeyCode::Enter => {
                    if leaving {
                        After::Quit
                    } else {
                        After::Continue
                    }
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    self.modal = Some(Modal::Leave { leaving: !leaving });
                    After::Continue
                }
                _ => {
                    self.modal = Some(Modal::Leave { leaving });
                    After::Continue
                }
            },
        }
    }

    fn insert(&mut self, r#ref: &str, c: char) {
        if let Some(Input::Text { value, cursor }) = self.inputs.get_mut(r#ref) {
            let at = byte_at(value, *cursor);
            value.insert(at, c);
            *cursor += 1;
        }
    }

    /// Move a list's highlight one row, skipping disabled rows.
    /// Returns false when there was nowhere to go in that direction.
    fn move_row(&mut self, e: &Element, down: bool) -> bool {
        let rows = rows_of(e);
        let Some(Input::Row(sel)) = self.inputs.get_mut(&e.r#ref) else {
            return false;
        };
        let mut i = match *sel {
            Some(i) => i,
            None => {
                *sel = rows.iter().position(row_enabled);
                return sel.is_some();
            }
        };
        loop {
            let candidate = if down { i + 1 } else { i.wrapping_sub(1) };
            let Some(row) = rows.get(candidate) else {
                return false;
            };
            i = candidate;
            if row_enabled(row) {
                *sel = Some(i);
                return true;
            }
        }
    }

    fn press(&mut self, action: &str, stream: &mut UnixStream) {
        let mut values: Map<String, Value> = Map::new();
        for e in self.elements() {
            if !e.enabled {
                continue;
            }
            match self.inputs.get(&e.r#ref) {
                Some(Input::Text { value, .. }) if !value.is_empty() => {
                    values.insert(e.r#ref.clone(), json!(value));
                }
                Some(Input::Flag(b)) => {
                    values.insert(e.r#ref.clone(), json!(b));
                }
                Some(Input::Row(Some(i))) => {
                    if let Some(row) = rows_of(&e).get(*i).filter(|r| row_enabled(r)) {
                        values.insert(e.r#ref.clone(), row["value"].clone());
                    }
                }
                _ => {}
            }
        }
        if let Some(ans) = self.session.answer(Some(action), values) {
            let _ = write_msg(stream, MsgType::Answer, &ans);
        }
    }
}

/// Byte offset of the `n`th character, for editing a `String` by
/// character position without assuming ASCII.
fn byte_at(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len())
}
