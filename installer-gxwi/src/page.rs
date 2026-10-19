//! What a browser is served: the page and its parts, which never change, and
//! the state, which is sent again whenever it does.
//!
//! The page is this program's own from top to bottom. It is GXWI's overlay,
//! not an app in a desktop, so nothing frames it and no script but its own
//! runs in it. Everything it needs is in this binary: a machine being
//! installed may have no route out, so nothing is fetched from anywhere else.

use std::sync::{OnceLock, mpsc};

use axum::Router;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::header::{self, HeaderValue};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{any, get};
use tokio::sync::watch;

use crate::Conversation;
use crate::input::{Admission, MAX_MESSAGE_BYTES};
use crate::setup::{Heard, Said};

mod live;
use crate::view::View;

const INDEX: &str = include_str!("../page/index.html");

const HTML: &str = "text/html; charset=utf-8";
const TEXT: &str = "text/plain; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const SCRIPT: &str = "text/javascript; charset=utf-8";
const FONT: &str = "font/woff2";

/// Immutable resources bundled into every frontend binary.
const ASSETS: &[(&str, &str, &[u8])] = &[
    ("/style.css", CSS, include_bytes!("../page/style.css")),
    ("/app.js", SCRIPT, include_bytes!("../page/app.js")),
    ("/bits.js", SCRIPT, include_bytes!("../page/bits.js")),
    ("/account.js", SCRIPT, include_bytes!("../page/account.js")),
    ("/confirm.js", SCRIPT, include_bytes!("../page/confirm.js")),
    ("/disk.js", SCRIPT, include_bytes!("../page/disk.js")),
    ("/ending.js", SCRIPT, include_bytes!("../page/ending.js")),
    ("/field.js", SCRIPT, include_bytes!("../page/field.js")),
    ("/intro.js", SCRIPT, include_bytes!("../page/intro.js")),
    ("/manual.js", SCRIPT, include_bytes!("../page/manual.js")),
    ("/naming.js", SCRIPT, include_bytes!("../page/naming.js")),
    ("/network.js", SCRIPT, include_bytes!("../page/network.js")),
    ("/outro.js", SCRIPT, include_bytes!("../page/outro.js")),
    (
        "/progress.js",
        SCRIPT,
        include_bytes!("../page/progress.js"),
    ),
    ("/restart.js", SCRIPT, include_bytes!("../page/restart.js")),
    (
        "/requests.js",
        SCRIPT,
        include_bytes!("../page/requests.js"),
    ),
    ("/welcome.js", SCRIPT, include_bytes!("../page/welcome.js")),
    (
        "/fonts/manrope.woff2",
        FONT,
        include_bytes!("../page/fonts/manrope.woff2"),
    ),
    (
        "/fonts/schibsted-grotesk.woff2",
        FONT,
        include_bytes!("../page/fonts/schibsted-grotesk.woff2"),
    ),
    (
        "/fonts/jetbrains-mono.woff2",
        FONT,
        include_bytes!("../page/fonts/jetbrains-mono.woff2"),
    ),
];

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
    admission: Admission,
}

pub fn routes(
    conversation: Conversation,
    view: watch::Receiver<View>,
    said: Said,
    tell: mpsc::Sender<Heard>,
) -> Router {
    let mut router = Router::new();
    for &(path, content_type, bytes) in ASSETS {
        router = router.route(path, get(move || async move { file(content_type, bytes) }));
    }
    router
        .route(
            "/",
            get(move || async move {
                with_policy(
                    file(HTML, index(conversation).as_bytes()),
                    policy(conversation),
                )
            }),
        )
        // The state, live. `any`, because a websocket arrives as a GET over
        // HTTP/1.1 and as a CONNECT over HTTP/2, which is how GXWI sends it.
        .route("/live", any(live))
        .route("/log.txt", get(log))
        .route("/hello", get(hello))
        .fallback(elsewhere)
        .with_state(Served {
            view,
            said,
            tell,
            admission: Admission::default(),
        })
}

/// Everything the job has said of itself, as a file to keep. A page shows
/// the last of it; the whole is what is wanted when a job has gone wrong, by
/// someone who is at another machine and has nowhere else to get it from.
async fn log(State(served): State<Served>) -> Response {
    let mut text = served
        .said
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    let mut response = text.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(TEXT));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"peios-setup.log\""),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The page, saying whose conversation it draws before it has been sent
/// anything, so that what it says while it starts is in the right words.
fn index(conversation: Conversation) -> &'static str {
    match conversation {
        Conversation::Install => INDEX,
        Conversation::Oobe => {
            static OOBE_INDEX: OnceLock<String> = OnceLock::new();
            OOBE_INDEX.get_or_init(|| {
                INDEX.replacen(
                    "<html lang=\"en\">",
                    "<html lang=\"en\" data-conversation=\"oobe\">",
                    1,
                )
            })
        }
    }
}

/// Who is answering at this address, and on which boot of the machine: what
/// a page waiting on a restart asks, to tell what came back. The installer
/// answers as `installer` and first-boot setup as `setup`; anything else
/// answering here is neither.
async fn hello(State(served): State<Served>) -> Response {
    let view = served.view.borrow();
    let program = format!(
        "{}/{}",
        view.conversation.program(),
        env!("CARGO_PKG_VERSION")
    );
    let who = match view.conversation {
        Conversation::Install => "installer",
        Conversation::Oobe => "setup",
    };
    let said = serde_json::json!({ who: program, "boot": view.boot });
    drop(view);
    let mut response = said.to_string().into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Everyone who reaches the machine is sent here, wherever they were going:
/// an address left over from a desktop means nothing to an installer.
async fn elsewhere(method: Method) -> Response {
    if method == Method::GET {
        Redirect::to("/").into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

fn policy(conversation: Conversation) -> &'static str {
    match conversation {
        Conversation::Install => POLICY,
        Conversation::Oobe => SETUP_POLICY,
    }
}

fn with_policy(mut response: Response, policy: &'static str) -> Response {
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(policy),
    );
    response
}

fn file(content_type: &'static str, bytes: &'static [u8]) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    // A new build is a new page: nothing is kept without asking again.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

async fn live(State(served): State<Served>, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| live::show(socket, served))
}

#[cfg(test)]
mod tests {
    #[test]
    fn oobe_index_is_initialized_once_and_install_index_is_unchanged() {
        let first = super::index(crate::Conversation::Oobe);
        let second = super::index(crate::Conversation::Oobe);
        assert!(std::ptr::eq(first, second));
        assert_eq!(first.matches("data-conversation=\"oobe\"").count(), 1);
        assert_eq!(super::index(crate::Conversation::Install), super::INDEX);
        assert!(!super::INDEX.contains("data-conversation=\"oobe\""));
    }

    /// A module of the page's that is not served fails to load, and takes
    /// the page down with it, in the browser and nowhere else.
    #[test]
    fn every_script_of_the_pages_is_served() {
        let page = concat!(env!("CARGO_MANIFEST_DIR"), "/page");
        for entry in std::fs::read_dir(page).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.ends_with(".js") {
                let path = format!("/{name}");
                let (_, content_type, bytes) = super::ASSETS
                    .iter()
                    .find(|(route, _, _)| *route == path)
                    .unwrap_or_else(|| panic!("page/{name} is not served"));
                assert_eq!(*content_type, super::SCRIPT);
                assert_eq!(*bytes, std::fs::read(format!("{page}/{name}")).unwrap());
            }
        }
    }
}
