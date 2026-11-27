//! When Axome captures: a per-session queue of compaction and close triggers, run one
//! at a time through `axo_memory::Memory::capture`, and the events each run emits.

use std::{
    collections::{hash_map::Entry as MapEntry, HashMap},
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use anyhow::Result;
use axo::{AgentSpec, Axo};
use axo_memory::{CaptureRequest, CaptureRun, FactOutcome};
use axome_protocol::memory::{
    MemoryCaptureChangedEvent, MemoryCaptureRun, MemoryCaptureTrigger, MemoryCapturedFact,
    MemoryFactOutcome,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::MemoryService;
use crate::events::{deliver, HostEvent};

/// The model role the writer runs on.
const MEMORY_ROLE: &str = "@memory";

/// The capture agent. Not among the bundled agents, so chats never list it as a persona.
static WRITER: LazyLock<AgentSpec> = LazyLock::new(|| {
    let mut agent = axo_memory::writer_agent();
    agent.model = Some(MEMORY_ROLE.into());
    agent
});

pub(crate) struct CaptureJob {
    pub folder: PathBuf,
    pub session_path: PathBuf,
    pub session_id: String,
    pub host: CaptureHost,
}

/// What Axome stores with each run as `CaptureRun::host`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureHost {
    pub title: Option<String>,
    pub trigger: MemoryCaptureTrigger,
    /// The compaction entry this capture follows.
    pub anchor_entry_id: Option<String>,
}

struct QueuedJob {
    job: CaptureJob,
    axo: Arc<Axo>,
}

pub(super) struct CaptureQueue {
    sender: mpsc::UnboundedSender<QueuedJob>,
    /// Sessions queued or running, with the latest trigger that arrived meanwhile.
    pending: Arc<Mutex<HashMap<String, Option<CaptureJob>>>>,
    stop: CancellationToken,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl MemoryService {
    #[cfg(test)]
    pub(crate) fn capture_pending(&self) -> bool {
        self.capture_queue
            .get()
            .is_some_and(|queue| !queue.pending.lock().is_empty())
    }

    pub(crate) fn capture(self: &Arc<Self>, job: CaptureJob, axo: Arc<Axo>) {
        let settings = self.settings(Some(&job.folder));
        if self.banks(&settings, &job.folder).is_none()
            || !job.session_path.is_file()
            || !match job.host.trigger {
                MemoryCaptureTrigger::Compaction => settings.capture.compaction,
                MemoryCaptureTrigger::Close => settings.capture.close,
            }
        {
            return;
        }
        let Some(runtime) = self.runtime() else {
            return;
        };
        let queue = self.capture_queue.get_or_init(|| {
            let (sender, mut receiver) = mpsc::unbounded_channel::<QueuedJob>();
            let pending = Arc::new(Mutex::new(HashMap::new()));
            let pending_worker = pending.clone();
            let stop = CancellationToken::new();
            let worker_stop = stop.clone();
            let weak = Arc::downgrade(self);
            let worker = runtime.spawn(async move {
                let stop = worker_stop;
                loop {
                    let mut queued = tokio::select! {
                        biased;
                        () = stop.cancelled() => break,
                        job = receiver.recv() => match job {
                            Some(job) => job,
                            None => break,
                        },
                    };
                    let Some(service) = weak.upgrade() else { break };
                    loop {
                        let result = service.capture_job(&queued, &stop).await;
                        if stop.is_cancelled() {
                            return;
                        }
                        if let Err(error) = result {
                            tracing::warn!(%error, session_id = queued.job.session_id, "Memory capture failed");
                        }
                        let mut pending = pending_worker.lock();
                        match pending.get_mut(&queued.job.session_id).and_then(Option::take) {
                            Some(next) => queued.job = next,
                            None => {
                                pending.remove(&queued.job.session_id);
                                break;
                            }
                        }
                    }
                }
            });
            CaptureQueue { sender, pending, stop, worker: Mutex::new(Some(worker)) }
        });
        if queue.stop.is_cancelled() {
            return;
        }
        let mut pending = queue.pending.lock();
        match pending.entry(job.session_id.clone()) {
            MapEntry::Occupied(mut running) => {
                running.insert(Some(job));
            }
            MapEntry::Vacant(slot) => {
                let id = slot.key().clone();
                slot.insert(None);
                if queue.sender.send(QueuedJob { job, axo }).is_err() {
                    pending.remove(&id);
                }
            }
        }
    }

    /// Stops capture and waits for the running job to release its writer session.
    pub(crate) async fn stop_capture(&self) {
        let Some(queue) = self.capture_queue.get() else {
            return;
        };
        queue.stop.cancel();
        let worker = queue.worker.lock().take();
        if let Some(worker) = worker {
            let _ = worker.await;
        }
    }

    async fn capture_job(
        self: &Arc<Self>,
        queued: &QueuedJob,
        stop: &CancellationToken,
    ) -> Result<()> {
        let job = &queued.job;
        let Some(banks) = self.active(&job.folder) else {
            return Ok(());
        };
        let service = Arc::clone(self);
        let run = self
            .store
            .capture(
                &queued.axo,
                CaptureRequest {
                    session: job.session_path.clone(),
                    banks,
                    agent: WRITER.clone(),
                    host: serde_json::to_value(&job.host)?,
                    progress: Some(Box::new(move |run| service.emit(to_protocol(run), false))),
                    cancel: stop.clone(),
                },
            )
            .await?;
        if stop.is_cancelled() {
            return Ok(());
        }
        if let Some(run) = run {
            let run = to_protocol(run);
            let notify = run.error.is_some()
                || (job.host.trigger == MemoryCaptureTrigger::Close
                    && self.settings(Some(&job.folder)).capture.notify
                    && run
                        .facts
                        .iter()
                        .any(|fact| fact.outcome != MemoryFactOutcome::Known));
            self.emit(run, notify);
        }
        Ok(())
    }

    pub(super) fn emit(&self, capture: MemoryCaptureRun, notify: bool) {
        deliver(
            &self.events,
            HostEvent::MemoryCaptureChanged(MemoryCaptureChangedEvent { capture, notify }),
        );
    }
}

pub(super) fn to_protocol(run: CaptureRun) -> MemoryCaptureRun {
    // ponytail: a malformed `host` loses all three fields; only `CaptureHost` and the B9
    // migration script write it.
    let host = serde_json::from_value(run.host).unwrap_or(CaptureHost {
        title: None,
        trigger: MemoryCaptureTrigger::Close,
        anchor_entry_id: None,
    });
    MemoryCaptureRun {
        title: host.title,
        anchor_entry_id: host.anchor_entry_id,
        trigger: host.trigger,
        error: if run.no_model {
            Some(format!("No model is set for {MEMORY_ROLE}."))
        } else {
            run.error
        },
        id: run.id,
        session_id: run.session_id,
        folder_path: run.cwd,
        bank: run.bank,
        started_at: run.started_at,
        ended_at: run.ended_at,
        no_model: run.no_model,
        facts: run
            .facts
            .into_iter()
            .map(|fact| MemoryCapturedFact {
                id: fact.id,
                outcome: match fact.outcome {
                    FactOutcome::New => MemoryFactOutcome::New,
                    FactOutcome::Updated => MemoryFactOutcome::Updated,
                    FactOutcome::Known => MemoryFactOutcome::Known,
                },
                category: fact.category,
                text: fact.text,
                previous: fact.previous,
                note: fact.note,
                path: fact.path,
                reverted: fact.reverted,
            })
            .collect(),
    }
}
