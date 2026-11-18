//! `web_fetch` host tool implementation (extraction-proxy only, with `local://web/` spillover).

use std::sync::Arc;

use async_trait::async_trait;
use axo::{SessionTools, Target};
use axo_agent::{ExecutionMode, Replay, Tool, ToolContext, ToolError, ToolOutput};
use axo_ai::{ToolCall, UserBlock};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use url::Url;

use super::providers::run_extract;
use super::WebTools;

const MAX_URL_CHARS: usize = 2048;
const MAX_FOCUS_CHARS: usize = 400;
const INLINE_CHAR_LIMIT: usize = 14_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebFetchArgs {
    url: String,
    #[serde(default)]
    focus: Option<String>,
}

pub(crate) struct WebFetchTool {
    shared: Arc<WebTools>,
    session_tools: Arc<SessionTools>,
    parameters: Value,
}

impl WebFetchTool {
    pub(crate) fn new(shared: Arc<WebTools>, session_tools: Arc<SessionTools>) -> Self {
        Self {
            shared,
            session_tools,
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Public http:// or https:// URL to read."
                    },
                    "focus": {
                        "type": "string",
                        "description": "Optional question or topic to extract only the relevant sections."
                    }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Read a web page as clean Markdown via extraction proxies. Optional focus narrows extraction to a specific question; oversized pages save full text to local://web/ for follow-up read or grep."
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

    async fn execute(&self, cx: ToolContext) -> Result<ToolOutput, ToolError> {
        let ToolContext { call, cancel, .. } = cx;
        let args: WebFetchArgs = serde_json::from_value(Value::Object(call.arguments.clone()))
            .map_err(|e| ToolError(e.to_string()))?;
        let raw_url = args.url.trim();
        if raw_url.is_empty() || raw_url.len() > MAX_URL_CHARS {
            return Err(ToolError("invalid url".into()));
        }
        let parsed = Url::parse(raw_url).map_err(|e| ToolError(format!("invalid url: {e}")))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(ToolError("url must use http:// or https://".into()));
        }
        let focus = args
            .focus
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(MAX_FOCUS_CHARS).collect::<String>());

        let settings = super::settings(&self.shared.dirs, Some(&self.session_tools.cwd)).web_search;
        if !settings.enabled {
            return Err(ToolError("web_fetch is disabled in settings".into()));
        }

        let session_id = &self.session_tools.schemes.context().session_id;
        let extracted = run_extract(
            &self.shared.http,
            &self.shared.credentials,
            &settings,
            parsed.as_str(),
            focus.as_deref(),
            session_id,
            &cancel,
        )
        .await
        .map_err(ToolError)?;

        let total_chars = extracted.markdown.chars().count();
        let total_lines = extracted.markdown.lines().count();
        let total_bytes = extracted.markdown.len();

        let (text, saved_local_url, truncated) = if total_chars > INLINE_CHAR_LIMIT {
            let local_url =
                save_spillover(&self.session_tools, parsed.as_str(), &extracted.markdown).await?;
            let preview: String = extracted.markdown.chars().take(INLINE_CHAR_LIMIT).collect();
            let note = match &local_url {
                Some(u) => format!(
                    "{preview}\n\n…\n[Full page saved to {u} ({total_lines} lines, {total_bytes} bytes); use read with offset/limit or grep to inspect more.]"
                ),
                None => format!(
                    "{preview}\n\n…\n[Truncated at {INLINE_CHAR_LIMIT} characters ({total_lines} lines total).]"
                ),
            };
            (note, local_url, true)
        } else {
            (extracted.markdown, None, false)
        };

        let details = json!({
            "provider": extracted.provider,
            "url": extracted.url,
            "finalUrl": extracted.final_url,
            "title": extracted.title,
            "savedLocalUrl": saved_local_url,
            "totalLines": total_lines,
            "totalBytes": total_bytes,
            "truncated": truncated,
            "fallbackFrom": extracted.fallback_from,
        });

        Ok(ToolOutput {
            content: vec![UserBlock::Text { text }],
            details: Some(details),
            ..ToolOutput::default()
        })
    }
}

async fn save_spillover(
    tools: &SessionTools,
    url: &str,
    markdown: &str,
) -> Result<Option<String>, ToolError> {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    let host = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "page".to_owned())
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(24)
        .collect::<String>();
    let slug = format!("{}-{}", host.trim_matches('-'), &hex[..10]);
    let local_url = format!("local://web/{slug}.md");
    let Some(parsed) = tools.schemes.exact(&local_url) else {
        return Ok(None);
    };
    match tools.schemes.resolve(&parsed).await? {
        Target::File {
            path,
            writable: true,
        } => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    ToolError(format!("failed to create {}: {e}", parent.display()))
                })?;
            }
            tokio::fs::write(&path, markdown.as_bytes())
                .await
                .map_err(|e| ToolError(format!("failed to write {}: {e}", path.display())))?;
            Ok(Some(local_url))
        }
        _ => Ok(None),
    }
}
