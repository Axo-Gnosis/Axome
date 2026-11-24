//! Host-supplied web search, web fetch, and docs cache tools + `doc://` scheme.

mod auth;
mod doc_scheme;
mod docs_cache;
mod docs_tool;
mod fetch_tool;
mod grounded;
mod html_md;
mod providers;
mod search_tool;

use std::path::Path;
use std::sync::{Arc, Weak};

use axo::{HostTool, Scheme, SessionTools};
use axo_ai::{Credential, CredentialStore, FileCredentialStore, Model, Models};
use axome_protocol::axo::AxoWebToolsStatusPayload;

use crate::errors::ApiError;
use crate::HostDirs;
use axome_protocol::axo_settings::{AxoSettings, AxoWebSearchSettings};
use doc_scheme::DocScheme;
use docs_cache::DocsCache;
use docs_tool::SearchDocsTool;
use fetch_tool::WebFetchTool;
use search_tool::WebSearchTool;

pub(super) fn settings(dirs: &HostDirs, folder: Option<&Path>) -> AxoSettings {
    crate::services::settings_resolution::resolve_effective_settings_for_folder_path(dirs, folder)
        .settings
        .axo
}

/// What grounded search authenticates with: the registry, the session's provider, and the
/// preferred grounded model (Settings › Tools, else `@dash`).
pub(crate) struct Grounded {
    pub models: Arc<Models>,
    pub active_provider: Option<String>,
    pub preferred: Option<Model>,
}

pub(crate) struct WebTools {
    pub dirs: HostDirs,
    pub credentials: Arc<FileCredentialStore>,
    pub http: reqwest::Client,
    pub docs: DocsCache,
    service: Weak<super::AxoService>,
}

impl WebTools {
    pub(crate) fn new(
        dirs: HostDirs,
        credentials: Arc<FileCredentialStore>,
        service: Weak<super::AxoService>,
    ) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .user_agent("Axome/1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let docs = DocsCache::new(
            dirs.cache.join("docs"),
            http.clone(),
            Arc::clone(&credentials),
        );
        Arc::new(Self {
            dirs,
            credentials,
            http,
            docs,
            service,
        })
    }

    pub(crate) async fn grounded(
        &self,
        session_id: &str,
        folder: &Path,
        settings: &AxoWebSearchSettings,
    ) -> Option<Grounded> {
        self.service
            .upgrade()?
            .grounded(session_id, folder, settings)
            .await
    }

    pub(crate) fn host_schemes(&self) -> Vec<Arc<dyn Scheme>> {
        vec![Arc::new(DocScheme::new(self.docs.pages_dir()))]
    }

    pub(crate) fn host_tools(self: &Arc<Self>, folder: &Path) -> Vec<HostTool> {
        let cfg = settings(&self.dirs, Some(folder));
        let mut tools = Vec::new();
        if cfg.web_search.enabled {
            let shared = Arc::clone(self);
            tools.push(HostTool::direct(Arc::new(move |st: &Arc<SessionTools>| {
                Arc::new(WebSearchTool::new(Arc::clone(&shared), Arc::clone(st)))
            })));
            let shared = Arc::clone(self);
            tools.push(HostTool::direct(Arc::new(move |st: &Arc<SessionTools>| {
                Arc::new(WebFetchTool::new(Arc::clone(&shared), Arc::clone(st)))
            })));
        }
        if cfg.docs.enabled {
            let shared = Arc::clone(self);
            tools.push(HostTool::direct(Arc::new(move |st: &Arc<SessionTools>| {
                Arc::new(SearchDocsTool::new(Arc::clone(&shared), Arc::clone(st)))
            })));
        }
        tools
    }
}

fn search_spec(provider: &str) -> Result<&'static auth::SearchProviderSpec, ApiError> {
    auth::find_search_spec(provider)
        .ok_or_else(|| ApiError::InvalidArgument(format!("Unknown search provider: {provider}")))
}

impl super::AxoService {
    /// What grounded search authenticates with for `session_id` in `folder`; `None` when the
    /// engine is unavailable. An unresolvable selector only leaves `preferred` empty.
    async fn grounded(
        &self,
        session_id: &str,
        folder: &Path,
        settings: &AxoWebSearchSettings,
    ) -> Option<Grounded> {
        let engine = self.engine().await.ok()?;
        let active_provider = self
            .sessions
            .lock()
            .get(session_id)
            .map(|live| live.session.model().provider);
        let selector = settings
            .grounded_model
            .as_deref()
            .map(str::trim)
            .filter(|selector| !selector.is_empty())
            .unwrap_or("@dash");
        let preferred = super::bundled::catalog(&engine.axo, folder, super::project_access(folder))
            .ok()
            .and_then(|catalog| {
                catalog
                    .config
                    .settings
                    .resolve_model(&engine.models, selector)
                    .ok()
            })
            .and_then(|choice| choice.model);
        Some(Grounded {
            models: engine.models.clone(),
            active_provider,
            preferred,
        })
    }

    pub(crate) async fn web_tools_status(&self) -> Result<AxoWebToolsStatusPayload, ApiError> {
        let cfg = settings(&self.dirs, None);
        let search_providers =
            auth::search_provider_statuses(&self.credentials, &cfg.web_search).await;
        let (docs_sources, cache_bytes) = self.web.docs.status(&cfg.docs)?;
        Ok(AxoWebToolsStatusPayload {
            search_providers,
            docs_sources,
            cache_bytes,
        })
    }

    pub(crate) async fn search_key_set(&self, provider: &str, key: String) -> Result<(), ApiError> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return Err(ApiError::InvalidArgument(
                "API key must not be empty".into(),
            ));
        }
        let store_key = search_spec(provider)?.store_key;
        self.credentials
            .set(
                store_key,
                Credential::ApiKey {
                    key: trimmed.to_owned(),
                },
            )
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        Ok(())
    }

    pub(crate) async fn search_key_remove(&self, provider: &str) -> Result<(), ApiError> {
        let store_key = search_spec(provider)?.store_key;
        self.credentials
            .remove(store_key)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        Ok(())
    }

    pub(crate) async fn docs_refresh(&self, source: Option<String>) -> Result<(), ApiError> {
        let cfg = settings(&self.dirs, None);
        self.web
            .docs
            .refresh_source(&cfg.web_search, &cfg.docs, source.as_deref())
            .await?;
        Ok(())
    }

    pub(crate) async fn docs_remove(&self, source: Option<String>) -> Result<(), ApiError> {
        let cfg = settings(&self.dirs, None);
        self.web.docs.remove_source(&cfg.docs, source.as_deref())?;
        Ok(())
    }
}
