use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axo::{HostTool, SessionTools};
use playwright_rs::api::LaunchOptions;
use playwright_rs::protocol::{
    Browser, BrowserContext, BrowserContextOptions, Playwright, Viewport,
};
use tokio::sync::Mutex;

use crate::discovery::resolve_executable;
use crate::tab::TabState;
use crate::tool::BrowserTool;

#[derive(Debug, Clone)]
pub struct BrowserConfig {
    pub enabled: bool,
    pub headless: bool,
    pub cdp_url: Option<String>,
    pub executable_path: Option<String>,
    pub idle_close_secs: u64,
    pub spill_dir: PathBuf,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            headless: true,
            cdp_url: None,
            executable_path: None,
            idle_close_secs: 1800,
            spill_dir: std::env::temp_dir().join("axome-browser"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedTabInfo {
    pub name: String,
    pub url: String,
    pub title: String,
    pub persist: bool,
}

struct BrowserPool {
    playwright: Option<Playwright>,
    headless: Option<(Option<PathBuf>, Browser)>,
    headed: Option<(Option<PathBuf>, Browser)>,
    cdp: HashMap<String, Browser>,
}

impl BrowserPool {
    fn new() -> Self {
        Self {
            playwright: None,
            headless: None,
            headed: None,
            cdp: HashMap::new(),
        }
    }

    async fn playwright(&mut self) -> Result<&Playwright, String> {
        if self.playwright.is_none() {
            let pw = Playwright::launch().await.map_err(|err| {
                format!(
                    "Failed to start Playwright driver: {err}. \
                     Ensure the Playwright driver is installed or PLAYWRIGHT_DRIVER_PATH / PLAYWRIGHT_NODE_EXE + PLAYWRIGHT_CLI_JS are configured."
                )
            })?;
            self.playwright = Some(pw);
        }
        Ok(self.playwright.as_ref().expect("just initialized"))
    }

    fn slot_mut(&mut self, headless: bool) -> &mut Option<(Option<PathBuf>, Browser)> {
        if headless {
            &mut self.headless
        } else {
            &mut self.headed
        }
    }

    async fn acquire_browser(
        &mut self,
        headless: bool,
        executable: Option<PathBuf>,
        cdp_url: Option<&str>,
    ) -> Result<Browser, String> {
        if let Some(url) = cdp_url.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(existing) = self.cdp.get(url) {
                if existing.is_connected() {
                    return Ok(existing.clone());
                }
            }
            let pw = self.playwright().await?;
            match pw.chromium().connect_over_cdp(url, None).await {
                Ok(browser) => {
                    self.cdp.insert(url.to_owned(), browser.clone());
                    return Ok(browser);
                }
                Err(err) => {
                    self.playwright.take();
                    return Err(format!("Failed to connect over CDP to '{url}': {err}"));
                }
            }
        }

        if let Some((cached_exe, existing)) = self.slot_mut(headless).as_ref() {
            if cached_exe == &executable && existing.is_connected() {
                return Ok(existing.clone());
            }
        }

        let mut opts = LaunchOptions::default().headless(headless);
        if let Some(ref exe) = executable {
            opts = opts.executable_path(exe.to_string_lossy().into_owned());
        }

        let pw = self.playwright().await?;
        let browser = match pw.chromium().launch_with_options(opts).await {
            Ok(browser) => browser,
            Err(err) => {
                self.playwright.take();
                let target = executable
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Playwright Chromium".to_owned());
                return Err(format!("Failed to launch browser ({target}): {err}"));
            }
        };

        *self.slot_mut(headless) = Some((executable, browser.clone()));
        Ok(browser)
    }

    async fn shutdown(&mut self) {
        if let Some((_, browser)) = self.headless.take() {
            let _ = browser.close().await;
        }
        if let Some((_, browser)) = self.headed.take() {
            let _ = browser.close().await;
        }
        // Never kill user-owned CDP browsers; just drop our handles.
        self.cdp.clear();
        self.playwright.take();
    }
}

pub(crate) struct SessionBrowser {
    pub(crate) context: Option<BrowserContext>,
    pub(crate) tabs: HashMap<String, TabState>,
    pub(crate) headed: bool,
    pub(crate) cdp_url: Option<String>,
}

impl SessionBrowser {
    fn new() -> Self {
        Self {
            context: None,
            tabs: HashMap::new(),
            headed: false,
            cdp_url: None,
        }
    }
}

pub struct BrowserService {
    pool: Mutex<BrowserPool>,
    sessions: Mutex<HashMap<String, Arc<Mutex<SessionBrowser>>>>,
    tool_refs: parking_lot::Mutex<HashMap<String, usize>>,
}

impl Default for BrowserService {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserService {
    pub fn new() -> Self {
        Self {
            pool: Mutex::new(BrowserPool::new()),
            sessions: Mutex::new(HashMap::new()),
            tool_refs: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// Builds the deferred `browser` `HostTool` for one `axo` session.
    pub fn host_tools(self: &Arc<Self>, config: BrowserConfig) -> Vec<HostTool> {
        if !config.enabled {
            return Vec::new();
        }
        let service = Arc::clone(self);
        vec![HostTool::deferred(Arc::new(
            move |session_tools: &Arc<SessionTools>| {
                let raw_id = session_tools.schemes.context().session_id.trim();
                let session_key = if raw_id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    raw_id.to_owned()
                };
                *service
                    .tool_refs
                    .lock()
                    .entry(session_key.clone())
                    .or_insert(0) += 1;
                Arc::new(BrowserTool::new(
                    Arc::clone(&service),
                    Arc::clone(session_tools),
                    session_key,
                    config.clone(),
                ))
            },
        ))]
    }

    pub(crate) async fn release_tool_handle(&self, session_key: &str, fallback_dir: &Path) {
        let should_close = {
            let mut refs = self.tool_refs.lock();
            match refs.get_mut(session_key) {
                Some(count) if *count > 1 => {
                    *count -= 1;
                    false
                }
                Some(_) => {
                    refs.remove(session_key);
                    true
                }
                None => true,
            }
        };
        if should_close {
            self.close_session(session_key, false).await;
            let still_open = self.sessions.lock().await.contains_key(session_key);
            if !still_open {
                let _ = tokio::fs::remove_dir_all(fallback_dir).await;
            }
        }
    }

    pub(crate) async fn session_handle(&self, session_key: &str) -> Arc<Mutex<SessionBrowser>> {
        let mut sessions = self.sessions.lock().await;
        Arc::clone(
            sessions
                .entry(session_key.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(SessionBrowser::new()))),
        )
    }

    pub(crate) async fn open_or_get_tab(
        &self,
        session_key: &str,
        tab_name: &str,
        config: &BrowserConfig,
        headed_override: Option<bool>,
        cdp_url_override: Option<&str>,
        persist: Option<bool>,
    ) -> Result<Arc<Mutex<SessionBrowser>>, String> {
        let handle = self.session_handle(session_key).await;
        let mut session = handle.lock().await;

        let want_headed = headed_override.unwrap_or(!config.headless);
        let want_cdp = cdp_url_override
            .or(config.cdp_url.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        let existing_closed = session
            .tabs
            .get(tab_name)
            .is_some_and(|tab| tab.page.is_closed());
        if existing_closed {
            session.tabs.remove(tab_name);
        }

        if !session.tabs.is_empty()
            && ((headed_override.is_some() && session.headed != want_headed)
                || (cdp_url_override.is_some() && session.cdp_url != want_cdp))
        {
            return Err(
                "Cannot switch headed/cdp_url while tabs are open in this session; close existing tabs first."
                    .to_owned(),
            );
        }

        if let Some(tab) = session.tabs.get_mut(tab_name) {
            if let Some(p) = persist {
                tab.persist = p;
            }
            tab.touch();
            drop(session);
            return Ok(handle);
        }

        // One BrowserContext per session keeps cookies/storage shared across tabs;
        // recreation is only allowed when no tabs are open.
        let need_context = session.context.is_none()
            || (session.tabs.is_empty()
                && (session.headed != want_headed || session.cdp_url != want_cdp));

        if need_context {
            if let Some(old_ctx) = session.context.take() {
                if session.cdp_url.is_none() {
                    let _ = old_ctx.close().await;
                }
            }
            let exe = if want_cdp.is_some() {
                None
            } else {
                let configured = config.executable_path.clone();
                tokio::task::spawn_blocking(move || resolve_executable(configured.as_deref()))
                    .await
                    .map_err(|err| format!("Browser discovery task failed: {err}"))??
            };
            let browser = {
                let mut pool = self.pool.lock().await;
                pool.acquire_browser(!want_headed, exe, want_cdp.as_deref())
                    .await?
            };
            let ctx = if want_cdp.is_some() {
                // CDP attaches to the user's existing BrowserContext rather than creating an isolated one.
                let contexts = browser.contexts();
                if let Some(first) = contexts.into_iter().next() {
                    first
                } else {
                    browser
                        .new_context()
                        .await
                        .map_err(|err| format!("Failed to create browser context: {err}"))?
                }
            } else {
                let opts = BrowserContextOptions::builder()
                    .viewport(Viewport {
                        width: 1280,
                        height: 800,
                    })
                    .build();
                browser
                    .new_context_with_options(opts)
                    .await
                    .map_err(|err| format!("Failed to create browser context: {err}"))?
            };
            session.context = Some(ctx);
            session.headed = want_headed;
            session.cdp_url = want_cdp;
        }

        let ctx = session
            .context
            .as_ref()
            .ok_or_else(|| "Browser context unavailable".to_owned())?;
        let page = match ctx.new_page().await {
            Ok(page) => page,
            Err(err) => {
                if let Some(dead_ctx) = session.context.take() {
                    if session.cdp_url.is_none() {
                        let _ = dead_ctx.close().await;
                    }
                }
                return Err(format!("Failed to open browser tab: {err}"));
            }
        };
        let tab = TabState::attach(page, persist.unwrap_or(false)).await;
        session.tabs.insert(tab_name.to_owned(), tab);

        drop(session);
        Ok(handle)
    }

    pub async fn close_session(&self, session_key: &str, force_all: bool) {
        let handle = {
            let sessions = self.sessions.lock().await;
            sessions.get(session_key).cloned()
        };
        let Some(handle) = handle else {
            return;
        };
        let mut session = handle.lock().await;
        let is_cdp = session.cdp_url.is_some();
        close_matching_tabs(&mut session, |t| force_all || !t.persist).await;
        if session.tabs.is_empty() {
            if !is_cdp {
                if let Some(ctx) = session.context.take() {
                    let _ = ctx.close().await;
                }
            }
            drop(session);
            let mut sessions = self.sessions.lock().await;
            if let Some(current) = sessions.get(session_key) {
                if Arc::ptr_eq(current, &handle) {
                    sessions.remove(session_key);
                }
            }
        }
    }

    /// Reaps idle non-persistent headless tabs older than `idle_close_secs`.
    pub async fn reap_idle(&self, idle_close_secs: u64) {
        if idle_close_secs == 0 {
            return;
        }
        let cutoff = Duration::from_secs(idle_close_secs);
        let now = Instant::now();
        let handles: Vec<Arc<Mutex<SessionBrowser>>> = {
            let sessions = self.sessions.lock().await;
            sessions.values().cloned().collect()
        };
        for handle in handles {
            let mut session = handle.lock().await;
            if session.headed || session.cdp_url.is_some() {
                continue;
            }
            close_matching_tabs(&mut session, |tab| {
                !tab.persist && now.saturating_duration_since(tab.last_used) >= cutoff
            })
            .await;
            if session.tabs.is_empty() {
                if let Some(ctx) = session.context.take() {
                    let _ = ctx.close().await;
                }
            }
        }
    }

    pub async fn shutdown(&self) {
        let all_keys: Vec<String> = {
            let sessions = self.sessions.lock().await;
            sessions.keys().cloned().collect()
        };
        for key in all_keys {
            self.close_session(&key, true).await;
        }
        self.pool.lock().await.shutdown().await;
    }
}

async fn close_matching_tabs(
    session: &mut SessionBrowser,
    mut pred: impl FnMut(&TabState) -> bool,
) {
    let keys: Vec<String> = session
        .tabs
        .iter()
        .filter(|(_, tab)| pred(tab))
        .map(|(k, _)| k.clone())
        .collect();
    for key in keys {
        if let Some(tab) = session.tabs.remove(&key) {
            let _ = tab.page.close().await;
        }
    }
}
