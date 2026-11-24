use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::{errors::ApiError, services::history::normalize_title};
use axo::store::{Entry, RunOutcome, SeqEntry, SubmissionKind};
use axo_agent::AgentMessage;
use axo_ai::{AssistantBlock, Message, ServiceTier, StopReason, ThinkingLevel, Usage, UserBlock};
use axo_extensions::ExtensionOrigin;
use axome_protocol::chat::*;
use base64::{engine::general_purpose::STANDARD, Engine};

pub(super) fn parts_from_wire(parts: Vec<ChatPart>) -> Result<axo::PromptInput, ApiError> {
    let mut content = 0usize;
    parts
        .into_iter()
        .map(|part| {
            Ok(match part {
                ChatPart::Text { text } => axo::PromptPart::Text { text },
                ChatPart::Paste { text } => axo::PromptPart::Paste { text },
                ChatPart::File { path } => axo::PromptPart::File { path },
                ChatPart::Saved { url, image } => axo::PromptPart::Saved { url, image },
                ChatPart::Content { name, bytes } => {
                    let bytes = STANDARD.decode(bytes).map_err(|error| {
                        ApiError::InvalidArgument(format!("Invalid content bytes: {error}"))
                    })?;
                    content += bytes.len();
                    if content > MAX_CONTENT_BYTES {
                        return Err(ApiError::InvalidArgument(
                            "A message can contain at most 20 MiB of content.".into(),
                        ));
                    }
                    axo::PromptPart::Content { name, bytes }
                }
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()
        .map(axo::PromptInput::from)
}

pub(super) fn parts_to_wire(parts: &[axo::PromptPart]) -> Vec<ChatPart> {
    parts
        .iter()
        .map(|part| match part {
            axo::PromptPart::Text { text } => ChatPart::Text { text: text.clone() },
            axo::PromptPart::Paste { text } => ChatPart::Paste { text: text.clone() },
            axo::PromptPart::File { path } => ChatPart::File { path: path.clone() },
            axo::PromptPart::Saved { url, image } => ChatPart::Saved {
                url: url.clone(),
                image: *image,
            },
            axo::PromptPart::Content { name, bytes } => ChatPart::Content {
                name: name.clone(),
                bytes: STANDARD.encode(bytes),
            },
        })
        .collect()
}

fn parts_text(parts: &[axo::PromptPart]) -> String {
    let mut result = String::new();
    for part in parts {
        let reference = match part {
            axo::PromptPart::Text { text } | axo::PromptPart::Paste { text } => {
                result.push_str(text);
                continue;
            }
            axo::PromptPart::File { path } => path,
            axo::PromptPart::Saved { url, .. } => url,
            axo::PromptPart::Content { name, .. } => name,
        };
        result.push('@');
        let quoted = reference.chars().any(char::is_whitespace);
        if quoted {
            result.push('"');
        }
        result.push_str(reference);
        if quoted {
            result.push('"');
        }
    }
    result
}

pub(super) fn command(value: axo::CommandInfo) -> ChatCommand {
    ChatCommand {
        name: value.name,
        description: value.description,
        origin: value
            .origin
            .as_deref()
            .and_then(ExtensionOrigin::parse)
            .map(|origin| origin.id.to_owned()),
        policy: match value.policy {
            axo::CommandPolicy::Alongside => ChatCommandPolicy::Alongside,
            axo::CommandPolicy::WhenIdle => ChatCommandPolicy::WhenIdle,
            axo::CommandPolicy::Queued => ChatCommandPolicy::Queued,
        },
        kind: match value.kind {
            axo::CommandKind::Builtin { parameters } => ChatCommandKind::Builtin { parameters },
            axo::CommandKind::Skill { argument_hint } => ChatCommandKind::Skill { argument_hint },
            axo::CommandKind::Mode { argument_hint } => ChatCommandKind::Mode { argument_hint },
            axo::CommandKind::Registered { argument_hint } => {
                ChatCommandKind::Registered { argument_hint }
            }
        },
    }
}

/// A mode marked `axome: { plan: true }` in its frontmatter gets Axome's Plan view.
pub(super) fn mode_is_plan(mode: &axo::ModeSpec) -> bool {
    mode.extra
        .get("axome")
        .and_then(|value| value.get("plan"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(super) fn thinking_to_wire(level: ThinkingLevel) -> ChatThinking {
    match level {
        ThinkingLevel::Off => ChatThinking::Off,
        ThinkingLevel::Minimal => ChatThinking::Minimal,
        ThinkingLevel::Low => ChatThinking::Low,
        ThinkingLevel::Medium => ChatThinking::Medium,
        ThinkingLevel::High => ChatThinking::High,
        ThinkingLevel::XHigh => ChatThinking::Xhigh,
        ThinkingLevel::Max => ChatThinking::Max,
    }
}

pub(super) fn service_tier_to_wire(tier: ServiceTier) -> ChatServiceTier {
    match tier {
        ServiceTier::Standard => ChatServiceTier::Standard,
        ServiceTier::Flex => ChatServiceTier::Flex,
        ServiceTier::Fast => ChatServiceTier::Fast,
        ServiceTier::Ultrafast => ChatServiceTier::Ultrafast,
    }
}

pub(super) fn thinking_from_wire(level: ChatThinking) -> ThinkingLevel {
    match level {
        ChatThinking::Off => ThinkingLevel::Off,
        ChatThinking::Minimal => ThinkingLevel::Minimal,
        ChatThinking::Low => ThinkingLevel::Low,
        ChatThinking::Medium => ThinkingLevel::Medium,
        ChatThinking::High => ThinkingLevel::High,
        ChatThinking::Xhigh => ThinkingLevel::XHigh,
        ChatThinking::Max => ThinkingLevel::Max,
    }
}

pub(super) fn usage(value: &Usage) -> ChatUsage {
    ChatUsage {
        input: value.input,
        output: value.output,
        cache_read: value.cache_read,
        cache_write: value.cache_write,
        total_tokens: value.total_tokens,
        cost: value.cost.map(|cost| cost.total),
    }
}

pub(super) fn content(blocks: &[UserBlock]) -> Vec<ChatContent> {
    blocks
        .iter()
        .map(|block| match block {
            UserBlock::Text { text } => ChatContent::Text { text: text.clone() },
            UserBlock::Image { mime_type, data } => ChatContent::Image {
                mime_type: mime_type.clone(),
                data: data.to_string(),
            },
        })
        .collect()
}

pub(super) fn text(blocks: &[UserBlock]) -> String {
    let mut result = String::new();
    let mut first = true;
    for block in blocks {
        if let UserBlock::Text { text } = block {
            if !first {
                result.push('\n');
            }
            result.push_str(text);
            first = false;
        }
    }
    result
}

pub(super) fn block(value: &AssistantBlock) -> ChatBlock {
    match value {
        AssistantBlock::Text { text, .. } => ChatBlock::Text { text: text.clone() },
        AssistantBlock::Thinking {
            thinking, redacted, ..
        } => ChatBlock::Thinking {
            text: if *redacted {
                String::new()
            } else {
                thinking.clone()
            },
            redacted: *redacted,
        },
        AssistantBlock::ToolCall(call) => ChatBlock::ToolCall {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: serde_json::Value::Object(call.arguments.clone()),
        },
    }
}

pub(super) fn command_exit(exit: &axo::CommandExit) -> ChatCommandExit {
    match exit {
        axo::CommandExit::Exited { code } => ChatCommandExit::Exited { code: *code },
        axo::CommandExit::Signaled { signal } => ChatCommandExit::Signaled { signal: *signal },
        axo::CommandExit::TimedOut { after_secs } => ChatCommandExit::TimedOut {
            after_secs: *after_secs,
        },
        axo::CommandExit::Killed { by } => ChatCommandExit::Killed {
            by: match by {
                axo::Killer::Model => ChatKiller::Model,
                axo::Killer::Host => ChatKiller::Host,
            },
        },
        axo::CommandExit::Aborted => ChatCommandExit::Aborted,
        axo::CommandExit::Interrupted => ChatCommandExit::Interrupted,
    }
}

pub(super) fn team_ref(team: &axo::TeamRef) -> ChatTeamRef {
    ChatTeamRef {
        id: team.id.clone(),
        member: team.member.clone(),
        name: team.name.clone(),
    }
}

/// Job start/end positions, indexed on first use so converting a whole
/// transcript scans it once instead of once per job reference.
struct JobLookup<'a> {
    entries: &'a [SeqEntry],
    index: std::cell::OnceCell<JobIndex<'a>>,
}

struct JobIndex<'a> {
    ends: HashMap<&'a str, usize>,
    starts: HashMap<&'a str, usize>,
}

impl<'a> JobLookup<'a> {
    fn new(entries: &'a [SeqEntry]) -> Self {
        Self {
            entries,
            index: std::cell::OnceCell::new(),
        }
    }

    fn index(&self) -> &JobIndex<'a> {
        self.index.get_or_init(|| {
            let mut index = JobIndex {
                ends: HashMap::new(),
                starts: HashMap::new(),
            };
            for (at, seq) in self.entries.iter().enumerate() {
                match &seq.entry {
                    Entry::JobEnd { job_id, .. } => {
                        index.ends.insert(job_id.as_str(), at);
                    }
                    Entry::JobStart { job_id, .. } | Entry::ChildStart { job_id, .. } => {
                        index.starts.insert(job_id.as_str(), at);
                    }
                    _ => {}
                }
            }
            index
        })
    }

    fn deliverable(&self, msg_ref: &str) -> Option<ChatDeliverable> {
        let job_id = msg_ref.split('/').next().unwrap_or(msg_ref);
        let index = self.index();
        let Entry::JobEnd {
            text,
            is_error,
            partial,
            note,
            exit,
            value,
            ..
        } = &self.entries[*index.ends.get(msg_ref)?].entry
        else {
            unreachable!()
        };
        match &self.entries[*index.starts.get(job_id)?].entry {
            Entry::JobStart { command, .. } => Some(ChatDeliverable::Command {
                job_id: job_id.to_owned(),
                command: command.clone(),
                exit: exit.as_ref().map(command_exit),
                is_error: *is_error,
            }),
            Entry::ChildStart { agent, team, .. } => Some(ChatDeliverable::Child {
                job_id: job_id.to_owned(),
                agent: agent.clone(),
                team: team.as_ref().map(team_ref),
                note: *note,
                partial: *partial,
                is_error: *is_error,
                text: text.clone(),
                value: value.clone(),
            }),
            _ => unreachable!(),
        }
    }
}

fn deliverables(jobs: &JobLookup<'_>, refs: &[String]) -> Vec<ChatDeliverable> {
    refs.iter()
        .filter_map(|job_ref| jobs.deliverable(job_ref))
        .collect()
}

fn job_refs(details: Option<&serde_json::Value>, delivered_jobs: &[String]) -> Vec<String> {
    let from_details: Vec<String> = details
        .and_then(|d| d.get("jobs"))
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if from_details.is_empty() {
        delivered_jobs.to_vec()
    } else {
        from_details
    }
}

fn custom_extension_id(custom_type: &str) -> Option<&str> {
    let (id, local) = custom_type.split_once(':')?;
    (!id.is_empty() && !local.is_empty()).then_some(id)
}

fn notice_from_custom(
    jobs: &JobLookup<'_>,
    custom_type: &str,
    text: String,
    details: Option<&serde_json::Value>,
    delivered_jobs: &[String],
    extension_id: Option<&str>,
) -> ChatNotice {
    if let Some(id) = extension_id.or_else(|| custom_extension_id(custom_type)) {
        return ChatNotice::Extension {
            id: id.to_owned(),
            text,
        };
    }
    match custom_type {
        "job_result" => {
            let refs = job_refs(details, delivered_jobs);
            let deliverables = deliverables(jobs, &refs);
            ChatNotice::JobWake {
                jobs: refs,
                deliverables,
            }
        }
        "turn_budget" => ChatNotice::TurnBudget { text },
        "yield_nudge" => ChatNotice::YieldNudge { text },
        _ => ChatNotice::Host { text },
    }
}

fn submission<'a>(value: &Entry, entries: &'a [SeqEntry]) -> Option<&'a Entry> {
    let Entry::Message {
        submission_id: Some(id),
        ..
    } = value
    else {
        return None;
    };
    entries.iter().find_map(|seq| {
        if let Entry::Submission { submission_id, .. } = &seq.entry {
            (submission_id == id).then_some(&seq.entry)
        } else {
            None
        }
    })
}

fn anchor<'a>(value: &'a Entry, by_id: &HashMap<&'a str, &'a Entry>) -> Option<&'a str> {
    let mut parent = value.parent_id();
    // A corrupt cyclic parent chain must not hang a transcript conversion.
    for _ in 0..=by_id.len() {
        let id = parent?;
        let Some(ancestor) = by_id.get(id) else {
            return Some(id);
        };
        match ancestor {
            // Navigation leaves preserve their parent's position, not a new edit anchor.
            Entry::Leaf { .. }
            | Entry::Submission { .. }
            | Entry::SubmissionWithdrawn { .. }
            | Entry::RunStart { .. }
            | Entry::RunResumed { .. }
            | Entry::AgentChange { .. }
            | Entry::ModeChange { .. }
            | Entry::JobStart { .. }
            | Entry::ChildStart { .. }
            | Entry::JobEnd { .. }
            | Entry::Message {
                message: AgentMessage::Llm(Message::System(_)),
                ..
            } => {
                parent = ancestor.parent_id();
            }
            Entry::Message { .. }
            | Entry::ThinkingLevelChange { .. }
            | Entry::ServiceTierChange { .. }
            | Entry::ModelChange { .. }
            | Entry::Usage { .. }
            | Entry::Compaction { .. }
            | Entry::BranchSummary { .. }
            | Entry::Custom { .. }
            | Entry::CustomMessage { .. }
            | Entry::ContextEdit { .. }
            | Entry::Label { .. }
            | Entry::SessionInfo { .. }
            | Entry::RunEnd { .. }
            | Entry::Unknown(_) => return Some(id),
        }
    }
    None
}

fn moved_blocks(entries: &[SeqEntry], at: usize) -> u32 {
    let Entry::ContextEdit {
        target_id,
        replacement: Some(replacement),
        ..
    } = &entries[at].entry
    else {
        return 0;
    };
    let mut parent = entries[at].entry.parent_id();
    for seq in entries[..at].iter().rev() {
        if seq.entry.id() != parent {
            continue;
        }
        parent = seq.entry.parent_id();
        let previous = if let Entry::ContextEdit {
            target_id: previous_target,
            replacement: Some(previous),
            ..
        } = &seq.entry
        {
            if previous_target != target_id {
                continue;
            }
            std::borrow::Cow::Borrowed(&previous.content)
        } else if seq.entry.id() == Some(target_id.as_str()) {
            let content = match &seq.entry {
                Entry::Message { message, .. } => match message {
                    AgentMessage::Llm(Message::User(value)) => serde_json::to_value(&value.content),
                    AgentMessage::Llm(Message::Assistant(value)) => {
                        serde_json::to_value(&value.content)
                    }
                    AgentMessage::Llm(Message::ToolResult(value)) => {
                        serde_json::to_value(&value.content)
                    }
                    AgentMessage::Llm(Message::System(_)) => return 0,
                    AgentMessage::Custom(value) => serde_json::to_value(&value.content),
                },
                Entry::CustomMessage { content, .. } => serde_json::to_value(content),
                Entry::ThinkingLevelChange { .. }
                | Entry::ServiceTierChange { .. }
                | Entry::ModelChange { .. }
                | Entry::Usage { .. }
                | Entry::Compaction { .. }
                | Entry::BranchSummary { .. }
                | Entry::Custom { .. }
                | Entry::ContextEdit { .. }
                | Entry::Label { .. }
                | Entry::SessionInfo { .. }
                | Entry::Leaf { .. }
                | Entry::RunStart { .. }
                | Entry::RunEnd { .. }
                | Entry::RunResumed { .. }
                | Entry::Submission { .. }
                | Entry::SubmissionWithdrawn { .. }
                | Entry::AgentChange { .. }
                | Entry::ModeChange { .. }
                | Entry::JobStart { .. }
                | Entry::ChildStart { .. }
                | Entry::JobEnd { .. }
                | Entry::Unknown(_) => return 0,
            };
            let Ok(content) = content else {
                return 0;
            };
            std::borrow::Cow::Owned(content)
        } else {
            continue;
        };
        return replacement.content.as_array().map_or(0, |blocks| {
            let old = previous.as_array();
            blocks
                .iter()
                .enumerate()
                .filter(|(index, block)| old.and_then(|blocks| blocks.get(*index)) != Some(*block))
                .count()
                .try_into()
                .unwrap_or(u32::MAX)
        });
    }
    0
}

fn context_moved(entries: &[SeqEntry], at: usize) -> Option<ChatEntryBody> {
    // axo commits one elision as a contiguous, parent-linked batch of replacements.
    // There is no persisted maintenance id: adjacent passes without intervening entries
    // are indistinguishable, so contiguous parent-linked replacements share one row.
    // Omission edits (failed-response recovery) are not recoverable moves.
    let replacement = |entry: &Entry| {
        matches!(
            entry,
            Entry::ContextEdit {
                replacement: Some(_),
                ..
            }
        )
    };
    if at > 0
        && replacement(&entries[at - 1].entry)
        && entries[at].entry.parent_id() == entries[at - 1].entry.id()
    {
        return None;
    }
    let mut blocks = 0u32;
    for index in at..entries.len() {
        if !replacement(&entries[index].entry)
            || (index > at && entries[index].entry.parent_id() != entries[index - 1].entry.id())
        {
            break;
        }
        blocks = blocks.saturating_add(moved_blocks(entries, index));
    }
    (blocks > 0).then_some(ChatEntryBody::ContextMoved { blocks })
}

pub(super) fn entry(entries: &[SeqEntry], at: usize) -> Option<ChatEntry> {
    entry_with(entries, at, &JobLookup::new(entries))
}

pub(super) fn path_entries(entries: &[SeqEntry], path: &[usize]) -> Vec<ChatEntry> {
    let jobs = JobLookup::new(entries);
    path.iter()
        .filter_map(|at| entry_with(entries, *at, &jobs))
        .collect()
}

fn entry_with(entries: &[SeqEntry], at: usize, jobs: &JobLookup<'_>) -> Option<ChatEntry> {
    let value = &entries.get(at)?.entry;
    let base = value.base()?;
    let body = match value {
        Entry::Message {
            message: AgentMessage::Llm(Message::User(user)),
            ..
        } => {
            let submitted = submission(value, entries);
            if let Some(Entry::Submission {
                kind: kind @ (SubmissionKind::Quiet | SubmissionKind::Mail),
                sender,
                ..
            }) = submitted
            {
                let raw = text(&user.content);
                let notice = match kind {
                    SubmissionKind::Mail => {
                        let sender = sender.clone().unwrap_or_else(|| "teammate".into());
                        let prefix =
                            format!("Message from {sender} (reply with send to \"{sender}\"):\n\n");
                        let text = raw.strip_prefix(&prefix).unwrap_or(&raw).to_owned();
                        ChatNotice::Mail { sender, text }
                    }
                    SubmissionKind::Quiet => ChatNotice::Host { text: raw },
                    SubmissionKind::Prompt | SubmissionKind::Steer | SubmissionKind::FollowUp => {
                        unreachable!()
                    }
                };
                ChatEntryBody::Notice { notice }
            } else {
                let by_id: HashMap<_, _> = entries
                    .iter()
                    .filter_map(|seq| Some((seq.entry.id()?, &seq.entry)))
                    .collect();
                let notices: HashSet<&str> = entries
                    .iter()
                    .filter_map(|seq| {
                        if let Entry::Submission {
                            submission_id,
                            kind: SubmissionKind::Quiet | SubmissionKind::Mail,
                            ..
                        } = &seq.entry
                        {
                            Some(submission_id.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                let user_anchor = anchor(value, &by_id);
                let mut siblings: Vec<_> = entries
                    .iter()
                    .filter(|seq| {
                        matches!(
                            &seq.entry,
                            Entry::Message {
                                message: AgentMessage::Llm(Message::User(_)),
                                submission_id,
                                ..
                            } if !submission_id.as_deref().is_some_and(|id| notices.contains(id))
                        ) && anchor(&seq.entry, &by_id) == user_anchor
                    })
                    .collect();
                siblings.sort_by_key(|seq| seq.seq);
                ChatEntryBody::User {
                    parts: match submitted {
                        Some(Entry::Submission { parts, .. }) => parts_to_wire(parts),
                        _ => vec![ChatPart::Text {
                            text: text(&user.content),
                        }],
                    },
                    anchor: user_anchor.map(str::to_owned),
                    siblings: siblings
                        .into_iter()
                        .filter_map(|seq| seq.entry.id().map(str::to_owned))
                        .collect(),
                    from: match submitted {
                        Some(Entry::Submission {
                            provenance: Some(provenance),
                            ..
                        }) => Some(sender(provenance)),
                        _ => None,
                    },
                    keywords: match submitted {
                        Some(Entry::Submission { keywords, .. }) => keywords.clone(),
                        _ => Vec::new(),
                    },
                }
            }
        }
        Entry::Message {
            message: AgentMessage::Llm(Message::Assistant(assistant)),
            ..
        } => ChatEntryBody::Assistant {
            blocks: assistant.content.iter().map(block).collect(),
            model: format!("{}/{}", assistant.provider, assistant.model),
            service_tier: service_tier_to_wire(assistant.service_tier),
            thinking: assistant.thinking_level.map(thinking_to_wire),
            usage: usage(&assistant.usage),
            stop_reason: match assistant.stop_reason {
                StopReason::Stop => ChatStopReason::Stop,
                StopReason::Length => ChatStopReason::Length,
                StopReason::ToolUse => ChatStopReason::ToolUse,
                StopReason::Error => ChatStopReason::Error,
                StopReason::Aborted => ChatStopReason::Aborted,
            },
            error: assistant.error_message.clone(),
            provider_unavailable: assistant
                .failure
                .as_ref()
                .and_then(|failure| failure.code.as_deref())
                == Some("provider_unavailable"),
            duration_ms: assistant.duration_ms,
        },
        Entry::Message {
            message: AgentMessage::Llm(Message::ToolResult(result)),
            delivered_jobs,
            ..
        } => {
            let deliverables = if result.tool_name == "wait" {
                deliverables(jobs, &job_refs(result.details.as_ref(), delivered_jobs))
            } else {
                Vec::new()
            };
            ChatEntryBody::ToolResult {
                call_id: result.tool_call_id.clone(),
                tool_name: result.tool_name.clone(),
                content: content(&result.content),
                details: result.details.clone(),
                is_error: result.is_error,
                duration_ms: result.duration_ms,
                deliverables,
            }
        }
        Entry::Message {
            message: AgentMessage::Custom(custom),
            delivered_jobs,
            ..
        } if custom.display => ChatEntryBody::Notice {
            notice: notice_from_custom(
                jobs,
                &custom.custom_type,
                text(&custom.content),
                custom.details.as_ref(),
                delivered_jobs,
                match submission(value, entries) {
                    Some(Entry::Submission {
                        provenance: Some(axo::SubmissionProvenance::Extension { id }),
                        ..
                    }) => Some(id.as_str()),
                    _ => None,
                },
            ),
        },
        Entry::CustomMessage {
            custom_type,
            content,
            details,
            display: true,
            ..
        } => ChatEntryBody::Notice {
            notice: notice_from_custom(
                jobs,
                custom_type,
                text(content),
                details.as_ref(),
                &[],
                None,
            ),
        },
        Entry::AgentChange { agent, .. } => ChatEntryBody::AgentChange {
            agent: agent.clone(),
        },
        Entry::ModeChange { mode, .. } => ChatEntryBody::ModeChange { mode: mode.clone() },
        Entry::ModelChange {
            provider, model_id, ..
        } => ChatEntryBody::ModelChange {
            model: format!("{provider}/{model_id}"),
        },
        Entry::ThinkingLevelChange {
            thinking_level,
            automatic,
            service_tier,
            ..
        } => ChatEntryBody::ThinkingChange {
            thinking: thinking_to_wire(*thinking_level),
            automatic: *automatic,
            service_tier: service_tier.map(service_tier_to_wire),
        },
        Entry::ServiceTierChange { service_tier, .. } => ChatEntryBody::ServiceTierChange {
            service_tier: service_tier_to_wire(*service_tier),
        },
        Entry::Compaction {
            summary,
            tokens_before,
            ..
        } => ChatEntryBody::Compaction {
            summary: summary.clone(),
            tokens_before: *tokens_before,
        },
        Entry::BranchSummary { summary, .. } => ChatEntryBody::BranchSummary {
            summary: summary.clone(),
        },
        Entry::ContextEdit {
            replacement: Some(_),
            ..
        } => context_moved(entries, at)?,
        Entry::RunEnd {
            outcome, reason, ..
        } if reason.as_deref() != Some("fork") => ChatEntryBody::RunEnd {
            outcome: match outcome {
                RunOutcome::Done => ChatRunOutcome::Done,
                RunOutcome::Aborted => ChatRunOutcome::Aborted,
                RunOutcome::Failed => ChatRunOutcome::Failed,
                RunOutcome::Interrupted => ChatRunOutcome::Interrupted,
            },
            reason: reason.clone(),
        },
        Entry::Message {
            message: AgentMessage::Llm(Message::System(_)) | AgentMessage::Custom(_),
            ..
        }
        | Entry::CustomMessage { .. }
        | Entry::RunEnd { .. }
        | Entry::Usage { .. }
        | Entry::Custom { .. }
        | Entry::ContextEdit { .. }
        | Entry::Label { .. }
        | Entry::SessionInfo { .. }
        | Entry::Leaf { .. }
        | Entry::RunStart { .. }
        | Entry::RunResumed { .. }
        | Entry::Submission { .. }
        | Entry::SubmissionWithdrawn { .. }
        | Entry::JobStart { .. }
        | Entry::ChildStart { .. }
        | Entry::JobEnd { .. }
        | Entry::Unknown(_) => return None,
    };
    Some(ChatEntry {
        id: base.id.to_string(),
        timestamp: base.timestamp.clone(),
        body,
    })
}

fn sender(provenance: &axo::SubmissionProvenance) -> ChatSender {
    match provenance {
        axo::SubmissionProvenance::Agent { name, .. } => ChatSender::Agent { name: name.clone() },
        axo::SubmissionProvenance::Extension { id } => ChatSender::Extension { id: id.clone() },
    }
}

pub(super) fn branch(entries: &[SeqEntry]) -> Vec<usize> {
    axo::store::project(entries, None).path().to_vec()
}

pub(super) fn title(entries: &[SeqEntry]) -> Option<String> {
    title_of(entries.iter().map(|seq| &seq.entry))
}

pub(super) fn title_of<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Option<String> {
    let mut name = None;
    let mut prompt = None;
    for entry in entries {
        if let Entry::SessionInfo {
            name: Some(value), ..
        } = entry
        {
            name = Some(value.as_str());
        } else if let Entry::Submission {
            kind: SubmissionKind::Prompt,
            parts,
            ..
        } = entry
        {
            if prompt.is_none() {
                prompt = Some(parts_text(parts));
            }
        }
    }
    name.or(prompt.as_deref()).and_then(normalize_title)
}

pub(super) fn selection(
    entries: &[SeqEntry],
    path: &[usize],
) -> (Option<String>, ThinkingLevel, ServiceTier) {
    let mut model = None;
    let mut thinking = ThinkingLevel::Off;
    let mut tier = ServiceTier::Standard;
    for &at in path {
        match &entries[at].entry {
            Entry::ModelChange {
                provider, model_id, ..
            } => model = Some((provider, model_id)),
            Entry::ThinkingLevelChange {
                thinking_level,
                service_tier,
                ..
            } => {
                thinking = *thinking_level;
                if let Some(service_tier) = service_tier {
                    tier = *service_tier;
                }
            }
            Entry::ServiceTierChange { service_tier, .. } => tier = *service_tier,
            _ => {}
        }
    }
    (
        model.map(|(provider, id)| format!("{provider}/{id}")),
        thinking,
        tier,
    )
}

pub(super) fn entries_usage(entries: &[SeqEntry]) -> Usage {
    let mut usage = Usage::default();
    for seq in entries {
        if let Some(entry_usage) = seq.entry.usage() {
            usage += entry_usage;
        }
    }
    usage
}

pub(super) fn stats(
    entries: &[SeqEntry],
    path: &[usize],
    fallback_window: u64,
    context: Option<axo::ContextUsage>,
    tree: Option<&Usage>,
) -> ChatStats {
    let context_tokens = path
        .iter()
        .rev()
        .find_map(|&at| {
            if let Entry::Message {
                message: AgentMessage::Llm(Message::Assistant(value)),
                ..
            } = &entries[at].entry
            {
                Some(value.usage.total_tokens)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mut stats = ChatStats {
        context_tokens,
        context_window: fallback_window,
        context_trigger: None,
        context_reserve: None,
        usage: usage(&entries_usage(entries)),
        tree: tree.map(usage),
    };
    if let Some(ctx) = context {
        stats.context_tokens = ctx.tokens;
        stats.context_window = ctx.window;
        stats.context_trigger = Some(ctx.trigger);
        stats.context_reserve = Some(ctx.reserve);
    }
    stats
}

pub(super) fn queued(items: &[axo::QueuedInput]) -> Vec<ChatQueued> {
    items
        .iter()
        .filter_map(|item| {
            let kind = match item.kind {
                SubmissionKind::Prompt | SubmissionKind::Steer => ChatQueueKind::Steer,
                SubmissionKind::FollowUp => ChatQueueKind::FollowUp,
                SubmissionKind::Quiet | SubmissionKind::Mail => return None,
            };
            Some(ChatQueued {
                submission_id: item.submission_id.clone(),
                kind,
                parts: parts_to_wire(&item.parts),
                from: item.provenance.as_ref().map(sender),
            })
        })
        .collect()
}

pub(super) fn ask_block(value: &axo::AskBlock) -> ChatAskBlock {
    match value {
        axo::AskBlock::Markdown(block) => ChatAskBlock::Markdown {
            text: block.text.clone(),
        },
        axo::AskBlock::Choice(block) => ChatAskBlock::Choice {
            id: block.id.clone(),
            question: block.question.clone(),
            optional: block.optional,
            options: block
                .options
                .iter()
                .map(|option| ChatAskOption {
                    value: option.value.clone(),
                    label: option.label.clone(),
                    description: option.description.clone(),
                    recommended: option.recommended,
                })
                .collect(),
            multi: block.multi,
        },
        axo::AskBlock::Text(block) => ChatAskBlock::Text {
            id: block.id.clone(),
            question: block.question.clone(),
            optional: block.optional,
            multiline: block.multiline,
            placeholder: block.placeholder.clone(),
            prefill: block.prefill.clone(),
        },
        axo::AskBlock::Number(block) => ChatAskBlock::Number {
            id: block.id.clone(),
            question: block.question.clone(),
            optional: block.optional,
            integer: block.integer,
            min: block.min.clone(),
            max: block.max.clone(),
            default: block.default.clone(),
        },
    }
}

pub(super) fn answer(value: ChatAnswer) -> axo::Answer {
    axo::Answer {
        id: value.id,
        value: value.value.map(|value| match value {
            ChatAnswerValue::Text(text) => axo::AnswerValue::Text(text),
            ChatAnswerValue::Number(number) => axo::AnswerValue::Number(number),
            ChatAnswerValue::Many(values) => axo::AnswerValue::Many(values),
        }),
        other: value.other,
        note: value.note,
    }
}

pub(super) fn job(info: &axo::JobInfo, closed_session: bool) -> ChatJob {
    let status = if (closed_session
        && matches!(
            info.status,
            axo::JobStatus::Queued | axo::JobStatus::Running | axo::JobStatus::HelpersRunning
        ))
        || (matches!(info.status, axo::JobStatus::Failed)
            && matches!(
                &info.kind,
                axo::JobKind::Command {
                    exit: Some(axo::CommandExit::Interrupted),
                    ..
                }
            )) {
        ChatJobStatus::Interrupted
    } else {
        match info.status {
            axo::JobStatus::Queued => ChatJobStatus::Queued,
            axo::JobStatus::Running => ChatJobStatus::Running,
            axo::JobStatus::HelpersRunning => ChatJobStatus::HelpersRunning,
            axo::JobStatus::Finished => ChatJobStatus::Finished,
            axo::JobStatus::Failed => ChatJobStatus::Failed,
        }
    };
    let kind = match &info.kind {
        axo::JobKind::Command {
            command,
            artifact,
            promotion,
            ended_at,
            exit,
            ..
        } => ChatJobKind::Command {
            command: command.clone(),
            artifact: *artifact,
            promotion: promotion.map(|p| match p {
                axo::Promotion::Async => ChatPromotion::Async,
                axo::Promotion::Timer => ChatPromotion::Timer,
                axo::Promotion::Steer => ChatPromotion::Steer,
                axo::Promotion::Host => ChatPromotion::Host,
            }),
            ended_at: *ended_at,
            exit: exit.as_ref().map(command_exit),
        },
        axo::JobKind::Child {
            session_id,
            agent,
            model,
            team,
            progress,
            unread_mail,
            ..
        } => ChatJobKind::Child {
            session_id: session_id.clone(),
            agent: agent.clone(),
            model: model.clone(),
            team: team.as_ref().map(team_ref),
            progress: progress.as_deref().map(|p| ChatChildProgress {
                turns: p.turns,
                max_turns: p.max_turns,
                tools: p.tools.clone(),
            }),
            unread_mail: *unread_mail,
        },
    };
    ChatJob {
        id: info.id.clone(),
        status,
        call_id: info.call_id.clone(),
        started_at: info.started_at,
        kind,
    }
}

pub(super) fn tool_origins(tools: &[axo::SessionToolInfo]) -> BTreeMap<String, String> {
    tools
        .iter()
        .filter_map(|tool| {
            let origin = ExtensionOrigin::parse(tool.origin.as_deref()?)?;
            Some((tool.name.clone(), origin.id.to_owned()))
        })
        .collect()
}

pub(super) fn session_tools(tools: Vec<axo::SessionToolInfo>) -> Vec<ChatSessionTool> {
    tools
        .into_iter()
        .map(|info| ChatSessionTool {
            name: info.name,
            summary: info.summary,
            description: info.description,
            parameters: info.parameters,
            exposure: match info.exposure {
                axo::Exposure::Direct => ChatToolExposure::Direct,
                axo::Exposure::Deferred => ChatToolExposure::Deferred,
            },
            promoted: info.promoted,
            allowed_in_mode: info.allowed_in_mode,
        })
        .collect()
}

pub(super) fn display_path(path: &Path, folder: &Path) -> String {
    path.strip_prefix(folder)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

pub(super) fn toolbox(session: &axo::Session, folder: &Path) -> ChatToolbox {
    use axo::context::agents::{AgentDef, RunAs};
    use axo::context::modes::BashAccess;

    ChatToolbox {
        skills: session
            .skills()
            .iter()
            .map(|s| ChatSkillInfo {
                name: s.name.clone(),
                description: s.description.clone(),
                path: display_path(&s.path, folder),
                model_invocable: !s.disable_model_invocation,
                argument_hint: s.argument_hint.clone(),
            })
            .collect(),
        agents: session
            .agents()
            .iter()
            .map(|def| match def {
                AgentDef::Agent(spec) => ChatAgentInfo {
                    name: spec.name.clone(),
                    description: spec.description.clone(),
                    team: false,
                    run_as: Some(match spec.run_as {
                        RunAs::Root => ChatAgentRunAs::Root,
                        RunAs::Subagent => ChatAgentRunAs::Subagent,
                        RunAs::Any => ChatAgentRunAs::Any,
                    }),
                    model: spec.model.clone(),
                    tools: spec.tools.clone(),
                    bash_read_only: spec.bash == BashAccess::ReadOnly,
                    max_turns: spec.max_turns,
                    structured_output: spec.output.is_some(),
                    members: Vec::new(),
                    path: spec.path.as_deref().map(|path| display_path(path, folder)),
                },
                AgentDef::Team(spec) => ChatAgentInfo {
                    name: spec.name.clone(),
                    description: spec.description.clone(),
                    team: true,
                    run_as: None,
                    model: None,
                    tools: None,
                    bash_read_only: false,
                    max_turns: None,
                    structured_output: false,
                    members: spec
                        .members
                        .iter()
                        .map(|m| m.roster_name().to_owned())
                        .collect(),
                    path: spec.path.as_deref().map(|path| display_path(path, folder)),
                },
            })
            .collect(),
        modes: session
            .modes()
            .iter()
            .map(|m| ChatModeInfo {
                name: m.name.clone(),
                description: m.description.clone(),
                argument_hint: m.argument_hint.clone(),
                model_invocable: !m.disable_model_invocation,
                model: m.model.clone(),
                tools_allow: m.tools.allow.clone(),
                tools_deny: m.tools.deny.clone(),
                bash_read_only: m.bash == BashAccess::ReadOnly,
                writes: m.writes.clone(),
                result_reviewed: m.result.as_ref().map(|result| result.review),
                path: m.path.as_deref().map(|path| display_path(path, folder)),
                plan: mode_is_plan(m),
            })
            .collect(),
        keywords: session
            .keywords()
            .iter()
            .map(|k| ChatKeywordInfo {
                name: k.name.clone(),
                description: k.description.clone(),
                requires_tools: k.requires_tools.clone(),
                model: k.model.clone(),
                path: k.path.as_deref().map(|path| display_path(path, folder)),
            })
            .collect(),
        diagnostics: session
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axo::store::{EntryBase, EntryId};
    use axo_ai::{AssistantMessage, SystemMessage, UserMessage};

    fn base(id: &str, parent: Option<&str>) -> EntryBase {
        EntryBase {
            id: EntryId(id.into()),
            parent_id: parent.map(|id| EntryId(id.into())),
            timestamp: "2026-10-23T00:00:00.000Z".into(),
        }
    }

    fn log(entries: Vec<Entry>) -> Vec<SeqEntry> {
        entries
            .into_iter()
            .enumerate()
            .map(|(seq, entry)| SeqEntry {
                seq: seq as u64,
                entry,
            })
            .collect()
    }

    #[test]
    fn sender_provenance_is_tagged() {
        for name in [None, Some("scout")] {
            let provenance = axo::SubmissionProvenance::Agent {
                session_id: "parent".into(),
                route: axo::SendKind::Steer,
                name: name.map(str::to_owned),
            };
            assert_eq!(
                serde_json::to_value(sender(&provenance)).unwrap(),
                serde_json::json!({"kind": "agent", "name": name}),
            );
        }
        let provenance = axo::SubmissionProvenance::Extension {
            id: "scratch".into(),
        };
        assert_eq!(
            serde_json::to_value(sender(&provenance)).unwrap(),
            serde_json::json!({"kind": "extension", "id": "scratch"}),
        );
    }

    #[test]
    fn command_catalog_projects_only_extension_origins() {
        for (label, expected, kind) in [
            (
                Some("extension lab/notes"),
                Some("lab/notes"),
                axo::CommandKind::Registered {
                    argument_hint: None,
                },
            ),
            (
                Some("extension scratch (scope abc123)"),
                Some("scratch"),
                axo::CommandKind::Mode {
                    argument_hint: None,
                },
            ),
            (
                Some("project file"),
                None,
                axo::CommandKind::Skill {
                    argument_hint: None,
                },
            ),
            (
                Some("extension scratch (scope )"),
                None,
                axo::CommandKind::Mode {
                    argument_hint: None,
                },
            ),
        ] {
            let projected = command(axo::CommandInfo {
                name: "notes".into(),
                description: String::new(),
                policy: axo::CommandPolicy::Alongside,
                origin: label.map(str::to_owned),
                kind,
            });
            assert_eq!(
                serde_json::to_value(projected).unwrap()["origin"],
                serde_json::json!(expected)
            );
        }
        assert_eq!(
            serde_json::to_value(ChatEvent::ExtensionCatalog {
                commands: Vec::new(),
                tool_origins: BTreeMap::new(),
                diagnostics: Vec::new(),
            })
            .unwrap(),
            serde_json::json!({"type":"extensionCatalog","commands":[],"toolOrigins":{},"diagnostics":[]}),
        );
    }

    #[test]
    fn unknown_usage_cost_is_null_and_propagates() {
        let known = Usage::default();
        let unknown = Usage {
            cost: None,
            ..Usage::default()
        };
        let wire_cost = |value: &Usage| {
            serde_json::to_value(usage(value))
                .unwrap()
                .get("cost")
                .expect("cost is always present on the wire")
                .clone()
        };
        assert_eq!(wire_cost(&known), serde_json::json!(0.0));
        assert_eq!(wire_cost(&unknown), serde_json::Value::Null);
        for (mut first, second) in [(known.clone(), &unknown), (unknown.clone(), &known)] {
            first += second;
            assert_eq!(wire_cost(&first), serde_json::Value::Null);
        }
    }

    fn submission(id: &str, parent: Option<&str>, kind: SubmissionKind) -> Entry {
        Entry::Submission {
            base: base(id, parent),
            submission_id: id.into(),
            kind,
            parts: vec![axo::PromptPart::Text { text: id.into() }],
            keywords: vec![],
            request_id: None,
            sender: None,
            provenance: None,
            custom: None,
        }
    }

    fn user(id: &str, parent: Option<&str>, submission: &str) -> Entry {
        Entry::Message {
            base: base(id, parent),
            submission_id: Some(submission.into()),
            admitted_jobs: vec![],
            delivered_jobs: vec![],
            message: AgentMessage::Llm(Message::User(UserMessage {
                content: vec![UserBlock::Text { text: id.into() }],
                timestamp: 0,
            })),
        }
    }

    fn system(id: &str, parent: Option<&str>) -> Entry {
        Entry::Message {
            base: base(id, parent),
            submission_id: None,
            admitted_jobs: vec![],
            delivered_jobs: vec![],
            message: AgentMessage::Llm(Message::System(SystemMessage {
                content: vec![],
                sections: None,
                tools_added: vec![],
                tools_removed: vec![],
                timestamp: 0,
            })),
        }
    }

    #[test]
    fn content_over_the_message_limit_is_rejected() {
        let part = |len: usize| ChatPart::Content {
            name: "blob.bin".into(),
            bytes: STANDARD.encode(vec![0u8; len]),
        };
        let half = MAX_CONTENT_BYTES / 2 + 1;
        assert!(matches!(
            parts_from_wire(vec![part(half), part(half)]),
            Err(ApiError::InvalidArgument(_))
        ));
        assert!(parts_from_wire(vec![part(MAX_CONTENT_BYTES)]).is_ok());
    }

    #[test]
    fn edits_share_anchor_and_siblings_past_run_bookkeeping() {
        let entries = log(vec![
            Entry::RunEnd {
                base: base("a", None),
                run_id: "old".into(),
                outcome: RunOutcome::Done,
                reason: None,
                usage: None,
                queued: vec![],
            },
            submission("s1", Some("a"), SubmissionKind::Prompt),
            Entry::RunStart {
                base: base("r1", Some("s1")),
                run_id: "one".into(),
            },
            system("system", Some("r1")),
            user("u1", Some("system"), "s1"),
            Entry::Leaf {
                base: base("leaf", Some("a")),
            },
            submission("s2", Some("leaf"), SubmissionKind::Prompt),
            Entry::RunStart {
                base: base("r2", Some("s2")),
                run_id: "two".into(),
            },
            Entry::RunResumed {
                base: base("resume", Some("r2")),
                run_id: "two".into(),
                attempt: 1,
            },
            user("u2", Some("resume"), "s2"),
        ]);
        for at in [4, 9] {
            let ChatEntryBody::User {
                anchor, siblings, ..
            } = entry(&entries, at).unwrap().body
            else {
                panic!("expected user");
            };
            assert_eq!(anchor.as_deref(), Some("a"));
            assert_eq!(siblings, ["u1", "u2"]);
        }
    }

    fn assistant() -> AssistantMessage {
        AssistantMessage {
            content: vec![AssistantBlock::Text {
                text: "reply".into(),
                text_signature: None,
            }],
            api: axo_ai::ApiKind::OpenAiCompletions,
            provider: "test".into(),
            model: "m".into(),
            response_model: None,
            response_id: None,
            provider_thinking_level: None,
            thinking_level: None,
            service_tier: axo_ai::ServiceTier::Standard,
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            failure: None,
            raw_stop_reason: None,
            timestamp: 0,
            duration_ms: None,
        }
    }

    #[test]
    fn provider_unavailable_comes_only_from_the_failure_code() {
        for (code, message, expected) in [
            (None, "provider test is unavailable", false),
            (Some("other"), "provider test is unavailable", false),
            (Some("provider_unavailable"), "Runner interrupted", true),
        ] {
            let entries = log(vec![Entry::Message {
                base: base("failure", None),
                submission_id: None,
                admitted_jobs: vec![],
                delivered_jobs: vec![],
                message: AgentMessage::Llm(Message::Assistant(AssistantMessage {
                    failure: code.map(|code| axo_ai::ProviderFailure {
                        code: Some(code.into()),
                        ..Default::default()
                    }),
                    stop_reason: StopReason::Error,
                    error_message: Some(message.into()),
                    ..assistant()
                })),
            }]);
            let wire = serde_json::to_value(entry(&entries, 0).unwrap()).unwrap();
            assert_eq!(wire["body"]["providerUnavailable"], expected);
            assert_eq!(wire["body"]["error"], message);
        }
    }

    #[test]
    fn path_hides_system_bookkeeping_custom_and_fork_end() {
        let entries = log(vec![
            system("system", None),
            submission("s", Some("system"), SubmissionKind::Prompt),
            Entry::RunStart {
                base: base("r", Some("s")),
                run_id: "run".into(),
            },
            Entry::CustomMessage {
                base: base("hidden", Some("r")),
                custom_type: "hidden".into(),
                content: vec![],
                details: None,
                display: false,
            },
            Entry::ModelChange {
                base: base("model", Some("hidden")),
                provider: "test".into(),
                model_id: "m".into(),
            },
            Entry::Message {
                base: base("reply", Some("model")),
                message: AgentMessage::Llm(Message::Assistant(assistant())),
                submission_id: None,
                admitted_jobs: vec![],
                delivered_jobs: vec![],
            },
            Entry::RunEnd {
                base: base("end", Some("reply")),
                run_id: "run".into(),
                outcome: RunOutcome::Done,
                reason: Some("fork".into()),
                usage: None,
                queued: vec![],
            },
        ]);
        let converted: Vec<_> = branch(&entries)
            .into_iter()
            .filter_map(|at| entry(&entries, at))
            .collect();
        assert_eq!(converted.len(), 2);
        assert!(matches!(
            converted[0].body,
            ChatEntryBody::ModelChange { .. }
        ));
        assert!(matches!(converted[1].body, ChatEntryBody::Assistant { .. }));
    }

    #[test]
    fn displayed_extension_posts_prefer_provenance_and_hide_branch_state() {
        let content = || {
            vec![UserBlock::Text {
                text: "Synced notes.".into(),
            }]
        };
        let mut submitted = submission("submission", None, SubmissionKind::Quiet);
        if let Entry::Submission { provenance, .. } = &mut submitted {
            *provenance = Some(axo::SubmissionProvenance::Extension {
                id: "lab/notes-sync".into(),
            });
        }
        let entries = log(vec![
            submitted,
            Entry::Message {
                base: base("delivered", Some("submission")),
                submission_id: Some("submission".into()),
                admitted_jobs: vec![],
                delivered_jobs: vec![],
                message: AgentMessage::Custom(axo_agent::CustomMessage {
                    custom_type: "other:note".into(),
                    content: content(),
                    display: true,
                    details: None,
                    timestamp: 0,
                }),
            },
            Entry::CustomMessage {
                base: base("namespaced", Some("delivered")),
                custom_type: "lab/notes-sync:note".into(),
                content: content(),
                display: true,
                details: None,
            },
            Entry::CustomMessage {
                base: base("hidden", Some("namespaced")),
                custom_type: "lab/notes-sync:hidden".into(),
                content: content(),
                display: false,
                details: None,
            },
            Entry::Custom {
                base: base("private", Some("hidden")),
                custom_type: "lab/notes-sync:state".into(),
                data: Some(serde_json::json!({"saved": true})),
            },
        ]);
        for at in [1, 2] {
            let wire = serde_json::to_value(entry(&entries, at).unwrap()).unwrap();
            assert_eq!(
                wire["body"]["notice"],
                serde_json::json!({"kind":"extension","id":"lab/notes-sync","text":"Synced notes."}),
            );
        }
        assert!(entry(&entries, 3).is_none());
        assert!(entry(&entries, 4).is_none());
    }

    #[test]
    fn quiet_and_mail_inputs_are_not_editable_user_messages() {
        let cases = [
            (
                SubmissionKind::Quiet,
                None,
                "quiet note",
                ChatNotice::Host {
                    text: "quiet note".into(),
                },
            ),
            (
                SubmissionKind::Mail,
                Some("worker"),
                "Message from worker (reply with send to \"worker\"):\n\nhello",
                ChatNotice::Mail {
                    sender: "worker".into(),
                    text: "hello".into(),
                },
            ),
            (
                SubmissionKind::Mail,
                None,
                "plain message",
                ChatNotice::Mail {
                    sender: "teammate".into(),
                    text: "plain message".into(),
                },
            ),
        ];
        for (kind, sender, body_text, expected) in cases {
            let mut submitted = submission("submission", None, kind);
            if let Entry::Submission { sender: value, .. } = &mut submitted {
                *value = sender.map(str::to_owned);
            }
            let entries = log(vec![
                submitted,
                Entry::Message {
                    base: base("message", Some("submission")),
                    submission_id: Some("submission".into()),
                    admitted_jobs: vec![],
                    delivered_jobs: vec![],
                    message: AgentMessage::Llm(Message::User(UserMessage {
                        content: vec![UserBlock::Text {
                            text: body_text.into(),
                        }],
                        timestamp: 0,
                    })),
                },
            ]);
            assert_eq!(
                entry(&entries, 1).unwrap().body,
                ChatEntryBody::Notice { notice: expected }
            );
        }
    }

    #[test]
    fn job_result_hydrates_command_and_child_deliverables() {
        let entries = log(vec![
            Entry::JobStart {
                base: base("js1", None),
                session_id: "root".into(),
                job_id: "bg_1".into(),
                call_id: "c1".into(),
                command: "cargo test".into(),
                artifact: 42,
                started_at: 1,
                tool: Some("bash".into()),
                cwd: None,
                promotion: None,
            },
            Entry::ChildStart {
                base: base("cs1", Some("js1")),
                session_id: "root".into(),
                job_id: "agent_1".into(),
                call_id: Some("c2".into()),
                child_session_id: "child-1".into(),
                child_path: "child-1.jsonl".into(),
                agent: Some("scout".into()),
                model: "test/m".into(),
                team: Some(axo::TeamRef {
                    id: "team_1".into(),
                    member: "finder".into(),
                    name: Some("review".into()),
                }),
            },
            Entry::JobEnd {
                base: base("je0", Some("cs1")),
                job_id: "bg_1".into(),
                text: "timed out".into(),
                is_error: true,
                partial: false,
                run_id: None,
                note: false,
                exit: Some(axo::CommandExit::TimedOut { after_secs: 30.0 }),
                usage: None,
                value: None,
            },
            Entry::JobEnd {
                base: base("je1", Some("je0")),
                job_id: "agent_1/2".into(),
                text: "mid-run progress".into(),
                is_error: false,
                partial: false,
                run_id: Some("run-1".into()),
                note: true,
                exit: None,
                usage: None,
                value: None,
            },
            Entry::JobEnd {
                base: base("je2", Some("je1")),
                job_id: "agent_1".into(),
                text: "finished".into(),
                is_error: false,
                partial: false,
                run_id: Some("run-1".into()),
                note: false,
                exit: None,
                usage: None,
                value: Some(serde_json::json!({ "ok": true })),
            },
            Entry::CustomMessage {
                base: base("wake", Some("je2")),
                custom_type: "job_result".into(),
                content: vec![UserBlock::Text {
                    text: "jobs finished".into(),
                }],
                details: Some(serde_json::json!({
                    "jobs": ["bg_1", "agent_1/2", "agent_1"]
                })),
                display: true,
            },
        ]);
        assert_eq!(
            entry(&entries, 5).unwrap().body,
            ChatEntryBody::Notice {
                notice: ChatNotice::JobWake {
                    jobs: vec!["bg_1".into(), "agent_1/2".into(), "agent_1".into()],
                    deliverables: vec![
                        ChatDeliverable::Command {
                            job_id: "bg_1".into(),
                            command: "cargo test".into(),
                            exit: Some(ChatCommandExit::TimedOut { after_secs: 30.0 }),
                            is_error: true,
                        },
                        ChatDeliverable::Child {
                            job_id: "agent_1".into(),
                            agent: Some("scout".into()),
                            team: Some(ChatTeamRef {
                                id: "team_1".into(),
                                member: "finder".into(),
                                name: Some("review".into()),
                            }),
                            note: true,
                            partial: false,
                            is_error: false,
                            text: "mid-run progress".into(),
                            value: None,
                        },
                        ChatDeliverable::Child {
                            job_id: "agent_1".into(),
                            agent: Some("scout".into()),
                            team: Some(ChatTeamRef {
                                id: "team_1".into(),
                                member: "finder".into(),
                                name: Some("review".into()),
                            }),
                            note: false,
                            partial: false,
                            is_error: false,
                            text: "finished".into(),
                            value: Some(serde_json::json!({ "ok": true })),
                        },
                    ],
                },
            }
        );
    }

    #[test]
    fn wait_tool_result_populates_deliverables_while_other_tools_leave_empty() {
        let entries = log(vec![
            Entry::JobStart {
                base: base("js1", None),
                session_id: "root".into(),
                job_id: "bg_1".into(),
                call_id: "c1".into(),
                command: "echo hi".into(),
                artifact: 7,
                started_at: 1,
                tool: None,
                cwd: None,
                promotion: None,
            },
            Entry::JobEnd {
                base: base("je1", Some("js1")),
                job_id: "bg_1".into(),
                text: "hi".into(),
                is_error: false,
                partial: false,
                run_id: None,
                note: false,
                exit: Some(axo::CommandExit::Exited { code: 0 }),
                usage: None,
                value: None,
            },
            Entry::Message {
                base: base("tr_wait", Some("je1")),
                submission_id: None,
                admitted_jobs: vec![],
                delivered_jobs: vec!["bg_1".into()],
                message: AgentMessage::Llm(Message::ToolResult(axo_ai::ToolResultMessage {
                    tool_call_id: "call-wait".into(),
                    tool_name: "wait".into(),
                    content: vec![UserBlock::Text {
                        text: "done".into(),
                    }],
                    details: None,
                    is_error: false,
                    timestamp: 0,
                    duration_ms: Some(12),
                })),
            },
            Entry::Message {
                base: base("tr_bash", Some("tr_wait")),
                submission_id: None,
                admitted_jobs: vec![],
                delivered_jobs: vec!["bg_1".into()],
                message: AgentMessage::Llm(Message::ToolResult(axo_ai::ToolResultMessage {
                    tool_call_id: "call-bash".into(),
                    tool_name: "bash".into(),
                    content: vec![UserBlock::Text { text: "ok".into() }],
                    details: Some(serde_json::json!({ "jobs": ["bg_1"] })),
                    is_error: false,
                    timestamp: 0,
                    duration_ms: Some(5),
                })),
            },
        ]);
        let ChatEntryBody::ToolResult { deliverables, .. } = entry(&entries, 2).unwrap().body
        else {
            panic!("expected tool result");
        };
        assert_eq!(
            deliverables,
            vec![ChatDeliverable::Command {
                job_id: "bg_1".into(),
                command: "echo hi".into(),
                exit: Some(ChatCommandExit::Exited { code: 0 }),
                is_error: false,
            }]
        );
        let ChatEntryBody::ToolResult { deliverables, .. } = entry(&entries, 3).unwrap().body
        else {
            panic!("expected tool result");
        };
        assert!(deliverables.is_empty());
    }

    #[test]
    fn agent_and_mode_changes_project_and_are_transparent_to_anchor() {
        let entries = log(vec![
            Entry::RunEnd {
                base: base("a", None),
                run_id: "old".into(),
                outcome: RunOutcome::Done,
                reason: None,
                usage: None,
                queued: vec![],
            },
            submission("s1", Some("a"), SubmissionKind::Prompt),
            user("u1", Some("s1"), "s1"),
            Entry::AgentChange {
                base: base("ac1", Some("a")),
                agent: Some("planner".into()),
            },
            Entry::ModeChange {
                base: base("mc1", Some("ac1")),
                mode: Some("plan".into()),
                restore: None,
            },
            submission("s2", Some("mc1"), SubmissionKind::Prompt),
            user("u2", Some("s2"), "s2"),
            Entry::ModeChange {
                base: base("mc2", Some("u2")),
                mode: None,
                restore: None,
            },
            Entry::AgentChange {
                base: base("ac2", Some("mc2")),
                agent: None,
            },
        ]);
        assert_eq!(
            entry(&entries, 3).unwrap().body,
            ChatEntryBody::AgentChange {
                agent: Some("planner".into())
            }
        );
        assert_eq!(
            entry(&entries, 4).unwrap().body,
            ChatEntryBody::ModeChange {
                mode: Some("plan".into())
            }
        );
        assert_eq!(
            entry(&entries, 7).unwrap().body,
            ChatEntryBody::ModeChange { mode: None }
        );
        assert_eq!(
            entry(&entries, 8).unwrap().body,
            ChatEntryBody::AgentChange { agent: None }
        );
        let ChatEntryBody::User {
            anchor, siblings, ..
        } = entry(&entries, 6).unwrap().body
        else {
            panic!("expected user");
        };
        assert_eq!(anchor.as_deref(), Some("a"));
        assert_eq!(siblings, ["u1", "u2"]);
    }

    #[test]
    fn custom_notices_project_by_type_and_respect_display_flag() {
        let entries = log(vec![
            Entry::CustomMessage {
                base: base("tb", None),
                custom_type: "turn_budget".into(),
                content: vec![UserBlock::Text {
                    text: "2 turns left".into(),
                }],
                details: None,
                display: true,
            },
            Entry::Message {
                base: base("yn", Some("tb")),
                submission_id: None,
                admitted_jobs: vec![],
                delivered_jobs: vec![],
                message: AgentMessage::Custom(axo_agent::CustomMessage {
                    custom_type: "yield_nudge".into(),
                    content: vec![UserBlock::Text {
                        text: "call yield".into(),
                    }],
                    display: true,
                    details: None,
                    timestamp: 0,
                }),
            },
            Entry::CustomMessage {
                base: base("host", Some("yn")),
                custom_type: "custom_banner".into(),
                content: vec![UserBlock::Text {
                    text: "hello".into(),
                }],
                details: None,
                display: true,
            },
            Entry::CustomMessage {
                base: base("hidden1", Some("host")),
                custom_type: "mode_notice".into(),
                content: vec![UserBlock::Text {
                    text: "secret".into(),
                }],
                details: None,
                display: false,
            },
            Entry::Message {
                base: base("hidden2", Some("hidden1")),
                submission_id: None,
                admitted_jobs: vec![],
                delivered_jobs: vec![],
                message: AgentMessage::Custom(axo_agent::CustomMessage {
                    custom_type: "tool_catalog".into(),
                    content: vec![UserBlock::Text {
                        text: "catalog".into(),
                    }],
                    display: false,
                    details: None,
                    timestamp: 0,
                }),
            },
        ]);
        assert_eq!(
            entry(&entries, 0).unwrap().body,
            ChatEntryBody::Notice {
                notice: ChatNotice::TurnBudget {
                    text: "2 turns left".into()
                }
            }
        );
        assert_eq!(
            entry(&entries, 1).unwrap().body,
            ChatEntryBody::Notice {
                notice: ChatNotice::YieldNudge {
                    text: "call yield".into()
                }
            }
        );
        assert_eq!(
            entry(&entries, 2).unwrap().body,
            ChatEntryBody::Notice {
                notice: ChatNotice::Host {
                    text: "hello".into(),
                }
            }
        );
        assert!(entry(&entries, 3).is_none());
        assert!(entry(&entries, 4).is_none());
    }

    #[test]
    fn queue_at_attach_maps_snapshot_queued_in_delivery_order() {
        let input = |id: &str, kind, provenance| axo::QueuedInput {
            submission_id: id.into(),
            kind,
            parts: vec![axo::PromptPart::Text { text: id.into() }],
            custom: None,
            provenance,
        };
        let items = [
            input(
                "steer",
                SubmissionKind::Steer,
                Some(axo::SubmissionProvenance::Agent {
                    session_id: "parent".into(),
                    route: axo::SendKind::Steer,
                    name: Some("scout".into()),
                }),
            ),
            input("quiet", SubmissionKind::Quiet, None),
            input("reopened", SubmissionKind::Prompt, None),
            input("mail", SubmissionKind::Mail, None),
            input(
                "follow",
                SubmissionKind::FollowUp,
                Some(axo::SubmissionProvenance::Extension {
                    id: "scratch".into(),
                }),
            ),
        ];
        assert_eq!(
            serde_json::to_value(queued(&items)).unwrap(),
            serde_json::json!([
                {
                    "submissionId": "steer", "kind": "steer",
                    "parts": [{"type": "text", "text": "steer"}],
                    "from": {"kind": "agent", "name": "scout"}
                },
                {
                    "submissionId": "reopened", "kind": "steer",
                    "parts": [{"type": "text", "text": "reopened"}],
                    "from": null
                },
                {
                    "submissionId": "follow", "kind": "followUp",
                    "parts": [{"type": "text", "text": "follow"}],
                    "from": {"kind": "extension", "id": "scratch"}
                }
            ]),
        );
        assert!(queued(&[]).is_empty());
    }

    #[test]
    fn job_conversion_maps_closed_and_interrupted_statuses() {
        let cmd_kind = |exit: Option<axo::CommandExit>| axo::JobKind::Command {
            command: "echo hi".into(),
            tool: "bash".into(),
            cwd: std::path::PathBuf::from("/tmp"),
            artifact: 7,
            promotion: Some(axo::Promotion::Async),
            promoted_at: Some(10),
            ended_at: None,
            exit,
        };
        let msg = |delivery| axo::JobMessageInfo {
            id: "bg_1".into(),
            note: false,
            is_error: false,
            partial: false,
            run_id: None,
            delivery,
        };
        for active in [
            axo::JobStatus::Queued,
            axo::JobStatus::Running,
            axo::JobStatus::HelpersRunning,
        ] {
            let info = axo::JobInfo {
                id: "bg_1".into(),
                status: active,
                call_id: Some("call_1".into()),
                started_at: 5,
                kind: cmd_kind(None),
                messages: vec![msg(axo::MessageDelivery::Pending)],
            };
            let closed = job(&info, true);
            assert_eq!(closed.status, ChatJobStatus::Interrupted);
            let live = job(&info, false);
            assert_ne!(live.status, ChatJobStatus::Interrupted);
        }
        let finished = axo::JobInfo {
            id: "bg_2".into(),
            status: axo::JobStatus::Finished,
            call_id: None,
            started_at: 5,
            kind: cmd_kind(Some(axo::CommandExit::Exited { code: 0 })),
            messages: vec![msg(axo::MessageDelivery::Delivered)],
        };
        let finished_closed = job(&finished, true);
        assert_eq!(finished_closed.status, ChatJobStatus::Finished);

        let interrupted_failed = axo::JobInfo {
            id: "bg_3".into(),
            status: axo::JobStatus::Failed,
            call_id: None,
            started_at: 5,
            kind: cmd_kind(Some(axo::CommandExit::Interrupted)),
            messages: vec![],
        };
        assert_eq!(
            job(&interrupted_failed, false).status,
            ChatJobStatus::Interrupted
        );
    }
}
