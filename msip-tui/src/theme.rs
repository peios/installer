//! What the surface looks like: one set of colours, and two sets of
//! glyphs for two kinds of console.
//!
//! Colours are the sixteen ANSI names, never 256-colour indexes or
//! truecolour. On a terminal emulator that means the person's own
//! theme; on a Linux VT it means the palette [`VT_PALETTE`] programs.
//! Either way the surface never assumes what a colour looks like, only
//! what it means.

use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;

/// Every glyph the renderer draws that is not a letter, chosen in one
/// place so the two sets stay parallel.
pub(crate) struct Glyphs {
    pub border: border::Set,
    pub rule: &'static str,
    /// The text cursor, drawn after the value.
    pub cursor: &'static str,
    /// Marks the highlighted row of a list.
    pub marker: &'static str,
    pub done: &'static str,
    pub active: &'static str,
    pub pending: &'static str,
    /// One of these per character of a secret.
    pub secret: &'static str,
    pub fill: &'static str,
    pub track: &'static str,
    pub spinner: &'static [&'static str],
    pub ok: &'static str,
    pub fail: &'static str,
    pub warn: &'static str,
    pub checked: &'static str,
    pub unchecked: &'static str,
}

/// For terminal emulators, which have a real font.
pub(crate) const RICH: Glyphs = Glyphs {
    border: border::ROUNDED,
    rule: "─",
    cursor: "▏",
    marker: "▸",
    done: "✓",
    active: "●",
    pending: "○",
    secret: "•",
    fill: "█",
    track: "░",
    spinner: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
    ok: "✓",
    fail: "✗",
    warn: "!",
    checked: "[x]",
    unchecked: "[ ]",
};

/// For the Linux VT and anything else that might be a real terminal:
/// nothing outside CP437, which the kernel's font is built from. Box
/// drawing, blocks and shade are in it; ticks, circles and rounded
/// corners are not.
pub(crate) const PLAIN: Glyphs = Glyphs {
    border: border::PLAIN,
    rule: "─",
    cursor: "_",
    marker: ">",
    done: "*",
    active: ">",
    pending: " ",
    secret: "*",
    fill: "█",
    track: "░",
    spinner: &["|", "/", "-", "\\"],
    ok: "*",
    fail: "x",
    warn: "!",
    checked: "[x]",
    unchecked: "[ ]",
};

/// What the sixteen VT colours are set to, index by index, when the
/// console is a VT. A dark, low-contrast base with soft accents --
/// the same family of palette most terminal emulators ship by
/// default now, so the two consoles come out looking alike.
pub(crate) const VT_PALETTE: [&str; 16] = [
    "1e1e2e", // 0 black: the background
    "f38ba8", // 1 red
    "a6e3a1", // 2 green
    "f9e2af", // 3 yellow
    "89b4fa", // 4 blue
    "f5c2e7", // 5 magenta
    "94e2d5", // 6 cyan
    "bac2de", // 7 white: ordinary text
    "585b70", // 8 bright black: dim text
    "f38ba8", // 9
    "a6e3a1", // 10
    "f9e2af", // 11
    "89b4fa", // 12
    "f5c2e7", // 13
    "94e2d5", // 14
    "cdd6f4", // 15 bright white
];

pub(crate) struct Theme {
    pub glyphs: &'static Glyphs,
    pub accent: Color,
    pub dim: Color,
    pub danger: Color,
    pub ok: Color,
}

impl Theme {
    /// `plain` is the person overriding detection; `vt` is what the
    /// console turned out to be.
    pub(crate) fn detect(plain: bool, vt: bool) -> Theme {
        Theme {
            glyphs: if plain || vt { &PLAIN } else { &RICH },
            accent: Color::LightBlue,
            dim: Color::DarkGray,
            danger: Color::LightRed,
            ok: Color::LightGreen,
        }
    }

    pub(crate) fn dim(&self) -> Style {
        Style::default().fg(self.dim)
    }
    pub(crate) fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }
    pub(crate) fn heading(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
    pub(crate) fn label(&self, focused: bool) -> Style {
        if focused {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        }
    }
    /// The focus ring on a boxed input.
    pub(crate) fn frame(&self, focused: bool, enabled: bool) -> Style {
        match (enabled, focused) {
            (false, _) => self.dim(),
            (true, true) => self.accent(),
            (true, false) => Style::default(),
        }
    }
    /// A highlighted row or chip: inverse in the accent colour.
    pub(crate) fn highlight(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
    pub(crate) fn danger_highlight(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(self.danger)
            .add_modifier(Modifier::BOLD)
    }
    pub(crate) fn error(&self) -> Style {
        Style::default().fg(self.danger)
    }
    pub(crate) fn ok(&self) -> Style {
        Style::default().fg(self.ok)
    }
}
