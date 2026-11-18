//! LLM-grounded web search fallback adapters (Anthropic, OpenAI, OpenAI Codex, Google Gemini).

use std::time::Duration;

use axo_ai::auth::codex::BASE_URL as CODEX_BASE_URL;
use axo_ai::Credential;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::providers::{SearchHit, SearchRequest};

const GROUNDED_TIMEOUT: Duration = Duration::from_secs(25);
const SEARCH_INSTRUCTION: &str = "Search the web to answer the user's query concisely. Ground every claim in search results and include exact URLs.";

pub(crate) async fn search_grounded(
    http: &reqwest::Client,
    provider: &str,
    model_override: Option<&str>,
    credential: &Credential,
    req: &SearchRequest,
    cancel: &CancellationToken,
) -> Result<(Option<String>, Vec<SearchHit>), String> {
    let query = format_grounded_query(req);
    let fut = async {
        match provider {
            "anthropic" => search_anthropic(http, credential, model_override, &query, req).await,
            "openai" => search_openai(http, credential, model_override, &query, req).await,
            "openai-codex" => search_codex(http, credential, model_override, &query).await,
            "google" => search_google(http, credential, model_override, &query).await,
            other => Err(format!(
                "grounded search is not supported for provider {other}"
            )),
        }
    };
    tokio::select! {
        _ = cancel.cancelled() => Err("Operation aborted".into()),
        res = tokio::time::timeout(GROUNDED_TIMEOUT, fut) => match res {
            Ok(inner) => inner,
            Err(_) => Err(format!("{provider} grounded search timed out")),
        }
    }
}

fn format_grounded_query(req: &SearchRequest) -> String {
    let mut parts = vec![req.query.clone()];
    if let Some(recency) = &req.recency {
        parts.push(format!("(time filter: past {recency})"));
    }
    if !req.domains.is_empty() {
        parts.push(format!("(domains: {})", req.domains.join(", ")));
    }
    parts.join(" ")
}

async fn search_anthropic(
    http: &reqwest::Client,
    credential: &Credential,
    model_override: Option<&str>,
    query: &str,
    req: &SearchRequest,
) -> Result<(Option<String>, Vec<SearchHit>), String> {
    let model = model_override.unwrap_or("claude-haiku-4-5");
    let mut builder = http
        .post("https://api.anthropic.com/v1/messages")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json");
    match credential {
        Credential::ApiKey { key } => {
            builder = builder.header("x-api-key", key);
        }
        Credential::Oauth { access, .. } => {
            builder = builder
                .bearer_auth(access)
                .header("anthropic-beta", "oauth-2025-04-20");
        }
    }
    let mut tool = json!({
        "type": "web_search_20250305",
        "name": "web_search",
        "max_uses": 3
    });
    let allowed: Vec<&str> = req
        .domains
        .iter()
        .filter(|d| !d.starts_with('-'))
        .map(String::as_str)
        .collect();
    let blocked: Vec<&str> = req
        .domains
        .iter()
        .filter_map(|d| d.strip_prefix('-'))
        .collect();
    if !allowed.is_empty() {
        tool["allowed_domains"] = json!(allowed);
    } else if !blocked.is_empty() {
        tool["blocked_domains"] = json!(blocked);
    }

    let body = json!({
        "model": model,
        "max_tokens": 1024,
        "system": SEARCH_INSTRUCTION,
        "tools": [tool],
        "messages": [{ "role": "user", "content": query }]
    });
    let resp = builder
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Anthropic HTTP {status}: {}", clip_err(&text)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut answer_parts = Vec::new();
    let mut hits = Vec::new();
    let mut saw_search = false;
    if let Some(blocks) = value["content"].as_array() {
        for block in blocks {
            match block["type"].as_str() {
                Some("server_tool_use") if block["name"] == "web_search" => saw_search = true,
                Some("web_search_tool_result") => {
                    saw_search = true;
                    if let Some(items) = block["content"].as_array() {
                        for item in items {
                            if item["type"] == "web_search_result" {
                                if let Some(url) = item["url"].as_str() {
                                    hits.push(SearchHit {
                                        title: item["title"].as_str().unwrap_or(url).to_owned(),
                                        url: url.to_owned(),
                                        snippet: None,
                                        published_date: item["page_age"]
                                            .as_str()
                                            .map(str::to_owned),
                                    });
                                }
                            }
                        }
                    }
                }
                Some("text") => {
                    if let Some(t) = block["text"].as_str() {
                        answer_parts.push(t.to_owned());
                    }
                    if let Some(cites) = block["citations"].as_array() {
                        for cite in cites {
                            if let Some(url) = cite["url"].as_str() {
                                push_unique_hit(
                                    &mut hits,
                                    cite["title"].as_str().unwrap_or(url),
                                    url,
                                    cite["cited_text"].as_str(),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let answer = answer_parts.join("\n").trim().to_owned();
    if !saw_search && hits.is_empty() {
        return Err("Anthropic returned a completion without executing web_search".into());
    }
    Ok((
        (!answer.is_empty()).then_some(answer),
        hits.into_iter().take(req.limit).collect(),
    ))
}

async fn search_openai(
    http: &reqwest::Client,
    credential: &Credential,
    model_override: Option<&str>,
    query: &str,
    req: &SearchRequest,
) -> Result<(Option<String>, Vec<SearchHit>), String> {
    let token = match credential {
        Credential::ApiKey { key } => key.as_str(),
        Credential::Oauth { access, .. } => access.as_str(),
    };
    let model = model_override.unwrap_or("gpt-4o-mini");
    let body = json!({
        "model": model,
        "instructions": SEARCH_INSTRUCTION,
        "input": query,
        "tools": [{ "type": "web_search" }]
    });
    let resp = http
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("OpenAI HTTP {status}: {}", clip_err(&text)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    parse_responses_payload("openai", &value, req.limit)
}

async fn search_codex(
    http: &reqwest::Client,
    credential: &Credential,
    model_override: Option<&str>,
    query: &str,
) -> Result<(Option<String>, Vec<SearchHit>), String> {
    let (token, account_id) = match credential {
        Credential::Oauth { access, extra, .. } => (
            access.as_str(),
            extra.get("accountId").and_then(Value::as_str),
        ),
        Credential::ApiKey { key } => (key.as_str(), None),
    };
    let base = CODEX_BASE_URL.trim_end_matches('/');
    let url = format!("{base}/codex/responses");
    let model = model_override.unwrap_or("gpt-5.1-codex-mini");
    let body = json!({
        "model": model,
        "store": false,
        "stream": true,
        "instructions": SEARCH_INSTRUCTION,
        "input": [{ "role": "user", "content": [{ "type": "input_text", "text": query }] }],
        "tools": [{ "type": "web_search" }],
        "include": ["web_search_call.action.sources"]
    });
    let mut builder = http
        .post(&url)
        .bearer_auth(token)
        .header("OpenAI-Beta", "responses=experimental")
        .header("originator", "axo")
        .header("accept", "text/event-stream")
        .header("content-type", "application/json");
    if let Some(id) = account_id.filter(|id| !id.is_empty()) {
        builder = builder.header("chatgpt-account-id", id);
    }
    let resp = builder
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let sse = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Codex HTTP {status}: {}", clip_err(&sse)));
    }

    let mut completed_response: Option<Value> = None;
    let mut deltas = String::new();
    let mut saw_search = false;
    let mut hits = Vec::new();
    for line in sse.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        match event["type"].as_str() {
            Some("response.web_search_call.completed")
            | Some("response.web_search_call.in_progress") => saw_search = true,
            Some("response.output_text.delta") => {
                if let Some(delta) = event["delta"].as_str() {
                    deltas.push_str(delta);
                }
            }
            Some("response.output_text.annotation.added") => {
                if let Some(url) = event["annotation"]["url"].as_str() {
                    push_unique_hit(
                        &mut hits,
                        event["annotation"]["title"].as_str().unwrap_or(url),
                        url,
                        None,
                    );
                }
            }
            Some("response.completed") | Some("response.done") => {
                completed_response = Some(event["response"].clone());
            }
            _ => {}
        }
    }
    if let Some(resp_obj) = completed_response {
        return parse_responses_payload("openai-codex", &resp_obj, 10);
    }
    if !saw_search && hits.is_empty() {
        return Err("Codex returned a completion without executing web_search".into());
    }
    let answer = deltas.trim().to_owned();
    Ok(((!answer.is_empty()).then_some(answer), hits))
}

fn parse_responses_payload(
    provider: &str,
    value: &Value,
    limit: usize,
) -> Result<(Option<String>, Vec<SearchHit>), String> {
    let mut answer_parts = Vec::new();
    let mut hits = Vec::new();
    let mut saw_search = false;
    if let Some(outputs) = value["output"].as_array() {
        for item in outputs {
            match item["type"].as_str() {
                Some("web_search_call") => {
                    saw_search = true;
                    if let Some(sources) = item["action"]["sources"].as_array() {
                        for src in sources {
                            if let Some(url) = src["url"].as_str() {
                                push_unique_hit(
                                    &mut hits,
                                    src["title"].as_str().unwrap_or(url),
                                    url,
                                    None,
                                );
                            }
                        }
                    }
                }
                Some("message") => {
                    if let Some(contents) = item["content"].as_array() {
                        for part in contents {
                            if part["type"] == "output_text" {
                                if let Some(t) = part["text"].as_str() {
                                    answer_parts.push(t.to_owned());
                                }
                                if let Some(anns) = part["annotations"].as_array() {
                                    for ann in anns {
                                        if ann["type"] == "url_citation" {
                                            if let Some(url) = ann["url"].as_str() {
                                                push_unique_hit(
                                                    &mut hits,
                                                    ann["title"].as_str().unwrap_or(url),
                                                    url,
                                                    None,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if !saw_search && hits.is_empty() {
        return Err(format!(
            "{provider} returned a completion without executing web_search"
        ));
    }
    let answer = answer_parts.join("\n").trim().to_owned();
    Ok((
        (!answer.is_empty()).then_some(answer),
        hits.into_iter().take(limit).collect(),
    ))
}

async fn search_google(
    http: &reqwest::Client,
    credential: &Credential,
    model_override: Option<&str>,
    query: &str,
) -> Result<(Option<String>, Vec<SearchHit>), String> {
    let model = model_override.unwrap_or("gemini-2.5-flash");
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let mut builder = http.post(&url).header("content-type", "application/json");
    match credential {
        Credential::ApiKey { key } => {
            builder = builder.header("x-goog-api-key", key);
        }
        Credential::Oauth { access, .. } => {
            builder = builder.bearer_auth(access);
        }
    }
    let body = json!({
        "systemInstruction": { "parts": [{ "text": SEARCH_INSTRUCTION }] },
        "contents": [{ "role": "user", "parts": [{ "text": query }] }],
        "tools": [{ "google_search": {} }]
    });
    let resp = builder
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Google HTTP {status}: {}", clip_err(&text)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let candidate = &value["candidates"][0];
    let mut answer_parts = Vec::new();
    if let Some(parts) = candidate["content"]["parts"].as_array() {
        for part in parts {
            if let Some(t) = part["text"].as_str() {
                answer_parts.push(t.to_owned());
            }
        }
    }
    let mut hits = Vec::new();
    if let Some(chunks) = candidate["groundingMetadata"]["groundingChunks"].as_array() {
        for chunk in chunks {
            if let Some(url) = chunk["web"]["uri"].as_str() {
                push_unique_hit(
                    &mut hits,
                    chunk["web"]["title"].as_str().unwrap_or(url),
                    url,
                    None,
                );
            }
        }
    }
    if hits.is_empty() {
        return Err("Google returned no grounding sources".into());
    }
    let answer = answer_parts.join("\n").trim().to_owned();
    Ok(((!answer.is_empty()).then_some(answer), hits))
}

fn push_unique_hit(hits: &mut Vec<SearchHit>, title: &str, url: &str, snippet: Option<&str>) {
    if hits.iter().any(|existing| existing.url == url) {
        return;
    }
    hits.push(SearchHit {
        title: title.to_owned(),
        url: url.to_owned(),
        snippet: snippet.map(str::to_owned),
        published_date: None,
    });
}

fn clip_err(body: &str) -> String {
    body.chars().take(240).collect()
}
