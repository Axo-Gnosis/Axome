use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, OnceLock, Weak,
    },
};

use axo::store::{Entry, EntryId, SeqEntry};
use axo::{LiveSnapshot, MaintenanceKind, PromptRequest, SessionEvent};
use axo_agent::{AgentEvent, AgentMessage};
use axo_ai::{AssistantBlock, AssistantEvent, Message};
use axome_protocol::chat::*;
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot, watch, Notify};

use super::extensions::SessionExtensions;
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
    pub(super) extension_owner: SessionExtensions,
    pub(super) models: Arc<axo_ai::Models>,
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
    tool_origins: BTreeMap<String, String>,
    extension_diagnostics: Vec<String>,
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
        extension_diagnostics: Vec<String>,
        toolbox: ChatToolbox,
        is_child: bool,
        session: &axo::Session,
    ) -> Self {
        let tools = session.tools();
        let tool_origins = convert::tool_origins(&tools);
        let session_tools = convert::session_tools(tools);
        let commands = session
            .commands()
            .into_iter()
            .map(convert::command)
            .collect();
        let path = convert::branch(&entries);
        let model = &snapshot.model;
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
            Self::selection_window(session, model),
            context,
            has_children.then_some(&snapshot.tree_usage),
        );
        let queue = convert::queued(&snapshot.queued);
        let jobs = snapshot
            .jobs
            .iter()
            .map(|job| convert::job(job, false))
            .collect();
        let modes = session.modes();
        let live = ChatLive {
            title: convert::title(&entries),
            running: snapshot.running,
            maintenance: snapshot.maintenance.map(maintenance),
            model: Some(format!("{}/{}", model.provider, model.id)),
            thinking: convert::thinking_to_wire(snapshot.thinking),
            service_tier: convert::service_tier_to_wire(snapshot.service_tier),
            effective_service_tier: convert::service_tier_to_wire(
                model.effective_tier(snapshot.service_tier),
            ),
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
                .map(|(call_id, blocks)| ChatQuestions {
                    call_id,
                    blocks: blocks.iter().map(convert::ask_block).collect(),
                    deadline_ms: None,
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
                .map(|result| chat_review(&modes, result.call_id, result.mode, result.result))
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
            tool_origins,
            extension_diagnostics,
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

    fn selection_window(session: &axo::Session, model: &axo_ai::Model) -> u64 {
        session
            .config()
            .settings
            .compaction
            .max_context
            .map_or(model.context_window, |cap| cap.min(model.context_window))
    }

    fn apply_selection(
        &mut self,
        thinking: axo_ai::ThinkingLevel,
        tier: axo_ai::ServiceTier,
        effective: axo_ai::ServiceTier,
        window: u64,
        force_stats: bool,
    ) -> bool {
        let thinking = convert::thinking_to_wire(thinking);
        let effective = convert::service_tier_to_wire(effective);
        let tier = convert::service_tier_to_wire(tier);
        let window_changed = self.live.stats.context_window != window;
        let changed = self.live.thinking != thinking
            || self.live.service_tier != tier
            || self.live.effective_service_tier != effective
            || window_changed;
        self.live.thinking = thinking;
        self.live.service_tier = tier;
        self.live.effective_service_tier = effective;
        if self.context.is_some_and(|context| context.window != window) || window_changed {
            self.context = None;
        }
        if window_changed || force_stats {
            self.refresh_stats(window);
        }
        changed
    }

    fn refresh_selection(&mut self, session: &axo::Session) -> bool {
        let model = session.model();
        let thinking = session.thinking();
        let tier = session.service_tier();
        let model_changed = self.live.model.as_deref().and_then(|s| s.split_once('/'))
            != Some((model.provider.as_str(), model.id.as_str()));
        if model_changed {
            self.live.model = Some(format!("{}/{}", model.provider, model.id));
        }
        let window = Self::selection_window(session, &model);
        let effective = model.effective_tier(tier);
        model_changed | self.apply_selection(thinking, tier, effective, window, false)
    }

    fn project_selection(&mut self, session: &axo::Session, models: &axo_ai::Models) {
        // Selection entries are published before axo updates its live getters.
        let (model, thinking, tier) = convert::selection(&self.entries, &self.path);
        let current = session.model();
        let same_model = model
            .as_deref()
            .and_then(|selector| selector.split_once('/'))
            == Some((current.provider.as_str(), current.id.as_str()));
        let registered = if same_model {
            None
        } else {
            model
                .as_deref()
                .and_then(|selector| super::engine::model(models, selector))
        };
        let definition = if same_model {
            Some(&current)
        } else {
            registered.as_deref()
        };
        let window = definition.map_or(0, |model| Self::selection_window(session, model));
        let effective = definition.map_or(axo_ai::ServiceTier::Standard, |model| {
            model.effective_tier(tier)
        });
        self.live.model = model;
        self.apply_selection(thinking, tier, effective, window, true);
    }

    fn refresh_catalog(
        &mut self,
        session: &axo::Session,
        extension: Option<(Vec<String>, ChatToolbox)>,
    ) {
        let tools = session.tools();
        let tool_origins = convert::tool_origins(&tools);
        let tools = convert::session_tools(tools);
        if tools != self.session_tools {
            self.session_tools = tools.clone();
            self.broadcast(ChatEvent::SessionTools { tools });
        }
        let commands = session
            .commands()
            .into_iter()
            .map(convert::command)
            .collect::<Vec<_>>();
        let (diagnostics, toolbox) = match extension {
            Some((diagnostics, toolbox)) => (Some(diagnostics), Some(toolbox)),
            None => (None, None),
        };
        if commands != self.commands
            || tool_origins != self.tool_origins
            || diagnostics
                .as_ref()
                .is_some_and(|value| value != &self.extension_diagnostics)
            || toolbox.as_ref().is_some_and(|value| value != &self.toolbox)
        {
            self.commands = commands;
            self.tool_origins = tool_origins;
            if let Some(diagnostics) = diagnostics {
                self.extension_diagnostics = diagnostics;
            }
            if let Some(toolbox) = toolbox {
                self.toolbox = toolbox;
            }
            self.broadcast(ChatEvent::ExtensionCatalog {
                commands: self.commands.clone(),
                tool_origins: self.tool_origins.clone(),
                diagnostics: self.extension_diagnostics.clone(),
                toolbox: self.toolbox.clone(),
            });
        }
    }

    fn snapshot(&self, id: &str) -> ChatState {
        ChatState {
            session_id: id.to_owned(),
            entries: convert::path_entries(&self.entries, &self.path),
            streaming: self.streaming.clone(),
            live: self.live.clone(),
            commands: self.commands.clone(),
            tool_origins: self.tool_origins.clone(),
            extension_diagnostics: self.extension_diagnostics.clone(),
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
        let state = Box::new(self.snapshot(id));
        self.broadcast(ChatEvent::Snapshot { state });
    }

    fn broadcast_live(&mut self) {
        self.broadcast(ChatEvent::Live {
            live: Box::new(self.live.clone()),
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

    fn streaming_event(&mut self, event: &AssistantEvent) {
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
                *content_index,
                ChatBlock::Text {
                    text: String::new(),
                },
            ),
            AssistantEvent::ThinkingStart { content_index } => self.set_block(
                *content_index,
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
                if *content_index == blocks.len() {
                    blocks.push(ChatBlock::Text {
                        text: String::new(),
                    });
                }
                if let Some(ChatBlock::Text { text }) = blocks.get_mut(*content_index) {
                    text.push_str(delta);
                }
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::Text {
                        index: *content_index,
                        text: delta.clone(),
                    },
                });
            }
            AssistantEvent::ThinkingDelta {
                content_index,
                delta,
            } => {
                let blocks = self.streaming.get_or_insert_with(Vec::new);
                if *content_index == blocks.len() {
                    blocks.push(ChatBlock::Thinking {
                        text: String::new(),
                        redacted: false,
                    });
                }
                if let Some(ChatBlock::Thinking { text, .. }) = blocks.get_mut(*content_index) {
                    text.push_str(delta);
                }
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::Thinking {
                        index: *content_index,
                        text: delta.clone(),
                    },
                });
            }
            AssistantEvent::ToolCallStart {
                content_index,
                id,
                name,
            } => {
                self.set_block(
                    *content_index,
                    ChatBlock::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: serde_json::Value::Null,
                    },
                );
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::ToolCall {
                        index: *content_index,
                        id: id.clone(),
                        name: name.clone(),
                        arguments: None,
                    },
                });
            }
            AssistantEvent::ToolCallEnd {
                content_index,
                block: block @ AssistantBlock::ToolCall(call),
            } => {
                self.set_block(*content_index, convert::block(block));
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::ToolCall {
                        index: *content_index,
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: Some(serde_json::Value::Object(call.arguments.clone())),
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
        event: &SessionEvent,
        id: &str,
        session: &axo::Session,
        models: &axo_ai::Models,
        queue: Option<Vec<ChatQueued>>,
    ) -> (bool, bool) {
        let mut refresh = false;
        let mut unload = false;
        let mut changed = false;
        if let Some(queue) = queue {
            changed = self.live.queue != queue;
            self.live.queue = queue;
        }
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
                self.entries.push(seq.clone());
                if linear {
                    self.path.push(at);
                } else {
                    self.path = convert::branch(&self.entries);
                }
                let entry = &self.entries[at].entry;
                match entry {
                    Entry::Submission { request_id, .. } => {
                        if let Some(request_id) = request_id {
                            if let Some(waiter) = self.waiters.remove(request_id) {
                                let _ = waiter.send(());
                            }
                        }
                        if !self.submitted {
                            self.submitted = true;
                            self.live.title = convert::title(&self.entries);
                            refresh = true;
                        }
                    }
                    Entry::Message { message, .. } => match message {
                        AgentMessage::Llm(Message::Assistant(_)) => {
                            self.streaming = None;
                            self.refresh_stats(self.live.stats.context_window);
                        }
                        AgentMessage::Llm(Message::ToolResult(result)) => self
                            .live
                            .tools
                            .retain(|run| run.call_id != result.tool_call_id),
                        AgentMessage::Custom(custom) if custom.custom_type == "tool_catalog" => {
                            self.refresh_catalog(session, None);
                        }
                        AgentMessage::Llm(Message::User(_) | Message::System(_))
                        | AgentMessage::Custom(_) => {}
                    },
                    Entry::ModelChange { .. }
                    | Entry::ThinkingLevelChange { .. }
                    | Entry::ServiceTierChange { .. }
                        if linear =>
                    {
                        self.project_selection(session, models);
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
                        self.refresh_catalog(session, None);
                    }
                    Entry::CustomMessage { custom_type, .. } if custom_type == "tool_catalog" => {
                        self.refresh_catalog(session, None);
                    }
                    Entry::ModelChange { .. }
                    | Entry::SubmissionWithdrawn { .. }
                    | Entry::ModeChange { .. }
                    | Entry::ThinkingLevelChange { .. }
                    | Entry::ServiceTierChange { .. }
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
                    self.live.agent = session.agent().map(|agent| agent.name);
                    self.project_selection(session, models);
                    self.broadcast_snapshot(id);
                } else if let Some(entry) = convert::entry(&self.entries, at) {
                    self.broadcast(ChatEvent::Entry { entry });
                }
                changed |= self.live != previous;
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
                if !self.live.tools.iter().any(|run| run.call_id == *call_id) {
                    self.live.tools.push(ChatToolRun {
                        call_id: call_id.clone(),
                        name: name.clone(),
                        arguments: serde_json::Value::Object(args.clone()),
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
                    .find(|run| run.call_id == *call_id)
                {
                    run.output = content.clone();
                    run.details = update.details.clone();
                }
                self.broadcast(ChatEvent::Delta {
                    delta: ChatDelta::ToolOutput {
                        call_id: call_id.clone(),
                        content,
                        details: update.details.clone(),
                    },
                });
            }
            SessionEvent::Agent(AgentEvent::ToolExecutionEnd { call_id, .. }) => {
                let before = self.live.tools.len();
                self.live.tools.retain(|run| run.call_id != *call_id);
                changed = before != self.live.tools.len();
            }
            SessionEvent::Agent(AgentEvent::MessageUpdate { event }) => self.streaming_event(event),
            SessionEvent::QuestionPending { call_id, blocks } => {
                let arrival = chrono::Utc::now().timestamp_millis();
                let deadline_ms = session
                    .config()
                    .settings
                    .ask
                    .timeout_ms
                    .and_then(|timeout| i64::try_from(timeout).ok())
                    .and_then(|timeout| arrival.checked_add(timeout));
                let pending = ChatQuestions {
                    call_id: call_id.clone(),
                    blocks: blocks.iter().map(convert::ask_block).collect(),
                    deadline_ms,
                };
                if let Some(row) = self
                    .live
                    .questions
                    .iter_mut()
                    .find(|q| q.call_id == *call_id)
                {
                    *row = pending;
                } else {
                    self.live.questions.push(pending);
                }
                changed = true;
            }
            SessionEvent::QuestionResolved { call_id } => {
                let before = self.live.questions.len();
                self.live.questions.retain(|q| q.call_id != *call_id);
                changed = before != self.live.questions.len();
                unload = true;
            }
            SessionEvent::MaintenanceEnd { error, aborted, .. } => {
                if let Some(message) = error.as_ref().filter(|_| !*aborted) {
                    self.broadcast(ChatEvent::Error {
                        message: message.clone(),
                    });
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
                    attempt: *attempt,
                    max_retries: *max_retries,
                    delay_ms: *delay_ms,
                    error: error.clone(),
                });
                changed = true;
            }
            SessionEvent::Job(info) => {
                let job = convert::job(info, false);
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
                self.live.jobs.retain(|j| j.id != *id);
                changed = true;
                unload = true;
            }
            SessionEvent::ModeResult {
                call_id,
                mode,
                result,
                awaiting_review: true,
            } => {
                let review = chat_review(
                    &session.modes(),
                    call_id.clone(),
                    mode.clone(),
                    result.clone(),
                );
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
                self.live.reviews.retain(|r| r.call_id != *call_id);
                changed = before != self.live.reviews.len();
                unload = true;
            }
            SessionEvent::Context(ctx) => {
                changed |= self.refresh_selection(session);
                // A context event may still describe the model a newer selection just replaced.
                if ctx.window == self.live.stats.context_window && self.context != Some(*ctx) {
                    self.context = Some(*ctx);
                    self.refresh_stats(ctx.window);
                    changed = true;
                }
            }
            SessionEvent::TreeUsage { usage } => {
                self.tree_usage = usage.clone();
                self.refresh_stats(self.live.stats.context_window);
                changed = true;
            }
            SessionEvent::AutoPaused { paused } => {
                self.live.auto_paused = *paused;
                changed = true;
            }
            SessionEvent::Mode { mode } => {
                changed |= self.live.mode != *mode;
                self.live.mode = mode.clone();
                changed |= self.refresh_selection(session);
            }
            // Tool allowance follows the effective mode, which changes at a request boundary.
            SessionEvent::EffectiveMode { mode } => {
                self.live.effective_mode = mode.clone();
                self.refresh_catalog(session, None);
                changed = true;
            }
            SessionEvent::QueueChanged => changed = true,
            SessionEvent::Agent(AgentEvent::TurnStart) => {
                changed |= self.refresh_selection(session);
            }
            SessionEvent::ModeResult {
                awaiting_review: false,
                ..
            }
            | SessionEvent::MaintenanceStart { .. }
            | SessionEvent::Agent(
                AgentEvent::AgentStart
                | AgentEvent::AgentEnd { .. }
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
        (extension_owner, models): (SessionExtensions, Arc<axo_ai::Models>),
        diagnostics: Vec<String>,
    ) -> Arc<Self> {
        let id = session.id().to_owned();
        let is_child = root_id.is_some();
        let attach = session.attach(None).await;
        let toolbox = convert::toolbox(&session, &folder);
        let mut initial_state = RelayState::new(
            attach.missed,
            attach.live,
            diagnostics,
            toolbox,
            is_child,
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
            extension_owner,
            models,
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
                        let event = event.as_ref();
                        let refresh_queue = match event {
                            SessionEvent::QueueChanged => true,
                            SessionEvent::Entry(seq) => match &seq.entry {
                                // Pinned axo publishes durable entries, not QueueChanged,
                                // for delivery and successful withdrawals. Invalidate only:
                                // axo's current queue still owns the order and provenance.
                                Entry::SubmissionWithdrawn { submission_id, .. }
                                | Entry::Message { submission_id: Some(submission_id), .. } => live
                                    .state
                                    .lock()
                                    .live
                                    .queue
                                    .iter()
                                    .any(|item| item.submission_id == *submission_id),
                                _ => false,
                            },
                            _ => false,
                        };
                        let queue = if refresh_queue {
                            // Skip copying missed entries; attach still builds the live snapshot.
                            let queued = session.attach(Some(u64::MAX)).await.live.queued;
                            Some(convert::queued(&queued))
                        } else {
                            None
                        };
                        let started = matches!(event, SessionEvent::MaintenanceStart { .. });
                        let compacted = matches!(event, SessionEvent::Entry(seq) if matches!(seq.entry, Entry::Compaction { .. }));
                        let (refresh, unload, summary_changed) = {
                            let mut state = live.state.lock();
                            let (refresh, unload) =
                                state.apply(event, &live.id, &live.session, &live.models, queue);
                            let summary_changed = state.take_summary_change(&live.id);
                            (refresh, unload, summary_changed)
                        };
                        if compacted {
                            if let Some(service) = service.upgrade() {
                                service.memory_capture(&live, true);
                            }
                        }
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
                        let compacted = attach.missed.iter().any(|seq| matches!(seq.entry, Entry::Compaction { .. }));
                        let (maintaining, summary_changed) = {
                            let mut state = live.state.lock();
                            state.entries.extend(attach.missed);
                            let entries = std::mem::take(&mut state.entries);
                            let mut rebuilt = RelayState::new(
                                entries,
                                attach.live,
                                std::mem::take(&mut state.extension_diagnostics),
                                std::mem::take(&mut state.toolbox),
                                live.root_id.is_some(),
                                &session,
                            );
                            for pending in &mut rebuilt.live.questions {
                                if let Some(previous) = state.live.questions.iter().find(|previous| {
                                    previous.call_id == pending.call_id && previous.blocks == pending.blocks
                                }) {
                                    pending.deadline_ms = previous.deadline_ms;
                                }
                            }
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
                        if compacted {
                            if let Some(service) = service.upgrade() {
                                service.memory_capture(&live, true);
                            }
                        }
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

    pub(super) fn refresh_extension_catalog(&self, diagnostics: Vec<String>) {
        let toolbox = convert::toolbox(&self.session, &self.folder);
        self.state
            .lock()
            .refresh_catalog(&self.session, Some((diagnostics, toolbox)));
    }

    pub(super) async fn release(&self) {
        if let Some(service) = self.service.upgrade().filter(|_| self.root_id.is_none()) {
            match service.owner_for_session(self).await {
                Ok(owner) => owner.detach(&self.id),
                Err(error) => {
                    tracing::error!(%error, session_id = %self.id, "could not detach extensions")
                }
            }
        }
        self.release_detached().await;
    }

    pub(super) async fn release_detached(&self) {
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
            state: Box::new(state.snapshot(&self.id)),
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

    pub(super) fn command_job(&self, job_id: &str) -> Option<u64> {
        self.state
            .lock()
            .live
            .jobs
            .iter()
            .find_map(|job| match &job.kind {
                ChatJobKind::Command { artifact, .. } if job.id == job_id => Some(*artifact),
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
    ) -> Result<Option<ChatJobOutput>, ApiError> {
        self.unwatch_job(&attach_id);
        let Some(output) = self.session.job_output(&job_id) else {
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
                                    job_id: job_id.clone(),
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

    /// Registered handlers resolve only on completion. Built-ins may resolve when maintenance
    /// starts; its later failure reaches attachments as `ChatEvent::Error` via `MaintenanceEnd`.
    pub(super) async fn run_command(&self, command: axo::Command) -> Result<(), ApiError> {
        let started = (!matches!(command, axo::Command::Registered { .. }))
            .then(|| self.maintenance_starts.subscribe());
        let session = self.session.clone();
        let mut done = self
            .tasks
            .spawn(async move { session.run_command(command).await });
        // ponytail: another attachment's maintenance starting in the same instant also resolves this early, dropping this command's Busy error.
        let result = if let Some(mut started) = started {
            tokio::select! {
                biased;
                result = &mut done => result,
                Ok(()) = started.changed() => return Ok(()),
            }
        } else {
            done.await
        };
        result
            .map_err(|error| ApiError::Internal(format!("axo command task failed: {error}")))?
            .map_err(axo_error)
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
