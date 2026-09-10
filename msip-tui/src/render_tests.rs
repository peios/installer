//! Frames rendered into a `TestBackend` and read back as text. These
//! pin the layout rules a person would notice breaking -- where the
//! column sits, what is highlighted, what a table drops first -- and
//! leave how it *looks* to a person looking at it.

use std::os::unix::net::UnixStream;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::{Value, json};

use msip::frame::MsgType;

use crate::app::App;
use crate::render::COLUMN;
use crate::theme::Theme;

fn app(elements: Value, class: Value) -> App {
    let mut app = App::new(Theme::detect(false, false), "Peios Test", "testd", "leave");
    app.handle(MsgType::Welcome, json!({"daemon": "testd", "kinds": ["t"]}));
    app.handle(MsgType::Bound, json!({"conversation": "c1", "seq": 0}));
    app.handle(
        MsgType::Turn,
        json!({"seq": 1, "id": "p", "name": "A page", "class": class, "elements": elements}),
    );
    app
}

fn frame(app: &App, w: u16, h: u16) -> Vec<String> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| app.render(f)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn bg_at(app: &App, w: u16, h: u16, x: u16, y: u16) -> Color {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| app.render(f)).unwrap();
    t.backend().buffer()[(x, y)].bg
}

fn press(app: &mut App, code: KeyCode) {
    let (mut a, _b) = UnixStream::pair().unwrap();
    app.key(KeyEvent::new(code, KeyModifiers::NONE), &mut a);
}

fn col(lines: &[String], needle: &str) -> Option<(usize, usize)> {
    lines
        .iter()
        .enumerate()
        .find_map(|(y, l)| l.find(needle).map(|x| (l[..x].chars().count(), y)))
}

fn form() -> Value {
    json!([
        {"ref": "intro", "type": "text", "text": "Some words."},
        {"ref": "name", "type": "string", "name": "User name"},
        {"ref": "pw", "type": "string", "name": "Password", "secret": true},
        {"ref": "back", "type": "action", "name": "Back"},
        {"ref": "next", "type": "action", "name": "Next", "primary": true}
    ])
}

/// The one rule that changes the look most: content lives in a column
/// no wider than [`COLUMN`], centred, however wide the terminal is.
#[test]
fn the_page_is_a_centred_column_on_a_wide_terminal() {
    let a = app(form(), json!([]));
    let lines = frame(&a, 200, 40);
    let (x, _) = col(&lines, "A page").unwrap();
    assert_eq!(x, ((200 - COLUMN) / 2) as usize);
}

#[test]
fn an_eighty_column_terminal_keeps_a_two_cell_margin() {
    let a = app(form(), json!([]));
    let lines = frame(&a, 80, 24);
    let (x, _) = col(&lines, "A page").unwrap();
    assert_eq!(x, 2);
}

#[test]
fn the_header_names_the_program_and_the_footer_offers_help() {
    let a = app(form(), json!([]));
    let lines = frame(&a, 100, 30);
    assert!(lines[0].contains("Peios Test"));
    assert!(lines[29].contains("F1: help"));
}

/// Typed into a secret field, the value never reaches the screen.
#[test]
fn a_secret_is_masked_on_screen() {
    let mut a = app(form(), json!([]));
    press(&mut a, KeyCode::Tab); // name -> pw
    for c in "hunter2".chars() {
        press(&mut a, KeyCode::Char(c));
    }
    let lines = frame(&a, 100, 30);
    let all = lines.join("\n");
    assert!(!all.contains("hunter2"));
    assert!(col(&lines, "•••••••").is_some());
}

#[test]
fn the_focused_action_is_highlighted_in_the_bar() {
    let mut a = app(form(), json!([]));
    for _ in 0..3 {
        press(&mut a, KeyCode::Tab); // name -> pw -> back -> next
    }
    assert_eq!(a.focused_ref().as_deref(), Some("next"));
    let lines = frame(&a, 100, 30);
    let (x, y) = col(&lines, "[ Next ]").unwrap();
    assert_eq!(bg_at(&a, 100, 30, x as u16 + 1, y as u16), Color::LightBlue);
}

#[test]
fn a_menu_page_lists_its_actions_down_the_body() {
    let a = app(
        json!([
            {"ref": "a", "type": "action", "name": "Install", "primary": true},
            {"ref": "b", "type": "action", "name": "Repair"},
            {"ref": "c", "type": "action", "name": "Upgrade", "enabled": false, "help": "Not yet."}
        ]),
        json!(["menu"]),
    );
    let lines = frame(&a, 100, 30);
    let (_, ya) = col(&lines, "Install").unwrap();
    let (_, yb) = col(&lines, "Repair").unwrap();
    let (_, yc) = col(&lines, "Upgrade").unwrap();
    assert!(ya < yb && yb < yc, "actions stack vertically");
    assert!(lines[ya].contains("▸ Install"), "the first is highlighted");
    assert!(
        lines[yc + 1].contains("Not yet."),
        "a disabled item explains itself beneath"
    );
}

/// Two disabled fields and a Next: the cursor opens on Next, because
/// that is the only thing that can be answered.
#[test]
fn focus_opens_on_the_first_enabled_element() {
    let a = app(
        json!([
            {"ref": "lang", "type": "select", "name": "Language", "enabled": false, "choices": []},
            {"ref": "kbd", "type": "select", "name": "Keyboard", "enabled": false, "choices": []},
            {"ref": "next", "type": "action", "name": "Next", "primary": true}
        ]),
        json!([]),
    );
    assert_eq!(a.focused_ref().as_deref(), Some("next"));
}

/// Tab still reaches a disabled thing, and the footer says why it is.
#[test]
fn a_disabled_element_takes_focus_and_explains_itself_in_the_footer() {
    let mut a = app(
        json!([
            {"ref": "lang", "type": "select", "name": "Language", "enabled": false, "choices": [],
             "help": "No locale data yet."},
            {"ref": "next", "type": "action", "name": "Next", "primary": true}
        ]),
        json!([]),
    );
    press(&mut a, KeyCode::Tab);
    assert_eq!(a.focused_ref().as_deref(), Some("lang"));
    let lines = frame(&a, 100, 30);
    assert!(lines[29].contains("No locale data yet."));
}

/// Landing on a list highlights its first row; the arrows walk it and
/// only leave at the ends.
#[test]
fn arrows_walk_a_list_and_leave_it_at_the_ends() {
    let mut a = app(
        json!([
            {"ref": "pick", "type": "select", "name": "Pick",
             "choices": [{"value": "a", "name": "A"}, {"value": "b", "name": "B"}]},
            {"ref": "next", "type": "action", "name": "Next"}
        ]),
        json!([]),
    );
    assert_eq!(a.focused_ref().as_deref(), Some("pick"));
    let lines = frame(&a, 100, 30);
    assert!(
        lines.iter().any(|l| l.contains("▸ A")),
        "first row highlighted on arrival"
    );
    press(&mut a, KeyCode::Down);
    let lines = frame(&a, 100, 30);
    assert!(lines.iter().any(|l| l.contains("▸ B")));
    press(&mut a, KeyCode::Down);
    assert_eq!(
        a.focused_ref().as_deref(),
        Some("next"),
        "past the end, focus moves on"
    );
    press(&mut a, KeyCode::Up);
    assert_eq!(a.focused_ref().as_deref(), Some("pick"));
    assert!(
        frame(&a, 100, 30).iter().any(|l| l.contains("▸ B")),
        "coming back lands where it left"
    );
}

fn disks() -> Value {
    json!([{
        "ref": "disk", "type": "table", "name": "Target disk", "required": true,
        "columns": [
            {"key": "device", "name": "Device"},
            {"key": "model", "name": "Model"},
            {"key": "size", "name": "Size", "align": "right"},
            {"key": "bus", "name": "Bus"}
        ],
        "rows": [
            {"value": "/dev/vda", "cells": {"device": "/dev/vda", "model": "QEMU DVD", "size": "966 MiB", "bus": "virtio"},
             "enabled": false, "note": "boot medium"},
            {"value": "/dev/vdb", "cells": {"device": "/dev/vdb", "model": "Virtio disk", "size": "8.0 GiB", "bus": "virtio"}}
        ]
    }, {"ref": "next", "type": "action", "name": "Next"}])
}

/// The medium is listed, so nobody wonders where it went, but it cannot
/// be chosen, so the first highlight lands on the disk that can.
#[test]
fn a_table_shows_its_columns_and_skips_disabled_rows() {
    let a = app(disks(), json!([]));
    let lines = frame(&a, 120, 30);
    let (_, header) = col(&lines, "Device").unwrap();
    assert!(
        lines[header].contains("Model")
            && lines[header].contains("Size")
            && lines[header].contains("Bus")
    );
    assert!(lines.iter().any(|l| l.contains("boot medium")));
    assert!(
        lines.iter().any(|l| l.contains("▸ /dev/vdb")),
        "highlight skipped the medium"
    );
    assert!(!lines.iter().any(|l| l.contains("▸ /dev/vda")));
}

/// Column order is priority order (a2): when the box is too narrow the
/// rightmost column goes first, and the cells that remain are whole.
#[test]
fn a_narrow_table_drops_columns_from_the_right() {
    let a = app(disks(), json!([]));
    let lines = frame(&a, 44, 30);
    let (_, header) = col(&lines, "Device").unwrap();
    assert!(lines[header].contains("Model"));
    assert!(
        !lines[header].contains("Bus"),
        "the last column is the first to go"
    );
    assert!(lines.iter().any(|l| l.contains("/dev/vdb")));
}

#[test]
fn a_finished_phase_is_ticked_and_the_next_is_active() {
    let a = app(
        json!([
            {"ref": "p1", "type": "progress", "name": "Partitioning", "value": 100, "max": 100},
            {"ref": "p2", "type": "progress", "name": "Formatting", "value": 40, "max": 100},
            {"ref": "p3", "type": "progress", "name": "Copying", "value": 0, "max": 100},
            {"ref": "out", "type": "log", "name": "Details", "lines": ["one", "two"]}
        ]),
        json!(["progress"]),
    );
    let lines = frame(&a, 100, 30);
    assert!(lines.iter().any(|l| l.contains("✓ Partitioning")));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("● Formatting") && l.contains("40%"))
    );
    assert!(lines.iter().any(|l| l.contains("○ Copying")));
    assert!(col(&lines, "two").is_some());
    let (_, bottom) = col(&lines, "╰").unwrap();
    assert!(
        bottom >= 26,
        "the log pane fills the page rather than sitting under the phases"
    );
}

/// Not a test: prints every kind of page so the layout can be looked
/// at without a machine. `cargo test -p msip-tui show -- --ignored --nocapture`
#[test]
#[ignore]
fn show() {
    let pages: Vec<(&str, Value, Value)> = vec![
        (
            "menu",
            json!([
                {"ref": "i", "type": "text", "text": "Set up Peios on this machine, or repair a system that is already installed."},
                {"ref": "a", "type": "action", "name": "Install Peios", "primary": true},
                {"ref": "b", "type": "action", "name": "Repair an existing system"},
                {"ref": "c", "type": "action", "name": "Upgrade an installation", "enabled": false,
                 "help": "Offline upgrade is not a v1 mode; upgrades are peipkg's job on the running system."}
            ]),
            json!(["menu"]),
        ),
        (
            "disk",
            {
                let mut v = disks();
                let arr = v.as_array_mut().unwrap();
                arr.insert(0, json!({"ref": "i", "type": "text", "text": "Choose the disk to install onto. Everything on it will be erased."}));
                arr.insert(
                    2,
                    json!({"ref": "rescan", "type": "action", "name": "Rescan disks"}),
                );
                arr.insert(3, json!({"ref": "back", "type": "action", "name": "Back"}));
                v
            },
            json!([]),
        ),
        (
            "account",
            json!([
                {"ref": "i", "type": "text", "text": "This account administers the machine. It is created here rather than shipped in the image, so nothing on this system carries a password anyone else could know."},
                {"ref": "name", "type": "string", "name": "User name", "default": "peios"},
                {"ref": "pw", "type": "string", "name": "Password", "secret": true},
                {"ref": "pw2", "type": "string", "name": "Confirm password", "secret": true, "error": "The passwords differ."},
                {"ref": "back", "type": "action", "name": "Back"},
                {"ref": "next", "type": "action", "name": "Next", "primary": true}
            ]),
            json!([]),
        ),
        (
            "confirm",
            json!([
                {"ref": "i", "type": "text", "text": "Peios will be installed onto Virtio disk, 8.0 GiB (/dev/vdb). The whole disk will be erased: partitioned, formatted, and overwritten. This cannot be undone."},
                {"ref": "back", "type": "action", "name": "Back"},
                {"ref": "go", "type": "action", "name": "Erase disk and install", "primary": true, "destructive": true}
            ]),
            json!(["confirm"]),
        ),
        (
            "progress",
            json!([
                {"ref": "p1", "type": "progress", "name": "Partitioning", "value": 100, "max": 100},
                {"ref": "p2", "type": "progress", "name": "Formatting", "value": 100, "max": 100},
                {"ref": "p3", "type": "progress", "name": "Copying the system", "value": 46, "max": 100},
                {"ref": "p4", "type": "progress", "name": "Setting up boot", "value": 0, "max": 100},
                {"ref": "out", "type": "log", "name": "Details", "lines": ["$ part /dev/vdb --whole-disk", "  wrote GPT", "$ mkfs.vfat -F 32 /dev/vdb1", "$ mke2fs -t ext4 /dev/vdb2", "copying /usr…"]}
            ]),
            json!(["progress"]),
        ),
    ];
    for (name, elements, class) in pages {
        let mut a = app(elements, class);
        if name == "confirm" {
            press(&mut a, KeyCode::Tab);
        }
        println!("\n===== {name} (100x30) =====");
        for l in frame(&a, 100, 30) {
            println!("|{l}|");
        }
    }
    let mut a = app(form(), json!([]));
    press(&mut a, KeyCode::Esc);
    println!("\n===== leave modal (80x24) =====");
    for l in frame(&a, 80, 24) {
        println!("|{l}|");
    }
    let mut a = app(form(), json!([]));
    a.handle(MsgType::End, json!({"seq": 1, "outcome": "complete", "message": "Installation complete. Reboot to start Peios."}));
    println!("\n===== finished (80x24) =====");
    for l in frame(&a, 80, 24) {
        println!("|{l}|");
    }
}

#[test]
fn esc_asks_before_leaving_and_enter_on_stay_stays() {
    let mut a = app(form(), json!([]));
    press(&mut a, KeyCode::Esc);
    let lines = frame(&a, 100, 30);
    assert!(lines.iter().any(|l| l.contains("Leave?")));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("[ Stay ]") && l.contains("[ Leave ]"))
    );
    press(&mut a, KeyCode::Enter);
    assert!(a.modal.is_none());
    assert!(a.finished.is_none());
}

#[test]
fn the_plain_glyph_set_stays_inside_cp437() {
    let mut a = App::new(Theme::detect(true, false), "Peios Test", "testd", "leave");
    a.handle(MsgType::Welcome, json!({"daemon": "testd", "kinds": ["t"]}));
    a.handle(MsgType::Bound, json!({"conversation": "c1", "seq": 0}));
    a.handle(
        MsgType::Turn,
        json!({"seq": 1, "name": "P", "elements": disks()}),
    );
    let all = frame(&a, 100, 30).join("");
    for forbidden in ["▸", "✓", "●", "○", "╭", "╮", "╰", "╯"] {
        assert!(
            !all.contains(forbidden),
            "{forbidden} is not in the VT font"
        );
    }
    assert!(all.contains("> /dev/vdb"));
}

#[test]
fn a_disabled_menu_item_explains_itself_however_long_the_reason() {
    let long = "Offline upgrade is not a v1 mode; upgrades are peipkg's job on the running system.";
    let a = app(
        json!([
            {"ref": "a", "type": "action", "name": "Install", "primary": true},
            {"ref": "c", "type": "action", "name": "Upgrade", "enabled": false, "help": long}
        ]),
        json!(["menu"]),
    );
    let lines = frame(&a, 100, 30);
    let (_, y) = col(&lines, "Upgrade").unwrap();
    assert!(
        lines[y + 1].contains("Offline upgrade"),
        "row after:\n{:?}\n{:?}",
        lines[y],
        lines[y + 1]
    );
}
