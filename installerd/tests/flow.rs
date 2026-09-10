//! installerd driven end to end over its socket by a scripted
//! surface: mode → disk (with a rescan) → confirm → progress → END,
//! plus a second surface attaching mid-install and a surface that
//! cannot render being refused at bind time.

use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::thread;

use serde_json::{Value, json};

use installerd::executor::{Disk, DryRun};
use installerd::flow_impl::Install;
use msip::element::types;
use msip::frame::{MsgType, read_msg, write_msg};
use msip::msg::{Answer, Hello, Start};
use msip::surface::{Event, Session};
use msip_serve::{Server, serve};

fn start_daemon(dir: &std::path::Path) -> std::path::PathBuf {
    let socket = dir.join("installerd.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let executor: Arc<dyn installerd::executor::Executor> = Arc::new(DryRun {
        step_ms: 1,
        disks: Some(vec![Disk {
            device: "/dev/vdb".into(),
            model: "Virtio disk".into(),
            size: 8 * 1024 * 1024 * 1024,
            bus: "virtio".into(),
            removable: false,
            medium: false,
        }]),
    });
    let server = Server::new("install", "installerd/test", move || {
        Box::new(Install::new(Arc::clone(&executor)))
    });
    thread::spawn(move || serve(listener, server));
    socket
}

struct Surface {
    stream: UnixStream,
    session: Session,
}

impl Surface {
    fn connect(socket: &std::path::Path, element_types: &[&str]) -> Surface {
        let mut stream = UnixStream::connect(socket).unwrap();
        write_msg(
            &mut stream,
            MsgType::Hello,
            &Hello {
                surface: Some("test".into()),
                element_types: element_types.iter().map(|s| s.to_string()).collect(),
            },
        )
        .unwrap();
        let mut session = Session::new();
        let (t, body) = read_msg(&mut stream).unwrap();
        assert!(matches!(
            session.handle(t, body).unwrap(),
            Event::Welcome(_)
        ));
        Surface { stream, session }
    }

    fn recv(&mut self) -> Event {
        let (t, body) = read_msg(&mut self.stream).unwrap();
        self.session.handle(t, body).unwrap()
    }

    fn answer(&mut self, action: &str, values: Vec<(&str, Value)>) {
        let ans = Answer {
            seq: self.session.page().unwrap().turn.seq,
            action: Some(action.into()),
            values: values
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        };
        write_msg(&mut self.stream, MsgType::Answer, &ans).unwrap();
    }

    fn turn_id(&self) -> String {
        self.session
            .page()
            .unwrap()
            .turn
            .id
            .clone()
            .unwrap_or_default()
    }
}

#[test]
fn full_install_flow_with_late_joiner() {
    let dir = std::env::temp_dir().join(format!("msip-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let socket = start_daemon(&dir);

    let mut a = Surface::connect(&socket, types::ALL);
    write_msg(
        &mut a.stream,
        MsgType::Start,
        &Start {
            kind: "install".into(),
        },
    )
    .unwrap();
    let Event::Bound(b) = a.recv() else {
        panic!("expected Bound")
    };
    assert!(!b.attached);
    assert!(matches!(a.recv(), Event::NewTurn));
    assert_eq!(a.turn_id(), "mode");

    a.answer("act.install", vec![]);
    assert!(matches!(a.recv(), Event::NewTurn));
    assert_eq!(a.turn_id(), "disk.choose");

    // Rescan refreshes choices in place, same seq.
    let seq_before = a.session.page().unwrap().turn.seq;
    a.answer("act.rescan", vec![]);
    assert!(matches!(a.recv(), Event::Updated));
    assert_eq!(a.session.page().unwrap().turn.seq, seq_before);
    let rows = a
        .session
        .page()
        .unwrap()
        .element("disk.target")
        .unwrap()
        .state["rows"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!rows.is_empty(), "dry-run probe found no disks to offer");
    let disk = rows[0]["value"].as_str().unwrap().to_string();

    // Next without a disk: validation error reaches the surface; turn stays open.
    a.answer("nav.next", vec![]);
    assert!(matches!(a.recv(), Event::Updated));
    assert_eq!(
        a.session
            .page()
            .unwrap()
            .element("disk.target")
            .unwrap()
            .error
            .as_deref(),
        Some("required")
    );

    a.answer("nav.next", vec![("disk.target", json!(disk))]);
    assert!(matches!(a.recv(), Event::NewTurn));
    assert_eq!(a.turn_id(), "confirm");
    a.answer("act.begin", vec![]);
    assert!(matches!(a.recv(), Event::NewTurn));
    assert_eq!(a.turn_id(), "install.progress");

    // A second surface STARTs mid-install and is attached to the same
    // conversation, receiving the progress page in current state.
    let mut late = Surface::connect(&socket, types::ALL);
    write_msg(
        &mut late.stream,
        MsgType::Start,
        &Start {
            kind: "install".into(),
        },
    )
    .unwrap();
    let Event::Bound(b) = late.recv() else {
        panic!("expected Bound")
    };
    assert!(b.attached);
    assert!(matches!(late.recv(), Event::NewTurn));
    assert_eq!(late.turn_id(), "install.progress");

    // Both ride the broadcast to END.
    let mut a_done = false;
    let mut late_done = false;
    while !(a_done && late_done) {
        if !a_done && let Event::Ended(end) = a.recv() {
            assert_eq!(end.outcome, msip::msg::Outcome::Complete);
            a_done = true;
        }
        if !late_done && let Event::Ended(end) = late.recv() {
            assert_eq!(end.outcome, msip::msg::Outcome::Complete);
            late_done = true;
        }
    }
    // The log accumulated through folding: the late joiner's copy has
    // the early lines it never saw broadcast.
    // (Checked on `late`'s last page before END cleared it — instead
    // assert via a third connection being refused post-END.)
    let mut post = Surface::connect(&socket, types::ALL);
    write_msg(
        &mut post.stream,
        MsgType::Start,
        &Start {
            kind: "install".into(),
        },
    )
    .unwrap();
    let Event::Bound(b) = post.recv() else {
        panic!("expected Bound")
    };
    assert!(!b.attached, "END must have retired the conversation");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn surface_missing_element_types_is_refused_at_bind() {
    let dir = std::env::temp_dir().join(format!("msip-test-refuse-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let socket = start_daemon(&dir);

    let mut poor = Surface::connect(&socket, &[types::TEXT, types::ACTION]);
    write_msg(
        &mut poor.stream,
        MsgType::Start,
        &Start {
            kind: "install".into(),
        },
    )
    .unwrap();
    let Event::Refused(r) = poor.recv() else {
        panic!("expected Refused")
    };
    assert_eq!(r.reason, msip::msg::RefusedReason::UnsupportedElements);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn back_navigation_returns_to_prior_pages() {
    let dir = std::env::temp_dir().join(format!("msip-test-back-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let socket = start_daemon(&dir);

    let mut s = Surface::connect(&socket, types::ALL);
    write_msg(
        &mut s.stream,
        MsgType::Start,
        &Start {
            kind: "install".into(),
        },
    )
    .unwrap();
    let Event::Bound(_) = s.recv() else { panic!() };
    assert!(matches!(s.recv(), Event::NewTurn));

    s.answer("act.repair", vec![]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.turn_id(), "disk.choose");
    // Back skips validation despite the required, unfilled select.
    s.answer("nav.back", vec![]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.turn_id(), "mode");

    // Into the repair menu and check the disabled repairs are present.
    s.answer("act.repair", vec![]);
    assert!(matches!(s.recv(), Event::NewTurn));
    let rows = s
        .session
        .page()
        .unwrap()
        .element("disk.target")
        .unwrap()
        .state["rows"]
        .as_array()
        .unwrap()
        .clone();
    let disk = rows[0]["value"].as_str().unwrap().to_string();
    s.answer("nav.next", vec![("disk.target", json!(disk))]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.turn_id(), "repair.menu");
    assert!(
        !s.session
            .page()
            .unwrap()
            .element("repair.verify")
            .unwrap()
            .enabled
    );
    assert!(
        !s.session
            .page()
            .unwrap()
            .element("repair.reinstall")
            .unwrap()
            .enabled
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The upgrade page is reached through the same disk page, says what
/// the disk holds and what the medium carries, and -- the dry run
/// claiming to be one revision ahead -- offers the upgrade.
#[test]
fn upgrade_flow_reaches_a_live_upgrade_button_and_finishes() {
    let dir = std::env::temp_dir().join(format!("msip-test-upgrade-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let socket = start_daemon(&dir);

    let mut s = Surface::connect(&socket, types::ALL);
    write_msg(
        &mut s.stream,
        MsgType::Start,
        &Start {
            kind: "install".into(),
        },
    )
    .unwrap();
    let Event::Bound(_) = s.recv() else { panic!() };
    assert!(matches!(s.recv(), Event::NewTurn));
    assert!(
        s.session
            .page()
            .unwrap()
            .element("act.upgrade")
            .unwrap()
            .enabled
    );

    s.answer("act.upgrade", vec![]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.turn_id(), "disk.choose");
    let rows = s
        .session
        .page()
        .unwrap()
        .element("disk.target")
        .unwrap()
        .state["rows"]
        .as_array()
        .unwrap()
        .clone();
    let disk = rows[0]["value"].as_str().unwrap().to_string();
    s.answer("nav.next", vec![("disk.target", json!(disk))]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.turn_id(), "upgrade.confirm");
    let page = s.session.page().unwrap();
    let summary = page.element("confirm.summary").unwrap().state["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(summary.contains("2026.8-1"), "{summary}");
    assert!(summary.contains("2026.8-2"), "{summary}");
    assert!(page.element("act.begin").unwrap().enabled);

    s.answer("act.begin", vec![]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.turn_id(), "upgrade.progress");
    loop {
        if let Event::Ended(end) = s.recv() {
            assert_eq!(end.outcome, msip::msg::Outcome::Complete);
            assert!(
                end.message
                    .unwrap_or_default()
                    .starts_with("Upgrade complete.")
            );
            break;
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// A medium that is not newer than the disk keeps the button, greyed,
/// with the reason as its help -- the page is the answer.
#[test]
fn an_upgrade_that_would_go_nowhere_is_offered_greyed_with_the_reason() {
    use installerd::executor::Release;
    use installerd::flow::upgrade_confirm_page;
    let same = Release {
        edition: "dev.peios.peios-experimental".into(),
        version: "2026.8-2".into(),
    };
    let page = upgrade_confirm_page(
        "/dev/vdb",
        "Virtio disk, 8.0 GiB",
        Ok(same.clone()),
        Ok(same),
    );
    let go = page
        .elements
        .iter()
        .find(|e| e.r#ref == "act.begin")
        .unwrap();
    assert!(!go.enabled);
    assert!(go.help.as_deref().unwrap().starts_with("Already current"));

    let page = upgrade_confirm_page(
        "/dev/vdb",
        "Virtio disk, 8.0 GiB",
        Err("no Peios system on /dev/vdb2".into()),
        Ok(Release {
            edition: "dev.peios.peios-experimental".into(),
            version: "2026.8-2".into(),
        }),
    );
    let go = page
        .elements
        .iter()
        .find(|e| e.r#ref == "act.begin")
        .unwrap();
    assert!(!go.enabled);
}
