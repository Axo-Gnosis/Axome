//! Chat wire model: axo session entries, live state and events as clients render them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatThinking {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatModel {
    /// "provider/id" selector.
    pub selector: String,
    pub provider: String,
    pub name: String,
    /// Supported levels, lowest first; `[Off]` without reasoning.
    pub thinking: Vec<ChatThinking>,
    pub context_window: u64,
    /// Accepts image input; text-only models get placeholders for images.
    pub images: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatModelRole {
    pub name: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatModelCatalog {
    /// Models of providers whose credentials resolve on the host, sorted by provider then name.
    pub models: Vec<ChatModel>,
    /// Explicit user roles in configured order; fallback-only slots are omitted.
    pub roles: Vec<ChatModelRole>,
    /// Resolved `defaultModel`, or `@default` when unset, when it is in `models`.
    pub default_model: Option<String>,
    pub default_thinking: ChatThinking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatContent {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        /// Base64-encoded image bytes.
        data: String,
    },
}

/// Most decoded Content bytes one message may carry; mirrored by CONTENT_LIMIT in src/lib/features/chat/parts/parts.ts.
pub const MAX_CONTENT_BYTES: usize = 20 * 1024 * 1024;

/// One part of a user message, mirroring `axo::PromptPart` field for field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatPart {
    Text {
        text: String,
    },
    /// Pasted text; reaches the model like `Text`, drawn collapsed.
    Paste {
        text: String,
    },
    /// A file or directory reference: relative to the session cwd, absolute, `~`, or a scheme URL.
    File {
        path: String,
    },
    /// Bytes handed over (a pasted image, a file from outside the project). Input only:
    /// axo saves them under `local://` and stores `Saved` instead.
    Content {
        name: String,
        /// Base64-encoded bytes.
        bytes: String,
    },
    /// A file axo saved from `Content`.
    Saved {
        url: String,
        image: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatBlock {
    Text {
        text: String,
    },
    /// Redacted thinking has empty text.
    Thinking {
        text: String,
        redacted: bool,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total_tokens: u64,
    /// USD.
    pub cost: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatStopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatRunOutcome {
    Done,
    Aborted,
    Failed,
    Interrupted,
}

/// One displayable entry on the session's active branch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatEntry {
    pub id: String,
    pub timestamp: String,
    pub body: ChatEntryBody,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatKiller {
    Model,
    Host,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatCommandExit {
    Exited { code: i32 },
    Signaled { signal: i32 },
    TimedOut { after_secs: f64 },
    Killed { by: ChatKiller },
    Aborted,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatTeamRef {
    pub id: String,
    pub member: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatDeliverable {
    Command {
        job_id: String,
        command: String,
        exit: Option<ChatCommandExit>,
        is_error: bool,
    },
    Child {
        job_id: String,
        agent: Option<String>,
        team: Option<ChatTeamRef>,
        note: bool,
        partial: bool,
        is_error: bool,
        text: String,
        value: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatNotice {
    JobWake {
        jobs: Vec<String>,
        deliverables: Vec<ChatDeliverable>,
    },
    Mail {
        sender: String,
        text: String,
    },
    TurnBudget {
        text: String,
    },
    YieldNudge {
        text: String,
    },
    Host {
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatEntryBody {
    /// `anchor`: where an edit of this message branches from (`None` = session start).
    /// `siblings`: ids of every user message with the same anchor (itself included), oldest first.
    /// `from`: set when a parent agent routed this input with `send`.
    /// `keywords`: names of the keywords that fired for this submission, first occurrence first.
    User {
        parts: Vec<ChatPart>,
        anchor: Option<String>,
        siblings: Vec<String>,
        from: Option<ChatSender>,
        keywords: Vec<String>,
    },
    Assistant {
        blocks: Vec<ChatBlock>,
        /// "provider/model" selector.
        model: String,
        usage: ChatUsage,
        stop_reason: ChatStopReason,
        error: Option<String>,
        duration_ms: Option<u64>,
    },
    ToolResult {
        call_id: String,
        tool_name: String,
        content: Vec<ChatContent>,
        details: Option<serde_json::Value>,
        is_error: bool,
        duration_ms: Option<u64>,
        deliverables: Vec<ChatDeliverable>,
    },
    /// A host message axo marks for display.
    Notice {
        notice: ChatNotice,
    },
    AgentChange {
        agent: Option<String>,
    },
    ModeChange {
        mode: Option<String>,
    },
    ModelChange {
        model: String,
    },
    ThinkingChange {
        thinking: ChatThinking,
    },
    Compaction {
        summary: String,
        tokens_before: u64,
    },
    BranchSummary {
        summary: String,
    },
    /// Blocks moved out of context into recoverable `artifact://` files (`/shake`, clearing).
    ContextMoved {
        blocks: u32,
    },
    RunEnd {
        outcome: ChatRunOutcome,
        reason: Option<String>,
    },
}

/// The agent session that routed a user entry; `name` is its display name when known.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSender {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatMaintenance {
    Compact,
    Shake,
    BranchSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatToolRun {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    /// Latest partial output.
    pub output: Vec<ChatContent>,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatQueueKind {
    Steer,
    FollowUp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueued {
    pub submission_id: String,
    pub kind: ChatQueueKind,
    pub parts: Vec<ChatPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatQuestionOption {
    pub label: String,
    pub description: Option<String>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatQuestion {
    pub id: String,
    pub question: String,
    pub header: Option<String>,
    pub options: Vec<ChatQuestionOption>,
    pub multi: bool,
    pub recommended: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatQuestions {
    pub call_id: String,
    pub questions: Vec<ChatQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatAnswer {
    pub id: String,
    pub selected: Vec<String>,
    pub custom: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatRetry {
    pub attempt: u32,
    pub max_retries: u32,
    pub delay_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatStats {
    /// total_tokens of the last response on the active branch.
    pub context_tokens: u64,
    /// Current model's window; 0 when unknown.
    pub context_window: u64,
    pub context_trigger: Option<u64>,
    pub context_reserve: Option<u64>,
    /// Sums over every response in the session file.
    pub usage: ChatUsage,
    /// Root plus subagents; present only when the session has children.
    pub tree: Option<ChatUsage>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatJobStatus {
    Queued,
    Running,
    HelpersRunning,
    Finished,
    Failed,
    Interrupted,
}

impl ChatJobStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::HelpersRunning)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatPromotion {
    Async,
    Timer,
    Steer,
    Host,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatChildProgress {
    pub turns: u32,
    pub max_turns: u32,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatJobKind {
    Command {
        command: String,
        artifact: u64,
        promotion: Option<ChatPromotion>,
        ended_at: Option<i64>,
        exit: Option<ChatCommandExit>,
    },
    Child {
        session_id: String,
        agent: Option<String>,
        model: String,
        team: Option<ChatTeamRef>,
        progress: Option<ChatChildProgress>,
        unread_mail: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatJob {
    pub id: String,
    pub status: ChatJobStatus,
    pub call_id: Option<String>,
    pub started_at: i64,
    pub kind: ChatJobKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatToolExposure {
    Direct,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionTool {
    pub name: String,
    pub summary: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub exposure: ChatToolExposure,
    pub promoted: bool,
    pub allowed_in_mode: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatOutputStream {
    Stdout,
    Stderr,
    Merged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatOutputChunk {
    pub stream: ChatOutputStream,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatJobOutput {
    pub chunks: Vec<ChatOutputChunk>,
    pub capture_failed: bool,
    pub truncated: bool,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatLiveSummary {
    pub session_id: String,
    pub running: bool,
    pub waiting: bool,
    pub auto_paused: bool,
    pub active_commands: u32,
    pub active_children: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummariesChangedEvent {
    pub folder_path: String,
    pub summaries: Vec<ChatLiveSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatLive {
    pub title: Option<String>,
    pub running: bool,
    pub auto_paused: bool,
    pub maintenance: Option<ChatMaintenance>,
    pub model: Option<String>,
    pub thinking: ChatThinking,
    pub tools: Vec<ChatToolRun>,
    pub queue: Vec<ChatQueued>,
    pub questions: Vec<ChatQuestions>,
    pub retry: Option<ChatRetry>,
    pub stats: ChatStats,
    pub jobs: Vec<ChatJob>,
    /// The selected persona; `None` = Default.
    pub agent: Option<String>,
    /// The selected mode.
    pub mode: Option<String>,
    /// The mode the current model request runs under; differs from `mode` until the next request.
    pub effective_mode: Option<String>,
    /// Mode results awaiting review.
    pub reviews: Vec<ChatReview>,
}

/// A mode result awaiting review, mirroring `axo::PendingResult`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatReview {
    pub call_id: String,
    pub mode: String,
    pub result: serde_json::Value,
    /// The mode is marked `axome: { plan: true }`.
    pub plan: bool,
}

/// When a built-in may run, mirroring `axo::CommandPolicy`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatCommandPolicy {
    /// Runs during a turn; takes effect from the next request.
    Alongside,
    /// Busy while a turn runs.
    WhenIdle,
    /// A skill: prompt text, sent through prompt, steer or follow-up.
    Queued,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatCommandKind {
    /// `parameters`: inlined JSON Schema object; `format: "model" | "entry"` mark semantic strings.
    Builtin { parameters: serde_json::Value },
    /// One free-text argument.
    Skill { argument_hint: Option<String> },
    /// Enters a mode; the rest of the message is its prompt.
    Mode { argument_hint: Option<String> },
}

/// One entry of the session's command catalog, mirroring `axo::CommandInfo` JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatCommand {
    pub name: String,
    pub description: String,
    pub policy: ChatCommandPolicy,
    #[serde(flatten)]
    pub kind: ChatCommandKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSkillInfo {
    pub name: String,
    pub description: String,
    pub path: String,
    pub model_invocable: bool,
    pub argument_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatAgentRunAs {
    Root,
    Subagent,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatAgentInfo {
    pub name: String,
    pub description: String,
    pub team: bool,
    pub run_as: Option<ChatAgentRunAs>,
    pub model: Option<String>,
    pub tools: Option<Vec<String>>,
    pub bash_read_only: bool,
    pub max_turns: Option<u32>,
    pub structured_output: bool,
    pub members: Vec<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatModeInfo {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub model_invocable: bool,
    pub model: Option<String>,
    pub tools_allow: Option<Vec<String>>,
    pub tools_deny: Vec<String>,
    pub bash_read_only: bool,
    /// Where `write` and `edit` may write; `None` = unrestricted.
    pub writes: Option<Vec<String>>,
    pub result_reviewed: Option<bool>,
    pub path: Option<String>,
    /// The mode is marked `axome: { plan: true }`.
    pub plan: bool,
}

/// What follows accepting a reviewed plan.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatPlanThen {
    /// The run ends after saving.
    Save,
    /// The run continues and implements the plan.
    Work,
    /// The run ends, the session compacts, then a new turn implements the plan.
    CompactWork,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatKeywordInfo {
    pub name: String,
    pub description: String,
    pub requires_tools: Vec<String>,
    pub model: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatToolbox {
    pub skills: Vec<ChatSkillInfo>,
    pub agents: Vec<ChatAgentInfo>,
    pub modes: Vec<ChatModeInfo>,
    pub keywords: Vec<ChatKeywordInfo>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatPromptSection {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextFile {
    pub path: String,
    pub content: String,
    pub nested: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextDetails {
    pub prompt: Vec<ChatPromptSection>,
    pub context_files: Vec<ChatContextFile>,
}

/// The session's `local://` files and the canonical directory they live in.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatLocalFiles {
    /// Canonical absolute path of the `local://` root, so callers can recognise ledger paths under it.
    pub root: String,
    pub files: Vec<ChatLocalFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatLocalFile {
    /// Relative to the `local://` root, `/`-separated.
    pub path: String,
    pub size: u64,
}

/// A keyword occurrence in composer text, in UTF-16 code units.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatKeywordMatch {
    pub name: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatState {
    pub session_id: String,
    pub entries: Vec<ChatEntry>,
    /// Blocks of the response being streamed; `None` when none is.
    pub streaming: Option<Vec<ChatBlock>>,
    pub live: ChatLive,
    pub commands: Vec<ChatCommand>,
    pub session_tools: Vec<ChatSessionTool>,
    pub toolbox: ChatToolbox,
    pub read_only: bool,
    pub is_child: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatDelta {
    /// A response started: `streaming` becomes `Some(vec![])`.
    Start,
    /// Appends to block `index` (creating a text block when `index == blocks.len()`).
    Text { index: usize, text: String },
    /// Appends to block `index` (creating a thinking block when `index == blocks.len()`).
    Thinking { index: usize, text: String },
    /// Sets tool-call block `index`; `arguments` is `None` until the call is complete.
    ToolCall {
        index: usize,
        id: String,
        name: String,
        arguments: Option<serde_json::Value>,
    },
    /// Replaces running tool `call_id`'s output.
    ToolOutput {
        call_id: String,
        content: Vec<ChatContent>,
        details: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatEvent {
    /// First event of every attach, and again whenever the active branch changes non-linearly.
    Snapshot {
        state: ChatState,
    },
    /// Appended to the active branch. An `Assistant` entry also ends `streaming`.
    Entry {
        entry: ChatEntry,
    },
    Live {
        live: ChatLive,
    },
    SessionTools {
        tools: Vec<ChatSessionTool>,
    },
    Delta {
        delta: ChatDelta,
    },
    Error {
        message: String,
    },
    /// The stream ended for good; `missing` = the session does not exist.
    Closed {
        message: String,
        missing: bool,
    },
    /// Emitted by transports (never the Host) while a remote stream reconnects; a `Snapshot` follows.
    Reconnecting,
    JobOutput {
        job_id: String,
        reset: bool,
        output: ChatJobOutput,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatForkInfo {
    pub session_id: String,
    /// The forked user message's parts, for the composer to edit and resubmit.
    pub editable: Option<Vec<ChatPart>>,
}

/// What a scheme URL shown in a chat names, resolved against the chat's session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatLinkTarget {
    /// A file or directory on the session's host (daemon-absolute for remote folders).
    File { path: String, is_dir: bool },
    /// Text a host scheme renders itself.
    Virtual { text: String },
    /// Not a scheme this session registers.
    Unhandled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSquashedChange {
    pub path: String,
    pub key: String,
    pub url: Option<String>,
    pub created: bool,
    pub before_sha: Option<String>,
    pub after_sha: String,
    pub binary: bool,
    pub omitted: bool,
    pub mutations: usize,
    pub discontinuous: bool,
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
}
