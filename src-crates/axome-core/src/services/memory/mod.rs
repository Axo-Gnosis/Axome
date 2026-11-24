pub(crate) mod captures;
pub mod embed;
pub(crate) mod scheme;
pub mod tools;
pub(crate) mod writer;

use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{Arc, LazyLock, OnceLock},
};

use anyhow::{anyhow, ensure, Context, Result};
use axome_memory::{index::EMBED_MODEL, links, search, sync, Index};
use axome_protocol::{
    memory::{MemoryBankStatus, MemoryEmbedState, MemoryEmbedStatus, MemoryStatus},
    settings::{MemorySettings, MEMORY_BANK_NAME_PATTERN},
};
use parking_lot::Mutex;
use regex::Regex;
use tokio::{runtime::Handle, sync::watch};

use crate::{
    events::{EventSink, HostEvent},
    services::{
        db::DatabaseService, settings_resolution::resolve_effective_settings_for_folder_path,
    },
    HostDirs,
};
use embed::{EmbedState, Embedder};

#[derive(Clone, Debug)]
pub struct BankRef {
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct FolderMemory {
    pub bank: BankRef,
    pub banks: Vec<BankRef>,
    folder: PathBuf,
}

#[derive(Default)]
pub struct SearchRequest {
    pub query: String,
    pub types: Vec<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub path: Option<String>,
    pub after: Option<String>,
    pub valid_at: Option<String>,
    pub all_banks: bool,
    pub limit: usize,
}

/// A search hit with its note's `memory://` URL and, on the first hit from each note,
/// the notes it links to and from.
#[derive(serde::Serialize)]
pub struct MemoryHit {
    #[serde(flatten)]
    pub hit: search::SearchHit,
    pub url: String,
    pub links: Vec<NoteLink>,
}

#[derive(serde::Serialize)]
pub struct NoteLink {
    pub relation_type: String,
    pub direction: links::Direction,
    pub url: String,
}

// ponytail: links shown per hit; a heavily linked note lists only its first ones.
const MAX_LINKS: usize = 10;

#[derive(Default)]
struct Lifecycle {
    enabled: bool,
    generation: u64,
    failure: Option<String>,
    download: Option<tokio::task::AbortHandle>,
}

#[derive(Default)]
struct EmbeddingWork {
    running: bool,
    dirty: bool,
}

pub struct MemoryService {
    dirs: HostDirs,
    http: reqwest::Client,
    db: Arc<DatabaseService>,
    events: EventSink<HostEvent>,
    runtime: OnceLock<Handle>,
    index: Mutex<Option<Index>>,
    pub(crate) embedder: Mutex<Option<Arc<Embedder>>>,
    pub embed_state: watch::Sender<EmbedState>,
    lifecycle: Mutex<Lifecycle>,
    embedding_work: Mutex<EmbeddingWork>,
    capture_queue: OnceLock<writer::CaptureQueue>,
}

impl MemoryService {
    pub fn new(
        dirs: HostDirs,
        http: reqwest::Client,
        db: Arc<DatabaseService>,
        events: EventSink<HostEvent>,
    ) -> Arc<Self> {
        if let Err(error) = captures::interrupt_unfinished(&db) {
            tracing::warn!(%error, "Could not close interrupted memory captures");
        }
        let (embed_state, _) = watch::channel(EmbedState::Disabled);
        let service = Arc::new(Self {
            dirs,
            http,
            db,
            events,
            runtime: OnceLock::new(),
            index: Mutex::new(None),
            embedder: Mutex::new(None),
            embed_state,
            lifecycle: Mutex::new(Lifecycle::default()),
            embedding_work: Mutex::new(EmbeddingWork::default()),
            capture_queue: OnceLock::new(),
        });
        service.refresh(false);
        service
    }

    fn runtime(&self) -> Option<&Handle> {
        if let Some(handle) = self.runtime.get() {
            return Some(handle);
        }
        let handle = Handle::try_current().ok()?;
        Some(self.runtime.get_or_init(|| handle))
    }

    fn settings(&self, folder: Option<&Path>) -> MemorySettings {
        resolve_effective_settings_for_folder_path(&self.dirs, folder)
            .settings
            .memory
    }

    pub fn bank_dir(
        &self,
        settings: &MemorySettings,
        folder: &Path,
        name: &str,
    ) -> Result<PathBuf> {
        Ok(
            match settings
                .banks
                .get(name)
                .and_then(|bank| bank.path.as_deref())
            {
                None | Some("") => self.dirs.data.join("memory/banks").join(name),
                Some(path) if path.starts_with("~/") => crate::services::folders::home_dir()
                    .context("HOME is not set")?
                    .join(&path[2..]),
                Some(path) => folder.join(path),
            },
        )
    }

    fn banks(&self, settings: &MemorySettings, folder: &Path) -> Result<Vec<BankRef>> {
        let mut names: Vec<_> = settings.banks.keys().cloned().collect();
        for name in [&settings.bank, &"main".to_owned()] {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
            .into_iter()
            .map(|name| {
                Ok(BankRef {
                    dir: self.bank_dir(settings, folder, &name)?,
                    name,
                })
            })
            .collect()
    }

    pub fn active(&self, folder: &Path) -> Option<FolderMemory> {
        let settings = self.settings(Some(folder));
        if !settings.enabled || settings.exclude {
            return None;
        }
        let banks = match self.banks(&settings, folder) {
            Ok(banks) => banks,
            Err(error) => {
                tracing::warn!(%error, "Could not resolve memory banks");
                return None;
            }
        };
        Some(FolderMemory {
            bank: banks
                .iter()
                .find(|bank| bank.name == settings.bank)?
                .clone(),
            banks,
            folder: folder.to_owned(),
        })
    }

    fn ensure_active(&self, fm: &FolderMemory) -> Result<()> {
        let current = self
            .active(&fm.folder)
            .context("Memory is disabled for this folder")?;
        ensure!(
            current.bank.name == fm.bank.name
                && current.bank.dir == fm.bank.dir
                && current
                    .banks
                    .iter()
                    .map(|b| (&b.name, &b.dir))
                    .eq(fm.banks.iter().map(|b| (&b.name, &b.dir))),
            "Memory banks changed; start a new chat to use the new configuration"
        );
        Ok(())
    }

    fn model_dir(&self) -> PathBuf {
        self.dirs.data.join("memory/models/bge-small-en-v1.5")
    }

    pub fn semantic_ready(&self) -> bool {
        matches!(*self.embed_state.borrow(), EmbedState::Ready)
    }

    /// Called on host startup and global settings notifications, including external edits.
    pub fn refresh(self: &Arc<Self>, retry: bool) {
        let settings = self.settings(None);
        let enabled = settings.enabled && settings.semantic;
        let mut lifecycle = self.lifecycle.lock();
        if !enabled {
            lifecycle.enabled = false;
            lifecycle.generation += 1;
            if let Some(task) = lifecycle.download.take() {
                task.abort();
            }
            self.embed_state.send_replace(EmbedState::Disabled);
            self.embedder.lock().take();
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
            self.embed_state
                .send_replace(EmbedState::Failed(error.clone()));
            return;
        }
        if lifecycle.download.is_some() {
            return;
        }
        lifecycle.generation += 1;
        let generation = lifecycle.generation;
        if embed::files_complete(&self.model_dir()) {
            self.embed_state.send_replace(EmbedState::Ready);
            drop(lifecycle);
            self.schedule_embedding();
            return;
        }
        self.embed_state.send_replace(EmbedState::Missing);
        let service = Arc::clone(self);
        let task = runtime.spawn(async move {
            let (progress, mut updates) = watch::channel(EmbedState::Missing);
            let dir = service.model_dir();
            let download = embed::download(&service.http, &dir, &progress);
            tokio::pin!(download);
            let result = loop {
                tokio::select! {
                    result = &mut download => break result,
                    changed = updates.changed() => {
                        if changed.is_err() { break Err(anyhow!("Download progress channel closed")); }
                        let lifecycle = service.lifecycle.lock();
                        if lifecycle.generation == generation && lifecycle.enabled {
                            service.embed_state.send_replace(updates.borrow_and_update().clone());
                        }
                    }
                }
            };
            let mut lifecycle = service.lifecycle.lock();
            if lifecycle.generation != generation || !lifecycle.enabled { return; }
            lifecycle.download = None;
            match result {
                Ok(()) => { service.embed_state.send_replace(EmbedState::Ready); }
                Err(error) => {
                    lifecycle.failure = Some(error.to_string());
                    service.embed_state.send_replace(EmbedState::Failed(error.to_string()));
                }
            }
            drop(lifecycle);
            service.schedule_embedding();
        });
        lifecycle.download = Some(task.abort_handle());
    }

    fn load_embedder(&self) -> Result<Option<Arc<Embedder>>> {
        let mut lifecycle = self.lifecycle.lock();
        if !lifecycle.enabled || lifecycle.failure.is_some() || !self.semantic_ready() {
            return Ok(None);
        }
        let mut slot = self.embedder.lock();
        if let Some(embedder) = slot.as_ref() {
            return Ok(Some(Arc::clone(embedder)));
        }
        match Embedder::load(&self.model_dir()) {
            Ok(embedder) => {
                let embedder = Arc::new(embedder);
                *slot = Some(Arc::clone(&embedder));
                Ok(Some(embedder))
            }
            Err(error) => {
                lifecycle.failure = Some(error.to_string());
                self.embed_state
                    .send_replace(EmbedState::Failed(error.to_string()));
                Err(error)
            }
        }
    }

    fn with_index<T>(&self, f: impl FnOnce(&mut Index) -> Result<T>) -> Result<T> {
        ensure!(self.settings(None).enabled, "Memory is disabled");
        let mut slot = self.index.lock();
        if slot.is_none() {
            let dir = self.dirs.data.join("memory");
            std::fs::create_dir_all(&dir)?;
            *slot = Some(Index::open(&dir.join("index.db"))?);
        }
        f(slot.as_mut().unwrap())
    }

    pub(crate) fn sync(&self, index: &mut Index, banks: &[BankRef]) -> Result<bool> {
        // ponytail: scan on every call; add notify if the bank walk shows up in profiles.
        let mut changed = false;
        for bank in banks {
            changed |= sync::sync_bank(index, &bank.dir)?.has_changes();
        }
        Ok(changed)
    }

    pub(crate) fn bank_id(index: &Index, bank: &BankRef) -> Result<i64> {
        Ok(index.upsert_bank(&bank.dir)?)
    }

    async fn indexed<T: Send + 'static>(
        self: &Arc<Self>,
        fm: &FolderMemory,
        banks: Vec<BankRef>,
        f: impl FnOnce(&Index) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let service = Arc::clone(self);
        let fm = fm.clone();
        tokio::task::spawn_blocking(move || {
            service.ensure_active(&fm)?;
            let result = service.with_index(|index| {
                service.sync(index, &banks)?;
                f(index)
            });
            service.schedule_embedding();
            result
        })
        .await?
    }

    pub fn schedule_embedding(self: &Arc<Self>) {
        if !self.semantic_ready() {
            return;
        }
        let Some(runtime) = self.runtime() else {
            return;
        };
        let mut work = self.embedding_work.lock();
        work.dirty = true;
        if work.running {
            return;
        }
        work.running = true;
        let service = Arc::clone(self);
        runtime.spawn(async move {
            loop {
                service.embedding_work.lock().dirty = false;
                let worker = Arc::clone(&service);
                let result = tokio::task::spawn_blocking(move || worker.embed_pending()).await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => tracing::warn!(%error, "Memory embedding failed"),
                    Err(error) => tracing::warn!(%error, "Memory embedding task failed"),
                }
                let mut work = service.embedding_work.lock();
                if !work.dirty {
                    work.running = false;
                    break;
                }
            }
        });
    }

    fn embed_pending(&self) -> Result<()> {
        // Do not create an index just because the model became available.
        if !self.dirs.data.join("memory/index.db").exists() {
            return Ok(());
        }
        let Some(embedder) = self.load_embedder()? else {
            return Ok(());
        };
        loop {
            if !self.semantic_ready() || !self.settings(None).enabled {
                return Ok(());
            }
            let chunks =
                self.with_index(|index| Ok(index.chunks_needing_embedding(EMBED_MODEL, 16)?))?;
            if chunks.is_empty() {
                return Ok(());
            }
            let texts: Vec<String> = chunks.iter().map(|(_, text)| text.clone()).collect();
            // Inference runs unlocked; the text guard skips a chunk whose ID sync recycled meanwhile.
            let vectors = embedder.embed_passages(&texts)?;
            self.with_index(|index| {
                for ((id, text), vector) in chunks.iter().zip(vectors) {
                    index.set_embedding(*id, EMBED_MODEL, text, &vector)?;
                }
                Ok(())
            })?;
        }
    }

    pub async fn search(
        self: &Arc<Self>,
        fm: &FolderMemory,
        req: SearchRequest,
    ) -> Result<Vec<MemoryHit>> {
        self.refresh(false);
        let service = Arc::clone(self);
        let query = req.query.clone();
        let embedding = tokio::task::spawn_blocking(move || match service.load_embedder() {
            Ok(Some(embedder)) => embedder.embed_query(&query).map(Some),
            Ok(None) => Ok(None),
            Err(error) => {
                tracing::warn!(%error, "Memory search is keyword-only");
                Ok(None)
            }
        })
        .await??;
        let banks = if req.all_banks {
            fm.banks.clone()
        } else {
            vec![fm.bank.clone()]
        };
        let selected = banks.clone();
        let memory = fm.clone();
        self.indexed(fm, banks, move |index| {
            // Links cross banks, so every bank this folder reaches names its URLs.
            let names = memory
                .banks
                .iter()
                .map(|bank| Ok((Self::bank_id(index, bank)?, bank.name.clone())))
                .collect::<Result<HashMap<_, _>>>()?;
            let ids = selected
                .iter()
                .map(|bank| Self::bank_id(index, bank))
                .collect::<Result<Vec<_>>>()?;
            let types = req.types.iter().map(String::as_str).collect::<Vec<_>>();
            let hits = search::search(
                index,
                &search::SearchQuery {
                    bank_ids: &ids,
                    text: &req.query,
                    query_embedding: embedding.as_deref(),
                    row_types: &types,
                    categories: &req.categories,
                    tags: &req.tags,
                    path_prefix: req.path.as_deref(),
                    after: req.after.as_deref(),
                    valid_at: req.valid_at.as_deref(),
                    limit: req.limit.clamp(1, 50),
                    ..Default::default()
                },
            )?;
            let url = |bank_id: i64, file_path: &str| {
                format!(
                    "memory://{}/{file_path}",
                    names.get(&bank_id).map_or("?", String::as_str)
                )
            };
            let resolvers = std::cell::RefCell::new(HashMap::<String, links::Resolver>::new());
            let resolve_cross = |name: &str, target: &str| -> axome_memory::Result<Option<i64>> {
                let Some(bank) = memory.banks.iter().find(|bank| bank.name == name) else {
                    return Ok(None);
                };
                let mut resolvers = resolvers.borrow_mut();
                if !resolvers.contains_key(name) {
                    let resolver = links::Resolver::load(index, index.upsert_bank(&bank.dir)?)?;
                    resolvers.insert(name.to_owned(), resolver);
                }
                Ok(resolvers[name].resolve(target))
            };
            let mut listed = HashSet::new();
            hits.into_iter()
                .map(|hit| {
                    let mut note_links = Vec::new();
                    if listed.insert(hit.entity_id) {
                        for neighbor in links::neighbors(index, hit.entity_id, &resolve_cross)?
                            .into_iter()
                            .take(MAX_LINKS)
                        {
                            if let Some(entity) = index.entity(neighbor.entity_id)? {
                                note_links.push(NoteLink {
                                    relation_type: neighbor.relation_type,
                                    direction: neighbor.direction,
                                    url: url(entity.bank_id, &entity.file_path),
                                });
                            }
                        }
                    }
                    Ok(MemoryHit {
                        url: url(hit.bank_id, &hit.file_path),
                        hit,
                        links: note_links,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn status(self: &Arc<Self>, folder: Option<&Path>) -> Result<MemoryStatus> {
        self.refresh(false);
        let service = Arc::clone(self);
        let folder = folder.map(Path::to_owned);
        tokio::task::spawn_blocking(move || {
            let settings = service.settings(folder.as_deref());
            let banks = service.banks(&settings, folder.as_deref().unwrap_or(Path::new("")))?;
            let mut rows = Vec::with_capacity(banks.len());
            for bank in banks {
                let exists = bank.dir.is_absolute() && bank.dir.is_dir();
                let notes = if settings.enabled && bank.dir.is_absolute() {
                    service.with_index(|index| {
                        service.sync(index, std::slice::from_ref(&bank))?;
                        Ok(index.note_count(Self::bank_id(index, &bank)?)?)
                    })?
                } else {
                    0
                };
                rows.push(MemoryBankStatus {
                    default: bank.name == settings.bank,
                    name: bank.name,
                    path: bank.dir.to_string_lossy().into_owned(),
                    exists,
                    notes,
                });
            }
            let mut embed = MemoryEmbedStatus {
                state: MemoryEmbedState::Disabled,
                received: 0,
                total: 0,
                error: None,
            };
            match &*service.embed_state.borrow() {
                EmbedState::Disabled => {}
                EmbedState::Missing => embed.state = MemoryEmbedState::Missing,
                EmbedState::Downloading { received, total } => {
                    embed.state = MemoryEmbedState::Downloading;
                    embed.received = *received;
                    embed.total = *total;
                }
                EmbedState::Ready => embed.state = MemoryEmbedState::Ready,
                EmbedState::Failed(error) => {
                    embed.state = MemoryEmbedState::Failed;
                    embed.error = Some(error.clone());
                }
            }
            service.schedule_embedding();
            Ok(MemoryStatus {
                enabled: settings.enabled,
                embed,
                banks: rows,
            })
        })
        .await?
    }

    pub async fn ensure_bank(self: &Arc<Self>, name: String) -> Result<PathBuf> {
        ensure!(valid_bank_name(&name), "Invalid memory bank name");
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
}

fn valid_bank_name(name: &str) -> bool {
    static BANK_NAME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(MEMORY_BANK_NAME_PATTERN).unwrap());
    BANK_NAME.is_match(name)
}

fn checked_path(bank: &BankRef, path: &Path) -> Result<PathBuf> {
    let relative = path
        .strip_prefix(&bank.dir)
        .context("Note is outside its bank")?;
    ensure!(
        relative
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "Invalid note path"
    );
    let mut current = bank.dir.clone();
    for component in relative.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "Memory notes cannot traverse symlinks"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(current)
}

pub(crate) fn atomic_write(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("Note has no parent directory")?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".axome-memory-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
