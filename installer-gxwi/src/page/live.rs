//! One browser connection: bounded input admission alongside live view updates.

use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use tokio::sync::watch;

use super::Served;
use crate::input::{Admission, BrowserInput};
use crate::setup::Heard;
use crate::view::View;

const BROWSER_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) async fn show(socket: WebSocket, served: Served) {
    Browser::new(socket, served).run().await;
}

struct Browser {
    socket: WebSocket,
    view: watch::Receiver<View>,
    tell: std::sync::mpsc::Sender<Heard>,
    admission: Admission,
    generation: u64,
    live: bool,
    write_timeout: Duration,
}

impl Browser {
    fn new(socket: WebSocket, served: Served) -> Self {
        let Served {
            view,
            tell,
            admission,
            ..
        } = served;
        let generation = view.borrow().generation;
        Self {
            socket,
            view,
            tell,
            admission,
            generation,
            live: true,
            write_timeout: BROWSER_WRITE_TIMEOUT,
        }
    }

    async fn run(mut self) {
        if !self.publish().await {
            return;
        }
        let mut carried = None;
        loop {
            let received = if let Some(message) = carried.take() {
                Some(Ok(message))
            } else {
                tokio::select! {
                    changed = self.view.changed(), if self.live => {
                        self.live = changed.is_ok();
                        if self.live && !self.publish().await { return; }
                        continue;
                    },
                    received = self.socket.recv() => received,
                }
            };
            match received {
                Some(Ok(Message::Text(text))) => {
                    let raw = text.to_string();
                    drop(text);
                    let Some(mut input) = self.admit(raw).await else {
                        return;
                    };
                    // A close cannot overtake earlier data on a websocket. Observe
                    // whatever is ready once before admission; do not wait/read ahead.
                    match poll_once(self.socket.recv()).await {
                        Poll::Ready(Some(Ok(Message::Close(_)))) => {
                            self.close(1000, "").await;
                            return;
                        }
                        Poll::Ready(Some(Err(_)) | None) => return,
                        Poll::Ready(Some(Ok(message))) => carried = Some(message),
                        Poll::Pending => {}
                    }
                    let feedback = input.feedback();
                    if self.tell.send(Heard::Asked(input)).is_err() {
                        return;
                    }
                    // Serialize this browser's actions through handling. Its next
                    // capped frame, if already observed above, is the only carryover.
                    if !self.handled(feedback).await {
                        return;
                    }
                }
                Some(Ok(Message::Close(_))) | None => return,
                Some(Err(_)) => {
                    self.close(1009, "Invalid or oversized browser request")
                        .await;
                    return;
                }
                Some(Ok(_)) => {}
            }
        }
    }

    async fn admit(&mut self, text: String) -> Option<BrowserInput> {
        let admission = self.admission.clone();
        let pending = admission.admit(text, self.generation);
        tokio::pin!(pending);
        loop {
            // Keep this same acquire future across updates, preserving semaphore FIFO.
            // Do not receive another frame while one waits outside the admitted queue.
            tokio::select! {
                input = &mut pending => {
                    // A simultaneously ready watch update must not relabel this input.
                    if self.view.borrow().generation != self.generation {
                        self.close(1012, "Conversation changed; reconnect without replaying input").await;
                        return None;
                    }
                    return input;
                },
                changed = self.view.changed(), if self.live => {
                    self.live = changed.is_ok();
                    if !self.live || !self.publish().await { return None; }
                },
            }
        }
    }

    async fn handled(&mut self, feedback: tokio::sync::oneshot::Receiver<()>) -> bool {
        tokio::pin!(feedback);
        loop {
            tokio::select! {
                rejected = &mut feedback => {
                    if rejected.is_ok() {
                        self.close(1009, "Request exceeds the daemon message limit").await;
                        return false;
                    }
                    return true;
                },
                changed = self.view.changed(), if self.live => {
                    self.live = changed.is_ok();
                    if !self.live || !self.publish().await { return false; }
                },
            }
        }
    }

    async fn publish(&mut self) -> bool {
        let (current, state) = {
            let view = self.view.borrow_and_update();
            (
                view.generation,
                serde_json::to_string(&*view).unwrap_or_default(),
            )
        };
        if current != self.generation {
            // Discard buffered old frames: seq numbers can repeat after reconnect.
            self.close(
                1012,
                "Conversation changed; reconnect without replaying input",
            )
            .await;
            return false;
        }
        bounded(
            self.socket.send(Message::Text(state.into())),
            self.write_timeout,
        )
        .await
    }

    async fn close(&mut self, code: u16, reason: &'static str) {
        // Best effort only; a non-reading peer must not retain an admission future.
        bounded(
            self.socket.send(Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            }))),
            self.write_timeout,
        )
        .await;
    }
}

async fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
    let mut future = pin!(future);
    poll_fn(|context| Poll::Ready(future.as_mut().poll(context))).await
}

async fn bounded<F, T, E>(future: F, timeout: Duration) -> bool
where
    F: Future<Output = Result<T, E>>,
{
    matches!(tokio::time::timeout(timeout, future).await, Ok(Ok(_)))
}

#[cfg(test)]
mod tests;
