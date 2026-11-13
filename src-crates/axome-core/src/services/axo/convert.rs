use std::collections::{HashMap, HashSet};

use crate::{errors::ApiError, services::history::normalize_title};
use axo::store::{Entry, RunOutcome, SeqEntry, SubmissionKind};
use axo::LiveSnapshot;
use axo_agent::AgentMessage;
use axo_ai::{AssistantBlock, Message, StopReason, ThinkingLevel, Usage, UserBlock};
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

pub(super) fn command(value: axo::CommandInfo) -> Option<ChatCommand> {
    Some(ChatCommand {
        name: value.name,
        description: value.description,
        policy: match value.policy {
            axo::CommandPolicy::Alongside => ChatCommandPolicy::Alongside,
            axo::CommandPolicy::WhenIdle => ChatCommandPolicy::WhenIdle,
            axo::CommandPolicy::Queued => ChatCommandPolicy::Queued,
        },
        kind: match value.kind {
            axo::CommandKind::Builtin { parameters } => ChatCommandKind::Builtin { parameters },
            axo::CommandKind::Skill { argument_hint } => ChatCommandKind::Skill { argument_hint },
            axo::CommandKind::Mode { .. } => return None,
        },
    })
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
        cost: value.cost.total,
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

pub(super) fn is_user(value: &Entry, entries: &[SeqEntry]) -> bool {
    matches!(
        value,
        Entry::Message {
            message: AgentMessage::Llm(Message::User(_)),
            ..
        }
    ) && !matches!(
        submission(value, entries),
        Some(Entry::Submission {
            kind: SubmissionKind::Quiet | SubmissionKind::Mail,
            ..
        })
    )
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
    let value = &entries.get(at)?.entry;
    let base = value.base()?;
    let body = match value {
        Entry::Message {
            message: AgentMessage::Llm(Message::User(user)),
            submission_id,
            ..
        } => {
            if let Some(Entry::Submission {
                kind: kind @ (SubmissionKind::Quiet | SubmissionKind::Mail),
                sender,
                ..
            }) = submission(value, entries)
            {
                let text = text(&user.content);
                return Some(ChatEntry {
                    id: base.id.to_string(),
                    timestamp: base.timestamp.clone(),
                    body: ChatEntryBody::Notice {
                        text: match (kind, sender) {
                            (SubmissionKind::Mail, Some(sender)) => {
                                format!("Mail from {sender}: {text}")
                            }
                            (
                                SubmissionKind::Prompt
                                | SubmissionKind::Steer
                                | SubmissionKind::FollowUp
                                | SubmissionKind::Quiet
                                | SubmissionKind::Mail,
                                _,
                            ) => text,
                        },
                    },
                });
            }
            let by_id: HashMap<_, _> = entries
                .iter()
                .filter_map(|seq| Some((seq.entry.id()?, &seq.entry)))
                .collect();
            let user_anchor = anchor(value, &by_id);
            let mut siblings: Vec<_> = entries
                .iter()
                .filter(|seq| {
                    is_user(&seq.entry, entries) && anchor(&seq.entry, &by_id) == user_anchor
                })
                .collect();
            siblings.sort_by_key(|seq| seq.seq);
            ChatEntryBody::User {
                parts: submission_id
                    .as_ref()
                    .and_then(|id| {
                        entries.iter().find_map(|seq| {
                            if let Entry::Submission {
                                submission_id,
                                parts,
                                ..
                            } = &seq.entry
                            {
                                (submission_id == id).then(|| parts_to_wire(parts))
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or_else(|| {
                        vec![ChatPart::Text {
                            text: text(&user.content),
                        }]
                    }),
                anchor: user_anchor.map(str::to_owned),
                siblings: siblings
                    .into_iter()
                    .filter_map(|seq| seq.entry.id().map(str::to_owned))
                    .collect(),
            }
        }
        Entry::Message {
            message: AgentMessage::Llm(Message::Assistant(assistant)),
            ..
        } => ChatEntryBody::Assistant {
            blocks: assistant.content.iter().map(block).collect(),
            model: format!("{}/{}", assistant.provider, assistant.model),
            usage: usage(&assistant.usage),
            stop_reason: match assistant.stop_reason {
                StopReason::Stop => ChatStopReason::Stop,
                StopReason::Length => ChatStopReason::Length,
                StopReason::ToolUse => ChatStopReason::ToolUse,
                StopReason::Error => ChatStopReason::Error,
                StopReason::Aborted => ChatStopReason::Aborted,
            },
            error: assistant.error_message.clone(),
            duration_ms: assistant.duration_ms,
        },
        Entry::Message {
            message: AgentMessage::Llm(Message::ToolResult(result)),
            ..
        } => ChatEntryBody::ToolResult {
            call_id: result.tool_call_id.clone(),
            tool_name: result.tool_name.clone(),
            content: content(&result.content),
            details: result.details.clone(),
            is_error: result.is_error,
            duration_ms: result.duration_ms,
        },
        Entry::Message {
            message: AgentMessage::Custom(custom),
            ..
        } if custom.display => ChatEntryBody::Notice {
            text: text(&custom.content),
        },
        Entry::CustomMessage {
            content,
            display: true,
            ..
        } => ChatEntryBody::Notice {
            text: text(content),
        },
        Entry::ModelChange {
            provider, model_id, ..
        } => ChatEntryBody::ModelChange {
            model: format!("{provider}/{model_id}"),
        },
        Entry::ThinkingLevelChange { thinking_level, .. } => ChatEntryBody::ThinkingChange {
            thinking: thinking_to_wire(*thinking_level),
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
        | Entry::AgentChange { .. }
        | Entry::ModeChange { .. }
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
        match entry {
            Entry::SessionInfo {
                name: Some(value), ..
            } => name = Some(value.as_str()),
            Entry::Submission {
                kind: SubmissionKind::Prompt,
                parts,
                ..
            } if prompt.is_none() => {
                prompt = Some(parts_text(parts));
            }
            Entry::Message { .. }
            | Entry::ThinkingLevelChange { .. }
            | Entry::ModelChange { .. }
            | Entry::Usage { .. }
            | Entry::Compaction { .. }
            | Entry::BranchSummary { .. }
            | Entry::Custom { .. }
            | Entry::CustomMessage { .. }
            | Entry::ContextEdit { .. }
            | Entry::Label { .. }
            | Entry::SessionInfo { .. }
            | Entry::Leaf { .. }
            | Entry::RunStart { .. }
            | Entry::RunEnd { .. }
            | Entry::RunResumed { .. }
            | Entry::Submission { .. }
            | Entry::AgentChange { .. }
            | Entry::ModeChange { .. }
            | Entry::JobStart { .. }
            | Entry::ChildStart { .. }
            | Entry::JobEnd { .. }
            | Entry::Unknown(_) => {}
        }
    }
    name.or(prompt.as_deref()).and_then(normalize_title)
}

pub(super) fn selection(entries: &[SeqEntry], path: &[usize]) -> (Option<String>, ThinkingLevel) {
    let mut model = None;
    let mut thinking = ThinkingLevel::Off;
    for &at in path {
        match &entries[at].entry {
            Entry::ModelChange {
                provider, model_id, ..
            } => model = Some(format!("{provider}/{model_id}")),
            Entry::ThinkingLevelChange { thinking_level, .. } => thinking = *thinking_level,
            Entry::Message { .. }
            | Entry::Usage { .. }
            | Entry::Compaction { .. }
            | Entry::BranchSummary { .. }
            | Entry::Custom { .. }
            | Entry::CustomMessage { .. }
            | Entry::ContextEdit { .. }
            | Entry::Label { .. }
            | Entry::SessionInfo { .. }
            | Entry::Leaf { .. }
            | Entry::RunStart { .. }
            | Entry::RunEnd { .. }
            | Entry::RunResumed { .. }
            | Entry::Submission { .. }
            | Entry::AgentChange { .. }
            | Entry::ModeChange { .. }
            | Entry::JobStart { .. }
            | Entry::ChildStart { .. }
            | Entry::JobEnd { .. }
            | Entry::Unknown(_) => {}
        }
    }
    (model, thinking)
}

pub(super) fn stats(entries: &[SeqEntry], path: &[usize], context_window: u64) -> ChatStats {
    let context_tokens = path
        .iter()
        .rev()
        .find_map(|&at| match &entries[at].entry {
            Entry::Message {
                message: AgentMessage::Llm(Message::Assistant(value)),
                ..
            } => Some(value.usage.total_tokens),
            Entry::Message { .. }
            | Entry::ThinkingLevelChange { .. }
            | Entry::ModelChange { .. }
            | Entry::Usage { .. }
            | Entry::Compaction { .. }
            | Entry::BranchSummary { .. }
            | Entry::Custom { .. }
            | Entry::CustomMessage { .. }
            | Entry::ContextEdit { .. }
            | Entry::Label { .. }
            | Entry::SessionInfo { .. }
            | Entry::Leaf { .. }
            | Entry::RunStart { .. }
            | Entry::RunEnd { .. }
            | Entry::RunResumed { .. }
            | Entry::Submission { .. }
            | Entry::AgentChange { .. }
            | Entry::ModeChange { .. }
            | Entry::JobStart { .. }
            | Entry::ChildStart { .. }
            | Entry::JobEnd { .. }
            | Entry::Unknown(_) => None,
        })
        .unwrap_or(0);
    let mut result = ChatStats {
        context_tokens,
        context_window,
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        cost: 0.0,
    };
    for seq in entries {
        let usage = match &seq.entry {
            Entry::Message {
                message: AgentMessage::Llm(Message::Assistant(value)),
                ..
            } => &value.usage,
            Entry::Usage { usage, .. } => usage,
            Entry::Compaction {
                usage: Some(usage), ..
            }
            | Entry::BranchSummary {
                usage: Some(usage), ..
            } => usage,
            Entry::Message { .. }
            | Entry::ThinkingLevelChange { .. }
            | Entry::ModelChange { .. }
            | Entry::Compaction { .. }
            | Entry::BranchSummary { .. }
            | Entry::Custom { .. }
            | Entry::CustomMessage { .. }
            | Entry::ContextEdit { .. }
            | Entry::Label { .. }
            | Entry::SessionInfo { .. }
            | Entry::Leaf { .. }
            | Entry::RunStart { .. }
            | Entry::RunEnd { .. }
            | Entry::RunResumed { .. }
            | Entry::Submission { .. }
            | Entry::AgentChange { .. }
            | Entry::ModeChange { .. }
            | Entry::JobStart { .. }
            | Entry::ChildStart { .. }
            | Entry::JobEnd { .. }
            | Entry::Unknown(_) => continue,
        };
        result.input += usage.input;
        result.output += usage.output;
        result.cache_read += usage.cache_read;
        result.cache_write += usage.cache_write;
        result.cost += usage.cost.total;
    }
    result
}

pub(super) fn queue_at_attach(
    entries: &[SeqEntry],
    snapshot: &LiveSnapshot,
) -> Vec<(String, ChatQueued)> {
    let matched: HashSet<_> = entries
        .iter()
        .filter_map(|seq| match &seq.entry {
            Entry::Message {
                submission_id: Some(id),
                ..
            } => Some(id.as_str()),
            Entry::Message { .. }
            | Entry::ThinkingLevelChange { .. }
            | Entry::ModelChange { .. }
            | Entry::Usage { .. }
            | Entry::Compaction { .. }
            | Entry::BranchSummary { .. }
            | Entry::Custom { .. }
            | Entry::CustomMessage { .. }
            | Entry::ContextEdit { .. }
            | Entry::Label { .. }
            | Entry::SessionInfo { .. }
            | Entry::Leaf { .. }
            | Entry::RunStart { .. }
            | Entry::RunEnd { .. }
            | Entry::RunResumed { .. }
            | Entry::Submission { .. }
            | Entry::AgentChange { .. }
            | Entry::ModeChange { .. }
            | Entry::JobStart { .. }
            | Entry::ChildStart { .. }
            | Entry::JobEnd { .. }
            | Entry::Unknown(_) => None,
        })
        .collect();
    let mut steering = Vec::new();
    let mut follow_up = Vec::new();
    for seq in entries {
        if let Entry::Submission {
            submission_id,
            kind,
            parts,
            ..
        } = &seq.entry
        {
            if matched.contains(submission_id.as_str()) {
                continue;
            }
            let (queue, kind) = match kind {
                SubmissionKind::Steer => (&mut steering, ChatQueueKind::Steer),
                SubmissionKind::FollowUp => (&mut follow_up, ChatQueueKind::FollowUp),
                SubmissionKind::Prompt | SubmissionKind::Quiet | SubmissionKind::Mail => continue,
            };
            queue.push((seq.seq, submission_id, kind, parts));
        }
    }
    steering.sort_by_key(|item| item.0);
    follow_up.sort_by_key(|item| item.0);
    let steering_skip = steering
        .len()
        .saturating_sub(snapshot.queued_steering.len());
    let follow_up_skip = follow_up
        .len()
        .saturating_sub(snapshot.queued_follow_up.len());
    let mut queued: Vec<_> = steering
        .into_iter()
        .skip(steering_skip)
        .chain(follow_up.into_iter().skip(follow_up_skip))
        .collect();
    queued.sort_by_key(|item| item.0);
    queued
        .into_iter()
        .map(|(_, id, kind, parts)| {
            (
                id.clone(),
                ChatQueued {
                    kind,
                    parts: parts_to_wire(parts),
                },
            )
        })
        .collect()
}

pub(super) fn question(value: &axo::Question) -> ChatQuestion {
    ChatQuestion {
        id: value.id.clone(),
        question: value.question.clone(),
        header: value.header.clone(),
        options: value
            .options
            .iter()
            .map(|option| ChatQuestionOption {
                label: option.label.clone(),
                description: option.description.clone(),
                preview: option.preview.clone(),
            })
            .collect(),
        multi: value.multi,
        recommended: value.recommended,
    }
}

pub(super) fn answer(value: ChatAnswer) -> axo::Answer {
    axo::Answer {
        id: value.id,
        selected: value.selected,
        custom: value.custom,
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

    fn submission(id: &str, parent: Option<&str>, kind: SubmissionKind) -> Entry {
        Entry::Submission {
            base: base(id, parent),
            submission_id: id.into(),
            kind,
            parts: vec![axo::PromptPart::Text { text: id.into() }],
            request_id: None,
            sender: None,
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

    #[test]
    fn path_hides_system_bookkeeping_custom_and_fork_end() {
        let assistant = AssistantMessage {
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
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            failure: None,
            raw_stop_reason: None,
            timestamp: 0,
            duration_ms: None,
        };
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
                message: AgentMessage::Llm(Message::Assistant(assistant)),
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
    fn quiet_and_mail_inputs_are_not_editable_user_messages() {
        for (kind, sender, expected) in [
            (SubmissionKind::Quiet, None, "message"),
            (
                SubmissionKind::Mail,
                Some("worker"),
                "Mail from worker: message",
            ),
            (SubmissionKind::Mail, None, "message"),
        ] {
            let mut submitted = submission("submission", None, kind);
            if let Entry::Submission { sender: value, .. } = &mut submitted {
                *value = sender.map(str::to_owned);
            }
            let entries = log(vec![
                submitted,
                user("message", Some("submission"), "submission"),
            ]);
            assert!(!is_user(&entries[1].entry, &entries));
            assert!(matches!(
                entry(&entries, 1).unwrap().body,
                ChatEntryBody::Notice { text } if text == expected
            ));
        }
    }

    #[test]
    fn queue_attach_keeps_unmatched_tail_in_sequence_order() {
        let entries = log(vec![
            submission("old-steer", None, SubmissionKind::Steer),
            submission("consumed", None, SubmissionKind::Steer),
            submission("old-follow", None, SubmissionKind::FollowUp),
            submission("follow", None, SubmissionKind::FollowUp),
            submission("steer", None, SubmissionKind::Steer),
            user("delivered", None, "consumed"),
            submission("prompt", None, SubmissionKind::Prompt),
        ]);
        let queued_message = AgentMessage::Llm(Message::User(UserMessage {
            content: vec![],
            timestamp: 0,
        }));
        let mut snapshot = LiveSnapshot {
            run_id: None,
            maintenance: None,
            running: false,
            streaming: None,
            pending_tool_calls: vec![],
            queued_steering: vec![queued_message.clone()],
            queued_follow_up: vec![queued_message],
            messages: vec![],
            pending_questions: vec![],
            pending_results: vec![],
            jobs: vec![],
            usage: Usage::default(),
            tree_usage: Usage::default(),
            model: axo_ai::Model {
                id: "m".into(),
                name: "Test".into(),
                api: axo_ai::ApiKind::OpenAiCompletions,
                provider: "test".into(),
                base_url: String::new(),
                reasoning: false,
                thinking_level_map: None,
                input: vec![],
                cost: Default::default(),
                context_window: 1000,
                max_tokens: 100,
                compat: Default::default(),
            },
            thinking: ThinkingLevel::Off,
            agent: None,
            mode: None,
            context: None,
            auto_paused: false,
        };
        assert_eq!(
            queue_at_attach(&entries, &snapshot),
            vec![
                (
                    "follow".into(),
                    ChatQueued {
                        kind: ChatQueueKind::FollowUp,
                        parts: vec![ChatPart::Text {
                            text: "follow".into()
                        }]
                    }
                ),
                (
                    "steer".into(),
                    ChatQueued {
                        kind: ChatQueueKind::Steer,
                        parts: vec![ChatPart::Text {
                            text: "steer".into()
                        }]
                    }
                ),
            ]
        );
        snapshot.queued_steering.clear();
        snapshot.queued_follow_up.clear();
        assert!(queue_at_attach(&entries, &snapshot).is_empty());
    }
}
