//! Multi-backend web search and extraction proxy providers with ordered fallback.

use std::collections::HashSet;
use std::time::Duration;

use axo_ai::FileCredentialStore;
use axome_protocol::axo_settings::AxoWebSearchSettings;
use serde::Serialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::auth::{resolve_grounded_auth, resolve_search_key, resolve_searxng_url};
use super::grounded::search_grounded;

const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(25);
const PARALLEL_MCP_URL: &str = "https://search.parallel.ai/mcp";
const DEFAULT_DEDICATED_ORDER: &[&str] = &["searxng", "tavily", "exa", "brave", "perplexity"];
const DEFAULT_GROUNDED_ORDER: &[&str] = &["openai-codex", "anthropic", "openai", "google"];
const EXTRACT_CHAIN: &[&str] = &["parallel", "tavily", "exa"];

#[derive(Debug, Clone)]
pub(crate) struct SearchRequest {
    pub query: String,
    pub recency: Option<String>,
    pub domains: Vec<String>,
    pub limit: usize,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchHit {
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderFailure {
    pub provider: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchResponse {
    pub provider: String,
    pub answer: Option<String>,
    pub hits: Vec<SearchHit>,
    pub fallback_from: Vec<ProviderFailure>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtractResponse {
    pub provider: String,
    pub url: String,
    pub final_url: Option<String>,
    pub title: Option<String>,
    pub markdown: String,
    pub fallback_from: Vec<ProviderFailure>,
}

pub(crate) async fn run_search(
    http: &reqwest::Client,
    credentials: &FileCredentialStore,
    settings: &AxoWebSearchSettings,
    active_provider: Option<&str>,
    req: &SearchRequest,
    cancel: &CancellationToken,
) -> Result<SearchResponse, String> {
    let chain = build_search_chain(settings, active_provider);
    let mut failures = Vec::new();
    let mut tried_any = false;

    for candidate in chain {
        if cancel.is_cancelled() {
            return Err("Operation aborted".into());
        }
        let attempt: Result<(Option<String>, Vec<SearchHit>), String> = match candidate.as_str() {
            "searxng" => {
                let Some(base) = resolve_searxng_url(settings) else {
                    continue;
                };
                tried_any = true;
                timed(cancel, SEARCH_TIMEOUT, search_searxng(http, &base, req))
                    .await
                    .map(|hits| (None, hits))
            }
            id @ ("tavily" | "exa" | "brave" | "perplexity") => {
                let Some(key) = resolve_search_key(credentials, id).await else {
                    continue;
                };
                tried_any = true;
                timed(cancel, SEARCH_TIMEOUT, search_keyed(http, id, &key, req))
                    .await
                    .map(|hits| (None, hits))
            }
            "parallel" => {
                tried_any = true;
                let key = resolve_search_key(credentials, "parallel").await;
                timed(
                    cancel,
                    SEARCH_TIMEOUT,
                    search_parallel(http, key.as_deref(), req),
                )
                .await
                .map(|hits| (None, hits))
            }
            grounded @ ("anthropic" | "openai" | "openai-codex" | "google") => {
                let Some(cred) = resolve_grounded_auth(credentials, http, grounded).await else {
                    continue;
                };
                tried_any = true;
                let model_override = grounded_model_for(settings, grounded);
                search_grounded(
                    http,
                    grounded,
                    model_override.as_deref(),
                    &cred,
                    req,
                    cancel,
                )
                .await
            }
            other => {
                failures.push(ProviderFailure {
                    provider: other.to_owned(),
                    reason: "unknown search provider".into(),
                });
                continue;
            }
        };

        match attempt {
            Ok((answer, mut hits)) => {
                let mut notes = Vec::new();
                if !req.domains.is_empty() && !hits.is_empty() {
                    let (filtered, relaxed) = filter_domains(&hits, &req.domains);
                    hits = filtered;
                    if relaxed {
                        notes.push(format!(
                            "no results matched domain filter ({}); relaxed constraint",
                            req.domains.join(", ")
                        ));
                    }
                }
                if hits.is_empty() && answer.is_none() {
                    failures.push(ProviderFailure {
                        provider: candidate,
                        reason: "returned no results".into(),
                    });
                    continue;
                }
                return Ok(SearchResponse {
                    provider: candidate,
                    answer,
                    hits,
                    fallback_from: failures,
                    notes,
                });
            }
            Err(reason) => {
                if cancel.is_cancelled() {
                    return Err("Operation aborted".into());
                }
                failures.push(ProviderFailure {
                    provider: candidate,
                    reason,
                });
            }
        }
    }

    if !tried_any {
        return Err("No search providers available".into());
    }
    let summary = summarize(&failures);
    Err(format!("All search providers failed: {summary}"))
}

pub(crate) async fn run_extract(
    http: &reqwest::Client,
    credentials: &FileCredentialStore,
    settings: &AxoWebSearchSettings,
    url: &str,
    focus: Option<&str>,
    session_id: &str,
    cancel: &CancellationToken,
) -> Result<ExtractResponse, String> {
    let mut order = Vec::new();
    for configured in &settings.providers {
        if EXTRACT_CHAIN.contains(&configured.as_str()) && !order.contains(&configured.as_str()) {
            order.push(configured.as_str());
        }
    }
    for default in EXTRACT_CHAIN {
        if !order.contains(default) {
            order.push(default);
        }
    }

    let mut failures = Vec::new();
    for provider in order {
        if cancel.is_cancelled() {
            return Err("Operation aborted".into());
        }
        let attempt = match provider {
            "parallel" => {
                let key = resolve_search_key(credentials, "parallel").await;
                timed(
                    cancel,
                    EXTRACT_TIMEOUT,
                    extract_parallel(http, key.as_deref(), url, focus, session_id),
                )
                .await
            }
            "tavily" => {
                let Some(key) = resolve_search_key(credentials, "tavily").await else {
                    continue;
                };
                timed(
                    cancel,
                    EXTRACT_TIMEOUT,
                    extract_tavily(http, &key, url, focus),
                )
                .await
            }
            "exa" => {
                let Some(key) = resolve_search_key(credentials, "exa").await else {
                    continue;
                };
                timed(cancel, EXTRACT_TIMEOUT, extract_exa(http, &key, url, focus)).await
            }
            _ => continue,
        };
        match attempt {
            Ok(mut extracted) => {
                if extracted.markdown.trim().is_empty() {
                    failures.push(ProviderFailure {
                        provider: provider.to_owned(),
                        reason: "returned empty content".into(),
                    });
                    continue;
                }
                extracted.fallback_from = failures;
                return Ok(extracted);
            }
            Err(reason) => {
                if cancel.is_cancelled() {
                    return Err("Operation aborted".into());
                }
                failures.push(ProviderFailure {
                    provider: provider.to_owned(),
                    reason,
                });
            }
        }
    }
    let summary = summarize(&failures);
    Err(format!(
        "All extraction backends failed for {url}: {summary}"
    ))
}

fn summarize(failures: &[ProviderFailure]) -> String {
    failures
        .iter()
        .map(|f| format!("{}: {}", f.provider, f.reason))
        .collect::<Vec<_>>()
        .join("; ")
}

fn build_search_chain(
    settings: &AxoWebSearchSettings,
    active_provider: Option<&str>,
) -> Vec<String> {
    if !settings.providers.is_empty() {
        return settings
            .providers
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let mut chain = Vec::new();
    let mut push = |id: &str| {
        if !chain.iter().any(|existing| existing == id) {
            chain.push(id.to_owned());
        }
    };
    for id in DEFAULT_DEDICATED_ORDER {
        push(id);
    }
    push("parallel");
    if let Some(active) = active_provider {
        if DEFAULT_GROUNDED_ORDER.contains(&active) {
            push(active);
        }
    }
    if let Some(model) = settings.grounded_model.as_deref() {
        if let Some((prov, _)) = model.split_once('/') {
            if DEFAULT_GROUNDED_ORDER.contains(&prov) {
                push(prov);
            }
        }
    }
    for id in DEFAULT_GROUNDED_ORDER {
        push(id);
    }
    chain
}

fn grounded_model_for(settings: &AxoWebSearchSettings, provider: &str) -> Option<String> {
    let raw = settings.grounded_model.as_deref()?.trim();
    if let Some((prov, id)) = raw.split_once('/') {
        (prov == provider && !id.is_empty()).then(|| id.to_owned())
    } else {
        None
    }
}

async fn timed<F, T>(cancel: &CancellationToken, dur: Duration, fut: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    tokio::select! {
        _ = cancel.cancelled() => Err("Operation aborted".into()),
        res = tokio::time::timeout(dur, fut) => match res {
            Ok(inner) => inner,
            Err(_) => Err(format!("request timed out after {}s", dur.as_secs())),
        }
    }
}

async fn parallel_rpc(
    http: &reqwest::Client,
    key: Option<&str>,
    tool: &str,
    mut arguments: Value,
    session_id: &str,
) -> Result<Value, String> {
    if let Some(obj) = arguments.as_object_mut() {
        let sid = if session_id.is_empty() {
            "axome-session".to_owned()
        } else {
            format!("axome-{}", session_id.chars().take(64).collect::<String>())
        };
        obj.insert("session_id".into(), Value::String(sid));
    }
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool,
            "arguments": arguments,
        }
    });
    let mut req = http
        .post(PARALLEL_MCP_URL)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        req = req.bearer_auth(key);
    }
    let resp = req.json(&body).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Parallel HTTP {status}"));
    }
    let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if !data["error"].is_null() {
        let msg = data["error"]["message"]
            .as_str()
            .unwrap_or("Parallel MCP error");
        return Err(msg.to_owned());
    }
    if let Some(structured) = data["result"]
        .get("structuredContent")
        .filter(|v| !v.is_null())
    {
        return Ok(structured.clone());
    }
    if let Some(raw) = data["result"]["content"][0]["text"].as_str() {
        return serde_json::from_str(raw).map_err(|e| format!("Parallel parse error: {e}"));
    }
    Ok(Value::Null)
}

async fn search_parallel(
    http: &reqwest::Client,
    key: Option<&str>,
    req: &SearchRequest,
) -> Result<Vec<SearchHit>, String> {
    let mut q = req.query.clone();
    if let Some(recency) = &req.recency {
        q.push_str(&format!(" past {recency}"));
    }
    let args = json!({
        "objective": req.query,
        "search_queries": [q]
    });
    let data = parallel_rpc(http, key, "web_search", args, &req.session_id).await?;
    let mut hits = Vec::new();
    if let Some(results) = data["results"].as_array() {
        for item in results.iter().take(req.limit) {
            let Some(url) = item["url"].as_str() else {
                continue;
            };
            let title = item["title"].as_str().unwrap_or(url).to_owned();
            let snippet = item["excerpts"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n\n")
                })
                .filter(|s| !s.is_empty());
            hits.push(SearchHit {
                title,
                url: url.to_owned(),
                snippet,
                published_date: item["publish_date"].as_str().map(str::to_owned),
            });
        }
    }
    Ok(hits)
}

async fn extract_parallel(
    http: &reqwest::Client,
    key: Option<&str>,
    url: &str,
    focus: Option<&str>,
    session_id: &str,
) -> Result<ExtractResponse, String> {
    let args = json!({
        "urls": [url],
        "objective": focus.unwrap_or("extract page content"),
        "full_content": true
    });
    let data = parallel_rpc(http, key, "web_fetch", args, session_id).await?;
    let item = data["results"]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| {
            data["errors"][0]["error_type"]
                .as_str()
                .unwrap_or("no content")
                .to_owned()
        })?;
    let markdown = item["full_content"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            item["excerpts"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
        })
        .unwrap_or_default();
    Ok(ExtractResponse {
        provider: "parallel".into(),
        url: url.to_owned(),
        final_url: item["url"].as_str().map(str::to_owned),
        title: item["title"].as_str().map(str::to_owned),
        markdown,
        fallback_from: Vec::new(),
    })
}

async fn send_json(req: reqwest::RequestBuilder, label: &str) -> Result<Value, String> {
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("{label} HTTP {status}"));
    }
    resp.json().await.map_err(|e| e.to_string())
}

fn hit(item: &Value, snippet: &str, date: &str) -> Option<SearchHit> {
    let url = item["url"].as_str()?;
    Some(SearchHit {
        title: item["title"].as_str().unwrap_or(url).to_owned(),
        url: url.to_owned(),
        snippet: item[snippet].as_str().map(str::to_owned),
        published_date: item[date].as_str().map(str::to_owned),
    })
}

async fn search_keyed(
    http: &reqwest::Client,
    id: &str,
    key: &str,
    req: &SearchRequest,
) -> Result<Vec<SearchHit>, String> {
    match id {
        "tavily" => search_tavily(http, key, req).await,
        "exa" => search_exa(http, key, req).await,
        "brave" => search_brave(http, key, req).await,
        "perplexity" => search_perplexity(http, key, req).await,
        _ => unreachable!("not a keyed search provider"),
    }
}

async fn search_tavily(
    http: &reqwest::Client,
    key: &str,
    req: &SearchRequest,
) -> Result<Vec<SearchHit>, String> {
    let (include, exclude) = split_domain_filters(&req.domains);
    let mut body = json!({
        "query": req.query,
        "search_depth": "basic",
        "auto_parameters": false,
        "max_results": req.limit,
        "include_answer": false,
        "include_raw_content": false,
    });
    if let Some(recency) = &req.recency {
        body["time_range"] = json!(recency);
    }
    if !include.is_empty() {
        body["include_domains"] = json!(include);
    }
    if !exclude.is_empty() {
        body["exclude_domains"] = json!(exclude);
    }
    let data = send_json(
        http.post("https://api.tavily.com/search")
            .bearer_auth(key)
            .json(&body),
        "Tavily",
    )
    .await?;
    let hits = data["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(req.limit)
        .filter_map(|item| hit(item, "content", "published_date"))
        .collect();
    Ok(hits)
}

async fn extract_tavily(
    http: &reqwest::Client,
    key: &str,
    url: &str,
    focus: Option<&str>,
) -> Result<ExtractResponse, String> {
    let mut body = json!({
        "urls": [url],
        "extract_depth": "basic",
        "format": "markdown",
        "timeout": 20
    });
    if let Some(focus) = focus {
        body["query"] = json!(focus);
        body["chunks_per_source"] = json!(5);
    }
    let data = send_json(
        http.post("https://api.tavily.com/extract")
            .bearer_auth(key)
            .json(&body),
        "Tavily extract",
    )
    .await?;
    let item = data["results"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| {
            data["failed_results"][0]["error"]
                .as_str()
                .unwrap_or("extract returned no content")
                .to_owned()
        })?;
    let markdown = item["raw_content"].as_str().unwrap_or_default().to_owned();
    Ok(ExtractResponse {
        provider: "tavily".into(),
        url: url.to_owned(),
        final_url: item["url"].as_str().map(str::to_owned),
        title: None,
        markdown,
        fallback_from: Vec::new(),
    })
}

async fn search_exa(
    http: &reqwest::Client,
    key: &str,
    req: &SearchRequest,
) -> Result<Vec<SearchHit>, String> {
    let (include, exclude) = split_domain_filters(&req.domains);
    let mut body = json!({
        "query": req.query,
        "numResults": req.limit,
        "contents": { "highlights": { "maxCharacters": 600 } }
    });
    if !include.is_empty() {
        body["includeDomains"] = json!(include);
    }
    if !exclude.is_empty() {
        body["excludeDomains"] = json!(exclude);
    }
    if let Some(start) = recency_iso_date(req.recency.as_deref()) {
        body["startPublishedDate"] = json!(start);
    }
    let data = send_json(
        http.post("https://api.exa.ai/search")
            .header("x-api-key", key)
            .json(&body),
        "Exa",
    )
    .await?;
    let hits = data["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(req.limit)
        .filter_map(|item| {
            let url = item["url"].as_str()?;
            let snippet = item["highlights"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .filter(|s| !s.is_empty())
                .or_else(|| item["text"].as_str().map(str::to_owned));
            Some(SearchHit {
                title: item["title"].as_str().unwrap_or(url).to_owned(),
                url: url.to_owned(),
                snippet,
                published_date: item["publishedDate"].as_str().map(str::to_owned),
            })
        })
        .collect();
    Ok(hits)
}

async fn extract_exa(
    http: &reqwest::Client,
    key: &str,
    url: &str,
    focus: Option<&str>,
) -> Result<ExtractResponse, String> {
    let body = if let Some(focus) = focus {
        json!({
            "urls": [url],
            "highlights": { "query": focus, "maxCharacters": 8000 }
        })
    } else {
        json!({ "urls": [url], "text": true })
    };
    let data = send_json(
        http.post("https://api.exa.ai/contents")
            .header("x-api-key", key)
            .json(&body),
        "Exa contents",
    )
    .await?;
    let item = data["results"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| "Exa returned no content".to_owned())?;
    let markdown = item["text"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            item["highlights"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
        })
        .unwrap_or_default();
    Ok(ExtractResponse {
        provider: "exa".into(),
        url: url.to_owned(),
        final_url: item["url"].as_str().map(str::to_owned),
        title: item["title"].as_str().map(str::to_owned),
        markdown,
        fallback_from: Vec::new(),
    })
}

async fn search_brave(
    http: &reqwest::Client,
    key: &str,
    req: &SearchRequest,
) -> Result<Vec<SearchHit>, String> {
    let mut query = req.query.clone();
    for domain in &req.domains {
        if let Some(neg) = domain.strip_prefix('-') {
            query.push_str(&format!(" -site:{neg}"));
        } else {
            query.push_str(&format!(" site:{domain}"));
        }
    }
    let mut url =
        Url::parse("https://api.search.brave.com/res/v1/web/search").map_err(|e| e.to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", &query);
        pairs.append_pair("count", &req.limit.to_string());
        if let Some(freshness) = match req.recency.as_deref() {
            Some("day") => Some("pd"),
            Some("week") => Some("pw"),
            Some("month") => Some("pm"),
            Some("year") => Some("py"),
            _ => None,
        } {
            pairs.append_pair("freshness", freshness);
        }
    }
    let data = send_json(
        http.get(url)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", key),
        "Brave",
    )
    .await?;
    let hits = data["web"]["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(req.limit)
        .filter_map(|item| hit(item, "description", "age"))
        .collect();
    Ok(hits)
}

async fn search_perplexity(
    http: &reqwest::Client,
    key: &str,
    req: &SearchRequest,
) -> Result<Vec<SearchHit>, String> {
    let mut body = json!({
        "query": req.query,
        "max_results": req.limit,
    });
    if let Some(recency) = &req.recency {
        body["search_recency_filter"] = json!(recency);
    }
    if !req.domains.is_empty() {
        body["search_domain_filter"] = json!(req.domains);
    }
    let data = send_json(
        http.post("https://api.perplexity.ai/search")
            .bearer_auth(key)
            .json(&body),
        "Perplexity",
    )
    .await?;
    let hits = data["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(req.limit)
        .filter_map(|item| hit(item, "snippet", "date"))
        .collect();
    Ok(hits)
}

async fn search_searxng(
    http: &reqwest::Client,
    base: &str,
    req: &SearchRequest,
) -> Result<Vec<SearchHit>, String> {
    let mut url = Url::parse(base.trim_end_matches('/')).map_err(|e| e.to_string())?;
    if !url.path().ends_with("/search") {
        let path = format!("{}/search", url.path().trim_end_matches('/'));
        url.set_path(&path);
    }
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", &req.query);
        pairs.append_pair("format", "json");
        if let Some(recency) = &req.recency {
            pairs.append_pair("time_range", recency);
        }
    }
    let data = send_json(
        http.get(url).header("Accept", "application/json"),
        "SearXNG",
    )
    .await?;
    let mut seen = HashSet::new();
    let hits = data["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["url"].as_str().is_some_and(|url| seen.insert(url)))
        .filter_map(|item| hit(item, "content", "publishedDate"))
        .take(req.limit)
        .collect();
    Ok(hits)
}

fn split_domain_filters(domains: &[String]) -> (Vec<&str>, Vec<&str>) {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for raw in domains {
        let d = raw.trim();
        if let Some(neg) = d.strip_prefix('-') {
            if !neg.is_empty() {
                exclude.push(neg);
            }
        } else if !d.is_empty() {
            include.push(d);
        }
    }
    (include, exclude)
}

fn host_matches(host: &str, domain: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn filter_domains(hits: &[SearchHit], domains: &[String]) -> (Vec<SearchHit>, bool) {
    let (include, exclude) = split_domain_filters(domains);
    let filtered: Vec<SearchHit> = hits
        .iter()
        .filter(|hit| {
            let Ok(parsed) = Url::parse(&hit.url) else {
                return false;
            };
            let Some(host) = parsed.host_str() else {
                return false;
            };
            if exclude.iter().any(|d| host_matches(host, d)) {
                return false;
            }
            include.is_empty() || include.iter().any(|d| host_matches(host, d))
        })
        .cloned()
        .collect();
    if filtered.is_empty() && !hits.is_empty() {
        (hits.to_vec(), true)
    } else {
        (filtered, false)
    }
}

fn recency_iso_date(recency: Option<&str>) -> Option<String> {
    let days = match recency? {
        "day" => 1,
        "week" => 7,
        "month" => 30,
        "year" => 365,
        _ => return None,
    };
    let dt = chrono::Utc::now() - chrono::Duration::days(days);
    Some(dt.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_filter_includes_subdomains_and_relaxes_when_empty() {
        let hits = vec![
            SearchHit {
                title: "Docs".into(),
                url: "https://sub.docs.rs/foo".into(),
                snippet: None,
                published_date: None,
            },
            SearchHit {
                title: "Other".into(),
                url: "https://example.com/bar".into(),
                snippet: None,
                published_date: None,
            },
        ];
        let (kept, relaxed) = filter_domains(&hits, &["docs.rs".into()]);
        assert!(!relaxed);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].url, "https://sub.docs.rs/foo");

        let (kept_all, relaxed_all) = filter_domains(&hits, &["nomatch.org".into()]);
        assert!(relaxed_all);
        assert_eq!(kept_all.len(), 2);
    }
}
