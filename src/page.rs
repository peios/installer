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
use tokio::sync::watch;

use crate::view::View;

const HTML: &str = "text/html; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const SCRIPT: &str = "text/javascript; charset=utf-8";
const FONT: &str = "font/woff2";

/// Only what is served from here runs, styles or is fetched, and the page
/// talks to nothing but this program.
const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; \
    img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

pub fn routes(view: watch::Receiver<View>) -> Router {
    let part = |content_type, bytes: &'static [u8]| get(move || async move { file(content_type, bytes) });
    Router::new()
        .route("/", part(HTML, include_bytes!("../page/index.html")))
        .route("/style.css", part(CSS, include_bytes!("../page/style.css")))
        .route("/app.js", part(SCRIPT, include_bytes!("../page/app.js")))
        .route("/field.js", part(SCRIPT, include_bytes!("../page/field.js")))
        .route("/intro.js", part(SCRIPT, include_bytes!("../page/intro.js")))
        .route("/fonts/manrope.woff2", part(FONT, include_bytes!("../page/fonts/manrope.woff2")))
        .route("/fonts/schibsted-grotesk.woff2", part(FONT, include_bytes!("../page/fonts/schibsted-grotesk.woff2")))
        // The state, live. `any`, because a websocket arrives as a GET over
        // HTTP/1.1 and as a CONNECT over HTTP/2, which is how GXWI sends it.
        .route("/live", any(live))
        .fallback(elsewhere)
        .with_state(view)
}

/// Everyone who reaches the machine is sent here, wherever they were going:
/// an address left over from a desktop means nothing to an installer.
async fn elsewhere(method: Method) -> Response {
    if method == Method::GET { Redirect::to("/").into_response() } else { StatusCode::NOT_FOUND.into_response() }
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

async fn live(State(view): State<watch::Receiver<View>>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| show(socket, view))
}

/// Sends one browser the state as it stands, and again each time it changes,
/// until the browser goes. What the browser sends is not listened to yet.
async fn show(mut socket: WebSocket, mut view: watch::Receiver<View>) {
    loop {
        let state = serde_json::to_string(&*view.borrow_and_update()).unwrap_or_default();
        if socket.send(Message::Text(state.into())).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                changed = view.changed() => match changed {
                    Ok(()) => break,
                    Err(_) => return,
                },
                heard = socket.recv() => match heard {
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                    Some(Ok(_)) => {}
                },
            }
        }
    }
}
