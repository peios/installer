//! Serving an MSIP conversation, without the conversation.
//!
//! The msip crate is deliberately transport-free: it arbitrates turns
//! and answers and leaves sockets, fan-out and access control to the
//! daemon. This crate is that daemon half, minus the part that differs
//! — a [`Flow`] decides which page follows which answer, and everything
//! around it is the same whatever the flow is about.
//!
//! It serves **one kind, one conversation at a time**. A `START` while
//! a conversation is live attaches to it (`Bound { attached: true }`),
//! which is what both of Peios' flows want: there is one installation
//! and one first boot, and a second surface should join rather than be
//! turned away.

use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use msip::daemon::{Conversation, Disposition, TurnSpec, ValidAnswer};
use msip::frame::{FrameError, MsgType, read_msg, write_msg};
use msip::msg::{
    Answer, Attach, Bound, ErrorCode, ErrorMsg, Hello, Listing, ListingEntry, Outcome, Refused,
    RefusedReason, Start, Welcome,
};
use serde_json::{Map, Value};

/// Say something operational, where a boot can see it.
///
/// Goes to stderr, which a service manager routes to the log — and to
/// `/dev/kmsg`, which reaches the serial console. The second is the
/// point: a daemon that runs before anyone can log in has its stderr
/// delivered somewhere only a logged-in operator can read, so a failure
/// at that stage leaves no trace anywhere the operator can look.
///
/// One `write` per line, with a syslog priority prefix, because the
/// kernel makes one record per write. Best-effort: a least-privileged
/// service cannot open `/dev/kmsg`, and that is not worth a word of its
/// own on a console this daemon may be about to draw on.
/// What this library calls itself when it has something to say. A
/// daemon's own messages carry its own tag.
const TAG: &str = "msip";

mod console;

pub fn note(tag: &str, line: &str) {
    eprintln!("{tag}: {line}");
    static KMSG: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let kmsg = KMSG.get_or_init(|| {
        std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/kmsg")
            .ok()
            .map(Mutex::new)
    });
    if let Some(kmsg) = kmsg {
        use std::io::Write;
        // <6> is LOG_INFO on the kernel facility: what a console showing
        // ordinary boot messages displays.
        if let Ok(mut f) = kmsg.lock() {
            let _ = f.write_all(format!("<6>{tag}: {line}\n").as_bytes());
        }
    }
}

/// Tell the service manager this daemon is ready, if it is listening.
///
/// peinit passes `NOTIFY_SOCKET` to a service whose `Readiness` is
/// `Notify` and holds its dependents until `READY=1` arrives. That
/// matters here more than for most daemons: a surface is usually
/// declared to `Requires` its engine, and with `Alive` readiness
/// "started" means the process exists — which says nothing about
/// whether the socket it is supposed to connect to has been bound yet.
/// The surface then fails to connect, exits, and on a console takes the
/// terminal with it.
///
/// Silent when the variable is absent: a daemon run from a shell has no
/// service manager to tell, and that is not an error.
pub fn notify_ready() {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    match std::os::unix::net::UnixDatagram::unbound() {
        Ok(sock) => {
            if let Err(e) = sock.send_to(b"READY=1", &path) {
                note(TAG, &format!("notifying readiness on {path:?}: {e}"));
            }
        }
        Err(e) => note(TAG, &format!("opening a notify socket: {e}")),
    }
}

/// Progress from a long-running job, reported to the open page.
pub trait Progress: Send + Sync {
    /// Set a `progress` element's value, by ref.
    fn phase(&self, r#ref: &str, percent: u8);
    /// Append a line to the page's `log` element.
    fn log(&self, line: String);
}

/// A job that runs off the connection threads while its page stays open.
pub type Job = Box<dyn FnOnce(&dyn Progress) -> Result<(), String> + Send>;

/// What a valid answer leads to.
pub enum Step {
    /// Broadcast this page and wait for its answer.
    Page(TurnSpec),
    /// Patch the open page in place — a refreshed choice list, say.
    Patch(Vec<Map<String, Value>>),
    /// Domain validation failed: stamp these `ref -> message` errors
    /// onto the open page, which stays open for every surface.
    Reject(Vec<(String, String)>),
    /// Show `page`, then run `job`; its outcome ends the conversation.
    Work { page: TurnSpec, job: Job },
    /// Finish here.
    End(Outcome, Option<String>),
    /// The answer does not advance anything — a protocol error to whoever sent it.
    Refuse(&'static str),
}

/// The part that differs between one daemon and the next.
pub trait Flow: Send + 'static {
    /// Element types a surface must be able to render to drive this
    /// flow. A surface declaring less is refused at bind time rather
    /// than sent a page it cannot draw.
    fn needs(&self) -> &'static [&'static str];
    /// The first page.
    fn open(&mut self) -> TurnSpec;
    /// A structurally valid answer arrived.
    fn advance(&mut self, answer: &ValidAnswer) -> Step;
    /// What to say when a job finished successfully.
    fn completion_message(&self) -> Option<String> {
        None
    }
    /// The conversation has ended and this flow is about to be dropped.
    ///
    /// Where a flow that is *done with the machine* says so — a
    /// first-boot setup that has to remove its own service definition so
    /// it does not run again, say. Called once, after the END has been
    /// broadcast, so anything done here happens with the surfaces
    /// already told the outcome.
    fn finished(&mut self, outcome: Outcome) {
        let _ = outcome;
    }
}

struct Live {
    conversation: Conversation,
    flow: Box<dyn Flow>,
    /// A job is running: the open page takes no answers.
    working: bool,
    subscribers: Vec<UnixStream>,
    /// The kernel console, held quiet for as long as this conversation
    /// lives and restored when it is dropped. Never read: it is the
    /// drop that does the work, and hanging it here is what makes every
    /// way a conversation can end put the console back.
    _console: console::Quiet,
}

impl Live {
    fn broadcast<T: serde::Serialize>(&mut self, t: MsgType, body: &T) {
        self.subscribers
            .retain_mut(|s| write_msg(s, t, body).is_ok());
    }

    fn issue(&mut self, spec: TurnSpec) {
        let turn = self
            .conversation
            .issue(spec)
            .expect("issuing on a live conversation")
            .clone();
        self.broadcast(MsgType::Turn, &turn);
    }
}

pub struct Server {
    kind: &'static str,
    name: String,
    make_flow: Box<dyn Fn() -> Box<dyn Flow> + Send + Sync>,
    live: Mutex<Option<Live>>,
}

impl Server {
    pub fn new(
        kind: &'static str,
        name: impl Into<String>,
        make_flow: impl Fn() -> Box<dyn Flow> + Send + Sync + 'static,
    ) -> Arc<Server> {
        Arc::new(Server {
            kind,
            name: name.into(),
            make_flow: Box::new(make_flow),
            live: Mutex::new(None),
        })
    }

    fn start_job(self: &Arc<Self>, job: Job) {
        let server = Arc::clone(self);
        thread::spawn(move || {
            struct Reporter<'a>(&'a Server);
            impl Reporter<'_> {
                fn push(&self, patch: Map<String, Value>) {
                    let mut guard = self.0.live.lock().unwrap();
                    if let Some(live) = guard.as_mut()
                        && let Ok(upd) = live.conversation.update(vec![patch])
                    {
                        live.broadcast(MsgType::Update, &upd);
                    }
                }
            }
            impl Progress for Reporter<'_> {
                fn phase(&self, r#ref: &str, percent: u8) {
                    let mut p = Map::new();
                    p.insert("ref".into(), Value::String(r#ref.to_string()));
                    p.insert("value".into(), Value::from(percent));
                    self.push(p);
                }
                fn log(&self, line: String) {
                    let mut p = Map::new();
                    p.insert("ref".into(), Value::String("out".into()));
                    p.insert("append".into(), Value::Array(vec![Value::String(line)]));
                    self.push(p);
                }
            }

            let result = job(&Reporter(&server));
            let mut guard = server.live.lock().unwrap();
            if let Some(live) = guard.as_mut() {
                let (outcome, message) = match result {
                    Ok(()) => (Outcome::Complete, live.flow.completion_message()),
                    Err(e) => (Outcome::Failed, Some(e)),
                };
                if let Ok(end) = live.conversation.end(outcome, message) {
                    live.broadcast(MsgType::End, &end);
                }
                live.flow.finished(outcome);
            }
            // The conversation is over; the next START opens a fresh one.
            *guard = None;
        });
    }
}

/// Accept connections until the listener fails.
pub fn serve(listener: UnixListener, server: Arc<Server>) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let stream = stream?;
        let server = Arc::clone(&server);
        thread::spawn(move || {
            let _ = connection(stream, server);
        });
    }
    Ok(())
}

enum ConnState {
    ExpectHello,
    Unbound,
    Bound,
    /// Greeted, but cannot render what this flow needs.
    Unrenderable,
}

fn connection(mut stream: UnixStream, server: Arc<Server>) -> Result<(), FrameError> {
    let mut state = ConnState::ExpectHello;
    loop {
        let (t, body) = match read_msg(&mut stream) {
            Ok(m) => m,
            // A disconnect or garbage: drop the connection. Every other
            // attached surface carries on.
            Err(_) => return Ok(()),
        };
        match (&state, t) {
            (ConnState::ExpectHello, MsgType::Hello) => {
                let hello: Hello = match serde_json::from_value(body) {
                    Ok(h) => h,
                    Err(e) => return protocol(&mut stream, ErrorCode::Malformed, &e.to_string()),
                };
                write_msg(
                    &mut stream,
                    MsgType::Welcome,
                    &Welcome {
                        daemon: Some(server.name.clone()),
                        kinds: vec![server.kind.to_string()],
                    },
                )?;
                let needs = (server.make_flow)().needs().to_vec();
                let missing = needs
                    .iter()
                    .any(|n| !hello.element_types.iter().any(|t| t == n));
                state = if missing {
                    ConnState::Unrenderable
                } else {
                    ConnState::Unbound
                };
            }
            (ConnState::Unbound | ConnState::Unrenderable, MsgType::List) => {
                let guard = server.live.lock().unwrap();
                let conversations = guard
                    .as_ref()
                    .map(|l| {
                        vec![ListingEntry {
                            conversation: l.conversation.id().to_string(),
                            kind: l.conversation.kind().to_string(),
                            seq: l.conversation.seq(),
                        }]
                    })
                    .unwrap_or_default();
                drop(guard);
                write_msg(&mut stream, MsgType::Listing, &Listing { conversations })?;
            }
            // §3.6: a surface that cannot render what the flow needs is
            // refused at bind time, so an unrenderable page is never sent.
            (ConnState::Unrenderable, MsgType::Start | MsgType::Attach) => {
                write_msg(
                    &mut stream,
                    MsgType::Refused,
                    &Refused {
                        reason: RefusedReason::UnsupportedElements,
                        message: Some("this flow needs element types you did not declare".into()),
                    },
                )?;
            }
            (ConnState::Unbound, MsgType::Start) => {
                let start: Start = match serde_json::from_value(body) {
                    Ok(s) => s,
                    Err(e) => return protocol(&mut stream, ErrorCode::Malformed, &e.to_string()),
                };
                if start.kind != server.kind {
                    write_msg(
                        &mut stream,
                        MsgType::Refused,
                        &Refused {
                            reason: RefusedReason::UnknownKind,
                            message: None,
                        },
                    )?;
                    continue;
                }
                let mut guard = server.live.lock().unwrap();
                let fresh = guard.is_none();
                if fresh {
                    // Not `expect`. A conversation id is read from
                    // /dev/urandom, and on a machine where that read
                    // fails a panic here kills only this connection
                    // thread: the daemon stays up, the surface sits on a
                    // page that never arrives, and nothing anywhere says
                    // why. Told, it can say so on its own console.
                    match Conversation::new(server.kind) {
                        Ok(conversation) => {
                            *guard = Some(Live {
                                conversation,
                                flow: (server.make_flow)(),
                                working: false,
                                subscribers: Vec::new(),
                                _console: console::Quiet::hold(),
                            });
                        }
                        Err(e) => {
                            note(TAG, &format!("cannot open a conversation: {e}"));
                            drop(guard);
                            // Refused, not Error: §3.11 reserves Error
                            // for protocol violations, and nothing the
                            // surface did is wrong. §3.6's `unavailable`
                            // is the daemon admitting its own failure.
                            write_msg(
                                &mut stream,
                                MsgType::Refused,
                                &Refused {
                                    reason: RefusedReason::Unavailable,
                                    message: Some(format!("cannot open a conversation: {e}")),
                                },
                            )?;
                            continue;
                        }
                    }
                }
                let live = guard.as_mut().expect("a conversation was just opened");
                write_msg(
                    &mut stream,
                    MsgType::Bound,
                    &Bound {
                        conversation: live.conversation.id().to_string(),
                        seq: live.conversation.seq(),
                        attached: !fresh,
                    },
                )?;
                if fresh {
                    let first = live.flow.open();
                    match live.conversation.issue(first) {
                        Ok(turn) => {
                            let turn = turn.clone();
                            write_msg(&mut stream, MsgType::Turn, &turn)?;
                        }
                        Err(e) => {
                            // Same reasoning as above: a flow whose first
                            // page will not issue is a bug, and a bug the
                            // surface can report beats one it can only
                            // wait through. The half-made conversation is
                            // dropped so the next Start gets a clean one.
                            note(TAG, &format!("cannot issue the first turn: {e:?}"));
                            *guard = None;
                            drop(guard);
                            write_msg(
                                &mut stream,
                                MsgType::Refused,
                                &Refused {
                                    reason: RefusedReason::Unavailable,
                                    message: Some("the flow could not open its first page".into()),
                                },
                            )?;
                            continue;
                        }
                    }
                } else if let Some(turn) = live.conversation.current_turn() {
                    write_msg(&mut stream, MsgType::Turn, turn)?;
                }
                live.subscribers.push(stream.try_clone()?);
                state = ConnState::Bound;
            }
            (ConnState::Unbound, MsgType::Attach) => {
                let attach: Attach = match serde_json::from_value(body) {
                    Ok(a) => a,
                    Err(e) => return protocol(&mut stream, ErrorCode::Malformed, &e.to_string()),
                };
                let mut guard = server.live.lock().unwrap();
                match guard.as_mut() {
                    Some(live) if live.conversation.id() == attach.conversation => {
                        write_msg(
                            &mut stream,
                            MsgType::Bound,
                            &Bound {
                                conversation: live.conversation.id().to_string(),
                                seq: live.conversation.seq(),
                                attached: true,
                            },
                        )?;
                        if let Some(turn) = live.conversation.current_turn() {
                            write_msg(&mut stream, MsgType::Turn, turn)?;
                        }
                        live.subscribers.push(stream.try_clone()?);
                        state = ConnState::Bound;
                    }
                    _ => write_msg(
                        &mut stream,
                        MsgType::Refused,
                        &Refused {
                            reason: RefusedReason::UnknownKind,
                            message: None,
                        },
                    )?,
                }
            }
            (ConnState::Bound, MsgType::Answer) => {
                let answer: Answer = match serde_json::from_value(body) {
                    Ok(a) => a,
                    Err(e) => return protocol(&mut stream, ErrorCode::Malformed, &e.to_string()),
                };
                let mut guard = server.live.lock().unwrap();
                let Some(live) = guard.as_mut() else { continue };
                // A page with a job under it takes no answers; anything
                // arriving is stale by construction.
                if live.working {
                    continue;
                }
                match live.conversation.handle_answer(&answer) {
                    Disposition::Stale => {}
                    Disposition::Protocol(e) => {
                        let _ = write_msg(&mut stream, MsgType::Error, &e);
                    }
                    Disposition::Invalid(errors) => {
                        if let Ok(upd) = live.conversation.reject(&errors) {
                            live.broadcast(MsgType::Update, &upd);
                        }
                    }
                    Disposition::Valid(valid) => match apply(live, valid, &mut stream) {
                        After::Nothing => {}
                        After::Finished => *guard = None,
                        After::Job(job) => {
                            // The job reports progress through the same
                            // lock, so it cannot start while we hold it.
                            drop(guard);
                            server.start_job(job);
                        }
                    },
                }
            }
            (_, t) => {
                let code = match &state {
                    ConnState::Bound => ErrorCode::UnexpectedType,
                    _ => ErrorCode::Unbound,
                };
                let _ = write_msg(
                    &mut stream,
                    MsgType::Error,
                    &ErrorMsg {
                        code,
                        message: Some(format!("{t:?} not valid here")),
                    },
                );
            }
        }
    }
}

/// What the caller must do after the flow decided, once it can let go
/// of the lock. Returned rather than done here because starting a job
/// means releasing the lock the job itself takes to report progress.
enum After {
    Nothing,
    Finished,
    Job(Job),
}

/// Take the flow's decision and act on everything that can be done
/// while holding the conversation.
fn apply(live: &mut Live, valid: ValidAnswer, stream: &mut UnixStream) -> After {
    match live.flow.advance(&valid) {
        Step::Page(spec) => {
            live.issue(spec);
            After::Nothing
        }
        Step::Patch(patches) => {
            if let Ok(upd) = live.conversation.update(patches) {
                live.broadcast(MsgType::Update, &upd);
            }
            After::Nothing
        }
        Step::Reject(errors) => {
            if let Ok(upd) = live.conversation.reject(&errors) {
                live.broadcast(MsgType::Update, &upd);
            }
            After::Nothing
        }
        Step::End(outcome, message) => {
            if let Ok(end) = live.conversation.end(outcome, message) {
                live.broadcast(MsgType::End, &end);
            }
            live.flow.finished(outcome);
            After::Finished
        }
        Step::Work { page, job } => {
            live.working = true;
            live.issue(page);
            After::Job(job)
        }
        Step::Refuse(why) => {
            let _ = write_msg(
                stream,
                MsgType::Error,
                &ErrorMsg {
                    code: ErrorCode::BadValue,
                    message: Some(why.into()),
                },
            );
            After::Nothing
        }
    }
}

fn protocol(stream: &mut UnixStream, code: ErrorCode, msg: &str) -> Result<(), FrameError> {
    let _ = write_msg(
        stream,
        MsgType::Error,
        &ErrorMsg {
            code,
            message: Some(msg.into()),
        },
    );
    Ok(())
}

/// Stamp a descriptor on a daemon's socket, naming who may drive it.
///
/// An MSIP daemon holds privilege and its surfaces do not: a person
/// runs the surface from an ordinary administrator session and the
/// daemon does the work. That only functions if such a session can open
/// the socket, and a socket inherits whatever its directory happens to
/// grant rather than anything anyone decided — so it is stated here
/// instead, which is what PGSS Logon does for its own channel.
///
/// Shelling out to `sd` rather than linking libpeios keeps these
/// daemons free of the C ABI. A failure is a warning, not fatal: on a
/// machine with no `sd` the daemon is still useful, and on a real
/// system the socket keeps its inherited descriptor, which is more
/// restrictive rather than less.
pub fn secure_socket(path: &str, sddl: &str) {
    match std::process::Command::new("sd")
        .args(["set", path, sddl])
        .output()
    {
        Ok(out) if out.status.success() => {}
        Ok(out) => note(
            TAG,
            &format!(
                "could not set the descriptor on {path}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ),
        Err(e) => note(TAG, &format!("could not run sd on {path}: {e}")),
    }
}

/// SYSTEM and Administrators. Driving an installation or a first boot
/// means erasing a disk or minting the machine's first account, so it
/// is not Everyone.
pub const ADMIN_SOCKET_SDDL: &str = "O:SYG:SYD:(A;;GA;;;SY)(A;;GA;;;BA)";
