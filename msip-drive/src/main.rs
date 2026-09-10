//! A surface with no user: it fills the refs it was told to fill,
//! presses the actions it was told to press, and prints every turn it
//! is sent.
//!
//! It exists to drive an installation from a script — a serial console
//! and a TUI make a poor test harness — but it is a general MSIP
//! client and knows nothing about installation. It is also the
//! smallest demonstration of the property the protocol was designed
//! for: a surface that answers from its own sources rather than from a
//! person needs no cooperation from the daemon, because actions carry
//! no data and every input is named by a stable ref.
//!
//!     msip-drive --socket /run/installerd.sock --kind install \
//!         --set disk.target=/dev/vdb \
//!         --press act.install --press nav.next --press act.begin
//!
//! Presses are consumed in order, one per turn that offers actions. A
//! turn with no enabled action is waited on rather than answered,
//! which is what a progress page wants.

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::process::exit;

use serde_json::{Map, Value, json};

use msip::element::{Element, types};
use msip::frame::{MsgType, read_msg, write_msg};
use msip::msg::{Hello, Outcome, Start};
use msip::surface::{Event, Session};

const USAGE: &str = "Usage: msip-drive [--socket PATH] [--kind NAME] [--set REF=VALUE]... [--set-file REF=PATH]... [--press REF]...\n\
\n\
Drive an MSIP conversation non-interactively and print every turn.\n\
\n\
Options:\n\
  --socket PATH     MSIP socket (default: /run/installerd.sock)\n\
  --kind NAME       conversation kind (default: install)\n\
  --set REF=VALUE   answer a named non-secret input; may be repeated\n\
  --set-file REF=PATH
                    answer from a file without exposing the value in argv\n\
  --press REF       press one action on the next actionable turn; repeat in order\n\
  -h, --help        show this help\n\
  -V, --version     show the version";

fn required(args: &mut impl Iterator<Item = String>, option: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("msip-drive: {option} needs a value\n{USAGE}");
        exit(2);
    })
}

fn assignment(value: &str, option: &str) -> (String, String) {
    let Some((r#ref, value)) = value.split_once('=') else {
        eprintln!("msip-drive: {option} wants REF=VALUE\n{USAGE}");
        exit(2);
    };
    if r#ref.is_empty() {
        eprintln!("msip-drive: {option} needs a non-empty REF\n{USAGE}");
        exit(2);
    }
    (r#ref.to_string(), value.to_string())
}

fn main() {
    let mut socket = "/run/installerd.sock".to_string();
    let mut kind = "install".to_string();
    let mut sets: HashMap<String, String> = HashMap::new();
    let mut presses: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = required(&mut args, "--socket"),
            "--kind" => kind = required(&mut args, "--kind"),
            "--press" => presses.push(required(&mut args, "--press")),
            "--set" => {
                let (r#ref, value) = assignment(&required(&mut args, "--set"), "--set");
                sets.insert(r#ref, value);
            }
            "--set-file" => {
                let (r#ref, path) = assignment(&required(&mut args, "--set-file"), "--set-file");
                let value = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    eprintln!("msip-drive: cannot read {path}: {error}");
                    exit(1);
                });
                sets.insert(r#ref, value);
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "-V" | "--version" => {
                println!("msip-drive {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("msip-drive: unknown argument {other}\n{USAGE}");
                exit(2);
            }
        }
    }

    let mut stream = match UnixStream::connect(&socket) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("connect {socket}: {e}");
            exit(1);
        }
    };
    write_msg(
        &mut stream,
        MsgType::Hello,
        &Hello {
            surface: Some(concat!("msip-drive/", env!("CARGO_PKG_VERSION")).into()),
            element_types: types::ALL.iter().map(|s| s.to_string()).collect(),
        },
    )
    .expect("hello");

    let mut session = Session::new();
    let mut presses = presses.into_iter();
    let mut started = false;
    loop {
        let (t, body) = match read_msg(&mut stream) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("connection lost: {e}");
                exit(1);
            }
        };
        let event = match session.handle(t, body) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("protocol: {e}");
                exit(1);
            }
        };
        match event {
            Event::Welcome(w) => {
                println!(
                    "welcome: {} kinds={:?}",
                    w.daemon.unwrap_or_default(),
                    w.kinds
                );
                write_msg(&mut stream, MsgType::Start, &Start { kind: kind.clone() })
                    .expect("start");
                started = true;
            }
            Event::Bound(b) => {
                println!(
                    "bound: {} seq={} attached={}",
                    b.conversation, b.seq, b.attached
                );
            }
            Event::Refused(r) => {
                eprintln!("refused: {:?} {}", r.reason, r.message.unwrap_or_default());
                exit(1);
            }
            Event::NewTurn | Event::Updated => {
                let Some(page) = session.page() else { continue };
                let is_new = matches!(event, Event::NewTurn);
                if is_new {
                    println!(
                        "\n== turn {} [{}] {}",
                        page.turn.seq,
                        page.turn.id.clone().unwrap_or_default(),
                        page.turn.name.clone().unwrap_or_default()
                    );
                }
                for e in &page.turn.elements {
                    print_element(e, is_new);
                }
                if !is_new {
                    continue;
                }
                let has_action = page
                    .turn
                    .elements
                    .iter()
                    .any(|e| e.is_action() && e.enabled);
                if !has_action {
                    println!("   (no actions: waiting)");
                    continue;
                }
                let Some(press) = presses.next() else {
                    eprintln!("no --press left for turn {}", page.turn.seq);
                    exit(1);
                };
                let mut values: Map<String, Value> = Map::new();
                for e in &page.turn.elements {
                    if e.is_action() || !e.enabled {
                        continue;
                    }
                    if let Some(v) = sets.get(&e.r#ref) {
                        values.insert(e.r#ref.clone(), coerce(e, v));
                    }
                }
                // §3.4: a surface must not log a secret's value. This
                // one prints everything it sends, so it has to mask the
                // elements the daemon marked secret -- the ref is what
                // matters for reading the trace anyway.
                let shown: Map<String, Value> = values
                    .iter()
                    .map(|(k, v)| {
                        let secret = page.turn.elements.iter().any(|e| &e.r#ref == k && e.secret);
                        (
                            k.clone(),
                            if secret { json!("<secret>") } else { v.clone() },
                        )
                    })
                    .collect();
                println!("   -> press {press} with {}", Value::Object(shown));
                let ans = session.answer(Some(&press), values).expect("answer");
                write_msg(&mut stream, MsgType::Answer, &ans).expect("send answer");
            }
            Event::Ended(end) => {
                println!(
                    "\n== end: {:?} {}",
                    end.outcome,
                    end.message.unwrap_or_default()
                );
                exit(match end.outcome {
                    Outcome::Complete => 0,
                    _ => 1,
                });
            }
            Event::ProtocolError(e) => {
                eprintln!(
                    "protocol error: {:?} {}",
                    e.code,
                    e.message.unwrap_or_default()
                );
                exit(1);
            }
            Event::Listing(_) => {}
        }
        let _ = started;
    }
}

/// Give a --set string the JSON type the element expects.
fn coerce(e: &Element, raw: &str) -> Value {
    match e.r#type.as_str() {
        types::BOOLEAN => json!(matches!(raw, "true" | "yes" | "1")),
        _ => json!(raw),
    }
}

fn print_element(e: &Element, verbose: bool) {
    let name = e.name.clone().unwrap_or_else(|| e.r#ref.clone());
    match e.r#type.as_str() {
        types::TEXT if verbose => {
            println!(
                "   {}",
                e.state.get("text").and_then(Value::as_str).unwrap_or("")
            )
        }
        types::PROGRESS => {
            let v = e.state.get("value").and_then(Value::as_u64).unwrap_or(0);
            println!("   [{name}] {v}%");
        }
        types::LOG => {
            if let Some(lines) = e.state.get("lines").and_then(Value::as_array) {
                for l in lines.iter().rev().take(3).rev() {
                    println!("   | {}", l.as_str().unwrap_or(""));
                }
            }
        }
        _ if verbose => {
            let mark = if e.enabled { "" } else { " (disabled)" };
            let err = e
                .error
                .as_ref()
                .map(|x| format!("  !! {x}"))
                .unwrap_or_default();
            println!("   - {} {name}{mark}{err}", e.r#ref);
            if let Some(cs) = e.state.get("choices").and_then(Value::as_array) {
                for c in cs {
                    println!(
                        "       {} — {}",
                        c["value"].as_str().unwrap_or(""),
                        c["name"].as_str().unwrap_or("")
                    );
                }
            }
            // A table's rows, one per line: the value, then every cell
            // in column order, then why a row cannot be chosen.
            if let Some(rows) = e.state.get("rows").and_then(Value::as_array) {
                let keys: Vec<String> = e
                    .state
                    .get("columns")
                    .and_then(Value::as_array)
                    .map(|cols| {
                        cols.iter()
                            .filter_map(|c| c["key"].as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                for r in rows {
                    let cells: Vec<&str> = keys
                        .iter()
                        .map(|k| r["cells"][k].as_str().unwrap_or(""))
                        .collect();
                    let off = if r["enabled"].as_bool() == Some(false) {
                        " (disabled)"
                    } else {
                        ""
                    };
                    let note = r["note"]
                        .as_str()
                        .map(|n| format!("  {n}"))
                        .unwrap_or_default();
                    println!(
                        "       {} — {}{off}{note}",
                        r["value"].as_str().unwrap_or(""),
                        cells.join(" | ")
                    );
                }
            }
        }
        _ => {
            if let Some(err) = &e.error {
                println!("   - {} !! {err}", e.r#ref);
            }
        }
    }
}
