//! The first-boot flow driven over a real socket, including the two
//! paths that matter most: a mistyped password, and a rerun after the
//! account already exists.

use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use msip::element::types;
use msip::frame::{MsgType, read_msg, write_msg};
use msip::msg::{Answer, Hello, Outcome, Start};
use msip::surface::{Event, Session};
use msip_serve::{Progress, Server, serve};
use oobed::flow_impl::Oobe;
use oobed::setup::Setup;

/// Records what setup was asked to do, and can pretend the account is
/// already there.
#[derive(Default)]
struct Recorder {
    done: Mutex<Vec<String>>,
    account_present: bool,
    /// Make account creation fail, to exercise the path where setup
    /// must NOT retire itself.
    account_fails: bool,
}

/// What `Setup::retire` records instead of ending the process. The real
/// one does not return, which is the whole reason it sits behind the
/// trait: a flow that exits cannot be tested, and a flow that does not
/// retire would run again on a machine somebody is already using.
const RETIRED: &str = "retire";

impl Setup for Recorder {
    fn network_status(&self) -> String {
        "test network".into()
    }
    fn suggested_hostname(&self) -> String {
        "peios-test".into()
    }
    fn account_exists(&self, _name: &str) -> bool {
        self.account_present
    }
    fn create_account(&self, name: &str, password: &str, _p: &dyn Progress) -> Result<(), String> {
        self.done
            .lock()
            .unwrap()
            .push(format!("create {name}:{password}"));
        if self.account_fails {
            return Err("lpsd said no".into());
        }
        Ok(())
    }
    fn set_hostname(&self, name: &str, _p: &dyn Progress) -> Result<(), String> {
        self.done.lock().unwrap().push(format!("hostname {name}"));
        Ok(())
    }
    fn retire(&self) {
        self.done.lock().unwrap().push(RETIRED.to_string());
    }
}

struct Surface {
    stream: UnixStream,
    session: Session,
}

impl Surface {
    fn open(dir: &std::path::Path, setup: Arc<Recorder>) -> Surface {
        let socket = dir.join("oobed.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = Server::new("oobe", "oobed/test", move || {
            Box::new(Oobe::new(Arc::clone(&setup) as Arc<dyn Setup>))
        });
        thread::spawn(move || serve(listener, server));

        let mut stream = UnixStream::connect(&socket).unwrap();
        write_msg(
            &mut stream,
            MsgType::Hello,
            &Hello {
                surface: Some("test".into()),
                element_types: types::ALL.iter().map(|s| s.to_string()).collect(),
            },
        )
        .unwrap();
        let mut session = Session::new();
        let (t, body) = read_msg(&mut stream).unwrap();
        assert!(matches!(
            session.handle(t, body).unwrap(),
            Event::Welcome(_)
        ));
        write_msg(
            &mut stream,
            MsgType::Start,
            &Start {
                kind: "oobe".into(),
            },
        )
        .unwrap();
        let (t, body) = read_msg(&mut stream).unwrap();
        assert!(matches!(session.handle(t, body).unwrap(), Event::Bound(_)));
        let mut s = Surface { stream, session };
        assert!(matches!(s.recv(), Event::NewTurn));
        s
    }

    fn recv(&mut self) -> Event {
        let (t, body) = read_msg(&mut self.stream).unwrap();
        self.session.handle(t, body).unwrap()
    }

    fn id(&self) -> String {
        self.session
            .page()
            .unwrap()
            .turn
            .id
            .clone()
            .unwrap_or_default()
    }

    fn seq(&self) -> u64 {
        self.session.page().unwrap().turn.seq
    }

    fn press(&mut self, action: &str, values: &[(&str, &str)]) {
        let ans = Answer {
            seq: self.seq(),
            action: Some(action.into()),
            values: values
                .iter()
                .map(|(k, v)| (k.to_string(), json!(v)))
                .collect(),
        };
        write_msg(&mut self.stream, MsgType::Answer, &ans).unwrap();
    }

    /// Walk to the account page, which every test needs.
    fn advance_to_account(&mut self) {
        assert_eq!(self.id(), "oobe.locale");
        self.press("nav.next", &[]);
        assert!(matches!(self.recv(), Event::NewTurn));
        assert_eq!(self.id(), "oobe.network");
        self.press("nav.next", &[]);
        assert!(matches!(self.recv(), Event::NewTurn));
        assert_eq!(self.id(), "oobe.account");
    }

    fn drain_to_end(&mut self) -> Outcome {
        loop {
            if let Event::Ended(end) = self.recv() {
                return end.outcome;
            }
        }
    }
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oobe-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// END is intentionally broadcast before `Flow::finished` runs, so a client
/// can observe the outcome while the flow performs its final local cleanup.
/// Wait for that hook when a test asserts its side effects rather than racing
/// the server thread immediately after receiving END.
fn wait_for_steps(setup: &Recorder, count: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let done = setup.done.lock().unwrap().clone();
        if done.len() >= count {
            return done;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for setup completion hook: {done:?}"
        );
        thread::yield_now();
    }
}

#[test]
fn a_full_setup_creates_the_account_and_names_the_machine() {
    let dir = scratch("full");
    let setup = Arc::new(Recorder::default());
    let mut s = Surface::open(&dir, Arc::clone(&setup));

    s.advance_to_account();
    s.press(
        "nav.next",
        &[
            ("account.name", "jack"),
            ("account.password", "correct horse"),
            ("account.confirm", "correct horse"),
        ],
    );
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.id(), "oobe.naming");

    // The machine name is offered, so accepting it is one keypress.
    let host = s
        .session
        .page()
        .unwrap()
        .element("hostname")
        .unwrap()
        .clone();
    assert_eq!(host.default, Some(json!("peios-test")));
    // Domain join is present but not yet possible.
    assert!(
        !s.session
            .page()
            .unwrap()
            .element("domain.join")
            .unwrap()
            .enabled
    );

    s.press("nav.finish", &[("hostname", "workshop")]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.id(), "oobe.applying");
    assert_eq!(s.drain_to_end(), Outcome::Complete);

    // Retirement last, and only after everything else succeeded: it
    // removes the definitions that start setup, so a machine reaches it
    // exactly once, and only once it is actually usable.
    let done = wait_for_steps(&setup, 3);
    assert_eq!(
        done,
        vec!["create jack:correct horse", "hostname workshop", RETIRED]
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// The property the whole first-boot story rests on: a setup that did
/// not finish does not retire itself.
///
/// Retiring removes the service definitions that start setup, and a
/// machine with no account cannot be logged into to put them back. So
/// the only recovery from a failed run is to reboot into the same
/// question — which exists only while the definitions do.
#[test]
fn a_failed_setup_does_not_retire_itself() {
    let dir = scratch("failed");
    let setup = Arc::new(Recorder {
        account_fails: true,
        ..Recorder::default()
    });
    let mut s = Surface::open(&dir, Arc::clone(&setup));

    s.press("nav.next", &[]);
    assert!(matches!(s.recv(), Event::NewTurn));
    s.press("nav.next", &[]);
    assert!(matches!(s.recv(), Event::NewTurn));
    s.press(
        "nav.next",
        &[
            ("account.name", "jack"),
            ("account.password", "x"),
            ("account.confirm", "x"),
        ],
    );
    assert!(matches!(s.recv(), Event::NewTurn));
    s.press("nav.finish", &[("hostname", "workshop")]);
    assert!(matches!(s.recv(), Event::NewTurn));

    assert_eq!(s.drain_to_end(), Outcome::Failed);
    let done = setup.done.lock().unwrap().clone();
    assert!(
        !done.iter().any(|step| step == RETIRED),
        "a failed setup must be able to run again: {done:?}",
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_mistyped_password_keeps_the_page_open() {
    let dir = scratch("mismatch");
    let setup = Arc::new(Recorder::default());
    let mut s = Surface::open(&dir, Arc::clone(&setup));

    s.advance_to_account();
    let seq = s.seq();
    s.press(
        "nav.next",
        &[
            ("account.name", "jack"),
            ("account.password", "one"),
            ("account.confirm", "two"),
        ],
    );
    // An update, not a new turn: the page stays open for every surface.
    assert!(matches!(s.recv(), Event::Updated));
    assert_eq!(s.seq(), seq);
    assert_eq!(s.id(), "oobe.account");
    let confirm = s
        .session
        .page()
        .unwrap()
        .element("account.confirm")
        .unwrap();
    assert_eq!(
        confirm.error.as_deref(),
        Some("The passwords do not match.")
    );
    // The error is on the field they retype, not on the one they meant.
    assert!(
        s.session
            .page()
            .unwrap()
            .element("account.password")
            .unwrap()
            .error
            .is_none()
    );

    // Correcting it proceeds, on the same page.
    s.press(
        "nav.next",
        &[
            ("account.name", "jack"),
            ("account.password", "one"),
            ("account.confirm", "one"),
        ],
    );
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.id(), "oobe.naming");
    assert!(setup.done.lock().unwrap().is_empty(), "nothing applied yet");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_rerun_keeps_an_account_that_already_exists() {
    // The failure this guards against is lpsd-first-account's: a
    // provisioner that treats "already done" as an error and fails on
    // every boot after the first.
    let dir = scratch("rerun");
    let setup = Arc::new(Recorder {
        account_present: true,
        ..Default::default()
    });
    let mut s = Surface::open(&dir, Arc::clone(&setup));

    s.advance_to_account();
    s.press(
        "nav.next",
        &[
            ("account.name", "jack"),
            ("account.password", "x"),
            ("account.confirm", "x"),
        ],
    );
    assert!(matches!(s.recv(), Event::NewTurn));
    s.press("nav.finish", &[("hostname", "workshop")]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.drain_to_end(), Outcome::Complete);

    // The account was left alone; the rest of setup still ran.
    let done = wait_for_steps(&setup, 2);
    assert_eq!(done, vec!["hostname workshop", RETIRED]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn going_back_offers_the_name_again_but_not_the_password() {
    let dir = scratch("back");
    let setup = Arc::new(Recorder::default());
    let mut s = Surface::open(&dir, Arc::clone(&setup));

    s.advance_to_account();
    s.press(
        "nav.next",
        &[
            ("account.name", "ada"),
            ("account.password", "p"),
            ("account.confirm", "p"),
        ],
    );
    assert!(matches!(s.recv(), Event::NewTurn));
    s.press("nav.back", &[]);
    assert!(matches!(s.recv(), Event::NewTurn));
    assert_eq!(s.id(), "oobe.account");

    let page = s.session.page().unwrap();
    assert_eq!(
        page.element("account.name").unwrap().default,
        Some(json!("ada"))
    );
    assert!(page.element("account.password").unwrap().default.is_none());
    assert!(page.element("account.password").unwrap().secret);
    std::fs::remove_dir_all(&dir).ok();
}

/// Values are never echoed back to any surface, secret or not — the
/// daemon acknowledges an answer with the next page (§3.9).
#[test]
fn a_password_is_never_sent_back_out() {
    let dir = scratch("noecho");
    let setup = Arc::new(Recorder::default());
    let mut s = Surface::open(&dir, Arc::clone(&setup));

    s.advance_to_account();
    s.press(
        "nav.next",
        &[
            ("account.name", "jack"),
            ("account.password", "hunter2"),
            ("account.confirm", "hunter2"),
        ],
    );
    assert!(matches!(s.recv(), Event::NewTurn));
    let rendered = serde_json::to_string(&s.session.page().unwrap().turn).unwrap();
    assert!(
        !rendered.contains("hunter2"),
        "a page carried the password: {rendered}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

fn _unused(_: Value) {}
