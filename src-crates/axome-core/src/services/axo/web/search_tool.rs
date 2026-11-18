//! `web_search` host tool implementation.

use std::sync::Arc;

use async_trait::async_trait;
use axo::SessionTools;
use axo_agent::{ExecutionMode, Replay, Tool, ToolContext, ToolError, ToolOutput};
use axo_ai::{ToolCall, UserBlock};
use serde::Deserialize;
use serde_json::{json, Value};

use super::providers::{run_search, SearchHit, SearchRequest, SearchResponse};
use super::WebTools;

const MAX_QUERY_CHARS: usize = 400;
const MAX_SNIPPET_CHARS: usize = 600;
const DEFAULT_LIMIT: usize = 5;
const GUIDELINES: &[&str] = &[
    "Use web_search for current releases, versions, docs lookup, and verifying real-world facts; follow up with web_fetch or search_docs to read full pages.",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebSearchArgs {
    query: String,
    #[serde(default)]
    recency: Option<String>,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    limit: Option<u64>,
}

pub(crate) struct WebSearchTool {
    shared: Arc<WebTools>,
    session_tools: Arc<SessionTools>,
    parameters: Value,
}

impl WebSearchTool {
    pub(crate) fn new(shared: Arc<WebTools>, session_tools: Arc<SessionTools>) -> Self {
        Self {
            shared,
            session_tools,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (max 400 characters)."
                    },
                    "recency": {
                        "type": "string",
                        "enum": ["day", "week", "month", "year"],
                        "description": "Optional recency filter."
                    },
                    "domains": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional domain filters (prefix with '-' to exclude, e.g. ['docs.rs', '-reddit.com'])."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "description": "Maximum number of results to return (default 5, max 10)."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for up-to-date information, documentation, releases, and real-world facts. Returns ranked URLs and snippets (and a grounded summary when served by an LLM search backend)."
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
        let ToolContext { call, cancel, .. } = cx;
        let args: WebSearchArgs = serde_json::from_value(Value::Object(call.arguments.clone()))
            .map_err(|e| ToolError(e.to_string()))?;
        let query = args.query.trim();
        if query.is_empty() {
            return Err(ToolError("query must not be empty".into()));
        }
        let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
        let recency = match args.recency.as_deref() {
            Some(r @ ("day" | "week" | "month" | "year")) => Some(r.to_owned()),
            Some(other) => return Err(ToolError(format!("invalid recency filter: {other}"))),
            None => None,
        };
        let limit = args
            .limit
            .map(|n| (n as usize).clamp(1, 10))
            .unwrap_or(DEFAULT_LIMIT);

        let settings = super::settings(&self.shared.dirs, Some(&self.session_tools.cwd)).web_search;
        if !settings.enabled {
            return Err(ToolError("web_search is disabled in settings".into()));
        }

        let session_id = &self.session_tools.schemes.context().session_id;
        let active_provider = self.shared.session_provider(session_id);
        let req = SearchRequest {
            query: query.clone(),
            recency,
            domains: args
                .domains
                .into_iter()
                .map(|d| d.trim().to_owned())
                .filter(|d| !d.is_empty())
                .collect(),
            limit,
            session_id: session_id.clone(),
        };

        let response = run_search(
            &self.shared.http,
            &self.shared.credentials,
            &settings,
            active_provider.as_deref(),
            &req,
            &cancel,
        )
        .await
        .map_err(ToolError)?;

        let text = format_search_output(&response);
        let details = json!({
            "provider": response.provider,
            "query": query,
            "answer": response.answer,
            "hits": response.hits,
            "fallbackFrom": response.fallback_from,
            "notes": response.notes,
        });

        Ok(ToolOutput {
            content: vec![UserBlock::Text { text }],
            details: Some(details),
            ..ToolOutput::default()
        })
    }
}

fn format_search_output(resp: &SearchResponse) -> String {
    let mut parts = Vec::new();
    if !resp.fallback_from.is_empty() {
        let failed: Vec<&str> = resp
            .fallback_from
            .iter()
            .map(|f| f.provider.as_str())
            .collect();
        parts.push(format!(
            "Note: answered by {} after {} failed.",
            resp.provider,
            failed.join(", ")
        ));
    }
    for note in &resp.notes {
        parts.push(format!("Note: {note}"));
    }
    if let Some(answer) = &resp.answer {
        parts.push(answer.clone());
        if !resp.hits.is_empty() {
            parts.push("## Sources".to_owned());
        }
    }
    if resp.hits.is_empty() && resp.answer.is_none() {
        parts.push("No results.".to_owned());
    } else {
        for (idx, hit) in resp.hits.iter().enumerate() {
            parts.push(format_hit(idx + 1, hit));
        }
    }
    parts.join("\n\n")
}

fn format_hit(index: usize, hit: &SearchHit) -> String {
    let date = hit
        .published_date
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| format!(" ({d})"))
        .unwrap_or_default();
    let mut out = format!("[{index}] {}{date}\n{}", hit.title, hit.url);
    if let Some(snippet) = hit
        .snippet
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let clipped: String = snippet.chars().take(MAX_SNIPPET_CHARS).collect();
        out.push('\n');
        out.push_str(&clipped);
    }
    out
}
