//! The installer as an [`msip_serve::Flow`]: state, plus the mapping
//! from a decision in [`crate::flow`] to something the server can do.

use std::sync::{Arc, Mutex};

use msip::daemon::{TurnSpec, ValidAnswer};
use msip::element::types;
use msip_serve::{Flow, Step};
use serde_json::{Map, Value, json};

use crate::executor::{Executor, JobKind};
use crate::flow::{self, Advance, FlowState};

/// Element types the install flow needs a surface to render.
const NEEDED: &[&str] = &[
    types::TEXT,
    types::SELECT,
    types::PROGRESS,
    types::LOG,
    types::ACTION,
];

pub struct Install {
    executor: Arc<dyn Executor>,
    state: FlowState,
    /// Set once a job starts, so the completion message can name what ran.
    kind: Option<JobKind>,
    /// What the job had to say for itself, if anything. Filled in by the
    /// job closure and read by `completion_message`, which the server
    /// calls on this same flow once the job has returned -- the shared
    /// slot is how a sentence gets from one to the other.
    found: Arc<Mutex<Option<String>>>,
}

impl Install {
    pub fn new(executor: Arc<dyn Executor>) -> Install {
        Install {
            executor,
            state: FlowState::Mode,
            kind: None,
            found: Arc::new(Mutex::new(None)),
        }
    }
}

impl Flow for Install {
    fn needs(&self) -> &'static [&'static str] {
        NEEDED
    }

    fn open(&mut self) -> TurnSpec {
        flow::mode_page()
    }

    fn advance(&mut self, answer: &ValidAnswer) -> Step {
        match flow::advance(&self.state, answer, self.executor.as_ref()) {
            Some(Advance::Page(next, spec)) => {
                self.state = next;
                Step::Page(spec)
            }
            Some(Advance::Rescan) => {
                let disks = self.executor.probe_disks();
                let mut patch = Map::new();
                patch.insert("ref".into(), json!("disk.target"));
                patch.insert("rows".into(), flow::disk_rows(&disks));
                Step::Patch(vec![patch])
            }
            Some(Advance::Begin { kind, target }) => {
                self.state = FlowState::Running;
                self.kind = Some(kind);
                let page = flow::progress_page(kind, self.executor.as_ref());
                let executor = Arc::clone(&self.executor);
                let found = Arc::clone(&self.found);
                Step::Work {
                    page,
                    job: Box::new(move |progress| {
                        let said = executor.run(kind, &target, progress)?;
                        *found.lock().unwrap_or_else(|e| e.into_inner()) = said;
                        Ok(())
                    }),
                }
            }
            None => Step::Refuse("answer does not advance the flow"),
        }
    }

    fn completion_message(&self) -> Option<String> {
        let mut message: String = match self.kind {
            Some(JobKind::Install) => "Installation complete. Reboot to start Peios.".into(),
            Some(JobKind::Upgrade) => "Upgrade complete.".into(),
            _ => "Repair complete.".into(),
        };
        if let Some(said) = self
            .found
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_deref()
        {
            message.push(' ');
            message.push_str(said);
        }
        Some(message)
    }
}

// Kept so `Value` stays used when the patch shape changes.
#[allow(unused)]
fn _value(_: Value) {}
