//! Admission accounting for browser actions, separate from daemon events.

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use crate::setup::Asked;

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_ADMITTED_BYTES: usize = 4 * 1024 * 1024;
// Charge small requests for queue nodes and their metadata as well as payload.
const ENTRY_RESERVE_BYTES: usize = 128;

#[derive(Clone)]
pub struct Admission(Arc<Semaphore>);

impl Default for Admission {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(MAX_ADMITTED_BYTES)))
    }
}

impl Admission {
    /// Await space without blocking a runtime thread. Dropping this future
    /// cancels admission; the caller may retain one capped frame while waiting.
    pub async fn admit(&self, text: String, generation: u64) -> Option<BrowserInput> {
        if text.len() > MAX_MESSAGE_BYTES {
            return None;
        }
        let charge = u32::try_from(text.len() + ENTRY_RESERVE_BYTES).ok()?;
        let reservation = self.0.clone().acquire_many_owned(charge).await.ok()?;
        Some(BrowserInput {
            text,
            generation,
            _reservation: reservation,
            rejection: None,
        })
    }
}

pub struct BrowserInput {
    text: String,
    generation: u64,
    _reservation: OwnedSemaphorePermit,
    rejection: Option<oneshot::Sender<()>>,
}

impl BrowserInput {
    pub fn feedback(&mut self) -> oneshot::Receiver<()> {
        let (tell, receive) = oneshot::channel();
        self.rejection = Some(tell);
        receive
    }

    pub fn reject_oversize(&mut self) {
        if let Some(tell) = self.rejection.take() {
            // The requesting browser may have disconnected already.
            let _ = tell.send(());
        }
    }

    /// Parse only when handled, retaining the reservation through the caller's
    /// action. A replacement conversation must never inherit an old request.
    pub fn decode(&self, generation: u64) -> Option<Asked> {
        (self.generation == generation)
            .then(|| serde_json::from_str(&self.text).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests;
