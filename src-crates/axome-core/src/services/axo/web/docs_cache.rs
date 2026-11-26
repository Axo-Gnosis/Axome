//! Persistent SQLite FTS5 documentation cache with HTML->Markdown, Nix/local files, and `llms.txt` support.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axo_ai::FileCredentialStore;
use axome_protocol::axo::AxoDocsSourceInfo;
use axome_protocol::axo_settings::{AxoDocsSettings, AxoWebSearchSettings};
use futures_util::stream::{self, StreamExt};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::html_md::{html_to_markdown, looks_like_html, looks_like_js_shell};
use super::providers::run_extract;

const SCHEMA_VERSION: i64 = 1;
const CHUNK_CHARS: usize = 2000;
const MAX_LOCAL_FILES: usize = 5000;
const LLMS_MAX_PAGES: usize = 50;
const LLMS_MAX_BYTES: usize = 5 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone)]
pub(crate) struct DocSectionHit {
    pub source: String,
    pub display_path: String,
    pub start: u32,
    pub end: u32,
    pub anchor: String,
    pub crumb: String,
    pub body: String,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct DocTocEntry {
    pub crumb: String,
    pub display_path: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone)]
pub(crate) enum DocsOutcome {
    List {
        cached: Vec<AxoDocsSourceInfo>,
        configured: Vec<String>,
    },
    Toc {
        source: String,
        note: Option<String>,
        entries: Vec<DocTocEntry>,
    },
    Search {
        source: Option<String>,
        note: Option<String>,
        loose: bool,
        hits: Vec<DocSectionHit>,
        known_sources: Vec<String>,
    },
}

struct Section {
    crumb: String,
    anchor: String,
    body: String,
    start: u32,
    end: u32,
}

struct ParsedPage {
    display_path: String,
    web_url: Option<String>,
    sections: Vec<Section>,
}

pub(crate) struct DocsCache {
    http: reqwest::Client,
    credentials: Arc<FileCredentialStore>,
    root: PathBuf,
    pages: PathBuf,
    db: Mutex<Option<Connection>>,
}

impl DocsCache {
    pub(crate) fn new(
        root: PathBuf,
        http: reqwest::Client,
        credentials: Arc<FileCredentialStore>,
    ) -> Self {
        let pages = root.join("pages");
        Self {
            http,
            credentials,
            root,
            pages,
            db: Mutex::new(None),
        }
    }

    pub(crate) fn pages_dir(&self) -> PathBuf {
        self.pages.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn search(
        &self,
        web_settings: &AxoWebSearchSettings,
        docs_settings: &AxoDocsSettings,
        query: Option<&str>,
        source: Option<&str>,
        limit: usize,
        refresh: bool,
        session_id: &str,
        cancel: &CancellationToken,
    ) -> Result<DocsOutcome> {
        let query = query.map(str::trim).filter(|s| !s.is_empty());
        let source = source.map(str::trim).filter(|s| !s.is_empty());
        let (key, note) = match source {
            Some(source) => {
                let (key, note) = self
                    .ensure_indexed(
                        web_settings,
                        docs_settings,
                        source,
                        refresh,
                        session_id,
                        cancel,
                    )
                    .await?;
                (Some(key), note)
            }
            None => (None, None),
        };
        match (query, key) {
            (None, None) => {
                let cached = self.list_sources(docs_settings)?;
                let cached_keys: HashSet<_> = cached.iter().map(|s| &s.key).collect();
                let configured = docs_settings
                    .sources
                    .iter()
                    .filter(|s| !cached_keys.contains(s))
                    .cloned()
                    .collect();
                Ok(DocsOutcome::List { cached, configured })
            }
            (None, Some(key)) => {
                self.touch(std::slice::from_ref(&key))?;
                let entries = self.toc(&key, docs_settings)?;
                Ok(DocsOutcome::Toc {
                    source: key,
                    note,
                    entries,
                })
            }
            (Some(q), key) => {
                let (hits, loose, known_sources) =
                    self.query_index(q, key.as_deref(), limit, docs_settings)?;
                if let Some(key) = &key {
                    self.touch(std::slice::from_ref(key))?;
                } else {
                    let touched = hits
                        .iter()
                        .map(|h| &h.source)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.touch(&touched)?;
                }
                Ok(DocsOutcome::Search {
                    source: key,
                    note,
                    loose,
                    hits,
                    known_sources,
                })
            }
        }
    }

    pub(crate) fn status(
        &self,
        settings: &AxoDocsSettings,
    ) -> Result<(Vec<AxoDocsSourceInfo>, u64)> {
        let sources = self.list_sources(settings)?;
        let bytes = dir_size(&self.root);
        Ok((sources, bytes))
    }

    pub(crate) async fn refresh_source(
        &self,
        web_settings: &AxoWebSearchSettings,
        docs_settings: &AxoDocsSettings,
        source: Option<&str>,
    ) -> Result<()> {
        let targets = match source {
            Some(s) => vec![s.to_owned()],
            None => self
                .list_sources(docs_settings)?
                .into_iter()
                .map(|s| s.key)
                .collect(),
        };
        let cancel = CancellationToken::new();
        for key in targets {
            let _ = self
                .ensure_indexed(
                    web_settings,
                    docs_settings,
                    &key,
                    true,
                    "settings-refresh",
                    &cancel,
                )
                .await;
        }
        Ok(())
    }

    pub(crate) fn remove_source(
        &self,
        settings: &AxoDocsSettings,
        source: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn(settings)?;
        match source {
            Some(key) => {
                let slug: Option<String> = conn
                    .query_row(
                        "SELECT slug FROM sources WHERE key = ?1",
                        params![key],
                        |row| row.get(0),
                    )
                    .optional()?;
                drop_source(&conn, &self.pages, key, slug.as_deref())?;
            }
            None => {
                conn.execute("DELETE FROM sections", [])?;
                conn.execute("DELETE FROM sources", [])?;
                let _ = std::fs::remove_dir_all(&self.pages);
                let _ = std::fs::create_dir_all(&self.pages);
            }
        }
        Ok(())
    }

    fn conn(&self, settings: &AxoDocsSettings) -> Result<MappedMutexGuard<'_, Connection>> {
        let mut slot = self.db.lock();
        if slot.is_none() {
            std::fs::create_dir_all(&self.pages)?;
            let conn = Connection::open(self.root.join("index.db"))?;
            conn.busy_timeout(Duration::from_secs(5))?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
            if version != SCHEMA_VERSION {
                conn.execute_batch("DROP TABLE IF EXISTS sections; DROP TABLE IF EXISTS sources;")?;
                let _ = std::fs::remove_dir_all(&self.pages);
                std::fs::create_dir_all(&self.pages)?;
                conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS sources (
                    key TEXT PRIMARY KEY,
                    slug TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    fingerprint TEXT NOT NULL,
                    indexed_at INTEGER NOT NULL,
                    used_at INTEGER NOT NULL
                );
                CREATE VIRTUAL TABLE IF NOT EXISTS sections USING fts5(
                    source UNINDEXED,
                    display_path UNINDEXED,
                    web_url UNINDEXED,
                    start UNINDEXED,
                    end UNINDEXED,
                    anchor UNINDEXED,
                    crumb,
                    body,
                    tokenize = 'porter unicode61'
                );",
            )?;
            prune_expired(&conn, &self.pages, settings.retention_days)?;
            *slot = Some(conn);
        }
        Ok(MutexGuard::map(slot, |slot| {
            slot.as_mut().expect("initialized")
        }))
    }

    fn list_sources(&self, settings: &AxoDocsSettings) -> Result<Vec<AxoDocsSourceInfo>> {
        let conn = self.conn(settings)?;
        let mut stmt = conn.prepare(
            "SELECT s.key, s.slug, s.kind, s.indexed_at,
                        (SELECT COUNT(*) FROM sections sec WHERE sec.source = s.key) AS cnt
                 FROM sources s
                 ORDER BY s.used_at DESC, s.key ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let slug: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let indexed_at: i64 = row.get(3)?;
            let sections: u32 = row.get(4)?;
            let stale = kind != "local" && expired(indexed_at, settings);
            let doc_path = if kind == "url" || kind == "llms" {
                format!("doc://{slug}/")
            } else {
                key.clone()
            };
            Ok(AxoDocsSourceInfo {
                key,
                kind,
                doc_path,
                sections,
                stale,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn touch(&self, keys: &[String]) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let mut guard = self.db.lock();
        let Some(conn) = guard.as_mut() else {
            return Ok(());
        };
        let now = now_ms();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE sources SET used_at = ?1 WHERE key = ?2")?;
            for key in keys {
                stmt.execute(params![now, key])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn toc(&self, key: &str, settings: &AxoDocsSettings) -> Result<Vec<DocTocEntry>> {
        let conn = self.conn(settings)?;
        let mut stmt = conn.prepare(
            "SELECT crumb, display_path, start, end
                 FROM sections WHERE source = ?1 LIMIT 120",
        )?;
        let rows = stmt.query_map(params![key], |row| {
            Ok(DocTocEntry {
                crumb: row.get(0)?,
                display_path: row.get(1)?,
                start: row.get(2)?,
                end: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn query_index(
        &self,
        query: &str,
        key: Option<&str>,
        limit: usize,
        settings: &AxoDocsSettings,
    ) -> Result<(Vec<DocSectionHit>, bool, Vec<String>)> {
        let fts_terms = build_fts_terms(query);
        if fts_terms.is_empty() {
            bail!("Query has no searchable words");
        }
        let conn = self.conn(settings)?;

        let and_query = fts_terms.join(" AND ");
        let mut hits = run_fts_query(&conn, &and_query, key, limit)?;
        let mut loose = false;
        if hits.is_empty() && fts_terms.len() > 1 {
            let or_query = fts_terms.join(" OR ");
            hits = run_fts_query(&conn, &or_query, key, limit)?;
            loose = !hits.is_empty();
        }
        let mut known = Vec::new();
        if hits.is_empty() && key.is_none() {
            let mut stmt = conn.prepare("SELECT key FROM sources ORDER BY key")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            known = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok((hits, loose, known))
    }

    async fn ensure_indexed(
        &self,
        web_settings: &AxoWebSearchSettings,
        docs_settings: &AxoDocsSettings,
        source_raw: &str,
        refresh: bool,
        session_id: &str,
        cancel: &CancellationToken,
    ) -> Result<(String, Option<String>)> {
        let is_url = is_http_url(source_raw);
        let key = if is_url {
            raw_github_url(source_raw)
        } else {
            std::fs::canonicalize(source_raw)
                .map_err(|e| anyhow!("cannot access {source_raw}: {e}"))?
                .to_string_lossy()
                .into_owned()
        };
        let slug = source_slug(&key);

        let existing = {
            let conn = self.conn(docs_settings)?;
            conn.query_row(
                "SELECT fingerprint, indexed_at FROM sources WHERE key = ?1",
                params![&key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
        };

        if is_url {
            if let Some((_, indexed_at)) = existing {
                if !refresh && !expired(indexed_at, docs_settings) {
                    return Ok((key, None));
                }
            }
            match self
                .fetch_and_parse_url(web_settings, &key, &slug, session_id, cancel)
                .await
            {
                Ok((kind, pages, note)) => {
                    let count =
                        self.replace_source(docs_settings, &key, &slug, kind, "", &pages)?;
                    let full_note = Some(match note {
                        Some(extra) => format!("Indexed {key}: {count} sections ({extra})"),
                        None => format!("Indexed {key}: {count} sections"),
                    });
                    return Ok((key, full_note));
                }
                Err(err) => {
                    if let Some((_, indexed_at)) = existing {
                        if !cancel.is_cancelled() {
                            let date = format_date(indexed_at);
                            return Ok((
                                key.clone(),
                                Some(format!(
                                    "Re-fetch of {key} failed ({err}); searching cached index from {date}"
                                )),
                            ));
                        }
                    }
                    return Err(err);
                }
            }
        }

        let immutable = key.starts_with("/nix/store/");
        if existing.is_some() && !refresh && immutable {
            return Ok((key, None));
        }

        let root_path = PathBuf::from(&key);
        let files = collect_doc_files(&root_path)?;
        let fingerprint = if immutable {
            "immutable".to_owned()
        } else {
            compute_files_fingerprint(&files)?
        };
        if let Some((prev_fp, _)) = existing {
            if !refresh && prev_fp == fingerprint {
                return Ok((key, None));
            }
        }

        self.clear_source_dir(&slug);

        let mut pages = Vec::with_capacity(files.len());
        for (idx, file) in files.iter().enumerate() {
            let raw = tokio::fs::read_to_string(file)
                .await
                .map_err(|e| anyhow!("failed to read {}: {e}", file.display()))?;
            let ext = file
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            let title = file
                .strip_prefix(&root_path)
                .ok()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(file)
                .to_string_lossy()
                .into_owned();
            if ext == "html" || ext == "htm" {
                let md = html_to_markdown(&raw).0;
                let rel_name = format!("{idx:04}-{}.md", safe_stem(file));
                pages.push(
                    self.cached_page(&slug, &rel_name, &md, &title, None)
                        .await?,
                );
            } else {
                let file_str = file.to_string_lossy().into_owned();
                let sections = split_sections(&raw, &title);
                pages.push(ParsedPage {
                    display_path: file_str,
                    web_url: None,
                    sections,
                });
            }
        }

        let count =
            self.replace_source(docs_settings, &key, &slug, "local", &fingerprint, &pages)?;
        Ok((
            key.clone(),
            Some(format!(
                "Indexed {key}: {count} sections from {} file(s)",
                files.len()
            )),
        ))
    }

    fn clear_source_dir(&self, slug: &str) {
        let _ = std::fs::remove_dir_all(self.pages.join(slug));
    }

    async fn cached_page(
        &self,
        slug: &str,
        name: &str,
        markdown: &str,
        title: &str,
        web_url: Option<String>,
    ) -> Result<ParsedPage> {
        let dir = self.pages.join(slug);
        tokio::fs::create_dir_all(&dir).await?;
        tokio::fs::write(dir.join(name), markdown).await?;
        Ok(ParsedPage {
            display_path: format!("doc://{slug}/{name}"),
            web_url,
            sections: split_sections(markdown, title),
        })
    }

    async fn fetch_and_parse_url(
        &self,
        web_settings: &AxoWebSearchSettings,
        url_str: &str,
        slug: &str,
        session_id: &str,
        cancel: &CancellationToken,
    ) -> Result<(&'static str, Vec<ParsedPage>, Option<String>)> {
        let fetched = self
            .fetch_markdown_or_extract(web_settings, url_str, session_id, cancel)
            .await?;

        self.clear_source_dir(slug);

        if looks_like_llms_txt(url_str, &fetched.markdown) {
            let base = Url::parse(url_str)?;
            let links = extract_llms_links(&fetched.markdown, &base);
            if !links.is_empty() {
                let mut total_bytes = fetched.markdown.len();

                let mut pages = Vec::new();
                pages.push(
                    self.cached_page(
                        slug,
                        "0000-llms.txt.md",
                        &fetched.markdown,
                        "llms.txt",
                        Some(url_str.to_owned()),
                    )
                    .await?,
                );

                let fetched_pages: Vec<(usize, Url, Result<FetchedUrl>)> =
                    stream::iter(links.into_iter().take(LLMS_MAX_PAGES).enumerate())
                        .map(|(i, link)| async move {
                            let res = self
                                .fetch_markdown_or_extract(
                                    web_settings,
                                    link.as_str(),
                                    session_id,
                                    cancel,
                                )
                                .await;
                            (i + 1, link, res)
                        })
                        .buffer_unordered(6)
                        .collect()
                        .await;

                let mut fetched_pages = fetched_pages;
                fetched_pages.sort_by_key(|(idx, _, _)| *idx);
                let mut skipped = 0usize;
                for (idx, link, result) in fetched_pages {
                    let Ok(page) = result else {
                        skipped += 1;
                        continue;
                    };
                    if total_bytes + page.markdown.len() > LLMS_MAX_BYTES {
                        break;
                    }
                    total_bytes += page.markdown.len();
                    let rel_name = format!("{idx:04}-{}.md", safe_url_stem(&link));
                    let title = link.path().trim_start_matches('/');
                    pages.push(
                        self.cached_page(
                            slug,
                            &rel_name,
                            &page.markdown,
                            title,
                            Some(link.to_string()),
                        )
                        .await?,
                    );
                }

                let note = format!(
                    "llms.txt tree: {} pages, {} KB{}",
                    pages.len(),
                    total_bytes / 1024,
                    if skipped > 0 {
                        format!(", {skipped} skipped")
                    } else {
                        String::new()
                    }
                );
                return Ok(("llms", pages, Some(note)));
            }
        }

        let note = Some(format!(
            "{} KB{}",
            fetched.markdown.len().div_ceil(1024),
            if fetched.via_extract {
                " via extraction fallback"
            } else {
                ""
            }
        ));
        Ok((
            "url",
            vec![
                self.cached_page(
                    slug,
                    "index.md",
                    &fetched.markdown,
                    url_str,
                    Some(url_str.to_owned()),
                )
                .await?,
            ],
            note,
        ))
    }

    fn replace_source(
        &self,
        settings: &AxoDocsSettings,
        key: &str,
        slug: &str,
        kind: &str,
        fingerprint: &str,
        pages: &[ParsedPage],
    ) -> Result<usize> {
        let mut conn = self.conn(settings)?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM sections WHERE source = ?1", params![key])?;
        let mut count = 0usize;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO sections (source, display_path, web_url, start, end, anchor, crumb, body)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;
            for page in pages {
                for s in &page.sections {
                    stmt.execute(params![
                        key,
                        &page.display_path,
                        &page.web_url,
                        s.start,
                        s.end,
                        &s.anchor,
                        &s.crumb,
                        &s.body,
                    ])?;
                    count += 1;
                }
            }
        }
        let now = now_ms();
        tx.execute(
            "INSERT OR REPLACE INTO sources (key, slug, kind, fingerprint, indexed_at, used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![key, slug, kind, fingerprint, now, now],
        )?;
        tx.commit()?;
        Ok(count)
    }
}

struct FetchedUrl {
    markdown: String,
    via_extract: bool,
}

impl DocsCache {
    async fn extract_fallback(
        &self,
        web_settings: &AxoWebSearchSettings,
        url: &str,
        session_id: &str,
        cancel: &CancellationToken,
    ) -> Option<FetchedUrl> {
        run_extract(
            &self.http,
            &self.credentials,
            web_settings,
            url,
            None,
            session_id,
            cancel,
        )
        .await
        .ok()
        .map(|ext| FetchedUrl {
            markdown: ext.markdown,
            via_extract: true,
        })
    }

    async fn fetch_markdown_or_extract(
        &self,
        web_settings: &AxoWebSearchSettings,
        url_str: &str,
        session_id: &str,
        cancel: &CancellationToken,
    ) -> Result<FetchedUrl> {
        let mut target = Url::parse(url_str).map_err(|e| anyhow!("invalid URL {url_str}: {e}"))?;
        for _hop in 0..4 {
            let req = self
                .http
                .get(target.clone())
                .header(
                    "Accept",
                    "text/markdown, text/plain;q=0.9, text/html;q=0.8, */*;q=0.5",
                )
                .header("User-Agent", "Mozilla/5.0 (compatible; axome-docs)")
                .timeout(HTTP_TIMEOUT);
            let resp = tokio::select! {
                _ = cancel.cancelled() => bail!("Operation aborted"),
                res = req.send() => res,
            };
            let resp = match resp {
                Ok(r) => r,
                Err(err) => {
                    if let Some(page) = self
                        .extract_fallback(web_settings, url_str, session_id, cancel)
                        .await
                    {
                        return Ok(page);
                    }
                    return Err(err.into());
                }
            };
            let status = resp.status();
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            let final_url = resp.url().clone();
            if status.as_u16() == 403 || status.as_u16() == 429 || status.as_u16() == 503 {
                if let Some(page) = self
                    .extract_fallback(web_settings, url_str, session_id, cancel)
                    .await
                {
                    return Ok(page);
                }
                return Err(anyhow!("GET {target}: HTTP {status}"));
            }
            if !status.is_success() {
                return Err(anyhow!("GET {target}: HTTP {status}"));
            }
            let body = resp.text().await?;
            let is_html = content_type.contains("html") || looks_like_html(&body);
            if !is_html {
                return Ok(FetchedUrl {
                    markdown: body,
                    via_extract: false,
                });
            }
            let (md, refresh) = html_to_markdown(&body);
            if body.len() <= 25_000 {
                if let Some(next) = refresh.and_then(|r| final_url.join(&r).ok()) {
                    target = next;
                    continue;
                }
            }
            if looks_like_js_shell(&body, &md) {
                if let Some(page) = self
                    .extract_fallback(web_settings, url_str, session_id, cancel)
                    .await
                {
                    return Ok(page);
                }
            }
            return Ok(FetchedUrl {
                markdown: md,
                via_extract: false,
            });
        }
        Err(anyhow!("Too many refresh redirects from {url_str}"))
    }
}

fn looks_like_llms_txt(url: &str, body: &str) -> bool {
    let path_is_llms = Url::parse(url)
        .ok()
        .is_some_and(|u| u.path().trim_end_matches('/').ends_with("/llms.txt"));
    path_is_llms && !looks_like_html(body)
}

fn extract_llms_links(markdown: &str, base: &Url) -> Vec<Url> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in markdown.lines() {
        let mut rest = line;
        while let Some(open) = rest.find("](") {
            let after = &rest[open + 2..];
            let Some(close) = after.find(')') else {
                break;
            };
            let raw_target = after[..close].split_whitespace().next().unwrap_or("");
            if let Ok(mut parsed) = base.join(raw_target) {
                parsed.set_fragment(None);
                let same_host = parsed.host_str() == base.host_str();
                let is_doc_ext = parsed.path().ends_with(".md")
                    || parsed.path().ends_with(".mdx")
                    || parsed.path().ends_with(".txt");
                if same_host
                    && (parsed.scheme() == "https" || parsed.scheme() == "http")
                    && is_doc_ext
                    && parsed != *base
                    && seen.insert(parsed.to_string())
                {
                    out.push(parsed);
                }
            }
            rest = &after[close + 1..];
        }
    }
    out
}

fn run_fts_query(
    conn: &Connection,
    match_expr: &str,
    key: Option<&str>,
    limit: usize,
) -> Result<Vec<DocSectionHit>> {
    let sql = "SELECT source, display_path, web_url, start, end, anchor, crumb, body
         FROM sections
         WHERE sections MATCH ?1 AND (?2 IS NULL OR source = ?2)
         ORDER BY bm25(sections, 0, 0, 0, 0, 0, 0, 4, 1)
         LIMIT ?3";
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<DocSectionHit> {
        Ok(DocSectionHit {
            source: row.get(0)?,
            display_path: row.get(1)?,
            web_url: row.get(2)?,
            start: row.get(3)?,
            end: row.get(4)?,
            anchor: row.get(5)?,
            crumb: row.get(6)?,
            body: row.get(7)?,
        })
    };
    let rows = stmt.query_map(params![match_expr, key, limit as i64], map_row)?;

    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn build_fts_terms(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in query.split_whitespace() {
        let prefix = token.ends_with('*');
        let mut words = Vec::new();
        let mut cur = String::new();
        for ch in token.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                cur.push(ch);
            } else if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            words.push(cur);
        }
        if !words.is_empty() {
            let phrase = words.join(" ");
            out.push(if prefix {
                format!("\"{phrase}\"*")
            } else {
                format!("\"{phrase}\"")
            });
        }
    }
    out
}

fn split_sections(text: &str, title: &str) -> Vec<Section> {
    let mut out = Vec::new();
    let mut crumbs: Vec<String> = Vec::new();
    let mut anchors: Vec<String> = Vec::new();
    let mut paras: Vec<Vec<(&str, u32)>> = vec![Vec::new()];
    let mut start: Option<u32> = Some(1);
    let mut fence: Option<String> = None;

    let flush = |crumbs: &[String],
                 anchors: &[String],
                 paras: &mut Vec<Vec<(&str, u32)>>,
                 start: &mut Option<u32>,
                 out: &mut Vec<Section>| {
        let crumb_parts: Vec<&str> = crumbs
            .iter()
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .collect();
        let crumb = if crumb_parts.is_empty() {
            title.to_owned()
        } else {
            crumb_parts.join(" > ")
        };
        let anchor = anchors
            .iter()
            .rev()
            .find(|a| !a.is_empty())
            .cloned()
            .unwrap_or_default();

        let mut chunk: Vec<(&str, u32)> = Vec::new();
        let mut size = 0usize;
        let emit = |chunk: &mut Vec<(&str, u32)>,
                    size: &mut usize,
                    start: &mut Option<u32>,
                    out: &mut Vec<Section>| {
            let first_nonblank = chunk
                .iter()
                .find(|(t, _)| !t.trim().is_empty())
                .map(|(_, n)| *n);
            let last_nonblank = chunk
                .iter()
                .rev()
                .find(|(t, _)| !t.trim().is_empty())
                .map(|(_, n)| *n);
            if let (Some(first), Some(last)) = (first_nonblank, last_nonblank) {
                let body = chunk
                    .iter()
                    .map(|(t, _)| *t)
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_owned();
                out.push(Section {
                    crumb: crumb.clone(),
                    anchor: anchor.clone(),
                    body,
                    start: start.unwrap_or(first),
                    end: last,
                });
                *start = None;
            }
            chunk.clear();
            *size = 0;
        };

        for para in paras.iter() {
            let len: usize = para.iter().map(|(t, _)| t.len() + 1).sum();
            if size > 0 && size + len > CHUNK_CHARS {
                emit(&mut chunk, &mut size, start, out);
            }
            chunk.extend(para.iter().copied());
            size += len;
        }
        emit(&mut chunk, &mut size, start, out);
        *paras = vec![Vec::new()];
    };

    for (idx, line) in text.split('\n').enumerate() {
        let n = (idx + 1) as u32;
        let marker = fence_marker(line);
        if let Some(m) = &marker {
            if let Some(active) = &fence {
                if m.starts_with(active.chars().next().unwrap_or('`')) && m.len() >= active.len() {
                    fence = None;
                }
            } else {
                fence = Some(m.clone());
            }
        }
        let loose = fence.is_none() && marker.is_none();
        if loose {
            if let Some((level, heading_text, anchor_id)) = parse_atx_heading(line) {
                flush(&crumbs, &anchors, &mut paras, &mut start, &mut out);
                crumbs.resize(level, String::new());
                anchors.resize(level, String::new());
                crumbs[level - 1] = heading_text;
                anchors[level - 1] = anchor_id;
                start = Some(n);
                continue;
            }
        }
        if loose && is_list_start(line) && !paras.last().is_some_and(Vec::is_empty) {
            paras.push(Vec::new());
        }
        paras.last_mut().expect("non-empty").push((line, n));
        if loose && line.trim().is_empty() && !paras.last().is_some_and(Vec::is_empty) {
            paras.push(Vec::new());
        }
    }
    flush(&crumbs, &anchors, &mut paras, &mut start, &mut out);
    out
}

fn fence_marker(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|c| *c == ch).count();
    (len >= 3).then(|| ch.to_string().repeat(len))
}

fn parse_atx_heading(line: &str) -> Option<(usize, String, String)> {
    let level = line.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let after = &line[level..];
    if !after.starts_with(' ') && !after.starts_with('\t') {
        return None;
    }
    let mut raw = after.trim().trim_end_matches('#').trim().to_owned();
    if raw.is_empty() {
        return None;
    }
    let mut anchor = String::new();
    while let Some(open) = raw.find("{#") {
        let rest = &raw[open + 2..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let id = rest[..close].trim();
        if !id.is_empty() && !id.contains(char::is_whitespace) {
            anchor = id.to_owned();
            raw = format!(
                "{} {}",
                raw[..open].trim_end(),
                rest[close + 1..].trim_start()
            )
            .trim()
            .to_owned();
        } else {
            break;
        }
    }
    Some((level, raw, anchor))
}

fn is_list_start(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.starts_with(b"- ") || bytes.starts_with(b"* ") || bytes.starts_with(b"+ ") {
        return true;
    }
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    digits > 0 && bytes.get(digits..digits + 2) == Some(b". ")
}

fn collect_doc_files(root: &Path) -> Result<Vec<PathBuf>> {
    let meta = std::fs::metadata(root).map_err(|e| anyhow!("{}: {e}", root.display()))?;
    if meta.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| anyhow!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') || name_str == "node_modules" {
                continue;
            }
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                dirs.push(path);
            } else if ft.is_file() && is_doc_ext(&path) {
                files.push(path);
                if files.len() > MAX_LOCAL_FILES {
                    return Err(anyhow!(
                        "{} has more than {MAX_LOCAL_FILES} doc files; pass a narrower directory",
                        root.display()
                    ));
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_doc_ext(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "md" | "mdx" | "markdown" | "txt" | "html" | "htm"
    )
}

fn compute_files_fingerprint(files: &[PathBuf]) -> Result<String> {
    let mut hasher = Sha256::new();
    for file in files {
        let meta = std::fs::metadata(file)?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        hasher.update(file.to_string_lossy().as_bytes());
        hasher.update(b":");
        hasher.update(mtime.to_string().as_bytes());
        hasher.update(b"\n");
    }
    Ok(crate::encode_hex(&hasher.finalize()))
}

fn drop_source(conn: &Connection, pages: &Path, key: &str, slug: Option<&str>) -> Result<()> {
    conn.execute("DELETE FROM sections WHERE source = ?1", params![key])?;
    conn.execute("DELETE FROM sources WHERE key = ?1", params![key])?;
    if let Some(slug) = slug {
        let _ = std::fs::remove_dir_all(pages.join(slug));
    }
    Ok(())
}

fn expired(indexed_at: i64, settings: &AxoDocsSettings) -> bool {
    now_ms() - indexed_at >= i64::from(settings.ttl_hours.max(1)) * 3_600_000
}

fn prune_expired(conn: &Connection, pages: &Path, retention_days: u32) -> Result<()> {
    let cutoff = now_ms() - i64::from(retention_days.max(1)) * 86_400_000;
    let mut stmt = conn.prepare("SELECT key, slug, used_at FROM sources")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let expired = rows
        .filter_map(|row| {
            row.map(|(key, slug, used_at)| {
                let vanished = !is_http_url(&key) && !Path::new(&key).exists();
                (used_at < cutoff || vanished).then_some((key, slug))
            })
            .transpose()
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if expired.is_empty() {
        return Ok(());
    }
    for (key, slug) in expired {
        drop_source(conn, pages, &key, Some(&slug))?;
    }
    Ok(())
}

pub(super) fn is_http_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn raw_github_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let mut parts = rest.splitn(4, '/');
        if let (Some(owner), Some(repo), Some("blob"), Some(tail)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        {
            return format!("https://raw.githubusercontent.com/{owner}/{repo}/{tail}");
        }
    }
    url.to_owned()
}

fn source_slug(key: &str) -> String {
    let hash = sha256_hex(key.as_bytes());
    let prefix = if let Ok(url) = Url::parse(key) {
        url.host_str().unwrap_or("web").to_owned()
    } else {
        Path::new(key)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("doc")
            .to_owned()
    };
    let clean: String = prefix
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(28)
        .collect();
    let clean = clean.trim_matches('-');
    format!(
        "{}-{}",
        if clean.is_empty() { "doc" } else { clean },
        &hash[..10]
    )
}

fn safe_stem(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    sanitize_slug(stem)
}

fn safe_url_stem(url: &Url) -> String {
    let seg = url
        .path_segments()
        .and_then(|mut s| s.rfind(|part| !part.is_empty()))
        .unwrap_or("page");
    sanitize_slug(seg.trim_end_matches(".md").trim_end_matches(".mdx"))
}

fn sanitize_slug(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(48)
        .collect();
    let trimmed = s.trim_matches('-');
    if trimmed.is_empty() {
        "page".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    crate::encode_hex(&Sha256::digest(bytes))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn format_date(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                total = total.saturating_add(dir_size(&p));
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_headings_anchors_and_fences() {
        let md = "\
# Intro {#intro}
Hello world.

```rust
# not a heading
fn x() {}
```

## Sub {#sub}
Details here.
";
        let sections = split_sections(md, "doc");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].crumb, "Intro");
        assert_eq!(sections[0].anchor, "intro");
        assert!(sections[0].body.contains("# not a heading"));
        assert_eq!(sections[1].crumb, "Intro > Sub");
        assert_eq!(sections[1].anchor, "sub");
    }

    #[test]
    fn fts_terms_sanitize_punctuation_and_preserve_prefix() {
        assert_eq!(
            build_fts_terms("services.nginx.virtualHosts foo*"),
            vec!["\"services nginx virtualHosts\"", "\"foo\"*"]
        );
    }

    #[test]
    fn indexes_local_html_and_supports_toc_list_and_search() {
        let base = std::env::temp_dir().join(format!("axome-docs-test-{}", uuid::Uuid::new_v4()));
        let doc_dir = base.join("manual");
        std::fs::create_dir_all(&doc_dir).unwrap();
        std::fs::write(
            doc_dir.join("builtins.html"),
            "<html><body><h1 id='map'>builtins.map</h1><p>Applies a function to each list element.</p></body></html>",
        )
        .unwrap();

        let http = reqwest::Client::new();
        let creds = FileCredentialStore::new(base.join("auth.json"));
        let cache = DocsCache::new(base.join("cache/docs"), http, Arc::new(creds));
        let web_cfg = AxoWebSearchSettings::default();
        let docs_cfg = AxoDocsSettings::default();
        let cancel = CancellationToken::new();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let outcome = cache
                .search(
                    &web_cfg,
                    &docs_cfg,
                    Some("builtins.map"),
                    Some(doc_dir.join("builtins.html").to_str().unwrap()),
                    5,
                    false,
                    "s1",
                    &cancel,
                )
                .await
                .unwrap();

            let DocsOutcome::Search { hits, .. } = outcome else {
                panic!("expected search outcome");
            };
            assert_eq!(hits.len(), 1);
            assert!(hits[0].display_path.starts_with("doc://"));
            assert!(hits[0].body.contains("Applies a function"));
            assert!(!hits[0].body.contains("<html>"));

            // TOC when query omitted
            let toc = cache
                .search(
                    &web_cfg,
                    &docs_cfg,
                    None,
                    Some(doc_dir.join("builtins.html").to_str().unwrap()),
                    5,
                    false,
                    "s1",
                    &cancel,
                )
                .await
                .unwrap();
            let DocsOutcome::Toc { entries, .. } = toc else {
                panic!("expected toc outcome");
            };
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].crumb, "builtins.map");
        });
        let _ = std::fs::remove_dir_all(&base);
    }
}
