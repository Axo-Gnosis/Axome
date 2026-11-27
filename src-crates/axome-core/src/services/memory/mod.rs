//! Axome's side of long-term memory: settings, bank paths, model download timing, the
//! embedding loop and capture triggers. The memory itself is `axo_memory`.

pub(crate) mod capture;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use anyhow::{ensure, Context, Result};
use axo::{HostTool, SessionTools};
use axo_agent::Tool;
use axo_memory::{Bank, Banks, BgeSmall, MemoryScheme, SearchTool, BGE_SMALL};
use axo_models::ModelState;
use axome_protocol::{
    memory::{
        MemoryBankStatus, MemoryCaptureQuery, MemoryCaptureRun, MemoryEmbedState,
        MemoryEmbedStatus, MemoryStatus,
    },
    settings::MemorySettings,
};
use parking_lot::Mutex;
use tokio::{runtime::Handle, sync::watch};

use crate::{
    events::{EventSink, HostEvent},
    services::settings_resolution::resolve_effective_settings_for_folder_path,
    HostDirs,
};

struct Lifecycle {
    /// Semantic search is on: memory and its `semantic` setting are enabled.
    enabled: bool,
    generation: u64,
    failure: Option<String>,
    download: Option<tokio::task::AbortHandle>,
    /// The embedding model's files; `Missing` while semantic search is off.
    model: ModelState,
}

pub(crate) struct MemoryService {
    dirs: HostDirs,
    http: reqwest::Client,
    events: EventSink<HostEvent>,
    runtime: OnceLock<Handle>,
    pub(crate) store: Arc<axo_memory::Memory>,
    lifecycle: Mutex<Lifecycle>,
    capture_queue: OnceLock<capture::CaptureQueue>,
}

impl MemoryService {
    pub(crate) fn new(
        dirs: HostDirs,
        http: reqwest::Client,
        events: EventSink<HostEvent>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            store: axo_memory::Memory::open(dirs.data.join("memory")),
            dirs,
            http,
            events,
            runtime: OnceLock::new(),
            lifecycle: Mutex::new(Lifecycle {
                enabled: false,
                generation: 0,
                failure: None,
                download: None,
                model: ModelState::Missing,
            }),
            capture_queue: OnceLock::new(),
        });
        service.refresh(false);
        service
    }

    /// The runtime, bound on first use from inside one; binding it starts the embedding loop.
    fn runtime(&self) -> Option<&Handle> {
        if let Some(handle) = self.runtime.get() {
            return Some(handle);
        }
        let handle = Handle::try_current().ok()?;
        Some(self.runtime.get_or_init(|| {
            handle.spawn(embed_loop(Arc::clone(&self.store)));
            handle
        }))
    }

    fn settings(&self, folder: Option<&Path>) -> MemorySettings {
        resolve_effective_settings_for_folder_path(&self.dirs, folder)
            .settings
            .memory
    }

    fn bank_dir(&self, settings: &MemorySettings, folder: &Path, name: &str) -> Result<PathBuf> {
        Ok(
            match settings
                .banks
                .get(name)
                .and_then(|bank| bank.path.as_deref())
            {
                None | Some("") => self.store.default_bank_dir(name),
                Some(path) if path.starts_with("~/") => crate::services::folders::home_dir()
                    .context("HOME is not set")?
                    .join(&path[2..]),
                Some(path) => folder.join(path),
            },
        )
    }

    /// Every configured bank with its directory, which may be relative when `folder` is.
    fn bank_dirs(
        &self,
        settings: &MemorySettings,
        folder: &Path,
    ) -> Result<Vec<(String, PathBuf)>> {
        let mut names: Vec<_> = settings.banks.keys().cloned().collect();
        for name in [&settings.bank, &"main".to_owned()] {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
            .into_iter()
            .map(|name| {
                let dir = self.bank_dir(settings, folder, &name)?;
                Ok((name, dir))
            })
            .collect()
    }

    /// The banks a session in `folder` reaches, or `None` when memory is off there.
    pub(crate) fn active(&self, folder: &Path) -> Option<Banks> {
        self.banks(&self.settings(Some(folder)), folder)
    }

    /// [`Self::active`] with `folder`'s settings already loaded.
    fn banks(&self, settings: &MemorySettings, folder: &Path) -> Option<Banks> {
        if !settings.enabled || settings.exclude {
            return None;
        }
        let banks = self.bank_dirs(settings, folder).and_then(|dirs| {
            let banks = dirs
                .into_iter()
                .map(|(name, dir)| Bank::new(name, dir))
                .collect::<axo_memory::Result<Vec<_>>>()?;
            Ok(Banks::new(banks, &settings.bank)?)
        });
        match banks {
            Ok(banks) => Some(banks),
            Err(error) => {
                tracing::warn!(%error, "Could not resolve memory banks");
                None
            }
        }
    }

    /// Refuses a live session's memory once settings disable it or change its banks.
    fn guard(self: &Arc<Self>, folder: &Path) -> axo_memory::Guard {
        let service = Arc::clone(self);
        let folder = folder.to_owned();
        Arc::new(move |banks: &Banks| {
            service.refresh(false);
            match service.active(&folder) {
                None => Err("Memory is disabled for this folder".into()),
                Some(current) if current != *banks => Err(
                    "Memory banks changed; start a new chat to use the new configuration".into(),
                ),
                Some(_) => Ok(()),
            }
        })
    }

    pub(crate) fn host_tools(self: &Arc<Self>, folder: &Path) -> Vec<HostTool> {
        self.refresh(false);
        let Some(banks) = self.active(folder) else {
            return Vec::new();
        };
        let tool: Arc<dyn Tool> =
            Arc::new(SearchTool::new(Arc::clone(&self.store), banks).guard(self.guard(folder)));
        vec![HostTool::direct(Arc::new(move |_: &Arc<SessionTools>| {
            Arc::clone(&tool)
        }))]
    }

    pub(crate) fn scheme(self: &Arc<Self>, folder: &Path) -> Option<Arc<dyn axo::Scheme>> {
        let banks = self.active(folder)?;
        Some(Arc::new(MemoryScheme::new(banks).guard(self.guard(folder))))
    }

    /// Called on host startup and global settings notifications, including external edits.
    pub(crate) fn refresh(self: &Arc<Self>, retry: bool) {
        let settings = self.settings(None);
        let enabled = settings.enabled && settings.semantic;
        let mut lifecycle = self.lifecycle.lock();
        if !enabled {
            lifecycle.enabled = false;
            lifecycle.generation += 1;
            if let Some(task) = lifecycle.download.take() {
                task.abort();
            }
            lifecycle.model = ModelState::Missing;
            self.store.set_embedder(None);
            return;
        }
        if retry {
            lifecycle.failure = None;
        } else if lifecycle.enabled {
            return;
        }
        let Some(runtime) = self.runtime() else {
            return;
        };
        lifecycle.enabled = true;
        if let Some(error) = &lifecycle.failure {
            lifecycle.model = ModelState::Failed(error.clone());
            return;
        }
        if lifecycle.download.is_some() {
            return;
        }
        lifecycle.generation += 1;
        let generation = lifecycle.generation;
        if BGE_SMALL.is_complete(&self.dirs.local_models_dir()) {
            lifecycle.model = ModelState::Ready;
            if !self.store.has_embedder() {
                self.load_embedder(runtime, generation);
            }
            return;
        }
        lifecycle.model = ModelState::Missing;
        let service = Arc::clone(self);
        let task = runtime.spawn(async move {
            // `progress` outlives the loop, so `changed` never fails while it runs.
            let (progress, mut updates) = watch::channel(ModelState::Missing);
            let root = service.dirs.local_models_dir();
            let download = BGE_SMALL.download(&service.http, &root, &progress);
            tokio::pin!(download);
            let result = loop {
                tokio::select! {
                    result = &mut download => break result.map_err(|error| error.to_string()),
                    _ = updates.changed() => {
                        let mut lifecycle = service.lifecycle.lock();
                        if lifecycle.generation == generation {
                            lifecycle.model = updates.borrow_and_update().clone();
                        }
                    }
                }
            };
            let mut lifecycle = service.lifecycle.lock();
            if lifecycle.generation != generation {
                return;
            }
            lifecycle.download = None;
            match result {
                Ok(()) => {
                    lifecycle.model = ModelState::Ready;
                    if let Some(runtime) = service.runtime() {
                        service.load_embedder(runtime, generation);
                    }
                }
                Err(error) => {
                    lifecycle.model = ModelState::Failed(error.clone());
                    lifecycle.failure = Some(error);
                }
            }
        });
        lifecycle.download = Some(task.abort_handle());
    }

    /// Loads the model off the runtime and hands it to the store, unless `generation`
    /// is stale by then.
    fn load_embedder(self: &Arc<Self>, runtime: &Handle, generation: u64) {
        let service = Arc::clone(self);
        runtime.spawn_blocking(move || {
            let result = BgeSmall::load(&BGE_SMALL.path(&service.dirs.local_models_dir()));
            let mut lifecycle = service.lifecycle.lock();
            if lifecycle.generation != generation {
                return;
            }
            match result {
                Ok(embedder) => service.store.set_embedder(Some(Arc::new(embedder))),
                Err(error) => {
                    lifecycle.failure = Some(error.to_string());
                    lifecycle.model = ModelState::Failed(error.to_string());
                }
            }
        });
    }

    pub(crate) async fn status(self: &Arc<Self>) -> Result<MemoryStatus> {
        self.refresh(false);
        let settings = self.settings(None);
        let banks = self.bank_dirs(&settings, Path::new(""))?;
        let mut rows = Vec::with_capacity(banks.len());
        for (name, dir) in banks {
            let notes = match Bank::new(name.clone(), dir.clone()) {
                Ok(bank) if settings.enabled => self.store.note_count(&bank).await?,
                _ => 0,
            };
            rows.push(MemoryBankStatus {
                default: name == settings.bank,
                name,
                exists: dir.is_absolute() && dir.is_dir(),
                path: dir.to_string_lossy().into_owned(),
                notes,
            });
        }
        let mut embed = MemoryEmbedStatus {
            state: MemoryEmbedState::Disabled,
            received: 0,
            total: 0,
            error: None,
        };
        let lifecycle = self.lifecycle.lock();
        if lifecycle.enabled {
            match &lifecycle.model {
                ModelState::Missing => embed.state = MemoryEmbedState::Missing,
                ModelState::Downloading { received, total } => {
                    embed.state = MemoryEmbedState::Downloading;
                    embed.received = *received;
                    embed.total = *total;
                }
                ModelState::Ready => embed.state = MemoryEmbedState::Ready,
                ModelState::Failed(error) => {
                    embed.state = MemoryEmbedState::Failed;
                    embed.error = Some(error.clone());
                }
            }
        }
        Ok(MemoryStatus {
            enabled: settings.enabled,
            embed,
            banks: rows,
        })
    }

    pub(crate) async fn ensure_bank(self: &Arc<Self>, name: String) -> Result<PathBuf> {
        ensure!(
            axo_memory::valid_bank_name(&name),
            "Invalid memory bank name"
        );
        let service = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let settings = service.settings(None);
            ensure!(settings.enabled, "Memory is disabled");
            let path = service.bank_dir(&settings, Path::new(""), &name)?;
            ensure!(
                path.is_absolute(),
                "This bank has a relative path; open it from its project folder"
            );
            std::fs::create_dir_all(&path)?;
            Ok(path.canonicalize()?)
        })
        .await?
    }

    pub(crate) fn captures(&self, query: MemoryCaptureQuery) -> Result<Vec<MemoryCaptureRun>> {
        Ok(self
            .store
            .captures(
                query.session_id.as_deref(),
                query.bank.as_deref(),
                query.limit,
            )?
            .into_iter()
            .map(capture::to_protocol)
            .collect())
    }

    /// Removes a captured fact's line and, for an updated fact, reopens the observation it closed.
    pub(crate) async fn revert_fact(self: &Arc<Self>, fact_id: i64) -> Result<MemoryCaptureRun> {
        let store = Arc::clone(&self.store);
        let run = tokio::task::spawn_blocking(move || store.capture_of_fact(fact_id)).await??;
        let banks = self
            .active(Path::new(&run.cwd))
            .context("Memory is disabled for this folder")?;
        let run = capture::to_protocol(self.store.revert_fact(fact_id, &banks).await?);
        self.emit(run.clone(), false);
        Ok(run)
    }
}

/// Embeds what the store reports waiting, for the life of the runtime.
async fn embed_loop(store: Arc<axo_memory::Memory>) {
    loop {
        store.embedding_wanted().await;
        let worker = Arc::clone(&store);
        let result = tokio::task::spawn_blocking(move || {
            while worker.embed_pending()? {}
            Ok::<_, axo_memory::Error>(())
        })
        .await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "Memory embedding failed"),
            Err(error) => tracing::warn!(%error, "Memory embedding task failed"),
        }
    }
}
