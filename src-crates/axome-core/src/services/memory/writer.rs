use std::{
    collections::{hash_map::Entry as MapEntry, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};

use anyhow::{anyhow, ensure, Context, Result};
use axo::{
    store::{Entry, SeqEntry},
    AgentSpec, Axo, PromptRequest, PromptResult, RecoveryPolicy, Session, SessionOptions,
};
use axo_agent::{AgentMessage, RunOutcome};
use axo_ai::Message;
use axome_memory::{edit, index::LocatedObservation, links};
use axome_protocol::memory::{
    MemoryCaptureChangedEvent, MemoryCaptureRun, MemoryCaptureTrigger, MemoryFactOutcome,
};
use chrono::Utc;
use parking_lot::Mutex;
use regex::Regex;
use rusqlite::OptionalExtension;
use tokio::sync::{mpsc, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use super::{
    atomic_write, captures,
    captures::{ClosedLine, NewFact, Undo},
    checked_path, FolderMemory, MemoryService, SearchRequest,
};
use crate::{
    events::{deliver, HostEvent},
    services::{axo::message_text, db::DatabaseService},
};

const EXTRACT_PROMPT: &str = r#"Project: {project}
Date: {date}

Session excerpt:
<session>
{transcript}
</session>

List the facts from this excerpt that will still matter in future sessions: decisions and their reasons, facts about how the project works, and preferences or corrections the user stated.
Leave out task progress, things being tried, tool output, and anything only the assistant claimed without the user accepting it.

One fact per line, at most 12 lines, in exactly this form:
<page> :: [<category>] <fact>

<page> is "{project}/<Topic>" for facts about the project, or "me/<Topic>" for facts about the user that hold across projects. Topics are short noun phrases; reuse the same page for related facts.
<category> is one of: decision, fact, preference, reason, problem.
<fact> is one self-contained sentence. Link another page by writing [[{project}/<Topic>]] inside it when it clearly relates.

If nothing qualifies, answer exactly: NONE"#;

const CLASSIFY_PROMPT: &str = r#"New fact:
[{category}] {fact}

Existing notes:
{numbered}

Compare the new fact with the existing notes. Answer with exactly one of:
NEW          - it says something none of them say
SAME <n>     - note <n> already says this
REPLACES <n> - it updates or contradicts note <n>, which is now out of date
SKIP         - it is not worth keeping after all"#;

static CANDIDATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<page>[^:\n]{1,120}?)\s*::\s*\[(?P<category>decision|fact|preference|reason|problem)\]\s+(?P<fact>.{8,400})$").unwrap()
});
static CLASSIFICATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(NEW|SKIP|(SAME|REPLACES)\s+(\d+))\s*$").unwrap());

pub(crate) struct CaptureJob {
    pub folder: PathBuf,
    pub session_path: PathBuf,
    pub session_id: String,
    pub trigger: MemoryCaptureTrigger,
    /// The compaction entry this capture follows.
    pub anchor_entry_id: Option<String>,
    pub title: Option<String>,
}

struct QueuedJob {
    job: CaptureJob,
    axo: Arc<Axo>,
    agents: Vec<AgentSpec>,
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

    pub(crate) fn capture(
        self: &Arc<Self>,
        job: CaptureJob,
        axo: Arc<Axo>,
        agents: Vec<AgentSpec>,
    ) {
        let settings = self.settings(Some(&job.folder));
        if self.active(&job.folder).is_none()
            || job.folder == self.dirs.data.join("memory/writer")
            || !job.session_path.is_file()
            || !match job.trigger {
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
                if queue.sender.send(QueuedJob { job, axo, agents }).is_err() {
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
        let Some(fm) = self.active(&job.folder) else {
            return Ok(());
        };
        let (header, entries) = queued.axo.open_session_read(&job.session_path).await?;
        if header.spawn.is_some() {
            return Ok(());
        }
        let id = job.session_id.clone();
        let (chunks, last_seq) = self
            .db(move |db| {
                let cursor = db
                    .read()
                    .conn()
                    .query_row(
                        "SELECT seq FROM memory_cursors WHERE session_id = ?1",
                        [&id],
                        |row| row.get::<_, u64>(0),
                    )
                    .optional()?
                    .unwrap_or(0);
                Ok(transcript(&entries, cursor))
            })
            .await?;
        if chunks.is_empty() {
            if let Some(seq) = last_seq {
                save_cursor(self.db.clone(), job.session_id.clone(), seq).await?;
            }
            return Ok(());
        }
        let run = captures::NewRun {
            session_id: job.session_id.clone(),
            folder_path: job.folder.to_string_lossy().into_owned(),
            title: job.title.clone(),
            bank: fm.bank.name.clone(),
            trigger: job.trigger,
            anchor_entry_id: job.anchor_entry_id.clone(),
        };
        let capture_id = self.db(move |db| captures::start(db, run)).await?;
        self.emit_capture(capture_id).await;
        let result = self
            .capture_chunks(queued, &fm, chunks, capture_id, stop)
            .await;
        let error = result.as_ref().err();
        // The writer could not resolve a model for `@memory`.
        let no_model = error.is_some_and(|error| {
            matches!(
                error.downcast_ref::<axo::Error>(),
                Some(
                    axo::Error::UnknownModelRole(_)
                        | axo::Error::ModelNotFound(_)
                        | axo::Error::NoModel
                )
            )
        });
        let error_text = error.map(|error| {
            if no_model {
                "No model is set for @memory.".to_owned()
            } else {
                error.to_string()
            }
        });
        let finished = self
            .db(move |db| {
                captures::finish(db, capture_id, error_text.as_deref(), no_model)?;
                captures::get(db, capture_id)
            })
            .await;
        match finished {
            Ok(run) => {
                let notify = run.error.is_some()
                    || (job.trigger == MemoryCaptureTrigger::Close
                        && self.settings(Some(&job.folder)).capture.notify
                        && run
                            .facts
                            .iter()
                            .any(|fact| fact.outcome != MemoryFactOutcome::Known));
                self.emit(run, notify);
            }
            Err(error) => tracing::warn!(%error, "Could not finish the memory capture log"),
        }
        result
    }

    async fn capture_chunks(
        self: &Arc<Self>,
        queued: &QueuedJob,
        fm: &FolderMemory,
        chunks: Vec<Chunk>,
        capture_id: i64,
        stop: &CancellationToken,
    ) -> Result<()> {
        let job = &queued.job;
        let project = job
            .folder
            .file_name()
            .map_or_else(|| "root".into(), |name| name.to_string_lossy());
        for chunk in chunks.into_iter().take(20) {
            self.ensure_active(fm)?;
            let cwd = self.dirs.data.join("memory/writer");
            tokio::fs::create_dir_all(&cwd).await?;
            let session = queued
                .axo
                .create_session(SessionOptions {
                    cwd,
                    agent: Some("memory-writer".into()),
                    host: axo::Registrations {
                        definitions: axo::Definitions {
                            agents: queued.agents.clone(),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ask_enabled: false,
                    keywords_enabled: false,
                    recovery: RecoveryPolicy::MarkInterrupted,
                    ..Default::default()
                })
                .await?;
            let result = tokio::select! {
                biased;
                () = stop.cancelled() => Err(anyhow!("Axome is shutting down")),
                result = self.capture_chunk(fm, &session, &chunk.text, &project, job, capture_id) => result,
            };
            let journal = session.path().to_owned();
            session.shutdown().await;
            drop(session);
            // The excerpt must not outlive its capture: remove the journal and its session directory.
            for result in [
                tokio::fs::remove_file(&journal).await,
                tokio::fs::remove_dir_all(journal.with_extension("")).await,
            ] {
                if let Err(error) = result {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!(%error, "Could not remove a memory writer journal");
                    }
                }
            }
            result?;
            save_cursor(self.db.clone(), job.session_id.clone(), chunk.seq).await?;
            self.emit_capture(capture_id).await;
        }
        Ok(())
    }

    fn emit(&self, capture: MemoryCaptureRun, notify: bool) {
        deliver(
            &self.events,
            HostEvent::MemoryCaptureChanged(MemoryCaptureChangedEvent { capture, notify }),
        );
    }

    async fn emit_capture(&self, capture_id: i64) {
        match self.db(move |db| captures::get(db, capture_id)).await {
            Ok(capture) => self.emit(capture, false),
            Err(error) => tracing::warn!(%error, "Could not read a memory capture"),
        }
    }

    /// Runs `f` against the app database off the async runtime.
    async fn db<T: Send + 'static>(
        &self,
        f: impl FnOnce(&DatabaseService) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || f(&db)).await?
    }

    async fn capture_chunk(
        self: &Arc<Self>,
        fm: &FolderMemory,
        session: &Session,
        transcript: &str,
        project: &str,
        job: &CaptureJob,
        capture_id: i64,
    ) -> Result<()> {
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let prompt = EXTRACT_PROMPT
            .replace("{project}", project)
            .replace("{date}", &date)
            .replace("{transcript}", transcript);
        let reply = ask(session, prompt).await?;
        for found in reply
            .lines()
            .filter_map(|line| CANDIDATE.captures(line))
            .take(12)
        {
            let candidate = Candidate {
                page: found["page"].trim().into(),
                category: found["category"].into(),
                fact: found["fact"].into(),
            };
            let hits = self
                .search(
                    fm,
                    SearchRequest {
                        query: candidate.fact.clone(),
                        types: vec!["observation".into()],
                        limit: 5,
                        ..Default::default()
                    },
                )
                .await?;
            let observations = self
                .indexed(fm, vec![fm.bank.clone()], move |index| {
                    hits.iter()
                        .map(|hit| {
                            index
                                .observation(hit.hit.row_id)?
                                .context("Memory observation changed during capture")
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .await?;
            let choice = if observations.is_empty() {
                Choice::New
            } else {
                // Classify full observation text, not truncated/highlighted search snippets.
                let numbered = observations
                    .iter()
                    .enumerate()
                    .map(|(n, located)| {
                        format!(
                            "{}. ({}) [{}] {}",
                            n + 1,
                            located.entity.title,
                            located.observation.category,
                            located.observation.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let prompt = CLASSIFY_PROMPT
                    .replace("{category}", &candidate.category)
                    .replace("{fact}", &candidate.fact)
                    .replace("{numbered}", &numbered);
                classify(&ask(session, prompt).await?, observations.len())
            };
            let fact = match choice {
                Choice::Skip => continue,
                Choice::Same(n) => {
                    let known = observations
                        .into_iter()
                        .nth(n)
                        .context("Memory observation missing")?;
                    let path = fm.bank.dir.join(&known.entity.file_path);
                    NewFact {
                        outcome: MemoryFactOutcome::Known,
                        category: known.observation.category,
                        text: known.observation.content,
                        previous: None,
                        note: note_name(fm, &path),
                        path: path.to_string_lossy().into_owned(),
                        undo: None,
                    }
                }
                Choice::New | Choice::Replace(_) => {
                    let old = match choice {
                        Choice::Replace(n) => observations.into_iter().nth(n),
                        _ => None,
                    };
                    let previous = old.as_ref().map(|old| old.observation.content.clone());
                    let (category, text) = (candidate.category.clone(), candidate.fact.clone());
                    let (path, undo) = self
                        .apply_candidate(fm, candidate, old, date.clone(), job.session_id.clone())
                        .await?;
                    NewFact {
                        outcome: if previous.is_some() {
                            MemoryFactOutcome::Updated
                        } else {
                            MemoryFactOutcome::New
                        },
                        category,
                        text,
                        previous,
                        note: note_name(fm, &path),
                        path: path.to_string_lossy().into_owned(),
                        undo: Some(undo),
                    }
                }
            };
            self.db(move |db| captures::add_fact(db, capture_id, fact))
                .await?;
        }
        // Also sync chunks with no accepted candidates, without creating a bank directory.
        self.indexed(fm, vec![fm.bank.clone()], |_| Ok(())).await?;
        self.schedule_embedding();
        Ok(())
    }

    async fn apply_candidate(
        self: &Arc<Self>,
        fm: &FolderMemory,
        candidate: Candidate,
        old: Option<LocatedObservation>,
        date: String,
        session_id: String,
    ) -> Result<(PathBuf, Undo)> {
        let mut relative = PathBuf::new();
        for segment in candidate.page.split('/') {
            let segment = segment.trim();
            ensure!(
                !segment.is_empty() && segment != "." && segment != "..",
                "Invalid memory page"
            );
            relative.push(segment.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-"));
        }
        let title = relative
            .file_name()
            .context("Memory page has no title")?
            .to_string_lossy()
            .into_owned();
        let bank = fm.bank.clone();
        let page = candidate.page.clone();
        let path = self
            .indexed(fm, vec![fm.bank.clone()], move |index| {
                let bank_id = Self::bank_id(index, &bank)?;
                let path = match links::resolve(index, bank_id, &page)? {
                    Some(id) => bank.dir.join(
                        index
                            .entity(id)?
                            .context("Memory note disappeared")?
                            .file_path,
                    ),
                    None => bank.dir.join(format!("{}.md", relative.display())),
                };
                checked_path(&bank, &path)
            })
            .await?;
        let old_path = old
            .as_ref()
            .map(|hit| checked_path(&fm.bank, &fm.bank.dir.join(&hit.entity.file_path)))
            .transpose()?;
        let guards = lock_notes([Some(path.clone()), old_path.clone()].into_iter().flatten()).await;
        tokio::task::spawn_blocking(move || -> Result<(PathBuf, Undo)> {
            let _guards = guards;
            let mut text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    edit::new_note(&title, "note", &[], "## Observations\n")
                }
                Err(error) => return Err(error.into()),
            };
            let mut closed_note = None;
            let mut closed_line = None;
            let temporal = if let (Some(hit), Some(old_path)) = (old, old_path) {
                let old_text = if old_path == path {
                    &text
                } else {
                    &std::fs::read_to_string(&old_path)?
                };
                let line = hit.observation.line;
                // The note may have changed since the search: refuse to close a different observation.
                ensure!(
                    axome_memory::format::parse(old_text)
                        .observations
                        .iter()
                        .any(|o| o.line == line
                            && o.category == hit.observation.category
                            && o.content == hit.observation.content
                            && o.context == hit.observation.context),
                    "Memory observation changed during capture"
                );
                let closed = edit::close_observation(old_text, line, &date);
                if closed != *old_text {
                    // What a revert restores: the closed line exactly as it read before.
                    let before = old_text
                        .lines()
                        .nth(line - 1)
                        .unwrap_or_default()
                        .to_owned();
                    let after = closed.lines().nth(line - 1).unwrap_or_default().to_owned();
                    closed_line = Some(ClosedLine {
                        path: old_path.to_string_lossy().into_owned(),
                        before,
                        after,
                    });
                }
                if old_path == path {
                    text = closed;
                } else if closed != *old_text {
                    closed_note = Some((old_path, closed));
                }
                format!("@[{date},) ")
            } else {
                String::new()
            };
            let line = format!(
                "- [{}] {temporal}{} (session {session_id} {date})",
                candidate.category, candidate.fact
            );
            let appended = edit::append_observation(&text, &line);
            ensure!(appended != text, "Cannot edit malformed frontmatter");
            atomic_write(&path, &appended)?;
            // The replacement is on disk first: a failed close leaves two valid facts, never none.
            if let Some((old_path, closed)) = closed_note {
                atomic_write(&old_path, &closed)?;
            }
            Ok((
                path,
                Undo {
                    line,
                    closed: closed_line,
                },
            ))
        })
        .await?
    }

    /// Removes a captured fact's line and, for an updated fact, reopens the observation it closed.
    pub(crate) async fn revert_fact(self: &Arc<Self>, fact_id: i64) -> Result<MemoryCaptureRun> {
        let (fact, run) = self
            .db(move |db| {
                let fact = captures::fact_edit(db, fact_id)?;
                let run = captures::get(db, fact.capture_id)?;
                Ok((fact, run))
            })
            .await?;
        let undo = fact.undo.context("A known fact has nothing to revert")?;
        ensure!(!fact.reverted, "This fact was already reverted");
        let fm = self
            .active(Path::new(&run.folder_path))
            .context("Memory is disabled for this folder")?;
        let bank = fm
            .banks
            .iter()
            .find(|bank| bank.name == run.bank)
            .with_context(|| format!("Memory bank {} is no longer configured", run.bank))?
            .clone();
        let path = checked_path(&bank, Path::new(&fact.path))?;
        let previous = match undo.closed {
            Some(closed) => Some((
                checked_path(&bank, Path::new(&closed.path))?,
                closed.before,
                closed.after,
            )),
            None => None,
        };
        let guards = lock_notes(
            [Some(path.clone()), previous.as_ref().map(|p| p.0.clone())]
                .into_iter()
                .flatten(),
        )
        .await;
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let _guards = guards;
            const CHANGED: &str = "This note changed since the capture. Open the note to edit it.";
            let removed = edit::replace_line(&std::fs::read_to_string(&path)?, &undo.line, None)
                .context(CHANGED)?;
            // Both edits are checked before either is written.
            let (note, restored) = match previous {
                Some((previous_path, before, after)) if previous_path == path => (
                    edit::replace_line(&removed, &after, Some(&before)).context(CHANGED)?,
                    None,
                ),
                Some((previous_path, before, after)) => {
                    let text = std::fs::read_to_string(&previous_path)?;
                    let restored =
                        edit::replace_line(&text, &after, Some(&before)).context(CHANGED)?;
                    (removed, Some((previous_path, restored)))
                }
                None => (removed, None),
            };
            // The old fact is back first: a failed second write leaves both, never neither.
            if let Some((previous_path, restored)) = restored {
                atomic_write(&previous_path, &restored)?;
            }
            atomic_write(&path, &note)?;
            captures::mark_reverted(&db, fact_id)
        })
        .await??;
        self.indexed(&fm, vec![bank], |_| Ok(())).await?;
        self.schedule_embedding();
        let id = run.id;
        let run = self.db(move |db| captures::get(db, id)).await?;
        self.emit(run.clone(), false);
        Ok(run)
    }
}

struct Candidate {
    page: String,
    category: String,
    fact: String,
}
enum Choice {
    New,
    Same(usize),
    Replace(usize),
    Skip,
}

/// Takes the same process-wide per-file lock axo's `write` and `edit` hold, so the agent's
/// edits through memory:// and the writer's never interleave. Sorted: no lock-order cycles.
async fn lock_notes(paths: impl IntoIterator<Item = PathBuf>) -> Vec<OwnedMutexGuard<()>> {
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    paths.dedup();
    let mut guards = Vec::with_capacity(paths.len());
    for path in &paths {
        guards.push(axo::tools::lock_file(path).await);
    }
    guards
}

/// `Axome/Pause menu` for `<bank>/Axome/Pause menu.md`.
fn note_name(fm: &FolderMemory, path: &Path) -> String {
    let relative = path
        .strip_prefix(&fm.bank.dir)
        .unwrap_or(path)
        .to_string_lossy();
    relative.strip_suffix(".md").unwrap_or(&relative).to_owned()
}

fn classify(reply: &str, count: usize) -> Choice {
    let Some(line) = reply.lines().find(|line| !line.trim().is_empty()) else {
        return Choice::Skip;
    };
    let Some(captures) = CLASSIFICATION.captures(line) else {
        return Choice::Skip;
    };
    if &captures[1] == "NEW" {
        return Choice::New;
    }
    let n = captures
        .get(3)
        .and_then(|n| n.as_str().parse::<usize>().ok())
        .filter(|n| *n > 0 && *n <= count);
    match (captures.get(2).map(|kind| kind.as_str()), n) {
        (Some("REPLACES"), Some(n)) => Choice::Replace(n - 1),
        (Some("SAME"), Some(n)) => Choice::Same(n - 1),
        _ => Choice::Skip,
    }
}

async fn ask(session: &Session, text: String) -> Result<String> {
    let result = tokio::time::timeout(
        Duration::from_secs(600),
        session.prompt_with(PromptRequest {
            input: text.into(),
            request_id: None,
        }),
    )
    .await
    .context("Memory writer timed out")??;
    ensure!(
        matches!(
            result,
            PromptResult::Settled {
                outcome: RunOutcome::Done,
                ..
            }
        ),
        "Memory writer did not finish successfully"
    );
    let attach = session.attach(None).await;
    let message = attach
        .live
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            AgentMessage::Llm(message @ Message::Assistant(_)) => Some(message),
            _ => None,
        })
        .context("Memory writer returned no assistant reply")?;
    Ok(message_text(message).collect::<Vec<_>>().join("\n"))
}

struct Chunk {
    text: String,
    seq: u64,
}

fn transcript(entries: &[SeqEntry], cursor: u64) -> (Vec<Chunk>, Option<u64>) {
    let notices: HashSet<_> = entries
        .iter()
        .filter_map(|seq| match &seq.entry {
            Entry::Submission {
                submission_id,
                kind: axo::SubmissionKind::Quiet | axo::SubmissionKind::Mail,
                ..
            } => Some(submission_id),
            _ => None,
        })
        .collect();
    let projection = axo::store::project(entries, None);
    let mut chunks = Vec::new();
    let mut text = String::new();
    let mut length = 0;
    let mut seq = cursor;
    let mut has_user = false;
    let last_seq = entries
        .last()
        .map(|entry| entry.seq)
        .filter(|seq| *seq > cursor);
    for &index in projection.path() {
        let entry = &entries[index];
        if entry.seq <= cursor {
            continue;
        }
        let Entry::Message {
            message: AgentMessage::Llm(message),
            submission_id,
            ..
        } = &entry.entry
        else {
            continue;
        };
        let role = match message {
            Message::User(_)
                if !submission_id
                    .as_ref()
                    .is_some_and(|id| notices.contains(id)) =>
            {
                "user"
            }
            Message::Assistant(_) => "assistant",
            _ => continue,
        };
        let content = message_text(message).collect::<Vec<_>>().join("\n");
        if content.trim().is_empty() {
            continue;
        }
        has_user |= role == "user";
        let rendered: String = format!("[{role}]\n{content}")
            .chars()
            .take(12_000)
            .collect();
        let count = rendered.chars().count();
        if !text.is_empty() && length + 2 + count > 12_000 {
            chunks.push(Chunk {
                text: std::mem::take(&mut text),
                seq,
            });
            length = 0;
        }
        if !text.is_empty() {
            text.push_str("\n\n");
            length += 2;
        }
        text.push_str(&rendered);
        length += count;
        seq = entry.seq;
    }
    if !text.is_empty() {
        chunks.push(Chunk {
            text,
            seq: last_seq.unwrap_or(seq),
        });
    }
    if !has_user {
        chunks.clear();
    }
    (chunks, last_seq)
}

pub(crate) async fn save_cursor(db: Arc<DatabaseService>, id: String, seq: u64) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        db.write().conn().execute("INSERT INTO memory_cursors (session_id, seq, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(session_id) DO UPDATE SET seq = excluded.seq, updated_at = excluded.updated_at", rusqlite::params![id, seq, Utc::now().to_rfc3339()])?;
        Ok(())
    }).await?
}
