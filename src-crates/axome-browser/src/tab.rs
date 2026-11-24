use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axo::{SessionTools, Target};
use parking_lot::Mutex;
use playwright_rs::protocol::{AriaSnapshotMode, AriaSnapshotOptions, Locator, Page, WaitUntil};
use serde::{Deserialize, Serialize};

const MAX_TELEMETRY_ENTRIES: usize = 200;
const INLINE_TEXT_CAP_BYTES: usize = 24 * 1024;
const SETTLE_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleEntry {
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEntry {
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub failure: Option<String>,
}

impl NetworkEntry {
    pub fn is_failure(&self) -> bool {
        self.failure.is_some() || self.status.is_some_and(|s| s >= 400)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogEntry {
    pub kind: String,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Default)]
pub struct TabTelemetry {
    pub console: VecDeque<ConsoleEntry>,
    pub console_cursor: usize,
    pub network: VecDeque<NetworkEntry>,
    pub network_cursor: usize,
    pub dialogs: VecDeque<DialogEntry>,
    pub dialog_cursor: usize,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryDelta {
    pub console_errors: Vec<ConsoleEntry>,
    pub failed_requests: Vec<NetworkEntry>,
    pub dialogs: Vec<DialogEntry>,
}

impl TelemetryDelta {
    pub fn is_empty(&self) -> bool {
        self.console_errors.is_empty() && self.failed_requests.is_empty() && self.dialogs.is_empty()
    }
}

fn push_bounded<T>(buf: &mut VecDeque<T>, cursor: &mut usize, entry: T) {
    if buf.len() >= MAX_TELEMETRY_ENTRIES {
        buf.pop_front();
        *cursor = cursor.saturating_sub(1);
    }
    buf.push_back(entry);
}

impl TabTelemetry {
    pub fn push_console(&mut self, entry: ConsoleEntry) {
        push_bounded(&mut self.console, &mut self.console_cursor, entry);
    }

    pub fn push_network(&mut self, entry: NetworkEntry) {
        push_bounded(&mut self.network, &mut self.network_cursor, entry);
    }

    pub fn push_dialog(&mut self, entry: DialogEntry) {
        push_bounded(&mut self.dialogs, &mut self.dialog_cursor, entry);
    }

    /// Returns only new console errors, failed network requests (4xx/5xx/requestfailed),
    /// and auto-handled dialogs since the previous call.
    pub fn drain_delta(&mut self) -> TelemetryDelta {
        let console_errors = self
            .console
            .iter()
            .skip(self.console_cursor)
            .filter(|e| matches!(e.level.as_str(), "error" | "assert"))
            .cloned()
            .collect();
        self.console_cursor = self.console.len();

        let failed_requests = self
            .network
            .iter()
            .skip(self.network_cursor)
            .filter(|e| e.is_failure())
            .cloned()
            .collect();
        self.network_cursor = self.network.len();

        let dialogs = self
            .dialogs
            .iter()
            .skip(self.dialog_cursor)
            .cloned()
            .collect();
        self.dialog_cursor = self.dialogs.len();

        TelemetryDelta {
            console_errors,
            failed_requests,
            dialogs,
        }
    }

    pub fn all_console(&mut self, clear: bool) -> Vec<ConsoleEntry> {
        let out = self.console.iter().cloned().collect();
        if clear {
            self.console.clear();
            self.console_cursor = 0;
        } else {
            self.console_cursor = self.console.len();
        }
        out
    }

    pub fn all_network(&mut self, failed_only: bool, clear: bool) -> Vec<NetworkEntry> {
        let out = self
            .network
            .iter()
            .filter(|e| !failed_only || e.is_failure())
            .cloned()
            .collect();
        if clear {
            self.network.clear();
            self.network_cursor = 0;
        } else {
            self.network_cursor = self.network.len();
        }
        out
    }
}

pub struct TabState {
    pub page: Page,
    pub persist: bool,
    pub last_used: Instant,
    pub telemetry: Arc<Mutex<TabTelemetry>>,
}

impl TabState {
    pub async fn attach(page: Page, persist: bool) -> Self {
        let telemetry = Arc::new(Mutex::new(TabTelemetry::default()));

        let t_console = Arc::clone(&telemetry);
        let _ = page
            .on_console(move |msg| {
                let t = Arc::clone(&t_console);
                let level = msg.type_().to_owned();
                let text = msg.text().to_owned();
                async move {
                    t.lock().push_console(ConsoleEntry { level, text });
                    Ok(())
                }
            })
            .await;

        let t_resp = Arc::clone(&telemetry);
        let _ = page
            .on_response(move |resp| {
                let t = Arc::clone(&t_resp);
                let status = resp.status();
                let url = resp.url().to_owned();
                async move {
                    t.lock().push_network(NetworkEntry {
                        // ResponseObject passed to on_response does not expose Request in playwright-rs 0.21.
                        method: String::new(),
                        url,
                        status: Some(status),
                        failure: None,
                    });
                    Ok(())
                }
            })
            .await;

        let t_fail = Arc::clone(&telemetry);
        let _ = page
            .on_request_failed(move |req| {
                let t = Arc::clone(&t_fail);
                let method = req.method();
                let url = req.url();
                let failure = req.failure().or_else(|| Some("request failed".to_owned()));
                async move {
                    t.lock().push_network(NetworkEntry {
                        method,
                        url,
                        status: None,
                        failure,
                    });
                    Ok(())
                }
            })
            .await;

        // Auto-handle JS dialogs so stray alert/confirm/prompt never hangs a tool call.
        let t_dialog = Arc::clone(&telemetry);
        let _ = page
            .on_dialog(move |dialog| {
                let t = Arc::clone(&t_dialog);
                let kind = dialog.type_().to_owned();
                let message = dialog.message().to_owned();
                async move {
                    let action = if kind == "alert" {
                        let _ = dialog.accept(None).await;
                        "accepted"
                    } else {
                        let _ = dialog.dismiss().await;
                        "dismissed"
                    };
                    t.lock().push_dialog(DialogEntry {
                        kind,
                        message,
                        action: action.to_owned(),
                    });
                    Ok(())
                }
            })
            .await;

        Self {
            page,
            persist,
            last_used: Instant::now(),
            telemetry,
        }
    }

    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    pub fn drain_delta(&self) -> TelemetryDelta {
        self.telemetry.lock().drain_delta()
    }
}

/// Converts a target string into a Playwright selector.
/// ARIA snapshot refs like `@e12` or `e12` map to Playwright's `aria-ref=e12`.
/// Everything else is passed directly to Playwright's locator engine.
pub fn normalize_target_selector(target: &str) -> String {
    let trimmed = target.trim();
    let candidate = trimmed.strip_prefix('@').unwrap_or(trimmed);
    if is_aria_ref(candidate) {
        format!("aria-ref={candidate}")
    } else {
        trimmed.to_owned()
    }
}

fn is_aria_ref(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('e') else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

pub fn resolve_locator(page: &Page, target: &str) -> Locator {
    let selector = normalize_target_selector(target);
    page.locator(&selector)
}

/// Bounded post-action settle: waits briefly for triggered navigation/parse,
/// never stalling on long-lived WebSockets or SSE streams.
pub async fn settle_after_action(page: &Page) {
    let _ = tokio::time::timeout(
        SETTLE_TIMEOUT,
        page.wait_for_load_state(Some(WaitUntil::DomContentLoaded)),
    )
    .await;
}

/// Captures an AI-mode ARIA snapshot from either a selector or the full page.
pub async fn capture_aria_snapshot(
    page: &Page,
    selector: Option<&str>,
    depth: Option<i32>,
    boxes: bool,
) -> Result<String, String> {
    let mut opts = AriaSnapshotOptions::default().mode(AriaSnapshotMode::Ai);
    if let Some(d) = depth {
        opts = opts.depth(d);
    }
    if boxes {
        opts = opts.boxes(true);
    }

    let result = match selector.map(str::trim).filter(|s| !s.is_empty()) {
        Some(sel) => {
            let loc = resolve_locator(page, sel);
            loc.aria_snapshot(Some(opts)).await
        }
        None => page.aria_snapshot(Some(opts)).await,
    };

    result.map_err(|err| format!("Failed to capture ARIA snapshot: {err}"))
}

/// Caps inline text output at `INLINE_TEXT_CAP_BYTES`. If exceeded, writes the
/// full content to `local://browser/` (or `fallback_dir`) and returns the truncated
/// preview plus saved target.
pub async fn cap_or_spill_text(
    text: &str,
    session_tools: Option<&SessionTools>,
    fallback_dir: &Path,
    prefix: &str,
) -> (String, Option<String>) {
    if text.len() <= INLINE_TEXT_CAP_BYTES {
        return (text.to_owned(), None);
    }

    let mut cut = INLINE_TEXT_CAP_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let preview = &text[..cut];

    let saved =
        save_spill_target(session_tools, fallback_dir, prefix, "txt", text.as_bytes()).await;
    let out = match &saved {
        Some(target) => format!(
            "{preview}\n\n[Truncated: {} total bytes. Full output saved to {target}]",
            text.len()
        ),
        None => format!("{preview}\n\n[Truncated: {} total bytes]", text.len()),
    };
    (out, saved)
}

pub async fn save_spill_target(
    session_tools: Option<&SessionTools>,
    fallback_dir: &Path,
    prefix: &str,
    ext: &str,
    bytes: &[u8],
) -> Option<String> {
    let filename = format!("{prefix}-{}.{ext}", uuid::Uuid::new_v4());

    if let Some(tools) = session_tools {
        let local_url = format!("local://browser/{filename}");
        if let Some(parsed) = tools.schemes.exact(&local_url) {
            if let Ok(Target::File {
                path,
                writable: true,
            }) = tools.schemes.resolve(&parsed).await
            {
                if write_async(&path, bytes).await {
                    return Some(local_url);
                }
            }
        }
    }

    let path = fallback_dir.join(filename);
    write_async(&path, bytes)
        .await
        .then(|| path.display().to_string())
}

async fn write_async(path: &Path, bytes: &[u8]) -> bool {
    if let Some(parent) = path.parent() {
        if tokio::fs::create_dir_all(parent).await.is_err() {
            return false;
        }
    }
    tokio::fs::write(path, bytes).await.is_ok()
}

pub fn parse_wait_until(raw: Option<&str>) -> WaitUntil {
    match raw.map(str::trim).unwrap_or("domcontentloaded") {
        "load" => WaitUntil::Load,
        "networkidle" => WaitUntil::NetworkIdle,
        "commit" => WaitUntil::Commit,
        _ => WaitUntil::DomContentLoaded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_aria_refs_and_preserves_locators() {
        assert_eq!(normalize_target_selector("@e12"), "aria-ref=e12");
        assert_eq!(normalize_target_selector("e5"), "aria-ref=e5");
        assert_eq!(normalize_target_selector("button.submit"), "button.submit");
        assert_eq!(normalize_target_selector("text=Sign in"), "text=Sign in");
        assert_eq!(normalize_target_selector("email"), "email");
    }

    #[test]
    fn telemetry_delta_only_reports_new_errors() {
        let mut t = TabTelemetry::default();
        t.push_console(ConsoleEntry {
            level: "log".into(),
            text: "hello".into(),
        });
        t.push_console(ConsoleEntry {
            level: "error".into(),
            text: "boom 1".into(),
        });
        t.push_network(NetworkEntry {
            method: "GET".into(),
            url: "http://localhost:5173/ok".into(),
            status: Some(200),
            failure: None,
        });
        t.push_network(NetworkEntry {
            method: "GET".into(),
            url: "http://localhost:5173/missing".into(),
            status: Some(404),
            failure: None,
        });

        let first = t.drain_delta();
        assert_eq!(first.console_errors.len(), 1);
        assert_eq!(first.console_errors[0].text, "boom 1");
        assert_eq!(first.failed_requests.len(), 1);
        assert_eq!(first.failed_requests[0].status, Some(404));

        let second = t.drain_delta();
        assert!(second.is_empty());

        t.push_network(NetworkEntry {
            method: "POST".into(),
            url: "http://localhost:5173/api".into(),
            status: None,
            failure: Some("net::ERR_CONNECTION_REFUSED".into()),
        });
        let third = t.drain_delta();
        assert_eq!(third.failed_requests.len(), 1);
        assert!(third.console_errors.is_empty());
    }

    #[tokio::test]
    async fn cap_or_spill_spills_large_text() {
        let dir = tempfile::tempdir().unwrap();
        let small = "hello world";
        let (out, path) = cap_or_spill_text(small, None, dir.path(), "snap").await;
        assert_eq!(out, small);
        assert!(path.is_none());

        let big = "a".repeat(INLINE_TEXT_CAP_BYTES + 100);
        let (out, path) = cap_or_spill_text(&big, None, dir.path(), "snap").await;
        let saved = path.expect("expected spilled file");
        assert!(Path::new(&saved).exists());
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), big);
        assert!(out.contains("Truncated:"));
    }
}
