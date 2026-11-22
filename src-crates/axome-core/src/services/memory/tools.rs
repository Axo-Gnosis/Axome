//! `memory_search`, the one memory tool: everything else about notes is plain file
//! work on `memory://` (see `scheme.rs`).

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use axo::{HostTool, SessionTools};
use axo_agent::{ExecutionMode, Replay, Tool, ToolContext, ToolError, ToolOutput};
use axo_ai::{ToolCall, UserBlock};
use axome_memory::links::Direction;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{FolderMemory, MemoryService, SearchRequest};

const GUIDELINES: &[&str] = &[
    "Long-term memory holds notes from past sessions: decisions and their reasons, project facts, and the user's preferences. Search it when a task depends on history that is not in the repository or this conversation.",
    "Memory records what was true when written. When it conflicts with the current code or the user, those win.",
];

pub fn host_tools(memory: &Arc<MemoryService>, folder: &Path) -> Vec<HostTool> {
    memory.refresh(false);
    let Some(fm) = memory.active(folder) else {
        return Vec::new();
    };
    let memory = Arc::clone(memory);
    vec![HostTool::direct(Arc::new(move |_: &Arc<SessionTools>| {
        Arc::new(SearchTool {
            memory: Arc::clone(&memory),
            fm: fm.clone(),
            parameters: parameters(),
        })
    }))]
}

fn parameters() -> Value {
    let string = json!({"type": "string"});
    let strings = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "type": "object",
        "properties": {
            "query": string,
            "types": {"type": "array", "items": {"type": "string", "enum": ["entity", "observation", "relation"]}},
            "categories": strings,
            "tags": strings,
            "path": {"type": "string", "description": "Bank-relative folder or file prefix."},
            "after": {"type": "string", "description": "ISO date; notes updated after it."},
            "valid_at": {"type": "string", "description": "ISO date or instant; facts valid then."},
            "all_banks": {"type": "boolean"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10}
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

struct SearchTool {
    memory: Arc<MemoryService>,
    fm: FolderMemory,
    parameters: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    path: Option<String>,
    after: Option<String>,
    valid_at: Option<String>,
    #[serde(default)]
    all_banks: bool,
    limit: Option<usize>,
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }
    fn description(&self) -> &str {
        "Search long-term memory notes by keywords and meaning. Each result gives the note's memory:// path and the notes it links to (→) and from (←); read and edit notes through those paths."
    }
    fn parameters(&self) -> &Value {
        &self.parameters
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Parallel
    }
    fn replay(&self, _call: &ToolCall) -> Replay {
        Replay::Safe
    }
    fn prompt_guidelines(&self) -> &[&str] {
        GUIDELINES
    }
    async fn execute(&self, cx: ToolContext) -> Result<ToolOutput, ToolError> {
        self.run(Value::Object(cx.call.arguments.clone()))
            .await
            .map_err(|error| ToolError(error.to_string()))
    }
}

impl SearchTool {
    async fn run(&self, value: Value) -> anyhow::Result<ToolOutput> {
        let args: SearchArgs = serde_json::from_value(value)?;
        anyhow::ensure!(
            args.types
                .iter()
                .all(|t| matches!(t.as_str(), "entity" | "observation" | "relation")),
            "Invalid memory row type"
        );
        let limit = args.limit.unwrap_or(10);
        anyhow::ensure!((1..=50).contains(&limit), "limit must be between 1 and 50");
        let hits = self
            .memory
            .search(
                &self.fm,
                SearchRequest {
                    query: args.query,
                    types: args.types,
                    categories: args.categories,
                    tags: args.tags,
                    path: args.path,
                    after: args.after,
                    valid_at: args.valid_at,
                    all_banks: args.all_banks,
                    limit,
                },
            )
            .await?;
        let text = if hits.is_empty() {
            "No matching notes.".into()
        } else {
            hits.iter()
                .enumerate()
                .map(|(i, hit)| {
                    let mut entry = format!(
                        "{}. {} ({}) [{}{}]\n   {}",
                        i + 1,
                        hit.hit.title,
                        hit.url,
                        hit.hit.row_type,
                        hit.hit
                            .category
                            .as_ref()
                            .map(|c| format!("/{c}"))
                            .unwrap_or_default(),
                        hit.hit.snippet.trim(),
                    );
                    for link in &hit.links {
                        let arrow = match link.direction {
                            Direction::Outgoing => "→",
                            Direction::Incoming => "←",
                        };
                        entry += &format!("\n   {arrow} {} {}", link.relation_type, link.url);
                    }
                    entry
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolOutput {
            content: vec![UserBlock::Text { text }],
            details: Some(json!({"hits": hits, "semantic": self.memory.semantic_ready()})),
            ..ToolOutput::default()
        })
    }
}
