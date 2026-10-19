//! What a browser is served: the page and its parts, which never change, and
//! the state, which is sent again whenever it does.
//!
//! The page is this program's own from top to bottom. It is GXWI's overlay,
//! not an app in a desktop, so nothing frames it and no script but its own
//! runs in it. Everything it needs is in this binary: a machine being
//! installed may have no route out, so nothing is fetched from anywhere else.

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::header::{self, HeaderValue};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{any, get};
use std::sync::mpsc;

use tokio::sync::watch;

use crate::Conversation;
use crate::setup::{Asked, Heard, Said};
use crate::view::View;

const INDEX: &str = include_str!("../page/index.html");

const HTML: &str = "text/html; charset=utf-8";
const TEXT: &str = "text/plain; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const SCRIPT: &str = "text/javascript; charset=utf-8";
const FONT: &str = "font/woff2";

/// Only what is served from here runs, styles or is fetched, and the page
/// talks to nothing but this program.
const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; \
    img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
/// First-boot setup's page, which may also ask another address of the
/// machine's whether it answers: an address given by hand moves the machine
/// out from under the page at the end of setup, and the page follows it
/// there once it answers. Only whether it answers: nothing that comes back
/// can be read, and what runs is still only what is served from here.
const SETUP_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; \
    img-src 'self' data:; connect-src 'self' http:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// What serving a browser takes: the state to show it, what the job has
/// said of itself, and the way to the thread that holds the conversation,
/// for what it asks.
#[derive(Clone)]
struct Served {
    view: watch::Receiver<View>,
    said: Said,
    tell: mpsc::Sender<Heard>,
}

pub fn routes(conversation: Conversation, view: watch::Receiver<View>, said: Said, tell: mpsc::Sender<Heard>) -> Router {
    let part = |content_type, bytes: &'static [u8]| get(move || async move { file(content_type, bytes) });
    Router::new()
        .route("/", get(move || async move { with_policy(file(HTML, index(conversation).as_bytes()), policy(conversation)) }))
        .route("/style.css", part(CSS, include_bytes!("../page/style.css")))
        .route("/app.js", part(SCRIPT, include_bytes!("../page/app.js")))
        .route("/bits.js", part(SCRIPT, include_bytes!("../page/bits.js")))
        .route("/account.js", part(SCRIPT, include_bytes!("../page/account.js")))
        .route("/confirm.js", part(SCRIPT, include_bytes!("../page/confirm.js")))
        .route("/disk.js", part(SCRIPT, include_bytes!("../page/disk.js")))
        .route("/ending.js", part(SCRIPT, include_bytes!("../page/ending.js")))
        .route("/field.js", part(SCRIPT, include_bytes!("../page/field.js")))
        .route("/intro.js", part(SCRIPT, include_bytes!("../page/intro.js")))
        .route("/manual.js", part(SCRIPT, include_bytes!("../page/manual.js")))
        .route("/naming.js", part(SCRIPT, include_bytes!("../page/naming.js")))
        .route("/network.js", part(SCRIPT, include_bytes!("../page/network.js")))
        .route("/outro.js", part(SCRIPT, include_bytes!("../page/outro.js")))
        .route("/progress.js", part(SCRIPT, include_bytes!("../page/progress.js")))
        .route("/restart.js", part(SCRIPT, include_bytes!("../page/restart.js")))
        .route("/welcome.js", part(SCRIPT, include_bytes!("../page/welcome.js")))
        .route("/fonts/manrope.woff2", part(FONT, include_bytes!("../page/fonts/manrope.woff2")))
        .route("/fonts/schibsted-grotesk.woff2", part(FONT, include_bytes!("../page/fonts/schibsted-grotesk.woff2")))
        .route("/fonts/jetbrains-mono.woff2", part(FONT, include_bytes!("../page/fonts/jetbrains-mono.woff2")))
        // The state, live. `any`, because a websocket arrives as a GET over
        // HTTP/1.1 and as a CONNECT over HTTP/2, which is how GXWI sends it.
        .route("/live", any(live))
        .route("/log.txt", get(log))
        .route("/hello", get(hello))
        .fallback(elsewhere)
        .with_state(Served { view, said, tell })
}

/// Everything the job has said of itself, as a file to keep. A page shows
/// the last of it; the whole is what is wanted when a job has gone wrong, by
/// someone who is at another machine and has nowhere else to get it from.
async fn log(State(served): State<Served>) -> Response {
    let mut text = served.said.lock().unwrap_or_else(|e| e.into_inner()).join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    let mut response = text.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(TEXT));
    headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment; filename=\"peios-setup.log\""));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The page, saying whose conversation it draws before it has been sent
/// anything, so that what it says while it starts is in the right words.
fn index(conversation: Conversation) -> &'static str {
    match conversation {
        Conversation::Install => INDEX,
        // Once, for the life of the process.
        Conversation::Oobe => Box::leak(INDEX.replacen("<html lang=\"en\">", "<html lang=\"en\" data-conversation=\"oobe\">", 1).into_boxed_str()),
    }
}

/// Who is answering at this address, and on which boot of the machine: what
/// a page waiting on a restart asks, to tell what came back. The installer
/// answers as `installer` and first-boot setup as `setup`; anything else
/// answering here is neither.
async fn hello(State(served): State<Served>) -> Response {
    let view = served.view.borrow();
    let program = format!("{}/{}", view.conversation.program(), env!("CARGO_PKG_VERSION"));
    let who = match view.conversation {
        Conversation::Install => "installer",
        Conversation::Oobe => "setup",
    };
    let said = serde_json::json!({ who: program, "boot": view.boot });
    drop(view);
    let mut response = said.to_string().into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Everyone who reaches the machine is sent here, wherever they were going:
/// an address left over from a desktop means nothing to an installer.
async fn elsewhere(method: Method) -> Response {
    if method == Method::GET { Redirect::to("/").into_response() } else { StatusCode::NOT_FOUND.into_response() }
}

fn policy(conversation: Conversation) -> &'static str {
    match conversation {
        Conversation::Install => POLICY,
        Conversation::Oobe => SETUP_POLICY,
    }
}

fn with_policy(mut response: Response, policy: &'static str) -> Response {
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(policy));
    response
}

fn file(content_type: &'static str, bytes: &'static [u8]) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(POLICY));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    // A new build is a new page: nothing is kept without asking again.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

async fn live(State(served): State<Served>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| show(socket, served))
}

/// Sends one browser the state as it stands, and again each time it changes,
/// until the browser goes. What the browser asks for is passed to the thread
/// that holds the conversation, which decides what comes of it; a browser
/// learns the outcome as everyone does, from the state.
async fn show(mut socket: WebSocket, Served { mut view, tell, .. }: Served) {
    // Whether the state can still change. It cannot once the thread that
    // holds the conversation has gone, and what it left is then what a
    // browser goes on being shown, for as long as it stays.
    let mut live = true;
    loop {
        let state = serde_json::to_string(&*view.borrow_and_update()).unwrap_or_default();
        if socket.send(Message::Text(state.into())).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                changed = view.changed(), if live => match changed {
                    Ok(()) => break,
                    Err(_) => live = false,
                },
                heard = socket.recv() => match heard {
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                    Some(Ok(Message::Text(text))) => {
                        // Something that is not a request is this page's
                        // mistake or somebody's mischief; either way it is
                        // nothing to act on.
                        if let Ok(asked) = serde_json::from_str::<Asked>(&text) {
                            let _ = tell.send(Heard::Asked(asked));
                        }
                    }
                    Some(Ok(_)) => {}
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// A module of the page's that is not served fails to load, and takes
    /// the page down with it, in the browser and nowhere else.
    #[test]
    fn every_script_of_the_pages_is_served() {
        let routes = include_str!("page.rs");
        let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
        for entry in std::fs::read_dir(page).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.ends_with(".js") {
                let route = format!(".route(\"/{name}\", part(SCRIPT, include_bytes!(\"../page/{name}\")))");
                assert!(routes.contains(&route), "page/{name} is not served");
            }
        }
    }
}
