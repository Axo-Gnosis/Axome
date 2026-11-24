use std::{
    collections::{hash_map::Entry as MapEntry, HashMap, HashSet},
    path::PathBuf,
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
use axome_protocol::memory::MemoryCaptureStatus;
use chrono::Utc;
use parking_lot::Mutex;
use regex::Regex;
use rusqlite::OptionalExtension;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{atomic_write, checked_path, FolderMemory, MemoryService, SearchRequest};
use crate::services::{axo::message_text, db::DatabaseService};

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
}

struct QueuedJob {
    job: CaptureJob,
    axo: Arc<Axo>,
    db: Arc<DatabaseService>,
    agents: Vec<AgentSpec>,
}

pub(super) struct CaptureQueue {
    sender: mpsc::UnboundedSender<QueuedJob>,
    /// Sessions queued or running; `true` when another trigger arrived meanwhile.
    pending: Arc<Mutex<HashMap<String, bool>>>,
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
        compaction: bool,
        axo: Arc<Axo>,
        db: Arc<DatabaseService>,
        agents: Vec<AgentSpec>,
    ) {
        let settings = self.settings(Some(&job.folder));
        if self.active(&job.folder).is_none()
            || job.folder == self.dirs.data.join("memory/writer")
            || !job.session_path.is_file()
            || !(if compaction {
                settings.capture.compaction
            } else {
                settings.capture.close
            })
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
                    let queued = tokio::select! {
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
                            let mut status = service.last_capture.lock();
                            let (added, replaced) = status.as_ref().filter(|s| s.session_id == queued.job.session_id)
                                .map(|s| (s.added, s.replaced)).unwrap_or_default();
                            *status = Some(MemoryCaptureStatus { at: Utc::now().to_rfc3339(), session_id: queued.job.session_id.clone(), added, replaced, error: Some(error.to_string()) });
                        }
                        let mut pending = pending_worker.lock();
                        match pending.get_mut(&queued.job.session_id) {
                            Some(again) if *again => *again = false,
                            _ => {
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
                running.insert(true);
            }
            MapEntry::Vacant(slot) => {
                let id = slot.key().clone();
                slot.insert(false);
                if queue
                    .sender
                    .send(QueuedJob {
                        job,
                        axo,
                        db,
                        agents,
                    })
                    .is_err()
                {
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
        let db = queued.db.clone();
        let id = job.session_id.clone();
        let (chunks, last_seq) = tokio::task::spawn_blocking(move || -> Result<_> {
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
        .await??;
        if chunks.is_empty() {
            if let Some(seq) = last_seq {
                save_cursor(queued.db.clone(), job.session_id.clone(), seq).await?;
            }
            return Ok(());
        }
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let project = job
            .folder
            .file_name()
            .map_or_else(|| "root".into(), |name| name.to_string_lossy());
        let mut added = 0;
        let mut replaced = 0;
        for chunk in chunks.into_iter().take(20) {
            self.ensure_active(&fm)?;
            let cwd = self.dirs.data.join("memory/writer");
            tokio::fs::create_dir_all(&cwd).await?;
            let session = queued
                .axo
                .create_session(SessionOptions {
                    cwd,
                    agent: Some("memory-writer".into()),
                    definitions: axo::Definitions {
                        agents: queued.agents.clone(),
                        ..Default::default()
                    },
                    ask_enabled: false,
                    keywords_enabled: false,
                    tools: vec![],
                    schemes: vec![],
                    recovery: RecoveryPolicy::MarkInterrupted,
                    ..Default::default()
                })
                .await
                .map_err(|error| match error {
                    axo::Error::UnknownModelRole(_)
                    | axo::Error::ModelNotFound(_)
                    | axo::Error::NoModel => anyhow!("No model is configured for @memory."),
                    error => anyhow!(error),
                })?;
            let result = tokio::select! {
                biased;
                () = stop.cancelled() => Err(anyhow!("Axome is shutting down")),
                result = self.capture_chunk(&fm, &session, &chunk.text, &project, &date, &job.session_id) => result,
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
            let counts = result?;
            added += counts.0;
            replaced += counts.1;
            save_cursor(queued.db.clone(), job.session_id.clone(), chunk.seq).await?;
            *self.last_capture.lock() = Some(MemoryCaptureStatus {
                at: Utc::now().to_rfc3339(),
                session_id: job.session_id.clone(),
                added,
                replaced,
                error: None,
            });
        }
        Ok(())
    }

    async fn capture_chunk(
        self: &Arc<Self>,
        fm: &FolderMemory,
        session: &Session,
        transcript: &str,
        project: &str,
        date: &str,
        session_id: &str,
    ) -> Result<(u32, u32)> {
        let prompt = EXTRACT_PROMPT
            .replace("{project}", project)
            .replace("{date}", date)
            .replace("{transcript}", transcript);
        let reply = ask(session, prompt).await?;
        let mut added = 0;
        let mut replaced = 0;
        for captures in reply
            .lines()
            .filter_map(|line| CANDIDATE.captures(line))
            .take(12)
        {
            let candidate = Candidate {
                page: captures["page"].trim().into(),
                category: captures["category"].into(),
                fact: captures["fact"].into(),
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
            let old = match choice {
                Choice::New => None,
                Choice::Replace(n) => observations.into_iter().nth(n),
                Choice::Skip => continue,
            };
            let replacing = old.is_some();
            self.apply_candidate(fm, candidate, old, date.to_owned(), session_id.to_owned())
                .await?;
            if replacing {
                replaced += 1;
            } else {
                added += 1;
            }
        }
        // Also sync chunks with no accepted candidates, without creating a bank directory.
        self.indexed(fm, vec![fm.bank.clone()], |_| Ok(())).await?;
        self.schedule_embedding();
        Ok((added, replaced))
    }

    async fn apply_candidate(
        self: &Arc<Self>,
        fm: &FolderMemory,
        candidate: Candidate,
        old: Option<LocatedObservation>,
        date: String,
        session_id: String,
    ) -> Result<()> {
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
        // The same process-wide per-file lock axo's `write` and `edit` hold, so the agent's
        // edits through memory:// and these never interleave. Sorted: no lock-order cycles.
        let mut paths: Vec<_> = [Some(path.clone()), old_path.clone()]
            .into_iter()
            .flatten()
            .collect();
        paths.sort();
        paths.dedup();
        let mut guards = Vec::with_capacity(paths.len());
        for path in &paths {
            guards.push(axo::tools::lock_file(path).await);
        }
        tokio::task::spawn_blocking(move || -> Result<()> {
            let _guards = guards;
            let mut text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    edit::new_note(&title, "note", &[], "## Observations\n")
                }
                Err(error) => return Err(error.into()),
            };
            let mut closed_note = None;
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
            Ok(())
        })
        .await?
    }
}

struct Candidate {
    page: String,
    category: String,
    fact: String,
}
enum Choice {
    New,
    Replace(usize),
    Skip,
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
    if captures
        .get(2)
        .is_some_and(|kind| kind.as_str() == "REPLACES")
    {
        if let Some(n) = captures
            .get(3)
            .and_then(|n| n.as_str().parse::<usize>().ok())
            .filter(|n| *n > 0 && *n <= count)
        {
            return Choice::Replace(n - 1);
        }
    }
    Choice::Skip
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
