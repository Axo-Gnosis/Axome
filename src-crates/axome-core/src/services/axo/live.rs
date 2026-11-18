use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, OnceLock, Weak,
    },
};

use axo::store::{Entry, EntryId, SeqEntry, SubmissionKind};
use axo::{LiveSnapshot, MaintenanceKind, PromptRequest, SessionEvent};
use axo_agent::{AgentEvent, AgentMessage};
use axo_ai::{AssistantBlock, AssistantEvent, Message};
use axome_protocol::chat::*;
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot, watch, Notify};

use super::{axo_error, convert, AxoService};
use crate::{errors::ApiError, events::EventSink};

pub(super) const MAX_JOB_SCROLLBACK_BYTES: u64 = 128 * 1024;

pub(super) fn same_sink(a: &EventSink<ChatEvent>, b: &EventSink<ChatEvent>) -> bool {
    std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
}

pub(super) fn utf8_tail(bytes: &[u8], truncated: bool) -> String {
    let mut start = 0;
    if truncated {
        while start < bytes.len() && start < 3 && (bytes[start] & 0b1100_0000) == 0b1000_0000 {
            start += 1;
        }
    }
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

// Keep an unfinished UTF-8 suffix until the next flush; never join different streams.
pub(super) fn decode_output(
    chunks: Vec<axo::OutputChunk>,
    pending: &mut Option<axo::OutputChunk>,
    truncated: bool,
    done: bool,
) -> Vec<ChatOutputChunk> {
    let mut merged: Vec<axo::OutputChunk> = Vec::new();
    for chunk in pending.take().into_iter().chain(chunks) {
        if let Some(last) = merged.last_mut().filter(|last| last.stream == chunk.stream) {
            last.bytes.extend(chunk.bytes);
        } else {
            merged.push(chunk);
        }
    }
    if !done {
        if let Some(last) = merged.last_mut() {
            let mut start = 0;
            while start < last.bytes.len() {
                match std::str::from_utf8(&last.bytes[start..]) {
                    Ok(_) => break,
                    Err(error) => {
                        start += error.valid_up_to();
                        if let Some(len) = error.error_len() {
                            start += len;
                        } else {
                            *pending = Some(axo::OutputChunk {
                                stream: last.stream,
                                bytes: last.bytes.split_off(start),
                            });
                            break;
                        }
                    }
                }
            }
        }
    }
    merged
        .into_iter()
        .enumerate()
        .filter_map(|(index, chunk)| {
            let text = utf8_tail(&chunk.bytes, truncated && index == 0);
            (!text.is_empty()).then_some(ChatOutputChunk {
                stream: match chunk.stream {
                    axo::OutputStream::Stdout => ChatOutputStream::Stdout,
                    axo::OutputStream::Stderr => ChatOutputStream::Stderr,
                    axo::OutputStream::Merged => ChatOutputStream::Merged,
                },
                text,
            })
        })
        .collect()
}

pub(super) async fn read_tail(
    output: &axo::CommandOutput,
) -> Result<
    (
        axo::OutputProgress,
        bool,
        Vec<ChatOutputChunk>,
        Option<axo::OutputChunk>,
    ),
    ApiError,
> {
    let progress = *output.progress().borrow();
    let truncated = progress.bytes > MAX_JOB_SCROLLBACK_BYTES;
    let start = progress.bytes.saturating_sub(MAX_JOB_SCROLLBACK_BYTES);
    let raw = output
        .read_chunks(start..progress.bytes)
        .await
        .map_err(|error| ApiError::Io(error.to_string()))?;
    let mut pending = None;
    let chunks = decode_output(raw, &mut pending, truncated, progress.done);
    Ok((progress, truncated, chunks, pending))
}

pub(crate) struct LiveSession {
    pub id: String,
    pub folder: PathBuf,
    pub root_id: Option<String>,
    pub session: Arc<axo::Session>,
    pub engine: Arc<super::engine::Engine>,
    service: Weak<AxoService>,
    state: Mutex<RelayState>,
    relay: OnceLock<tokio::task::AbortHandle>,
    tasks: tokio_util::task::TaskTracker,
    maintenance_starts: watch::Sender<u64>,
    pins: AtomicUsize,
    /// Woken whenever the relay may have gone idle, and on retirement.
    idle: Notify,
    /// Set when the relay stops; no idle notification follows.
    retired: AtomicBool,
}

struct RelayState {
    entries: Vec<SeqEntry>,
    path: Vec<usize>,
    streaming: Option<Vec<ChatBlock>>,
    live: ChatLive,
    commands: Vec<ChatCommand>,
    toolbox: ChatToolbox,
    session_tools: Vec<ChatSessionTool>,
    is_child: bool,
    subscribers: Vec<(String, EventSink<ChatEvent>)>,
    waiters: HashMap<String, oneshot::Sender<()>>,
    submitted: bool,
    context: Option<axo::ContextUsage>,
    tree_usage: axo_ai::Usage,
    has_children: bool,
    job_watchers: HashMap<String, tokio::task::AbortHandle>,
    last_summary: Option<ChatLiveSummary>,
}

fn maintenance(kind: MaintenanceKind) -> ChatMaintenance {
    match kind {
        MaintenanceKind::Compact => ChatMaintenance::Compact,
        MaintenanceKind::Shake => ChatMaintenance::Shake,
        MaintenanceKind::BranchSummary => ChatMaintenance::BranchSummary,
    }
}

fn chat_review(
    modes: &[Arc<axo::ModeSpec>],
    call_id: String,
    mode: String,
    result: serde_json::Value,
) -> ChatReview {
    let plan = modes
        .iter()
        .find(|spec| spec.name == mode)
        .is_some_and(|spec| convert::mode_is_plan(spec));
    ChatReview {
        call_id,
        mode,
        result,
        plan,
    }
}

impl RelayState {
    fn new(
        entries: Vec<SeqEntry>,
        snapshot: LiveSnapshot,
        commands: Vec<ChatCommand>,
        toolbox: ChatToolbox,
        is_child: bool,
        engine: &super::engine::Engine,
        session: &axo::Session,
    ) -> Self {
        let session_tools = convert::session_tools(session);
        let path = convert::branch(&entries);
        let (model, thinking) = convert::selection(&entries, &path);
        let context_window = engine.context_window(model.as_deref());
        let context = snapshot.context;
        let has_children = entries
            .iter()
            .any(|s| matches!(s.entry, Entry::ChildStart { .. }))
            || snapshot
                .jobs
                .iter()
                .any(|j| matches!(j.kind, axo::JobKind::Child { .. }));
        let stats = convert::stats(
            &entries,
            &path,
            context_window,
            context,
            has_children.then_some(&snapshot.tree_usage),
        );
        let queue = convert::queue_at_attach(&snapshot);
        let jobs = snapshot
            .jobs
            .iter()
            .map(|job| convert::job(job, false))
            .collect();
        let live = ChatLive {
            title: convert::title(&entries),
            running: snapshot.running,
            maintenance: snapshot.maintenance.map(maintenance),
            model,
            thinking: convert::thinking_to_wire(thinking),
            tools: snapshot
                .pending_tool_calls
                .iter()
                .map(|call| ChatToolRun {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: serde_json::Value::Object(call.arguments.clone()),
                    output: Vec::new(),
                    details: None,
                })
                .collect(),
            queue,
            questions: snapshot
                .pending_questions
                .into_iter()
                .map(|(call_id, questions)| ChatQuestions {
                    call_id,
                    questions: questions.iter().map(convert::question).collect(),
                })
                .collect(),
            retry: None,
            stats,
            auto_paused: snapshot.auto_paused,
            jobs,
            agent: snapshot.agent,
            mode: snapshot.mode,
            effective_mode: snapshot.effective_mode,
            reviews: snapshot
                .pending_results
                .into_iter()
                .map(|result| {
                    chat_review(session.modes(), result.call_id, result.mode, result.result)
                })
                .collect(),
        };
        let submitted = entries
            .iter()
            .any(|seq| matches!(seq.entry, Entry::Submission { .. }));
        Self {
            entries,
            path,
            streaming: snapshot
                .streaming
                .as_ref()
                .map(|m| m.content.iter().map(convert::block).collect()),
            live,
            commands,
            toolbox,
            session_tools,
            is_child,
            subscribers: Vec::new(),
            waiters: HashMap::new(),
            submitted,
            context,
            tree_usage: snapshot.tree_usage,
            has_children,
            job_watchers: HashMap::new(),
            last_summary: None,
        }
    }

    fn refresh_stats(&mut self, fallback_window: u64) {
        self.live.stats = convert::stats(
            &self.entries,
            &self.path,
            fallback_window,
            self.context,
            self.has_children.then_some(&self.tree_usage),
        );
    }

    fn refresh_tools(&mut self, session: &axo::Session) {
        let tools = convert::session_tools(session);
        if tools != self.session_tools {
            self.session_tools = tools.clone();
            self.broadcast(ChatEvent::SessionTools { tools });
        }
    }

    fn snapshot(&self, id: &str) -> ChatState {
        ChatState {
            session_id: id.to_owned(),
            entries: convert::path_entries(&self.entries, &self.path),
            streaming: self.streaming.clone(),
            live: self.live.clone(),
            commands: self.commands.clone(),
            session_tools: self.session_tools.clone(),
            toolbox: self.toolbox.clone(),
            read_only: false,
            is_child: self.is_child,
        }
    }

    fn busy(&self) -> bool {
        self.live.running
            || self.live.maintenance.is_some()
            || !self.live.questions.is_empty()
            || !self.live.reviews.is_empty()
            || self.live.jobs.iter().any(|job| job.status.is_active())
    }

    pub(super) fn summary(&self, session_id: &str) -> ChatLiveSummary {
        let running = self.live.running || self.live.maintenance.is_some();
        let waiting = !self.live.questions.is_empty() || !self.live.reviews.is_empty();
        let auto_paused = self.live.auto_paused;
        let mut active_commands = 0;
        let mut active_children = 0;
        for job in self.live.jobs.iter().filter(|job| job.status.is_active()) {
            match job.kind {
                ChatJobKind::Command { .. } => active_commands += 1,
                ChatJobKind::Child { .. } => active_children += 1,
            }
        }
        ChatLiveSummary {
            session_id: session_id.to_owned(),
            running,
            waiting,
            auto_paused,
            active_commands,
            active_children,
        }
    }

    fn take_summary_change(&mut self, id: &str) -> bool {
        let summary = self.summary(id);
        if self.last_summary.as_ref() == Some(&summary) {
            return false;
        }
        self.last_summary = Some(summary);
        true
    }

    fn broadcast(&mut self, event: ChatEvent) {
        self.subscribers
            .retain(|(_, sink)| sink(event.clone()).is_ok());
    }

    fn broadcast_snapshot(&mut self, id: &str) {
        let state = self.snapshot(id);
        self.broadcast(ChatEvent::Snapshot { state });
    }

    fn broadcast_live(&mut self) {
        self.broadcast(ChatEvent::Live {
            live: self.live.clone(),
        });
    }

    fn set_block(&mut self, index: usize, block: ChatBlock) {
        let blocks = self.streaming.get_or_insert_with(Vec::new);
        if index == blocks.len() {
            blocks.push(block);
        } else if let Some(slot) = blocks.get_mut(index) {
            *slot = block;
        }
    }

    fn streaming_event(&mut self, event: AssistantEvent) {
        match event {
            AssistantEvent::Start { .. } => {
                self.streaming = Some(Vec::new());
                let retry = self.live.retry.take().is_some();
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::Start,
                });
                if retry {
                    self.broadcast_live();
                }
            }
            AssistantEvent::TextStart { content_index } => self.set_block(
                content_index,
                ChatBlock::Text {
                    text: String::new(),
                },
            ),
            AssistantEvent::ThinkingStart { content_index } => self.set_block(
                content_index,
                ChatBlock::Thinking {
                    text: String::new(),
                    redacted: false,
                },
            ),
            AssistantEvent::TextDelta {
                content_index,
                delta,
            } => {
                let blocks = self.streaming.get_or_insert_with(Vec::new);
                if content_index == blocks.len() {
                    blocks.push(ChatBlock::Text {
                        text: String::new(),
                    });
                }
                if let Some(ChatBlock::Text { text }) = blocks.get_mut(content_index) {
                    text.push_str(&delta);
                }
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::Text {
                        index: content_index,
                        text: delta,
                    },
                });
            }
            AssistantEvent::ThinkingDelta {
                content_index,
                delta,
            } => {
                let blocks = self.streaming.get_or_insert_with(Vec::new);
                if content_index == blocks.len() {
                    blocks.push(ChatBlock::Thinking {
                        text: String::new(),
                        redacted: false,
                    });
                }
                if let Some(ChatBlock::Thinking { text, .. }) = blocks.get_mut(content_index) {
                    text.push_str(&delta);
                }
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::Thinking {
                        index: content_index,
                        text: delta,
                    },
                });
            }
            AssistantEvent::ToolCallStart {
                content_index,
                id,
                name,
            } => {
                self.set_block(
                    content_index,
                    ChatBlock::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: serde_json::Value::Null,
                    },
                );
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::ToolCall {
                        index: content_index,
                        id,
                        name,
                        arguments: None,
                    },
                });
            }
            AssistantEvent::ToolCallEnd {
                content_index,
                block: AssistantBlock::ToolCall(call),
            } => {
                self.set_block(
                    content_index,
                    convert::block(&AssistantBlock::ToolCall(call.clone())),
                );
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::ToolCall {
                        index: content_index,
                        id: call.id,
                        name: call.name,
                        arguments: Some(serde_json::Value::Object(call.arguments)),
                    },
                });
            }
            AssistantEvent::TextEnd { .. }
            | AssistantEvent::ThinkingEnd { .. }
            | AssistantEvent::ToolCallDelta { .. }
            | AssistantEvent::ToolCallEnd {
                block: AssistantBlock::Text { .. } | AssistantBlock::Thinking { .. },
                ..
            }
            | AssistantEvent::Done { .. }
            | AssistantEvent::Error { .. } => {}
        }
    }

    // Returns refresh/unload flags; registry work must happen after releasing this lock.
    fn apply(
        &mut self,
        event: SessionEvent,
        id: &str,
        session: &axo::Session,
        engine: &super::engine::Engine,
    ) -> (bool, bool) {
        let mut refresh = false;
        let mut unload = false;
        let mut changed = false;
        match event {
            SessionEvent::Entry(seq) => {
                let previous = self.live.clone();
                let linear = seq.entry.parent_id()
                    == self.path.last().and_then(|at| self.entries[*at].entry.id())
                    && !matches!(
                        seq.entry,
                        Entry::Leaf { .. } | Entry::ContextEdit { .. } | Entry::Compaction { .. }
                    );
                let at = self.entries.len();
                self.entries.push(seq);
                if linear {
                    self.path.push(at);
                } else {
                    self.path = convert::branch(&self.entries);
                }
                let entry = &self.entries[at].entry;
                match entry {
                    Entry::Submission {
                        submission_id,
                        kind,
                        parts,
                        request_id,
                        ..
                    } => {
                        if let Some(request_id) = request_id {
                            if let Some(waiter) = self.waiters.remove(request_id) {
                                let _ = waiter.send(());
                            }
                        }
                        let kind = match kind {
                            SubmissionKind::Prompt
                            | SubmissionKind::Quiet
                            | SubmissionKind::Mail => None,
                            SubmissionKind::Steer => Some(ChatQueueKind::Steer),
                            SubmissionKind::FollowUp => Some(ChatQueueKind::FollowUp),
                        };
                        if let Some(kind) = kind {
                            let item = ChatQueued {
                                submission_id: submission_id.clone(),
                                kind,
                                parts: convert::parts_to_wire(parts),
                            };
                            // axo delivers queued steers before follow-ups, FIFO within each kind.
                            let index = match kind {
                                ChatQueueKind::Steer => self
                                    .live
                                    .queue
                                    .iter()
                                    .position(|queued| queued.kind == ChatQueueKind::FollowUp)
                                    .unwrap_or(self.live.queue.len()),
                                ChatQueueKind::FollowUp => self.live.queue.len(),
                            };
                            self.live.queue.insert(index, item);
                        }
                        if !self.submitted {
                            self.submitted = true;
                            self.live.title = convert::title(&self.entries);
                            refresh = true;
                        }
                    }
                    Entry::SubmissionWithdrawn { submission_id, .. } => {
                        self.live
                            .queue
                            .retain(|queued| queued.submission_id != *submission_id);
                    }
                    Entry::Message {
                        message,
                        submission_id,
                        ..
                    } => {
                        if let Some(id) = submission_id.as_ref() {
                            self.live.queue.retain(|queued| queued.submission_id != *id);
                        }
                        match message {
                            AgentMessage::Llm(Message::Assistant(_)) => {
                                self.streaming = None;
                                self.refresh_stats(self.live.stats.context_window);
                            }
                            AgentMessage::Llm(Message::ToolResult(result)) => self
                                .live
                                .tools
                                .retain(|run| run.call_id != result.tool_call_id),
                            AgentMessage::Custom(custom)
                                if custom.custom_type == "tool_catalog" =>
                            {
                                self.refresh_tools(session);
                            }
                            AgentMessage::Llm(Message::User(_) | Message::System(_))
                            | AgentMessage::Custom(_) => {}
                        }
                    }
                    Entry::ModelChange { .. } | Entry::ThinkingLevelChange { .. }
                        if self.path.contains(&at) =>
                    {
                        let (model, thinking) = convert::selection(&self.entries, &self.path);
                        let new_window = engine.context_window(model.as_deref());
                        if self.context.is_some_and(|c| c.window != new_window) {
                            self.context = None;
                        }
                        self.live.model = model;
                        self.live.thinking = convert::thinking_to_wire(thinking);
                        self.refresh_stats(new_window);
                    }
                    Entry::RunEnd { .. } => {
                        self.live.tools.clear();
                        self.streaming = None;
                        refresh = true;
                    }
                    Entry::SessionInfo { .. } => {
                        self.live.title = convert::title(&self.entries);
                        refresh = true;
                    }
                    Entry::ChildStart { .. } => {
                        self.has_children = true;
                        self.refresh_stats(self.live.stats.context_window);
                    }
                    Entry::AgentChange { agent, .. } => {
                        // The journaled name, not `session.agent()`: axo swaps the spec after appending.
                        self.live.agent = agent.clone();
                        self.refresh_tools(session);
                    }
                    Entry::CustomMessage { custom_type, .. } if custom_type == "tool_catalog" => {
                        self.refresh_tools(session);
                    }
                    Entry::ModelChange { .. }
                    | Entry::ModeChange { .. }
                    | Entry::ThinkingLevelChange { .. }
                    | Entry::Usage { .. }
                    | Entry::Compaction { .. }
                    | Entry::BranchSummary { .. }
                    | Entry::Custom { .. }
                    | Entry::CustomMessage { .. }
                    | Entry::ContextEdit { .. }
                    | Entry::Label { .. }
                    | Entry::Leaf { .. }
                    | Entry::JobStart { .. }
                    | Entry::RunStart { .. }
                    | Entry::RunResumed { .. }
                    | Entry::JobEnd { .. }
                    | Entry::Unknown(_) => {}
                }
                if !linear {
                    let (model, thinking) = convert::selection(&self.entries, &self.path);
                    self.live.model = model;
                    self.live.thinking = convert::thinking_to_wire(thinking);
                    self.live.agent = session.agent().map(|agent| agent.name);
                    let window = engine.context_window(self.live.model.as_deref());
                    if self.context.is_some_and(|c| c.window != window) {
                        self.context = None;
                    }
                    self.refresh_stats(window);
                    self.broadcast_snapshot(id);
                } else if let Some(entry) = convert::entry(&self.entries, at) {
                    self.broadcast(ChatEvent::Entry { entry });
                }
                changed = self.live != previous;
            }
            // `running` includes maintenance and navigation ownership; idle is published
            // only after axo drains queued input and wakes, so it is the admission barrier.
            SessionEvent::Activity(activity) => {
                let next = activity.maintenance.map(maintenance);
                let idle = !activity.running && next.is_none();
                changed = self.live.running != activity.running
                    || self.live.maintenance != next
                    || (idle && self.live.retry.is_some());
                self.live.running = activity.running;
                self.live.maintenance = next;
                if idle {
                    self.live.retry = None;
                    unload = true;
                }
            }
            SessionEvent::Agent(AgentEvent::ToolExecutionStart {
                call_id,
                name,
                args,
            }) => {
                if !self.live.tools.iter().any(|run| run.call_id == call_id) {
                    self.live.tools.push(ChatToolRun {
                        call_id,
                        name,
                        arguments: serde_json::Value::Object(args),
                        output: Vec::new(),
                        details: None,
                    });
                    changed = true;
                }
            }
            SessionEvent::Agent(AgentEvent::ToolExecutionUpdate { call_id, update }) => {
                let content = convert::content(&update.content);
                if let Some(run) = self
                    .live
                    .tools
                    .iter_mut()
                    .find(|run| run.call_id == call_id)
                {
                    run.output = content.clone();
                    run.details = update.details.clone();
                }
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::ToolOutput {
                        call_id,
                        content,
                        details: update.details,
                    },
                });
            }
            SessionEvent::Agent(AgentEvent::ToolExecutionEnd { call_id, .. }) => {
                let before = self.live.tools.len();
                self.live.tools.retain(|run| run.call_id != call_id);
                changed = before != self.live.tools.len();
            }
            SessionEvent::Agent(AgentEvent::MessageUpdate { event }) => self.streaming_event(event),
            SessionEvent::QuestionPending { call_id, questions } => {
                if !self.live.questions.iter().any(|q| q.call_id == call_id) {
                    self.live.questions.push(ChatQuestions {
                        call_id,
                        questions: questions.iter().map(convert::question).collect(),
                    });
                    changed = true;
                }
            }
            SessionEvent::QuestionResolved { call_id } => {
                let before = self.live.questions.len();
                self.live.questions.retain(|q| q.call_id != call_id);
                changed = before != self.live.questions.len();
                unload = true;
            }
            SessionEvent::MaintenanceEnd { error, aborted, .. } => {
                if let Some(message) = error.filter(|_| !aborted) {
                    self.broadcast(ChatEvent::Error { message });
                }
            }
            SessionEvent::Retry {
                attempt,
                delay_ms,
                max_retries,
                error,
                ..
            } => {
                self.live.retry = Some(ChatRetry {
                    attempt,
                    max_retries,
                    delay_ms,
                    error,
                });
                changed = true;
            }
            SessionEvent::Job(info) => {
                let job = convert::job(&info, false);
                let active = job.status.is_active();
                if let Some(pos) = self.live.jobs.iter().position(|j| j.id == info.id) {
                    self.live.jobs[pos] = job;
                } else {
                    self.live.jobs.push(job);
                }
                if !active {
                    unload = true;
                }
                changed = true;
            }
            SessionEvent::JobGone { id } => {
                self.live.jobs.retain(|j| j.id != id);
                changed = true;
                unload = true;
            }
            SessionEvent::ModeResult {
                call_id,
                mode,
                result,
                awaiting_review: true,
            } => {
                let review = chat_review(session.modes(), call_id, mode, result);
                match self
                    .live
                    .reviews
                    .iter_mut()
                    .find(|r| r.call_id == review.call_id)
                {
                    Some(slot) => *slot = review,
                    None => self.live.reviews.push(review),
                }
                changed = true;
            }
            SessionEvent::ModeResultResolved { call_id, .. } => {
                let before = self.live.reviews.len();
                self.live.reviews.retain(|r| r.call_id != call_id);
                changed = before != self.live.reviews.len();
                unload = true;
            }
            SessionEvent::Context(ctx) => {
                self.context = Some(ctx);
                self.refresh_stats(ctx.window);
                changed = true;
            }
            SessionEvent::TreeUsage { usage } => {
                self.tree_usage = usage;
                self.refresh_stats(self.live.stats.context_window);
                changed = true;
            }
            SessionEvent::AutoPaused { paused } => {
                self.live.auto_paused = paused;
                changed = true;
            }
            SessionEvent::Mode { mode } => {
                self.live.mode = mode;
                changed = true;
            }
            // Tool allowance follows the effective mode, which changes at a request boundary.
            SessionEvent::EffectiveMode { mode } => {
                self.live.effective_mode = mode;
                self.refresh_tools(session);
                changed = true;
            }
            SessionEvent::ModeResult {
                awaiting_review: false,
                ..
            }
            | SessionEvent::MaintenanceStart { .. }
            | SessionEvent::Agent(
                AgentEvent::AgentStart
                | AgentEvent::AgentEnd { .. }
                | AgentEvent::TurnStart
                | AgentEvent::TurnEnd { .. }
                | AgentEvent::MessageStart { .. }
                | AgentEvent::MessageEnd { .. },
            ) => {}
        }
        if changed {
            self.broadcast_live();
        }
        (refresh, unload)
    }
}

impl LiveSession {
    pub(super) async fn start(
        service: Weak<AxoService>,
        folder: PathBuf,
        session: Arc<axo::Session>,
        root_id: Option<String>,
        engine: Arc<super::engine::Engine>,
    ) -> Arc<Self> {
        let id = session.id().to_owned();
        let is_child = root_id.is_some();
        let attach = session.attach(None).await;
        let commands = session
            .commands()
            .into_iter()
            .map(convert::command)
            .collect();
        let toolbox = convert::toolbox(&session, &folder);
        let mut initial_state = RelayState::new(
            attach.missed,
            attach.live,
            commands,
            toolbox,
            is_child,
            &engine,
            &session,
        );
        initial_state.last_summary = Some(initial_state.summary(&id));
        let live = Arc::new(Self {
            id,
            folder,
            root_id,
            session,
            service: service.clone(),
            state: Mutex::new(initial_state),
            engine,
            relay: OnceLock::new(),
            tasks: tokio_util::task::TaskTracker::new(),
            maintenance_starts: watch::channel(0).0,
            pins: AtomicUsize::new(0),
            idle: Notify::new(),
            retired: AtomicBool::new(false),
        });
        let weak = Arc::downgrade(&live);
        let session = live.session.clone();
        let relay = live.tasks.spawn(async move {
            let mut events = attach.events;
            loop {
                match events.recv().await {
                    Ok(event) => {
                        let Some(live) = weak.upgrade() else {
                            break;
                        };
                        let started = matches!(event, SessionEvent::MaintenanceStart { .. });
                        let (refresh, unload, summary_changed) = {
                            let mut state = live.state.lock();
                            let (refresh, unload) =
                                state.apply(event, &live.id, &live.session, &live.engine);
                            let summary_changed = state.take_summary_change(&live.id);
                            (refresh, unload, summary_changed)
                        };
                        if started {
                            live.maintenance_starts.send_modify(|starts| *starts += 1);
                        }
                        live.notify(summary_changed, refresh, unload);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let Some(live) = weak.upgrade() else {
                            break;
                        };
                        let last_seq = live.state.lock().entries.last().map(|seq| seq.seq);
                        let attach = session.attach(last_seq).await;
                        let (maintaining, summary_changed) = {
                            let mut state = live.state.lock();
                            state.entries.extend(attach.missed);
                            let entries = std::mem::take(&mut state.entries);
                            let mut rebuilt = RelayState::new(
                                entries,
                                attach.live,
                                std::mem::take(&mut state.commands),
                                std::mem::take(&mut state.toolbox),
                                live.root_id.is_some(),
                                &live.engine,
                                &session,
                            );
                            rebuilt.subscribers = std::mem::take(&mut state.subscribers);
                            rebuilt.waiters = std::mem::take(&mut state.waiters);
                            rebuilt.job_watchers = std::mem::take(&mut state.job_watchers);
                            rebuilt.last_summary = state.last_summary.take();
                            // The missed durable entries may include the submission a caller is awaiting.
                            for seq in &rebuilt.entries {
                                if let Entry::Submission {
                                    request_id: Some(request_id),
                                    ..
                                } = &seq.entry
                                {
                                    if let Some(waiter) = rebuilt.waiters.remove(request_id) {
                                        let _ = waiter.send(());
                                    }
                                }
                            }
                            let maintaining = rebuilt.live.maintenance.is_some();
                            let summary_changed = rebuilt.take_summary_change(&live.id);
                            *state = rebuilt;
                            state.broadcast_snapshot(&live.id);
                            (maintaining, summary_changed)
                        };
                        if maintaining {
                            live.maintenance_starts.send_modify(|starts| *starts += 1);
                        }
                        events = attach.events;
                        live.notify(summary_changed, true, true);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        let _ = live.relay.set(relay.abort_handle());
        live
    }

    fn notify(&self, summary_changed: bool, refresh: bool, unload: bool) {
        // `unload` covers every transition to idle (Activity) and every rebuild (Lagged).
        if unload {
            self.idle.notify_waiters();
        }
        if let Some(service) = self.service.upgrade() {
            if summary_changed && self.root_id.is_none() {
                service.emit_summaries(&self.folder);
            }
            if refresh {
                service.schedule_refresh(&self.folder);
            }
            if unload {
                service.maybe_unload(&self.id);
            }
        }
    }

    pub(super) async fn release(&self) {
        if self.root_id.is_some() {
            self.detach_only().await;
        } else {
            self.shutdown().await;
        }
    }

    pub(super) fn abort_relay(&self) {
        self.retired.store(true, Ordering::Release);
        self.idle.notify_waiters();
        if let Some(relay) = self.relay.get() {
            relay.abort();
        }
        for (_, handle) in self.state.lock().job_watchers.drain() {
            handle.abort();
        }
    }

    async fn shutdown(&self) {
        self.abort_relay();
        self.session.shutdown().await;
        // Shutdown settles the run; the relay and prompt drivers must also release
        // their Session Arcs before the retirement barrier permits reopening its file.
        self.tasks.close();
        self.tasks.wait().await;
    }

    async fn detach_only(&self) {
        self.abort_relay();
        self.tasks.close();
        self.tasks.wait().await;
    }
    pub(super) fn subscribe(
        &self,
        attach_id: String,
        sink: EventSink<ChatEvent>,
    ) -> Result<(), ApiError> {
        let mut state = self.state.lock();
        sink(ChatEvent::Snapshot {
            state: state.snapshot(&self.id),
        })
        .map_err(ApiError::Internal)?;
        state.subscribers.retain(|(id, _)| id != &attach_id);
        state.subscribers.push((attach_id, sink));
        Ok(())
    }

    pub(super) fn unsubscribe(&self, attach_id: &str, only: Option<&EventSink<ChatEvent>>) {
        let mut state = self.state.lock();
        // The id was re-registered by a newer stream; its subscriber and watcher stay.
        let replaced = only.is_some_and(|sink| {
            state
                .subscribers
                .iter()
                .any(|(id, held)| id == attach_id && !same_sink(held, sink))
        });
        if replaced {
            return;
        }
        state.subscribers.retain(|(id, _)| id != attach_id);
        if let Some(handle) = state.job_watchers.remove(attach_id) {
            handle.abort();
        }
    }

    pub(super) fn summary(&self) -> ChatLiveSummary {
        self.state.lock().summary(&self.id)
    }

    pub(super) fn is_active(&self) -> bool {
        self.state.lock().busy()
    }

    pub(super) fn command_job(&self, job_id: &str) -> Option<(Option<String>, u64)> {
        self.state
            .lock()
            .live
            .jobs
            .iter()
            .find_map(|job| match &job.kind {
                ChatJobKind::Command { artifact, .. } if job.id == job_id => {
                    Some((job.call_id.clone(), *artifact))
                }
                _ => None,
            })
    }

    pub(super) fn unwatch_job(&self, attach_id: &str) {
        if let Some(handle) = self.state.lock().job_watchers.remove(attach_id) {
            handle.abort();
        }
    }

    pub(super) async fn watch_command(
        self: &Arc<Self>,
        attach_id: String,
        job_id: String,
        call_id: Option<String>,
    ) -> Result<Option<ChatJobOutput>, ApiError> {
        self.unwatch_job(&attach_id);
        let Some(output) = call_id
            .as_deref()
            .and_then(|cid| self.session.command_output(cid))
        else {
            return Ok(None);
        };
        let mut rx = output.progress();
        let (progress, truncated, chunks, mut pending) = read_tail(&output).await?;
        if !progress.done {
            let should_watch = self
                .state
                .lock()
                .subscribers
                .iter()
                .any(|(id, _)| id == &attach_id);
            if should_watch {
                let weak = Arc::downgrade(self);
                let mut cursor = progress.bytes;
                let job_id_clone = job_id.clone();
                let attach_clone = attach_id.clone();
                let handle = self
                    .tasks
                    .spawn(async move {
                        loop {
                            if rx.changed().await.is_err() {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                            let next = *rx.borrow_and_update();
                            if next.bytes > cursor || next.done || next.capture_failed {
                                let (range, reset, chunk_truncated) =
                                    if next.bytes.saturating_sub(cursor) > MAX_JOB_SCROLLBACK_BYTES
                                    {
                                        (
                                            (next.bytes - MAX_JOB_SCROLLBACK_BYTES)..next.bytes,
                                            true,
                                            true,
                                        )
                                    } else {
                                        (
                                            cursor..next.bytes,
                                            false,
                                            next.bytes > MAX_JOB_SCROLLBACK_BYTES,
                                        )
                                    };
                                let Ok(raw) = output.read_chunks(range).await else {
                                    break;
                                };
                                cursor = next.bytes;
                                if reset {
                                    pending = None;
                                }
                                let chunks = decode_output(raw, &mut pending, reset, next.done);
                                let Some(live) = weak.upgrade() else {
                                    break;
                                };
                                let sink = live
                                    .state
                                    .lock()
                                    .subscribers
                                    .iter()
                                    .find(|(id, _)| id == &attach_clone)
                                    .map(|(_, sink)| Arc::clone(sink));
                                let Some(sink) = sink else {
                                    break;
                                };
                                if sink(ChatEvent::JobOutput {
                                    job_id: job_id_clone.clone(),
                                    reset,
                                    output: ChatJobOutput {
                                        chunks,
                                        capture_failed: next.capture_failed,
                                        truncated: chunk_truncated,
                                        done: next.done,
                                    },
                                })
                                .is_err()
                                    || next.done
                                {
                                    break;
                                }
                            }
                        }
                    })
                    .abort_handle();
                let mut state = self.state.lock();
                if state.subscribers.iter().any(|(id, _)| id == &attach_id) {
                    if let Some(prev) = state.job_watchers.insert(attach_id, handle) {
                        prev.abort();
                    }
                } else {
                    handle.abort();
                }
            }
        }
        Ok(Some(ChatJobOutput {
            chunks,
            capture_failed: progress.capture_failed,
            truncated,
            done: progress.done,
        }))
    }
    pub(super) fn pin(&self) {
        self.pins.fetch_add(1, Ordering::AcqRel);
    }

    pub(super) fn unpin(&self) {
        self.pins.fetch_sub(1, Ordering::AcqRel);
    }

    pub(super) fn idle_and_unattached(&self) -> bool {
        self.pins.load(Ordering::Acquire) == 0 && {
            let state = self.state.lock();
            state.subscribers.is_empty() && !state.busy()
        }
    }

    /// Resolves `true` once neither a run nor maintenance is active, `false` once the
    /// session is retired (its relay stopped, so idle would never be observed).
    pub(super) async fn wait_idle(&self) -> bool {
        loop {
            // Registered before checking, so a notification between check and await is kept.
            let mut notified = std::pin::pin!(self.idle.notified());
            notified.as_mut().enable();
            if self.retired.load(Ordering::Acquire) {
                return false;
            }
            {
                let state = self.state.lock();
                // axo's own flag too: the mirrored state lags it, and `compact` reserves against axo's.
                if !state.live.running
                    && state.live.maintenance.is_none()
                    && !self.session.is_running()
                {
                    return true;
                }
            }
            notified.await;
        }
    }

    /// The pending review `call_id`, if any.
    pub(super) fn review(&self, call_id: &str) -> Option<ChatReview> {
        self.state
            .lock()
            .live
            .reviews
            .iter()
            .find(|review| review.call_id == call_id)
            .cloned()
    }

    pub(super) fn broadcast_error(&self, message: String) {
        self.state.lock().broadcast(ChatEvent::Error { message });
    }

    /// Runs `task` on this session's tracker, so retirement waits for it before reopening.
    pub(super) fn track(&self, task: impl Future<Output = ()> + Send + 'static) {
        self.tasks.spawn(task);
    }
    pub(super) fn broadcast_closed(&self, message: String) {
        self.state.lock().broadcast(ChatEvent::Closed {
            message,
            missing: false,
        });
    }

    /// Resolves when the command finishes or when the maintenance run it starts begins; that
    /// run's later failure reaches attachments as `ChatEvent::Error` via `MaintenanceEnd`.
    pub(super) async fn run_command(&self, command: axo::Command) -> Result<(), ApiError> {
        let mut started = self.maintenance_starts.subscribe();
        let session = self.session.clone();
        let mut done = self
            .tasks
            .spawn(async move { session.run_command(command).await });
        // ponytail: another attachment's maintenance starting in the same instant also resolves this early, dropping this command's Busy error.
        tokio::select! {
            biased;
            result = &mut done => result
                .map_err(|error| ApiError::Internal(format!("axo command task failed: {error}")))?
                .map_err(axo_error),
            Ok(()) = started.changed() => Ok(()),
        }
    }

    pub(super) async fn prompt(
        self: &Arc<Self>,
        request_id: String,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let input = convert::parts_from_wire(parts)?;
        let (submitted, mut submission) = oneshot::channel();
        self.state
            .lock()
            .waiters
            .insert(request_id.clone(), submitted);
        let (mut done, mut settled) = oneshot::channel();
        let live = self.clone();
        let driver_request_id = request_id.clone();
        self.tasks.spawn(async move {
            let session = live.session.clone();
            let submitted_request_id = driver_request_id.clone();
            let mut driver = tokio::spawn(async move {
                session
                    .prompt_with(PromptRequest {
                        input,
                        request_id: Some(submitted_request_id),
                    })
                    .await
            });
            let result = tokio::select! {
                result = &mut driver => result,
                _ = done.closed() => {
                    live.state.lock().waiters.remove(&driver_request_id);
                    driver.await
                }
            };
            live.state.lock().waiters.remove(&driver_request_id);
            let result = match result {
                Ok(result) => result.map(|_| ()).map_err(|error| {
                    let message = error.to_string();
                    (axo_error(error), Some(message))
                }),
                Err(error) => {
                    let message = error.to_string();
                    if let Some(service) = live.service.upgrade() {
                        service.crashed(&live.id, message.clone());
                    }
                    Err((
                        ApiError::Internal(format!("axo session stopped unexpectedly: {message}")),
                        None,
                    ))
                }
            };
            if let Err(Err((_, Some(message)))) = done.send(result) {
                live.state.lock().broadcast(ChatEvent::Error { message });
            }
        });
        tokio::select! {
            biased;
            result = &mut settled => {
                self.state.lock().waiters.remove(&request_id);
                result.map_err(|_| ApiError::Internal("axo prompt driver stopped".into()))?.map_err(|(error, _)| error)
            }
            result = &mut submission => {
                if result.is_err() {
                    return settled.await.map_err(|_| ApiError::Internal("axo prompt driver stopped".into()))?.map_err(|(error, _)| error);
                }
                // Close before checking a result already sent, so no later driver error is lost.
                settled.close();
                if let Ok(Err((_, Some(message)))) = settled.try_recv() {
                    self.state.lock().broadcast(ChatEvent::Error { message });
                }
                Ok(())
            }
        }
    }

    pub(super) fn select_branch_target(&self, entry_id: &str) -> Result<EntryId, ApiError> {
        let state = self.state.lock();
        if !state
            .entries
            .iter()
            .any(|seq| seq.entry.id() == Some(entry_id))
        {
            return Err(ApiError::NotFound(format!("axo entry {entry_id}")));
        }
        let mut descendants = HashSet::from([entry_id.to_owned()]);
        let mut target = None;
        for seq in &state.entries {
            if seq.entry.id().is_some_and(|id| descendants.contains(id))
                || seq
                    .entry
                    .parent_id()
                    .is_some_and(|id| descendants.contains(id))
            {
                if let Some(id) = seq.entry.id() {
                    descendants.insert(id.to_owned());
                    if !matches!(seq.entry, Entry::Leaf { .. })
                        && target.as_ref().is_none_or(|(last, _)| *last < seq.seq)
                    {
                        target = Some((seq.seq, id.to_owned()));
                    }
                }
            }
        }
        target
            .map(|(_, id)| EntryId::from(id))
            .ok_or_else(|| ApiError::NotFound(format!("axo branch {entry_id}")))
    }
}
