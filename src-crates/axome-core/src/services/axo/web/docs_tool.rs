//! `search_docs` host tool implementation over the shared FTS5 `DocsCache`.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use axo::SessionTools;
use axo_agent::{ExecutionMode, Replay, Tool, ToolContext, ToolError, ToolOutput};
use axo_ai::{ToolCall, UserBlock};
use serde::Deserialize;
use serde_json::{json, Value};

use super::docs_cache::{self, DocSectionHit, DocsOutcome};
use super::WebTools;

const GUIDELINES: &[&str] = &[
    "Use search_docs for large reference manuals, library docs, llms.txt trees, or local/Nix docs instead of reading whole docs into context; omit query to list cached sources or inspect a source's table of contents.",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchDocsArgs {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    refresh: Option<bool>,
}

pub(crate) struct SearchDocsTool {
    shared: Arc<WebTools>,
    session_tools: Arc<SessionTools>,
    parameters: Value,
}

impl SearchDocsTool {
    pub(crate) fn new(shared: Arc<WebTools>, session_tools: Arc<SessionTools>) -> Self {
        Self {
            shared,
            session_tools,
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search terms ranked by BM25 with stemming (dotted/hyphenated identifiers match as phrases, trailing '*' matches prefixes). Omit to list cached sources or show a source's heading TOC."
                    },
                    "source": {
                        "type": "string",
                        "description": "URL (single page or llms.txt), local file/directory, or /nix/store path. Omit to search across all cached sources."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "description": "Maximum sections to return (default 5, max 20)."
                    },
                    "refresh": {
                        "type": "boolean",
                        "description": "Force re-indexing source even if within TTL / unchanged."
                    }
                },
                "additionalProperties": false
            }),
        }
    }
}

#[async_trait]
impl Tool for SearchDocsTool {
    fn name(&self) -> &str {
        "search_docs"
    }

    fn description(&self) -> &str {
        "Ranked full-text search over cached documentation (URLs, llms.txt doc trees, local Markdown/HTML files, and /nix/store manuals). Returns matching sections with heading path, doc:// or file:start-end references for read, and web #anchor links. Omit query to list cached sources or browse a source's heading TOC."
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
        let args: SearchDocsArgs = serde_json::from_value(Value::Object(call.arguments.clone()))
            .map_err(|e| ToolError(e.to_string()))?;

        let settings = super::settings(&self.shared.dirs, Some(&self.session_tools.cwd));
        if !settings.docs.enabled {
            return Err(ToolError("search_docs is disabled in settings".into()));
        }

        let limit = args.limit.map(|n| (n as usize).clamp(1, 20)).unwrap_or(5);
        let session_id = &self.session_tools.schemes.context().session_id;

        let source = args
            .source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                if docs_cache::is_http_url(s) || Path::new(s).is_absolute() {
                    s.to_owned()
                } else {
                    self.session_tools
                        .resolve_path(s)
                        .to_string_lossy()
                        .into_owned()
                }
            });
        let outcome = self
            .shared
            .docs
            .search(
                &settings.web_search,
                &settings.docs,
                args.query.as_deref(),
                source.as_deref(),
                limit,
                args.refresh.unwrap_or(false),
                session_id,
                &cancel,
            )
            .await
            .map_err(|e| ToolError(e.to_string()))?;

        let (text, details) = render_outcome(&outcome, args.query.as_deref());
        Ok(ToolOutput {
            content: vec![UserBlock::Text { text }],
            details: Some(details),
            ..ToolOutput::default()
        })
    }
}

fn render_outcome(outcome: &DocsOutcome, query: Option<&str>) -> (String, Value) {
    match outcome {
        DocsOutcome::List { cached, configured } => {
            let mut lines = Vec::new();
            if cached.is_empty() {
                lines.push("No documentation sources are indexed yet.".to_owned());
            } else {
                lines.push(format!("Indexed documentation sources ({}):", cached.len()));
                for src in cached {
                    let stale_tag = if src.stale { " [stale]" } else { "" };
                    lines.push(format!(
                        "- {} ({} sections, {}, {}){stale_tag}",
                        src.key, src.sections, src.kind, src.doc_path
                    ));
                }
            }
            if !configured.is_empty() {
                lines.push(String::new());
                lines.push("Configured sources (pass `source` to index lazily):".to_owned());
                for src in configured {
                    lines.push(format!("- {src}"));
                }
            }
            let details = json!({
                "mode": "list",
                "sources": cached,
                "configured": configured,
            });
            (lines.join("\n"), details)
        }
        DocsOutcome::Toc {
            source,
            note,
            entries,
        } => {
            let mut lines = Vec::new();
            if let Some(n) = note {
                lines.push(n.clone());
                lines.push(String::new());
            }
            lines.push(format!(
                "Table of contents for {source} ({} sections shown):",
                entries.len()
            ));
            for entry in entries {
                let loc = format!("{}:{}-{}", entry.display_path, entry.start, entry.end);
                lines.push(format!("- {} ({loc})", entry.crumb));
            }
            let details_entries: Vec<Value> = entries
                .iter()
                .map(|e| {
                    json!({
                        "crumb": e.crumb,
                        "path": e.display_path,
                        "docUrl": doc_url(&e.display_path),
                        "startLine": e.start,
                        "endLine": e.end,
                    })
                })
                .collect();
            let details = json!({
                "mode": "toc",
                "source": source,
                "entries": details_entries,
            });
            (lines.join("\n"), details)
        }
        DocsOutcome::Search {
            source,
            note,
            loose,
            hits,
            known_sources,
        } => {
            let mut lines = Vec::new();
            if let Some(n) = note {
                lines.push(n.clone());
                lines.push(String::new());
            }
            if *loose && !hits.is_empty() {
                lines.push(
                    "No section contains every term; showing sections that match any.".to_owned(),
                );
                lines.push(String::new());
            }
            if hits.is_empty() {
                let q = query.unwrap_or("");
                let scope = source.as_deref().unwrap_or("all indexed sources");
                lines.push(format!("No sections match \"{q}\" in {scope}."));
                if !known_sources.is_empty() {
                    lines.push("Indexed sources:".to_owned());
                    for k in known_sources {
                        lines.push(format!("- {k}"));
                    }
                }
            } else {
                let show_source = source.is_none();
                lines.push(
                    hits.iter()
                        .map(|h| render_hit(h, show_source))
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                );
            }
            let details_hits: Vec<Value> = hits
                .iter()
                .map(|h| {
                    let web_url = web_anchor_url(h);
                    json!({
                        "source": h.source,
                        "crumb": h.crumb,
                        "path": h.display_path,
                        "docUrl": doc_url(&h.display_path),
                        "startLine": h.start,
                        "endLine": h.end,
                        "webUrl": web_url,
                        "snippet": clip_snippet(&h.body),
                    })
                })
                .collect();
            let details = json!({
                "mode": "search",
                "source": source,
                "loose": loose,
                "hits": details_hits,
            });
            (lines.join("\n"), details)
        }
    }
}

fn doc_url(display_path: &str) -> Option<&str> {
    display_path.starts_with("doc://").then_some(display_path)
}

fn web_anchor_url(hit: &DocSectionHit) -> Option<String> {
    let base = hit.web_url.as_deref()?;
    let clean = base.split('#').next().unwrap_or(base);
    if hit.anchor.is_empty() {
        Some(clean.to_owned())
    } else {
        Some(format!("{clean}#{}", hit.anchor))
    }
}

fn render_hit(hit: &DocSectionHit, show_source: bool) -> String {
    let loc = format!("{}:{}-{}", hit.display_path, hit.start, hit.end);
    let origin = web_anchor_url(hit)
        .or_else(|| (show_source && hit.source != hit.display_path).then(|| hit.source.clone()));
    let header = match origin {
        Some(o) => format!("## {}\n{loc} — {o}", hit.crumb),
        None => format!("## {}\n{loc}", hit.crumb),
    };
    format!("{header}\n{}", clip_snippet(&hit.body))
}

const SNIPPET_CHARS: usize = 900;

fn clip_snippet(body: &str) -> String {
    if body.chars().count() <= SNIPPET_CHARS {
        return body.to_owned();
    }
    let clipped: String = body.chars().take(SNIPPET_CHARS).collect();
    format!("{clipped}\n…")
}
