mod bundled;
mod commands;
mod config;
mod convert;
mod engine;
mod env;
pub(crate) mod extensions;
mod live;
mod login;
mod plan;
mod providers;
#[cfg(test)]
mod tests;
mod web;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    ops::{ControlFlow, Deref},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock, Weak,
    },
    time::{Duration, SystemTime},
};

use crate::{HostDirs, SUPERVISOR_ROUTE};
use axo::store::{Entry, EntryId};
use axo::{Axo, NavigateOptions, ProjectAccess, RecoveryPolicy, SessionOptions};
use axo_ai::{FileCredentialStore, ServiceTier, ThinkingLevel};
use axome_protocol::axo::AxoKeywordInfo;
use axome_protocol::axo_settings::AxoSettings;
use axome_protocol::chat::*;
use axome_protocol::git::DiffFileContent;
use axome_protocol::memory::MemoryCaptureTrigger;
use axome_protocol::server_config::Supervision;
use axome_protocol::session::{IndexedChildSession, MessageRole};
use chrono::{DateTime, SecondsFormat, Utc};
use engine::Engine;
use extensions::{ExtensionOwner, ExtensionState, ProviderExtensionOrigin, SessionExtensions};
use parking_lot::Mutex;
use tokio::runtime::Handle;

use crate::{
    errors::ApiError,
    events::{deliver, EventSink, HostEvent},
    services::{
        db::DatabaseService,
        history::{self, HistoryRow, Listing},
        session_index::{SessionIndexService, SourceId},
    },
};
use live::LiveSession;

/// Where a session's stored data is read from: the live session, or its closed file.
enum SessionTarget {
    Live(Arc<LiveSession>),
    Closed(Arc<Axo>, PathBuf),
}

fn model_choice_key(
    choice: &axo::ModelChoice,
) -> (
    Option<(&str, &str)>,
    Option<ThinkingLevel>,
    Option<ServiceTier>,
) {
    (
        choice
            .model
            .as_ref()
            .map(|model| (model.provider.as_str(), model.id.as_str())),
        choice.thinking,
        choice.tier,
    )
}

/// Text contents for a snapshot diff; binary when either side has a NUL byte or is not UTF-8.
fn diff_content(before: Option<Vec<u8>>, after: Vec<u8>) -> DiffFileContent {
    let text = |bytes: Vec<u8>| {
        (!bytes.contains(&0))
            .then(|| String::from_utf8(bytes).ok())
            .flatten()
    };
    match (before.map(text), text(after)) {
        (Some(None), _) | (_, None) => DiffFileContent {
            old_content: None,
            new_content: None,
            binary: true,
        },
        (old, new) => DiffFileContent {
            old_content: old.flatten(),
            new_content: new,
            binary: false,
        },
    }
}

/// A session's scheme table, borrowed from the live session or built for a closed one.
enum SessionSchemes {
    Live(Arc<LiveSession>),
    Closed(axo::Schemes),
}

impl SessionSchemes {
    fn get(&self) -> &axo::Schemes {
        match self {
            Self::Live(live) => live.session.schemes(),
            Self::Closed(schemes) => schemes,
        }
    }
}

pub(crate) struct AxoService {
    this: Weak<Self>,
    db: Arc<DatabaseService>,
    runtime: OnceLock<Handle>,
    dirs: HostDirs,
    bun: Option<PathBuf>,
    supervision: Supervision,
    events: EventSink<HostEvent>,
    engine: tokio::sync::Mutex<EngineSlot>,
    credentials: Arc<FileCredentialStore>,
    web: Arc<web::WebTools>,
    pub(crate) browser: Arc<axome_browser::BrowserService>,
    pub(crate) memory: Arc<crate::services::memory::MemoryService>,
    login: tokio::sync::Mutex<Option<login::Login>>,
    sessions: Mutex<HashMap<String, Arc<LiveSession>>>,
    // Attachments to closed child sessions, by session id: (attach id, sink).
    readonly_subscribers: Mutex<HashMap<String, Vec<Subscriber>>>,
    // Reopening must wait for the retiring Session to release its file lock.
    retiring: Mutex<HashMap<String, RetiringSlot>>,
    // ponytail: one open/create at a time; per-id locks if opens contend.
    opening: tokio::sync::Mutex<()>,
    index: OnceLock<Weak<SessionIndexService>>,
    closed: AtomicBool,
}

type RetiringSlot = Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>;
type Subscriber = (String, EventSink<ChatEvent>);

enum Loaded<'a> {
    Live(Held<'a>),
    /// A child whose root is not loaded; nothing was opened.
    RootClosed {
        engine: Arc<Engine>,
        infos: Vec<axo::SessionInfo>,
    },
}

fn children_of<'a>(
    sessions: &'a HashMap<String, Arc<LiveSession>>,
    id: &'a str,
) -> impl Iterator<Item = &'a String> + 'a {
    sessions
        .iter()
        .filter(move |(_, live)| live.root_id.as_deref() == Some(id))
        .map(|(id, _)| id)
}

#[derive(Default)]
struct EngineSlot {
    engine: Option<Arc<Engine>>,
    models_watch: Option<tokio::task::JoinHandle<()>>,
    /// The `models.jsonc` error from the last build or reload.
    models_error: Option<String>,
    stamp: Option<(Option<(SystemTime, u64)>, bool)>,
}

impl Drop for EngineSlot {
    fn drop(&mut self) {
        if let Some(task) = self.models_watch.take() {
            task.abort();
        }
    }
}

pub(crate) fn project_access(_folder: &Path) -> ProjectAccess {
    // Workspace Trust (roadmap Phase 4) replaces this body.
    ProjectAccess::Trust
}

pub(super) fn axo_error(error: axo::Error) -> ApiError {
    match &error {
        axo::Error::Busy => {
            ApiError::InvalidArgument("axo is busy: a run or maintenance is active".into())
        }
        axo::Error::SessionBusy { .. }
        | axo::Error::NoModel
        | axo::Error::ModelNotFound(_)
        | axo::Error::InvalidCommand { .. }
        | axo::Error::AgentNotRoot(_)
        | axo::Error::AgentNotSubagent(_)
        | axo::Error::SpawnBlocked(_)
        | axo::Error::ChildSession { .. }
        | axo::Error::UnknownModelRole(_)
        | axo::Error::InvalidModelSelector { .. }
        | axo::Error::InvalidInput(_) => ApiError::InvalidArgument(error.to_string()),
        axo::Error::EntryNotFound(_)
        | axo::Error::NoPendingQuestion(_)
        | axo::Error::ModeNotFound(_)
        | axo::Error::AgentNotFound(_)
        | axo::Error::SessionNotFound(_)
        | axo::Error::NoPendingResult(_)
        | axo::Error::NotQueued(_) => ApiError::NotFound(error.to_string()),
        axo::Error::Ai(_)
        | axo::Error::Completion { .. }
        | axo::Error::SkillNotFound(_)
        | axo::Error::Agent(_)
        | axo::Error::Io { .. }
        | axo::Error::Config { .. }
        | axo::Error::Scheme(_)
        | axo::Error::Host(_)
        | axo::Error::InvalidSessionHeader { .. }
        | axo::Error::Compaction(_)
        | axo::Error::MaintenanceAborted
        | axo::Error::ContextChanged
        | axo::Error::InvalidHistory(_) => ApiError::Internal(format!("axo: {error}")),
    }
}

/// Converts axo's byte ranges into the UTF-16 offsets the composer indexes by.
fn utf16_matches(text: &str, matches: Vec<axo::KeywordMatch>) -> Vec<ChatKeywordMatch> {
    // axo returns matches in text order and never overlapping, so one forward walk
    // converts every boundary.
    let (mut byte, mut units) = (0usize, 0u32);
    let mut utf16 = |to: usize| {
        units += text[byte..to].encode_utf16().count() as u32;
        byte = to;
        units
    };
    matches
        .into_iter()
        .map(|found| {
            let start = utf16(found.range.start);
            let end = utf16(found.range.end);
            ChatKeywordMatch {
                start,
                end,
                name: found.name,
            }
        })
        .collect()
}

/// How long a session with no file yet stays loaded while unattached: long enough for a
/// reconnect or a window move to reattach, since a retired unwritten session cannot be reopened.
const UNWRITTEN_GRACE: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(60)
};

impl AxoService {
    pub(crate) fn new(
        db: Arc<DatabaseService>,
        dirs: HostDirs,
        supervision: Supervision,
        bun: Option<PathBuf>,
        events: EventSink<HostEvent>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|this: &Weak<Self>| {
            let credentials = Arc::new(FileCredentialStore::new(
                dirs.data.join("axo").join("auth.json"),
            ));
            let web = web::WebTools::new(dirs.clone(), Arc::clone(&credentials), this.clone());
            let browser = Arc::new(axome_browser::BrowserService::new());
            let memory = crate::services::memory::MemoryService::new(
                dirs.clone(),
                web.http.clone(),
                Arc::clone(&events),
            );
            Self {
                this: this.clone(),
                db,
                runtime: OnceLock::new(),
                credentials,
                web,
                browser,
                memory,
                dirs,
                supervision,
                bun,
                events,
                engine: tokio::sync::Mutex::new(EngineSlot::default()),
                login: tokio::sync::Mutex::new(None),
                sessions: Mutex::new(HashMap::new()),
                readonly_subscribers: Mutex::new(HashMap::new()),
                retiring: Mutex::new(HashMap::new()),
                opening: tokio::sync::Mutex::new(()),
                index: OnceLock::new(),
                closed: AtomicBool::new(false),
            }
        })
    }

    #[cfg(test)]
    pub(crate) async fn with_engine_and_events(
        db: Arc<DatabaseService>,
        dirs: HostDirs,
        engine: Engine,
        events: EventSink<HostEvent>,
    ) -> Arc<Self> {
        let service = Self::new(db, dirs, Supervision::ProcessGroup, None, events);
        service.bind_runtime();
        {
            let mut slot = service.engine.lock().await;
            engine.extensions.initialize(&service).await.unwrap();
            slot.models_watch = Some(service.watch_models(&engine.models));
            slot.engine = Some(Arc::new(engine));
            slot.models_error = None;
            slot.stamp = Some((None, false));
        }
        service
    }

    fn runtime(&self) -> Option<&Handle> {
        if let Some(handle) = self.runtime.get() {
            return Some(handle);
        }
        let handle = Handle::try_current().ok()?;
        Some(self.runtime.get_or_init(|| handle))
    }

    #[cfg(test)]
    pub(crate) fn bind_runtime(&self) {
        let _ = self.runtime();
    }
    pub(crate) fn bind_index(&self, index: &Arc<SessionIndexService>) {
        let _ = self.index.set(Arc::downgrade(index));
    }
    pub(crate) fn block_on<F: Future>(&self, f: F) -> Result<F::Output, ApiError> {
        Ok(self
            .runtime()
            .ok_or_else(|| ApiError::Internal("axo has no runtime".into()))?
            .block_on(f))
    }

    pub(crate) async fn engine(&self) -> Result<Arc<Engine>, ApiError> {
        let _ = self.runtime();
        let mut slot = self.engine.lock().await;
        if self.closed.load(Ordering::Acquire) {
            return Err(ApiError::Internal("Axome is shutting down".into()));
        }
        let path = self.dirs.models_path();
        let models_stamp = match tokio::fs::metadata(&path).await {
            Ok(meta) => Some((
                meta.modified().map_err(|e| ApiError::Io(e.to_string()))?,
                meta.len(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(ApiError::Io(format!("{}: {error}", path.display()))),
        };
        let extended_context = engine::read_extended_context(&self.dirs)?;
        let stamp = Some((models_stamp, extended_context));
        let engine = match slot.engine.clone() {
            None => {
                let supervision = match self.supervision {
                    Supervision::ProcessGroup => axo::Supervision::ProcessGroup,
                    Supervision::Cgroup => axo::Supervision::Cgroup {
                        program: std::env::current_exe()
                            .map_err(|error| ApiError::Internal(error.to_string()))?,
                        args: vec![SUPERVISOR_ROUTE.into()],
                        cgroup_root: None,
                    },
                };
                let (engine, models_error) = Engine::build(
                    &self.dirs,
                    &self.db,
                    self.credentials.clone(),
                    supervision,
                    extended_context,
                    self.bun.as_deref(),
                )
                .await?;
                let engine = Arc::new(engine);
                engine.extensions.initialize(self).await?;
                slot.models_watch = Some(self.watch_models(&engine.models));
                slot.engine = Some(engine.clone());
                slot.models_error = models_error;
                slot.stamp = stamp;
                engine
            }
            Some(engine) => {
                if slot.stamp != stamp {
                    slot.models_error =
                        engine::reload(&engine.models, &self.dirs, extended_context).await?;
                    slot.stamp = stamp;
                }
                engine
            }
        };
        Ok(engine)
    }

    /// Applies a `models.jsonc` edit reported by the settings watcher (called off the runtime).
    pub(crate) fn models_file_changed(&self) {
        let (Some(service), Some(runtime)) = (self.this.upgrade(), self.runtime()) else {
            return;
        };
        runtime.spawn(async move {
            // Nothing to reload yet: the first `engine()` reads the file.
            if service.engine.lock().await.engine.is_none() {
                return;
            }
            // `engine()` compares the stamp under the slot lock, so a burst of watcher events
            // reloads at most once per file change.
            if let Err(error) = service.engine().await {
                tracing::warn!(%error, "models.jsonc reload failed");
            }
        });
    }

    /// Re-runs model discovery after a credential or config change.
    pub(crate) async fn reload_models(&self) -> Result<(), ApiError> {
        let mut slot = self.engine.lock().await;
        if let Some(engine) = slot.engine.as_ref() {
            slot.models_error = engine::reload(
                &engine.models,
                &self.dirs,
                engine::read_extended_context(&self.dirs)?,
            )
            .await?;
        }
        Ok(())
    }

    /// Fetches the model catalog and re-runs discovery; the registry keeps its previous catalog
    /// when the fetch fails.
    pub(crate) async fn refresh_models(&self) -> Result<(), ApiError> {
        let engine = self.engine().await?;
        self.reload_models().await?;
        engine
            .models
            .refresh()
            .await
            .map_err(|error| ApiError::Internal(format!("Model catalog refresh failed: {error}")))
    }

    /// The `models.jsonc` error from the last build or reload, if any.
    pub(crate) async fn models_error(&self) -> Option<String> {
        self.engine.lock().await.models_error.clone()
    }

    pub(super) fn watch_models(&self, models: &axo_ai::Models) -> tokio::task::JoinHandle<()> {
        let mut changes = models.subscribe();
        let events = self.events.clone();
        tokio::spawn(async move {
            while changes.changed().await.is_ok() {
                deliver(&events, HostEvent::ModelsChanged);
            }
        })
    }

    fn emit_models_changed(&self) {
        deliver(&self.events, HostEvent::ModelsChanged);
    }

    pub(super) async fn sync_models_after_credential_change(&self) {
        if let Err(error) = self.reload_models().await {
            tracing::warn!(%error, "Reloading models after credential change failed");
        }
        self.emit_models_changed();
    }

    async fn axo(&self) -> Result<Arc<Axo>, ApiError> {
        Ok(self.engine().await?.axo.clone())
    }

    async fn options<'a>(
        &self,
        engine: &'a Engine,
        state: &mut ExtensionState,
        folder: &Path,
        model: Option<String>,
        thinking: Option<ThinkingLevel>,
    ) -> Result<(SessionOptions, SessionExtensions, ExtensionOwner<'a>), ApiError> {
        let canonical = extensions::canonical_folder(folder)?;
        let scope = engine.extensions.scope(state, self, &canonical).await?;
        let (recorded, owner) = match scope {
            Some(scope) => (
                SessionExtensions::Project(canonical),
                ExtensionOwner::Project(scope),
            ),
            None => (
                SessionExtensions::Host,
                ExtensionOwner::Host(&engine.extensions.manager),
            ),
        };
        let plans_dir = config::plans_dir(&self.dirs, folder).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read plansDir; using the default");
            axome_protocol::axo_settings::DEFAULT_PLANS_DIR.to_owned()
        });
        let mut tools = self.web.host_tools(folder);
        tools.extend(
            self.browser
                .host_tools(config::browser_config(&self.dirs, folder)),
        );
        tools.extend(self.memory.host_tools(folder));
        let options = owner.compose(SessionOptions {
            cwd: folder.to_owned(),
            model,
            thinking,
            ask_enabled: true,
            project_access: Some(project_access(folder)),
            exec_env: Some(Arc::new(env::FolderExecEnv::new(
                self.db.clone(),
                folder.to_owned(),
            ))),
            host: axo::Registrations {
                definitions: bundled::definitions(bundled::session_modes(&plans_dir)),
                sections: Vec::new(),
                tools,
                schemes: self.host_schemes(folder),
                commands: commands::host_commands(),
                ..Default::default()
            },
            recovery: RecoveryPolicy::Resume,
            ..Default::default()
        });
        Ok((options, recorded, owner))
    }

    fn host_schemes(&self, folder: &Path) -> Vec<Arc<dyn axo::Scheme>> {
        let mut schemes = self.web.host_schemes();
        schemes.extend(self.memory.scheme(folder));
        schemes
    }

    fn ensure_open(&self) -> Result<(), ApiError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ApiError::Internal("Axome is shutting down".into()));
        }
        Ok(())
    }

    fn held(&self, folder: &Path, id: &str) -> Result<Option<Held<'_>>, ApiError> {
        let live = {
            let sessions = self.sessions.lock();
            let Some(live) = sessions.get(id) else {
                return Ok(None);
            };
            if live.folder != folder {
                return Err(ApiError::NotFound(format!("axo session {id}")));
            }
            live.pin();
            live.clone()
        };
        Ok(Some(Held {
            service: self,
            live,
        }))
    }

    async fn live(&self, folder: &Path, id: &str) -> Result<Held<'_>, ApiError> {
        match self.load(folder, id, false).await? {
            Loaded::Live(held) => Ok(held),
            Loaded::RootClosed { .. } => Err(ApiError::InvalidArgument(
                "Session is read-only until resumed".into(),
            )),
        }
    }

    async fn load(&self, folder: &Path, id: &str, open_root: bool) -> Result<Loaded<'_>, ApiError> {
        self.ensure_open()?;
        if let Some(held) = self.held(folder, id)? {
            return Ok(Loaded::Live(held));
        }
        let _opening = self.opening.lock().await;
        self.ensure_open()?;
        if let Some(held) = self.held(folder, id)? {
            return Ok(Loaded::Live(held));
        }
        self.wait_retiring(id).await;
        let engine = self.engine().await?;
        let infos = engine.axo.list_sessions(folder).await.map_err(axo_error)?;
        let (info, root_info) = find_session_and_root(&infos, id)?;
        if info.spawn.is_none() {
            return self
                .reopen(folder, &info.path, &engine)
                .await
                .map(Loaded::Live);
        }
        let root_held = match root_info {
            Some(root) => self.held(folder, &root.id)?,
            None => None,
        };
        let root_held = if let Some(held) = root_held {
            held
        } else {
            if !open_root {
                return Ok(Loaded::RootClosed { engine, infos });
            }
            let root = root_info
                .ok_or_else(|| ApiError::NotFound("Parent root session no longer exists".into()))?;
            self.wait_retiring(&root.id).await;
            self.reopen(folder, &root.path, &engine).await?
        };
        let child = engine.axo.session(id).await.map_err(axo_error)?;
        self.register(
            folder.to_owned(),
            child,
            Some(root_held.id.clone()),
            engine,
            (root_held.extension_owner.clone(), root_held.models.clone()),
            None,
        )
        .await
        .map(Loaded::Live)
    }

    async fn reopen(
        &self,
        folder: &Path,
        path: &Path,
        engine: &Arc<Engine>,
    ) -> Result<Held<'_>, ApiError> {
        let mut state = engine.extensions.mutation.lock().await;
        self.ensure_open()?;
        let (options, recorded, owner) =
            self.options(engine, &mut state, folder, None, None).await?;
        let models = options
            .models
            .clone()
            .unwrap_or_else(|| engine.models.clone());
        let session = engine
            .axo
            .open_session(path, options)
            .await
            .map_err(axo_error)?;
        self.register(
            folder.to_owned(),
            Arc::new(session),
            None,
            engine.clone(),
            (recorded, models),
            Some(owner),
        )
        .await
    }

    async fn register(
        &self,
        folder: PathBuf,
        session: Arc<axo::Session>,
        root_id: Option<String>,
        engine: Arc<Engine>,
        selection: (SessionExtensions, Arc<axo_ai::Models>),
        owner: Option<ExtensionOwner<'_>>,
    ) -> Result<Held<'_>, ApiError> {
        let is_root = root_id.is_none();
        if let Some(owner) = &owner {
            owner.attach(&session).await;
        }
        let diagnostics = owner
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.session_diagnostics(session.id()));
        let live = LiveSession::start(
            self.this.clone(),
            folder.clone(),
            session,
            root_id.clone(),
            engine,
            selection,
            diagnostics,
        )
        .await;
        live.pin();
        let failure = {
            let mut sessions = self.sessions.lock();
            if let Err(error) = self.ensure_open() {
                Some(error)
            } else if root_id
                .as_deref()
                .is_some_and(|rid| !sessions.contains_key(rid))
            {
                Some(ApiError::NotFound(
                    "Parent root session no longer exists".into(),
                ))
            } else {
                sessions.insert(live.id.clone(), live.clone());
                None
            }
        };
        if let Some(error) = failure {
            live.unpin();
            if let Some(owner) = &owner {
                owner.detach(&live.id);
            }
            live.release_detached().await;
            return Err(error);
        }
        if is_root {
            self.emit_summaries(&folder);
        }
        Ok(Held {
            service: self,
            live,
        })
    }

    pub(crate) fn live_summaries(&self, folder: &Path) -> Vec<ChatLiveSummary> {
        let lives: Vec<_> = self
            .sessions
            .lock()
            .values()
            .filter(|live| live.folder == folder && live.root_id.is_none())
            .cloned()
            .collect();
        let mut summaries: Vec<_> = lives.into_iter().map(|live| live.summary()).collect();
        summaries.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        summaries
    }

    pub(crate) fn emit_summaries(&self, folder: &Path) {
        let summaries = self.live_summaries(folder);
        deliver(
            &self.events,
            HostEvent::ChatSummariesChanged(ChatSummariesChangedEvent {
                folder_path: folder.to_string_lossy().into_owned(),
                summaries,
            }),
        );
    }

    pub(crate) fn active_folder(&self, session_id: &str) -> Option<String> {
        let live = self.sessions.lock().get(session_id).cloned()?;
        (live.root_id.is_none() && live.is_active())
            .then(|| live.folder.to_string_lossy().into_owned())
    }

    fn schedule_refresh(&self, folder: &Path) {
        if let Some(index) = self.index.get().and_then(Weak::upgrade) {
            index.schedule(folder, SourceId::Axo, None);
        }
    }

    fn maybe_unload(&self, id: &str) {
        let mut parent_to_check = None;
        let retired_folder = {
            let mut sessions = self.sessions.lock();
            let Some(live) = sessions.get(id) else { return };
            if let Some(root_id) = live.root_id.clone() {
                if !live.idle_and_unattached() {
                    return;
                }
                let Some(live) = sessions.remove(id) else {
                    return;
                };
                self.retire(live, false);
                parent_to_check = Some(root_id);
                None
            } else {
                if !live.idle_and_unattached() || children_of(&sessions, id).next().is_some() {
                    return;
                }
                if live.session.path().exists() {
                    let Some(live) = sessions.remove(id) else {
                        return;
                    };
                    let folder = live.folder.clone();
                    self.retire(live, false);
                    Some(folder)
                } else {
                    drop(sessions);
                    // axo creates the session file on the first submission; a session without one cannot be reopened once retired, so its owner gets UNWRITTEN_GRACE to reattach.
                    self.retire_unwritten_later(id);
                    return;
                }
            }
        };
        if let Some(root_id) = parent_to_check {
            self.maybe_unload(&root_id);
        }
        if let Some(folder) = retired_folder {
            self.emit_summaries(&folder);
        }
    }

    fn retire_unwritten_later(&self, id: &str) {
        let Some(runtime) = self.runtime() else {
            return;
        };
        let this = self.this.clone();
        let id = id.to_owned();
        runtime.spawn(async move {
            tokio::time::sleep(UNWRITTEN_GRACE).await;
            let Some(service) = this.upgrade() else {
                return;
            };
            let retired_folder = {
                let mut sessions = service.sessions.lock();
                if sessions
                    .get(&id)
                    .is_some_and(|live| live.idle_and_unattached())
                    && children_of(&sessions, &id).next().is_none()
                {
                    sessions.remove(&id).map(|live| {
                        let folder = live.folder.clone();
                        service.retire(live, false);
                        folder
                    })
                } else {
                    None
                }
            };
            if let Some(folder) = retired_folder {
                service.emit_summaries(&folder);
            }
        });
    }
    // Called with the registry locked so reopen cannot miss the retirement barrier.
    fn retire(&self, live: Arc<LiveSession>, capture: bool) {
        live.abort_relay();
        if let Some(runtime) = self.runtime() {
            let id = live.id.clone();
            let service = self.this.clone();
            let task = runtime.spawn(async move {
                live.release().await;
                if capture && live.conversed() {
                    if let Some(service) = service.upgrade() {
                        service.memory_capture(&live, MemoryCaptureTrigger::Close, None);
                    }
                }
            });
            self.retiring
                .lock()
                .insert(id, Arc::new(tokio::sync::Mutex::new(Some(task))));
        }
    }

    fn memory_capture(
        &self,
        live: &LiveSession,
        trigger: MemoryCaptureTrigger,
        anchor_entry_id: Option<String>,
    ) {
        if self.closed.load(Ordering::Acquire) || live.root_id.is_some() {
            return;
        }
        self.memory.capture(
            crate::services::memory::capture::CaptureJob {
                folder: live.folder.clone(),
                session_path: live.session.path().to_owned(),
                session_id: live.id.clone(),
                host: crate::services::memory::capture::CaptureHost {
                    title: live.title(),
                    trigger,
                    anchor_entry_id,
                },
            },
            live.engine.axo.clone(),
        );
    }

    async fn wait_retiring(&self, id: &str) -> bool {
        let slot = self.retiring.lock().get(id).cloned();
        let Some(slot) = slot else {
            return false;
        };
        if let Some(task) = slot.lock().await.take() {
            if let Err(error) = task.await {
                tracing::warn!(%error, session_id = id, "axo retiring session task failed");
            }
        }
        let mut retiring = self.retiring.lock();
        if retiring
            .get(id)
            .is_some_and(|current| Arc::ptr_eq(current, &slot))
        {
            retiring.remove(id);
        }
        true
    }

    /// Removes `id` (and, for a root, its children) from the registry, tells their
    /// subscribers, and retires them under the registry lock. Returns them, target last.
    fn evict(&self, id: &str, message: &str, capture: bool) -> Vec<Arc<LiveSession>> {
        let mut sessions = self.sessions.lock();
        let Some(target) = sessions.remove(id) else {
            return Vec::new();
        };
        let mut evicted = Vec::new();
        if target.root_id.is_none() {
            let children: Vec<_> = children_of(&sessions, id).cloned().collect();
            evicted.extend(children.into_iter().filter_map(|id| sessions.remove(&id)));
        }
        evicted.push(target);
        for live in &evicted {
            live.broadcast_closed(message.to_owned());
            self.retire(live.clone(), capture);
        }
        evicted
    }

    fn crashed(&self, id: &str, message: String) {
        let notice = format!("axo session stopped unexpectedly: {message}");
        let evicted = self.evict(id, &notice, false);
        if let Some(target) = evicted.last().filter(|live| live.root_id.is_none()) {
            self.emit_summaries(&target.folder);
        }
    }

    pub(crate) async fn models(&self) -> Result<Vec<ChatModel>, ApiError> {
        let (registry, origins) = self.provider_origins_for_folder(None).await?;
        Ok(Self::available_models(&registry, &origins).await)
    }

    /// Whether each provider with models can authenticate now, checked with its first model.
    async fn provider_auth(models: &axo_ai::Models) -> HashMap<String, Result<(), axo_ai::Error>> {
        let mut auth = HashMap::new();
        for model in models.all().iter() {
            if !auth.contains_key(&model.provider) {
                auth.insert(model.provider.clone(), models.auth(model).await.map(drop));
            }
        }
        auth
    }

    async fn available_models(
        registry: &axo_ai::Models,
        origins: &HashMap<String, ProviderExtensionOrigin>,
    ) -> Vec<ChatModel> {
        let auth = Self::provider_auth(registry).await;
        let all = registry.all();
        let mut models: Vec<_> = all
            .iter()
            .filter(|m| auth.get(&m.provider).is_some_and(Result::is_ok))
            .map(|m| ChatModel {
                selector: format!("{}/{}", m.provider, m.id),
                provider: m.provider.clone(),
                origin: origins.get(&m.provider).map(|origin| origin.id.clone()),
                name: m.name.clone(),
                thinking: axo_ai::thinking::supported_levels(m)
                    .into_iter()
                    .map(convert::thinking_to_wire)
                    .collect(),
                service_tiers: m
                    .service_tiers
                    .iter()
                    .filter(|&(&tier, _)| {
                        tier != ServiceTier::Standard && m.effective_tier(tier) == tier
                    })
                    .map(|(&tier, &factor)| (convert::service_tier_to_wire(tier), factor))
                    .collect(),
                context_window: m.context_window,
                images: m.input.contains(&axo_ai::InputKind::Image),
            })
            .collect();
        models.sort_by(|a, b| {
            a.provider
                .cmp(&b.provider)
                .then_with(|| a.name.cmp(&b.name))
        });
        models
    }

    pub(crate) async fn keywords(&self) -> Result<Vec<AxoKeywordInfo>, ApiError> {
        let engine = self.engine().await?;
        Ok(
            bundled::catalog(&engine.axo, &self.dirs.config, ProjectAccess::Ignore)
                .map_err(axo_error)?
                .keywords
                .iter()
                .map(|keyword| AxoKeywordInfo {
                    name: keyword.name.clone(),
                    description: keyword.description.clone(),
                })
                .collect(),
        )
    }

    pub(crate) fn validate_model_selectors(
        &self,
        previous: &AxoSettings,
        settings: &AxoSettings,
        models: &axo_ai::Models,
    ) -> Result<(), ApiError> {
        for (name, selector) in &settings.model_roles {
            if !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                || selector.starts_with('@')
            {
                return Err(ApiError::InvalidArgument(format!("Invalid model role {name}: use a lowercase role name and a model selector, not another role")));
            }
        }

        let last_model = config::last_explicit_model(&self.db).map_err(axo_error)?;
        let prepare = |settings: &AxoSettings| -> Result<axo::Settings, ApiError> {
            let mut value = Some(
                serde_json::to_value(settings)
                    .map_err(|error| ApiError::Internal(error.to_string()))?,
            );
            if let Some(settings) = value.as_mut().and_then(serde_json::Value::as_object_mut) {
                settings.remove("extensions");
            }
            config::fill_roles(&mut value, last_model.as_deref());
            serde_json::from_value(value.expect("settings are present"))
                .map_err(|error| ApiError::InvalidArgument(error.to_string()))
        };
        let candidate = prepare(settings)?;
        let before = prepare(previous)?;
        let validate = |selector: &str, prior: Option<&str>| -> Result<(), ApiError> {
            let role_changed = || {
                let Some(role) = selector.strip_prefix('@') else {
                    return false;
                };
                let name = role.split(':').next().unwrap_or("");
                if before.model_roles.get(name) == candidate.model_roles.get(name) {
                    return false;
                }
                let bare = &selector[..name.len() + 1];
                let old = before.resolve_model(models, bare).ok();
                let new = candidate.resolve_model(models, bare).ok();
                old.as_ref().map(model_choice_key) != new.as_ref().map(model_choice_key)
            };
            if prior != Some(selector) || role_changed() {
                candidate
                    .resolve_model(models, selector)
                    .map_err(|error| ApiError::InvalidArgument(error.to_string()))?;
            }
            Ok(())
        };

        for (name, selector) in &settings.model_roles {
            validate(selector, previous.model_roles.get(name).map(String::as_str))?;
        }
        if let Some(selector) = &settings.default_model {
            validate(selector, previous.default_model.as_deref())?;
        }
        if let Some(selector) = &settings.subagents.default_model {
            validate(selector, previous.subagents.default_model.as_deref())?;
        }
        for (name, selector) in &settings.subagents.models {
            validate(
                selector,
                previous.subagents.models.get(name).map(String::as_str),
            )?;
        }
        Ok(())
    }

    pub(crate) async fn catalog(&self, folder: &Path) -> Result<ChatModelCatalog, ApiError> {
        let engine = self.engine().await?;
        let (registry, origins) = self.provider_origins_for_folder(Some(folder)).await?;
        let models = Self::available_models(&registry, &origins).await;
        let catalog =
            bundled::catalog(&engine.axo, folder, project_access(folder)).map_err(axo_error)?;
        let settings = &catalog.config.settings;
        let choice = settings
            .resolve_model(
                &registry,
                settings.default_model.as_deref().unwrap_or("@default"),
            )
            .ok();
        let user_settings = config::read_settings(
            &self.dirs.settings_path(),
            axome_protocol::settings::SettingsLayerKind::Global,
        )
        .map_err(axo_error)?;
        let roles = user_settings
            .as_ref()
            .and_then(|settings| settings.get("modelRoles"))
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flat_map(|roles| roles.keys())
            .map(|name| ChatModelRole {
                name: name.clone(),
                model: (|| {
                    let original = settings.model_roles.get(name)?;
                    let choice = settings
                        .resolve_model(&registry, &format!("@{name}"))
                        .ok()?;
                    let mut selector = if let Some(model) = &choice.model {
                        if !models.iter().any(|entry| entry.provider == model.provider) {
                            return None;
                        }
                        format!("{}/{}", model.provider, model.id)
                    } else {
                        String::new()
                    };
                    for suffix in [
                        choice.thinking.map(ThinkingLevel::as_str),
                        choice.tier.map(ServiceTier::as_str),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        selector.push(':');
                        selector.push_str(suffix);
                    }
                    // Reordering suffixes can accidentally name an actual model ID with colons.
                    if settings
                        .resolve_model(&registry, &selector)
                        .as_ref()
                        .is_ok_and(|canonical| {
                            model_choice_key(canonical) == model_choice_key(&choice)
                        })
                    {
                        Some(selector)
                    } else {
                        Some(original.clone())
                    }
                })(),
            })
            .collect();
        Ok(ChatModelCatalog {
            default_model: choice
                .as_ref()
                .and_then(|choice| choice.model.as_ref())
                .map(|model| format!("{}/{}", model.provider, model.id))
                .filter(|selector| models.iter().any(|model| &model.selector == selector)),
            default_service_tier: convert::service_tier_to_wire(
                choice
                    .as_ref()
                    .and_then(|choice| choice.tier)
                    .unwrap_or_default(),
            ),
            default_thinking: convert::thinking_to_wire(
                choice
                    .and_then(|choice| choice.thinking)
                    .unwrap_or(settings.default_thinking_level),
            ),
            models,
            roles,
        })
    }

    pub(crate) async fn create(
        &self,
        folder: &Path,
        model: String,
        thinking: ChatThinking,
        service_tier: ChatServiceTier,
    ) -> Result<String, ApiError> {
        let _opening = self.opening.lock().await;
        let engine = self.engine().await?;
        let mut state = engine.extensions.mutation.lock().await;
        self.ensure_open()?;
        let (mut options, recorded, owner) = self
            .options(
                &engine,
                &mut state,
                folder,
                Some(model),
                Some(convert::thinking_from_wire(thinking)),
            )
            .await?;
        let models = options
            .models
            .clone()
            .unwrap_or_else(|| engine.models.clone());
        options.agent = self.default_persona(&engine, folder)?;
        let session = engine
            .axo
            .create_session(options)
            .await
            .map_err(axo_error)?;
        let tier = convert::service_tier_from_wire(service_tier);
        session
            .run_command(axo::Command::Tier { tier })
            .await
            .map_err(axo_error)?;
        let id = self
            .register(
                folder.to_owned(),
                Arc::new(session),
                None,
                engine.clone(),
                (recorded, models),
                Some(owner),
            )
            .await?
            .id
            .clone();
        Ok(id)
    }

    /// `axo.defaultAgent` when it names a root persona in this folder's catalog.
    fn default_persona(&self, engine: &Engine, folder: &Path) -> Result<Option<String>, ApiError> {
        use axo::context::agents::AgentDef;

        let Some(name) = config::host_setting(&self.dirs, folder, "defaultAgent")
            .map_err(axo_error)?
            .and_then(|value| value.as_str().map(str::to_owned))
        else {
            return Ok(None);
        };
        let catalog =
            bundled::catalog(&engine.axo, folder, project_access(folder)).map_err(axo_error)?;
        let persona = catalog.agents.iter().any(|def| {
            matches!(def, AgentDef::Agent(spec)
                if spec.name == name && matches!(spec.run_as, axo::RunAs::Root | axo::RunAs::Any))
        });
        if persona {
            Ok(Some(name))
        } else {
            tracing::warn!("defaultAgent {name:?} is not a root persona; using Default");
            Ok(None)
        }
    }

    pub(crate) async fn attach(
        &self,
        folder: &Path,
        id: &str,
        attach_id: String,
        sink: EventSink<ChatEvent>,
    ) -> Result<(), ApiError> {
        loop {
            match self.load(folder, id, false).await? {
                Loaded::Live(held) => {
                    let subscribed = {
                        let sessions = self.sessions.lock();
                        sessions
                            .get(id)
                            .filter(|current| Arc::ptr_eq(current, &held.live))
                            .map(|_| held.subscribe(attach_id.clone(), sink.clone()))
                    };
                    if let Some(result) = subscribed {
                        return result;
                    }
                }
                Loaded::RootClosed { engine, infos } => {
                    let (info, _) = find_session_and_root(&infos, id)?;
                    let by_id: HashMap<&str, &axo::SessionInfo> =
                        infos.iter().map(|item| (item.id.as_str(), item)).collect();
                    {
                        let mut readonly = self.readonly_subscribers.lock();
                        self.ensure_open()?;
                        // A resume that finished since `load` has already drained this list.
                        if self.sessions.lock().contains_key(id) {
                            continue;
                        }
                        let subs = readonly.entry(id.to_owned()).or_default();
                        subs.retain(|(existing, _)| existing != &attach_id);
                        subs.push((attach_id.clone(), sink.clone()));
                    }
                    let state = match self.models_for_folder(Some(folder)).await {
                        Ok(models) => build_readonly_state(&engine, &models, info, &by_id).await,
                        Err(error) => Err(error),
                    };
                    let mut readonly = self.readonly_subscribers.lock();
                    let registered = readonly.get(id).is_some_and(|subs| {
                        subs.iter().any(|(existing, held)| {
                            existing == &attach_id && live::same_sink(held, &sink)
                        })
                    });
                    // Resume upgraded this sink to the live session, or close/detach removed it.
                    if !registered {
                        return Ok(());
                    }
                    let sent = state.and_then(|state| {
                        sink(ChatEvent::Snapshot {
                            state: Box::new(state),
                        })
                        .map_err(ApiError::Internal)
                    });
                    if sent.is_err() {
                        if let Some(subs) = readonly.get_mut(id) {
                            subs.retain(|(existing, held)| {
                                existing != &attach_id || !live::same_sink(held, &sink)
                            });
                            if subs.is_empty() {
                                readonly.remove(id);
                            }
                        }
                    }
                    return sent;
                }
            }
        }
    }

    pub(crate) async fn resume(&self, folder: &Path, id: &str) -> Result<(), ApiError> {
        self.ensure_open()?;
        let Loaded::Live(held) = self.load(folder, id, true).await? else {
            return Err(ApiError::NotFound(
                "Parent root session no longer exists".into(),
            ));
        };
        let subs = self
            .readonly_subscribers
            .lock()
            .remove(id)
            .unwrap_or_default();
        for (attach_id, sink) in subs {
            let _ = held.subscribe(attach_id, sink);
        }
        Ok(())
    }

    pub(crate) fn detach(&self, id: &str, attach_id: &str) {
        self.remove_subscriber(id, attach_id, None);
    }

    /// Detaches only while `sink` is still the registered one, so a stale
    /// stream cannot remove the subscriber that replaced it under the same id.
    pub(crate) fn detach_sink(&self, id: &str, attach_id: &str, sink: &EventSink<ChatEvent>) {
        self.remove_subscriber(id, attach_id, Some(sink));
    }

    fn remove_subscriber(&self, id: &str, attach_id: &str, only: Option<&EventSink<ChatEvent>>) {
        if let Some(live) = self.sessions.lock().get(id) {
            live.unsubscribe(attach_id, only);
        }
        {
            let mut readonly = self.readonly_subscribers.lock();
            if let Some(subs) = readonly.get_mut(id) {
                subs.retain(|(existing, sink)| {
                    existing != attach_id || only.is_some_and(|only| !live::same_sink(sink, only))
                });
                if subs.is_empty() {
                    readonly.remove(id);
                }
            }
        }
        self.maybe_unload(id);
    }

    pub(crate) async fn close(&self, id: &str) -> Result<(), ApiError> {
        let readonly_subs = self
            .readonly_subscribers
            .lock()
            .remove(id)
            .unwrap_or_default();
        for (_, sink) in readonly_subs {
            let _ = sink(ChatEvent::Closed {
                message: "Session closed".into(),
                missing: false,
            });
        }
        let evicted = self.evict(id, "Session closed", true);
        if let Some(target) = evicted.last().filter(|live| live.root_id.is_none()) {
            self.emit_summaries(&target.folder);
        }
        for live in &evicted {
            self.wait_retiring(&live.id).await;
        }
        if let Some(root_id) = evicted.last().and_then(|live| live.root_id.as_deref()) {
            self.maybe_unload(root_id);
        }
        Ok(())
    }

    pub(crate) async fn prompt(
        &self,
        folder: &Path,
        id: &str,
        request_id: String,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let held = self.live(folder, id).await?;
        held.live.prompt(request_id, parts).await
    }
    pub(crate) async fn steer(
        &self,
        folder: &Path,
        id: &str,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .steer(convert::parts_from_wire(parts)?)
            .await
            .map_err(axo_error)
    }
    pub(crate) async fn follow_up(
        &self,
        folder: &Path,
        id: &str,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .follow_up(convert::parts_from_wire(parts)?)
            .await
            .map_err(axo_error)
    }
    // ponytail: drops QueuedInput.custom; Axome posts no host messages (Session::post), so nothing custom reaches the queue. Carry it through if that changes.
    pub(crate) async fn withdraw(
        &self,
        folder: &Path,
        id: &str,
        submission_id: String,
    ) -> Result<Vec<ChatPart>, ApiError> {
        let input = self
            .live(folder, id)
            .await?
            .session
            .withdraw(&submission_id)
            .await
            .map_err(axo_error)?;
        Ok(convert::parts_to_wire(&input.parts))
    }
    /// Stops an active run, waits for it to settle idle, and immediately sends held input.
    pub(crate) async fn send_now(&self, folder: &Path, id: &str) -> Result<(), ApiError> {
        let held = self.live(folder, id).await?;
        held.session.abort();
        if !held.wait_idle().await {
            return Ok(());
        }
        held.session.resume_wakes();
        Ok(())
    }
    pub(crate) async fn abort(&self, folder: &Path, id: &str) -> Result<(), ApiError> {
        self.live(folder, id).await?.session.abort();
        Ok(())
    }
    pub(crate) async fn answer(
        &self,
        folder: &Path,
        id: &str,
        call_id: String,
        answers: Vec<ChatAnswer>,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .answer(&call_id, answers.into_iter().map(convert::answer).collect())
            .map_err(axo_error)
    }
    pub(crate) async fn dismiss(
        &self,
        folder: &Path,
        id: &str,
        call_id: String,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .dismiss(&call_id)
            .map_err(axo_error)
    }
    pub(crate) async fn set_agent(
        &self,
        folder: &Path,
        id: &str,
        agent: Option<String>,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .set_agent(agent.as_deref())
            .await
            .map_err(axo_error)
    }
    /// Names the session; `None` or a blank name clears it, and the first prompt titles the chat again.
    pub(crate) async fn set_title(
        &self,
        folder: &Path,
        id: &str,
        title: Option<String>,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .set_name(title)
            .await
            .map(|_| ())
            .map_err(axo_error)
    }
    /// Selects a mode, then steers any remaining parts so attachments and file chips survive.
    pub(crate) async fn set_mode(
        &self,
        folder: &Path,
        id: &str,
        mode: Option<String>,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let held = self.live(folder, id).await?;
        held.session
            .set_mode(mode.as_deref())
            .await
            .map_err(axo_error)?;
        if parts.is_empty() {
            return Ok(());
        }
        held.session
            .steer(convert::parts_from_wire(parts)?)
            .await
            .map_err(axo_error)
    }
    pub(crate) async fn keyword_matches(
        &self,
        folder: &Path,
        id: &str,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<ChatKeywordMatch>>, ApiError> {
        let held = self.live(folder, id).await?;
        Ok(texts
            .iter()
            .map(|text| utf16_matches(text, held.session.keyword_matches(text)))
            .collect())
    }
    pub(crate) async fn accept_result(
        &self,
        folder: &Path,
        id: &str,
        call_id: String,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .accept_result(&call_id, axo::AcceptResultOptions::default())
            .map_err(axo_error)
    }
    pub(crate) async fn revise_result(
        &self,
        folder: &Path,
        id: &str,
        call_id: String,
        feedback: String,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .revise_result(&call_id, feedback)
            .map_err(axo_error)
    }
    pub(crate) async fn background(
        &self,
        folder: &Path,
        id: &str,
        call_id: String,
    ) -> Result<bool, ApiError> {
        Ok(self.live(folder, id).await?.session.background(&call_id))
    }
    pub(crate) async fn kill_job(
        &self,
        folder: &Path,
        id: &str,
        job_id: String,
    ) -> Result<bool, ApiError> {
        Ok(self
            .live(folder, id)
            .await?
            .session
            .kill_job(&job_id)
            .stopping)
    }
    pub(crate) async fn resume_wakes(&self, folder: &Path, id: &str) -> Result<(), ApiError> {
        self.live(folder, id).await?.session.resume_wakes();
        Ok(())
    }
    pub(crate) async fn context_details(
        &self,
        folder: &Path,
        id: &str,
    ) -> Result<ChatContextDetails, ApiError> {
        let live = self.live(folder, id).await?;
        let inspection = live.session.inspect_prompt();
        let prompt = inspection
            .recorded
            .or(inspection.latest_built)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, body)| ChatPromptSection { name, body })
            .collect();
        let inventory = live.session.context_files();
        let context_files = inventory
            .startup
            .into_iter()
            .map(|file| (file, false))
            .chain(
                inventory
                    .projected_nested
                    .into_iter()
                    .map(|file| (file, true)),
            )
            .map(|(file, nested)| ChatContextFile {
                path: convert::display_path(&file.path, folder),
                content: file.content,
                nested,
            })
            .collect();
        Ok(ChatContextDetails {
            prompt,
            context_files,
        })
    }

    /// Lists the session's `local://` files, live or closed.
    pub(crate) async fn local_files(
        &self,
        folder: &Path,
        id: &str,
    ) -> Result<ChatLocalFiles, ApiError> {
        let schemes = self.schemes(folder, id).await?;
        let schemes = schemes.get();
        let Some(url) = schemes.exact("local://") else {
            return Ok(ChatLocalFiles::default());
        };
        let root = match schemes
            .resolve(&url)
            .await
            .map_err(|error| ApiError::InvalidArgument(error.to_string()))?
        {
            axo::schemes::Target::File { path, .. } => path,
            axo::schemes::Target::Virtual => return Ok(ChatLocalFiles::default()),
        };
        tokio::task::spawn_blocking(move || {
            fn walk(root: &Path, dir: &Path, out: &mut Vec<ChatLocalFile>) -> Result<(), ApiError> {
                let io = |path: &Path, err: std::io::Error| {
                    ApiError::Io(format!("{}: {err}", path.display()))
                };
                // `local://` exists only after the first write, and a running agent may
                // delete a directory mid-walk.
                let entries = match std::fs::read_dir(dir) {
                    Ok(entries) => entries,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                    Err(err) => return Err(io(dir, err)),
                };
                for entry in entries {
                    let entry = entry.map_err(|err| io(dir, err))?;
                    let path = entry.path();
                    let meta = match entry.metadata() {
                        Ok(meta) => meta,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(err) => return Err(io(&path, err)),
                    };
                    if meta.is_dir() {
                        walk(root, &path, out)?;
                    } else if let Ok(rel) = path.strip_prefix(root) {
                        out.push(ChatLocalFile {
                            path: rel.to_string_lossy().replace('\\', "/"),
                            size: meta.len(),
                        });
                    }
                }
                Ok(())
            }
            // Canonical so ledger paths (always canonical) can be matched against it.
            let root = std::fs::canonicalize(&root).unwrap_or(root);
            let mut files = Vec::new();
            walk(&root, &root, &mut files)?;
            files.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(ChatLocalFiles {
                root: root.to_string_lossy().into_owned(),
                files,
            })
        })
        .await
        .map_err(|error| ApiError::Internal(format!("listing local files failed: {error}")))?
    }

    async fn closed_info(
        &self,
        axo: &Axo,
        folder: &Path,
        id: &str,
    ) -> Result<axo::SessionInfo, ApiError> {
        axo.list_sessions(folder)
            .await
            .map_err(axo_error)?
            .into_iter()
            .find(|info| info.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("axo session {id}")))
    }

    /// The session's scheme table: the live session's, or a read-only one for a closed session.
    async fn schemes(&self, folder: &Path, id: &str) -> Result<SessionSchemes, ApiError> {
        match self.target(folder, id).await? {
            SessionTarget::Live(live) => Ok(SessionSchemes::Live(live)),
            SessionTarget::Closed(axo, path) => {
                let host_schemes = self.host_schemes(folder);
                Ok(SessionSchemes::Closed(
                    axo.schemes(&path, Some(project_access(folder)), &host_schemes)
                        .await
                        .map_err(axo_error)?,
                ))
            }
        }
    }

    async fn target(&self, folder: &Path, id: &str) -> Result<SessionTarget, ApiError> {
        if let Some(live) = self.sessions.lock().get(id).cloned() {
            if live.folder != folder {
                return Err(ApiError::NotFound(format!("axo session {id}")));
            }
            return Ok(SessionTarget::Live(live));
        }
        let axo = self.axo().await?;
        let info = self.closed_info(&axo, folder, id).await?;
        Ok(SessionTarget::Closed(axo, info.path))
    }

    pub(crate) async fn squash_changes(
        &self,
        folder: &Path,
        id: &str,
        changes: Vec<serde_json::Value>,
    ) -> Result<Vec<ChatSquashedChange>, ApiError> {
        let parsed: Vec<axo::FileChange> = changes
            .into_iter()
            .map(|value| {
                serde_json::from_value(value).map_err(|error| {
                    ApiError::InvalidArgument(format!("Invalid file change record: {error}"))
                })
            })
            .collect::<Result<_, _>>()?;
        let squashed = match self.target(folder, id).await? {
            SessionTarget::Live(live) => live.session.squash_changes(&parsed).await,
            SessionTarget::Closed(axo, path) => axo.squash_changes(&path, &parsed).await,
        };
        Ok(squashed
            .into_iter()
            .map(|file| ChatSquashedChange {
                path: file.path,
                key: file.key,
                url: file.url,
                created: file.created,
                before_sha: file.before.as_ref().map(|blob| blob.sha256.clone()),
                after_sha: file.after.sha256.clone(),
                binary: file.before.as_ref().is_some_and(|blob| blob.binary) || file.after.binary,
                omitted: file
                    .before
                    .as_ref()
                    .is_some_and(|blob| blob.omitted.is_some())
                    || file.after.omitted.is_some(),
                mutations: file.mutations,
                discontinuous: file.discontinuous,
                additions: file.diff.as_ref().map(|diff| diff.additions),
                deletions: file.diff.as_ref().map(|diff| diff.deletions),
            })
            .collect())
    }

    pub(crate) async fn turn_diff_file(
        &self,
        folder: &Path,
        id: &str,
        before_sha: Option<String>,
        after_sha: String,
    ) -> Result<DiffFileContent, ApiError> {
        let target = self.target(folder, id).await?;
        let read = |sha: String| {
            let target = &target;
            async move {
                match target {
                    SessionTarget::Live(live) => {
                        live.session.read_snapshot(&sha).await.map_err(axo_error)
                    }
                    SessionTarget::Closed(axo, path) => {
                        axo.read_snapshot(path, &sha).await.map_err(axo_error)
                    }
                }
            }
        };
        let (before_res, after_res) = match before_sha {
            Some(before) => {
                let (b, a) = tokio::join!(read(before), read(after_sha));
                (Some(b?), a?)
            }
            None => (None, read(after_sha).await?),
        };
        Ok(diff_content(before_res, after_res))
    }

    pub(crate) async fn watch_job(
        &self,
        folder: &Path,
        id: &str,
        attach_id: String,
        job_id: Option<String>,
    ) -> Result<Option<ChatJobOutput>, ApiError> {
        let live_opt = self.sessions.lock().get(id).cloned();
        let Some(job_id) = job_id else {
            if let Some(live) = live_opt {
                live.unwatch_job(&attach_id);
            }
            return Ok(None);
        };
        if let Some(live) = &live_opt {
            if live.folder != folder {
                return Err(ApiError::NotFound(format!("axo session {id}")));
            }
            let artifact = live
                .command_job(&job_id)
                .ok_or_else(|| ApiError::NotFound(format!("command job {job_id}")))?;
            if let Some(out) = live.watch_command(attach_id, job_id.clone()).await? {
                return Ok(Some(out));
            }
            return Self::read_artifact_tail(live.session.schemes(), job_id, artifact).await;
        }
        let axo = self.axo().await?;
        let info = self.closed_info(&axo, folder, id).await?;
        let (header, entries) = axo.open_session_read(&info.path).await.map_err(axo_error)?;
        let artifact = axo::jobs_from_entries(&header, &entries)
            .into_iter()
            .find_map(|job| match job.kind {
                axo::JobKind::Command { artifact, .. } if job.id == job_id => Some(artifact),
                _ => None,
            })
            .ok_or_else(|| ApiError::NotFound(format!("command job {job_id}")))?;
        let schemes = axo
            .schemes(&info.path, Some(project_access(folder)), &[])
            .await
            .map_err(axo_error)?;
        Self::read_artifact_tail(&schemes, job_id, artifact).await
    }

    async fn read_artifact_tail(
        schemes: &axo::Schemes,
        job_id: String,
        artifact: u64,
    ) -> Result<Option<ChatJobOutput>, ApiError> {
        // axo creates the artifact when the command starts, so a missing one means
        // its retained output was lost.
        fn unavailable() -> ChatJobOutput {
            ChatJobOutput {
                chunks: Vec::new(),
                capture_failed: true,
                truncated: false,
                done: true,
            }
        }

        let artifact_ref = format!("artifact://{artifact}");
        let Some(url) = schemes.exact(&artifact_ref) else {
            return Err(ApiError::NotFound(artifact_ref));
        };
        let target = match schemes.resolve(&url).await {
            Ok(target) => target,
            Err(error) if error.0.contains("not found") => {
                return Ok(Some(unavailable()));
            }
            Err(error) => return Err(ApiError::InvalidArgument(error.to_string())),
        };
        match target {
            axo::schemes::Target::File { path, .. } => {
                let output = match axo::CommandOutput::open(job_id.clone(), artifact, &path) {
                    Ok(output) => output,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(Some(unavailable()));
                    }
                    Err(err) => return Err(ApiError::Io(format!("{}: {err}", path.display()))),
                };
                let (progress, truncated, chunks, _) = live::read_tail(&output).await?;
                Ok(Some(ChatJobOutput {
                    chunks,
                    capture_failed: progress.capture_failed,
                    truncated,
                    done: true,
                }))
            }
            axo::schemes::Target::Virtual => {
                let text = schemes
                    .read(&url)
                    .await
                    .map_err(|error| ApiError::InvalidArgument(error.to_string()))?;
                let bytes = text.as_bytes();
                let truncated = bytes.len() as u64 > live::MAX_JOB_SCROLLBACK_BYTES;
                let start = bytes
                    .len()
                    .saturating_sub(live::MAX_JOB_SCROLLBACK_BYTES as usize);
                let text = live::utf8_tail(&bytes[start..], truncated);
                Ok(Some(ChatJobOutput {
                    chunks: vec![ChatOutputChunk {
                        stream: ChatOutputStream::Merged,
                        text,
                    }],
                    capture_failed: false,
                    truncated,
                    done: true,
                }))
            }
        }
    }
    pub(crate) async fn run_command(
        &self,
        folder: &Path,
        id: &str,
        name: String,
        args: serde_json::Value,
    ) -> Result<(), ApiError> {
        let live = self.live(folder, id).await?;
        let command = if live.session.commands().iter().any(|command| {
            command.name == name && matches!(command.kind, axo::CommandKind::Registered { .. })
        }) {
            let text = args
                .as_object()
                .filter(|object| object.len() == 1)
                .and_then(|object| object.get("args"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    ApiError::InvalidArgument(
                        "Registered command arguments must be an object containing only a string 'args' field".into(),
                    )
                })?;
            axo::Command::Registered {
                name,
                args: text.trim().to_owned(),
            }
        } else {
            axo::Command::decode(&name, args).map_err(axo_error)?
        };
        let explicit_model = match &command {
            axo::Command::Model { model } if !model.starts_with(['@', ':']) => live
                .session
                .config()
                .settings
                .resolve_model(&live.models, model)
                .ok()
                .and_then(|choice| choice.model)
                .map(|_| model.clone()),
            _ => None,
        };
        live.run_command(command).await?;
        if let Some(model) = explicit_model {
            let json = serde_json::to_string(&model)
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            crate::services::app_state_kv::AppStateKvService::put(
                self.db.write().conn(),
                "axo.lastExplicitModel",
                &json,
            )
            .map_err(ApiError::Database)?;
        }
        Ok(())
    }

    pub(crate) async fn resolve_link(
        &self,
        folder: &Path,
        id: &str,
        url: String,
    ) -> Result<ChatLinkTarget, ApiError> {
        let schemes = self.schemes(folder, id).await?;
        let schemes = schemes.get();
        let Some(url) = schemes.exact(&url) else {
            return Ok(ChatLinkTarget::Unhandled);
        };
        match schemes
            .resolve(&url)
            .await
            .map_err(|error| ApiError::InvalidArgument(error.to_string()))?
        {
            axo::schemes::Target::File { path, .. } => {
                let metadata = tokio::fs::metadata(&path)
                    .await
                    .map_err(|error| ApiError::NotFound(format!("{}: {error}", path.display())))?;
                Ok(ChatLinkTarget::File {
                    path: path.to_string_lossy().into_owned(),
                    is_dir: metadata.is_dir(),
                })
            }
            axo::schemes::Target::Virtual => {
                let text = schemes
                    .read(&url)
                    .await
                    .map_err(|error| ApiError::InvalidArgument(error.to_string()))?;
                Ok(ChatLinkTarget::Virtual { text })
            }
        }
    }
    pub(crate) async fn navigate(
        &self,
        folder: &Path,
        id: &str,
        entry_id: Option<String>,
    ) -> Result<(), ApiError> {
        self.live(folder, id)
            .await?
            .session
            .navigate(
                entry_id.map(EntryId::from),
                NavigateOptions {
                    summarize: false,
                    focus: None,
                },
            )
            .await
            .map_err(axo_error)
    }
    pub(crate) async fn select_branch(
        &self,
        folder: &Path,
        id: &str,
        entry_id: String,
    ) -> Result<(), ApiError> {
        let live = self.live(folder, id).await?;
        let target = live.select_branch_target(&entry_id)?;
        live.session
            .navigate(
                Some(target),
                NavigateOptions {
                    summarize: false,
                    focus: None,
                },
            )
            .await
            .map_err(axo_error)
    }
    pub(crate) async fn fork(
        &self,
        folder: &Path,
        id: &str,
        entry_id: String,
    ) -> Result<ChatForkInfo, ApiError> {
        let live = self.live(folder, id).await?;
        let _opening = self.opening.lock().await;
        let state = live.engine.extensions.mutation.lock().await;
        self.ensure_open()?;
        let owner = live
            .engine
            .extensions
            .recorded_owner(&state, &live.extension_owner)?;
        let fork = live
            .session
            .fork(&EntryId::from(entry_id))
            .await
            .map_err(axo_error)?;
        let session = self
            .register(
                folder.to_owned(),
                Arc::new(fork.session),
                None,
                live.engine.clone(),
                (live.extension_owner.clone(), live.models.clone()),
                Some(owner),
            )
            .await?;
        drop(state);
        self.schedule_refresh(folder);
        Ok(ChatForkInfo {
            session_id: session.id.clone(),
            editable: fork.editable.as_deref().map(convert::parts_to_wire),
        })
    }

    pub(crate) fn reload_exec_env(&self, folder: Option<&Path>) {
        let Some(runtime) = self.runtime() else {
            return;
        };
        for live in self
            .sessions
            .lock()
            .values()
            .filter(|live| folder.is_none_or(|folder| live.folder == folder))
        {
            let session = live.session.clone();
            runtime.spawn(async move {
                session.reload_exec_env().await;
            });
        }
    }

    pub(crate) fn list_history(&self, folder: &Path) -> Result<Listing, String> {
        let runtime = self.runtime().ok_or("axo has no runtime")?;
        let service = self.this.upgrade().ok_or("axo service dropped")?;
        let listing_folder = folder.to_owned();
        // Index workers can outlive runtime shutdown. Keep timer/IO futures owned
        // by the runtime so shutdown cancels them; only block on their join here.
        let listing = runtime.spawn(async move {
            service
                .axo()
                .await?
                .list_sessions(&listing_folder)
                .await
                .map_err(axo_error)
        });
        let infos = runtime
            .block_on(listing)
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let by_id: HashMap<&str, &axo::SessionInfo> =
            infos.iter().map(|info| (info.id.as_str(), info)).collect();
        let mut listing = Listing::default();
        let mut roots: HashMap<String, HistoryRow> = HashMap::new();
        for info in infos.iter().filter(|info| info.spawn.is_none()) {
            if let Some(row) = parse_axo_row(info, &mut listing.skipped) {
                roots.insert(row.id.clone(), row);
            }
        }
        for info in &infos {
            let Some(spawn) = &info.spawn else {
                continue;
            };
            let Some(root_row) = root_of(&by_id, info).and_then(|root| roots.get_mut(&root.id))
            else {
                continue;
            };
            let Some(child_row) = parse_axo_row(info, &mut listing.skipped) else {
                continue;
            };
            root_row.children.push(IndexedChildSession {
                session_id: info.id.clone(),
                agent: spawn.agent.clone(),
                team_member: spawn.team.as_ref().map(|t| t.member.clone()),
                title: child_row
                    .title
                    .as_deref()
                    .and_then(history::normalize_title),
                model: child_row.model,
                updated_at: child_row.updated_at,
            });
        }
        for root_row in roots.values_mut() {
            root_row.children.sort_by(|a, b| {
                b.updated_at
                    .cmp(&a.updated_at)
                    .then_with(|| a.session_id.cmp(&b.session_id))
            });
        }
        listing.rows = history::normalize(roots.into_values().collect(), Some(folder), None);
        Ok(listing)
    }

    pub(crate) fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.readonly_subscribers.lock().clear();
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .drain()
            .map(|(_, live)| {
                live.abort_relay();
                live
            })
            .collect();
        let retiring: Vec<_> = self.retiring.lock().drain().map(|(_, slot)| slot).collect();
        let Some(runtime) = self.runtime().cloned() else {
            return;
        };
        let service = self.this.upgrade();
        let browser = Arc::clone(&self.browser);
        let memory = Arc::clone(&self.memory);
        let result = std::thread::spawn(move || {
            runtime.block_on(async move {
                let mut extensions_engine = None;
                if let Some(service) = service {
                    let (models_watch, engine) = {
                        let mut slot = service.engine.lock().await;
                        (slot.models_watch.take(), slot.engine.clone())
                    };
                    extensions_engine = engine;
                    if let Some(task) = models_watch {
                        task.abort();
                        let _ = task.await;
                    }
                    service.login_stop().await;
                }
                let mut tasks = tokio::task::JoinSet::new();
                for live in sessions {
                    tasks.spawn(async move {
                        live.release().await;
                    });
                }
                for slot in retiring {
                    tasks.spawn(async move {
                        if let Some(task) = slot.lock().await.take() {
                            if let Err(error) = task.await {
                                tracing::warn!(%error, "axo retiring session task failed");
                            }
                        }
                    });
                }
                tokio::time::timeout(Duration::from_secs(10), async {
                    // Awaited here, not spawned: with no capture queue it returns at once,
                    // so a shutdown with nothing to release never needs the runtime to schedule.
                    memory.stop_capture().await;
                    while let Some(result) = tasks.join_next().await {
                        if let Err(error) = result {
                            tracing::warn!(%error, "axo shutdown task failed");
                        }
                    }
                    if let Some(engine) = extensions_engine {
                        engine.extensions.shutdown().await;
                    }
                    browser.shutdown().await;
                })
                .await
            })
        })
        .join();
        match result {
            Ok(Ok(())) => {}
            Ok(Err(_)) => tracing::warn!("axo shutdown timed out"),
            Err(_) => tracing::warn!("axo shutdown thread panicked"),
        }
    }

    #[cfg(test)]
    pub(crate) fn loaded(&self) -> usize {
        self.sessions.lock().len()
    }
}

fn root_of<'a>(
    by_id: &HashMap<&str, &'a axo::SessionInfo>,
    info: &'a axo::SessionInfo,
) -> Option<&'a axo::SessionInfo> {
    let mut parent = info.spawn.as_ref()?.parent_session_id.as_str();
    for _ in 0..by_id.len() {
        let &ancestor = by_id.get(parent)?;
        match &ancestor.spawn {
            Some(spawn) => parent = &spawn.parent_session_id,
            None => return Some(ancestor),
        }
    }
    None
}

fn find_session_and_root<'a>(
    infos: &'a [axo::SessionInfo],
    id: &str,
) -> Result<(&'a axo::SessionInfo, Option<&'a axo::SessionInfo>), ApiError> {
    let by_id: HashMap<&str, &axo::SessionInfo> =
        infos.iter().map(|info| (info.id.as_str(), info)).collect();
    let info = *by_id
        .get(id)
        .ok_or_else(|| ApiError::NotFound(format!("axo session {id}")))?;
    Ok((info, root_of(&by_id, info)))
}

fn subtree_usage<'a>(
    axo: &'a Axo,
    path: &'a Path,
    infos_by_id: &'a HashMap<&'a str, &'a axo::SessionInfo>,
    depth: usize,
    permits: &'a tokio::sync::Semaphore,
) -> std::pin::Pin<Box<dyn Future<Output = axo_ai::Usage> + Send + 'a>> {
    Box::pin(async move {
        // The permit covers only this read, so nested levels cannot starve each other.
        let read = {
            let _permit = permits.acquire().await.ok();
            axo.open_session_read(path).await.map_err(axo_error)
        };
        let (header, entries) = match read {
            Ok(read) => read,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "child transcript unreadable; tree usage is partial");
                return axo_ai::Usage::default();
            }
        };
        let mut usage = convert::entries_usage(&entries);
        if depth < 8 {
            usage += &children_usage(axo, &header.id, &entries, infos_by_id, depth, permits).await;
        }
        usage
    })
}

fn children_usage<'a>(
    axo: &'a Axo,
    session_id: &'a str,
    entries: &'a [axo::store::SeqEntry],
    infos_by_id: &'a HashMap<&'a str, &'a axo::SessionInfo>,
    depth: usize,
    permits: &'a tokio::sync::Semaphore,
) -> std::pin::Pin<Box<dyn Future<Output = axo_ai::Usage> + Send + 'a>> {
    Box::pin(async move {
        let children = entries.iter().filter_map(|seq| match &seq.entry {
            Entry::ChildStart {
                session_id: owner_id,
                child_session_id,
                child_path,
                ..
            } if owner_id == session_id => Some(
                infos_by_id
                    .get(child_session_id.as_str())
                    .map(|info| info.path.as_path())
                    .unwrap_or(child_path.as_path()),
            ),
            _ => None,
        });
        let reads = children.map(|path| subtree_usage(axo, path, infos_by_id, depth + 1, permits));
        let mut usage = axo_ai::Usage::default();
        for child in futures_util::future::join_all(reads).await {
            usage += &child;
        }
        usage
    })
}

async fn build_readonly_state(
    engine: &Engine,
    registry: &axo_ai::Models,
    info: &axo::SessionInfo,
    infos_by_id: &HashMap<&str, &axo::SessionInfo>,
) -> Result<ChatState, ApiError> {
    let (header, entries) = engine
        .axo
        .open_session_read(&info.path)
        .await
        .map_err(axo_error)?;
    let path = convert::branch(&entries);
    let (model, thinking, tier) = convert::selection(&entries, &path);
    let definition = model
        .as_deref()
        .and_then(|selector| engine::model(registry, selector));
    let window = definition.as_ref().map_or(0, |model| model.context_window);
    let effective = definition
        .as_ref()
        .map_or(ServiceTier::Standard, |model| model.effective_tier(tier));
    let mut tree_usage = convert::entries_usage(&entries);
    let permits = tokio::sync::Semaphore::new(8);
    tree_usage +=
        &children_usage(&engine.axo, &header.id, &entries, infos_by_id, 0, &permits).await;
    let has_children = entries
        .iter()
        .any(|s| matches!(s.entry, Entry::ChildStart { .. }));
    let stats = convert::stats(
        &entries,
        &path,
        window,
        None,
        has_children.then_some(&tree_usage),
    );
    let jobs: Vec<ChatJob> = axo::jobs_from_entries(&header, &entries)
        .iter()
        .map(|j| convert::job(j, true))
        .collect();
    Ok(ChatState {
        session_id: info.id.clone(),
        entries: convert::path_entries(&entries, &path),
        streaming: None,
        live: ChatLive {
            title: convert::title(&entries),
            running: false,
            maintenance: None,
            model,
            thinking: convert::thinking_to_wire(thinking),
            service_tier: convert::service_tier_to_wire(tier),
            effective_service_tier: convert::service_tier_to_wire(effective),
            tools: Vec::new(),
            queue: Vec::new(),
            questions: Vec::new(),
            retry: None,
            stats,
            auto_paused: false,
            jobs,
            agent: None,
            mode: None,
            effective_mode: None,
            reviews: Vec::new(),
        },
        toolbox: ChatToolbox::default(),
        commands: Vec::new(),
        tool_origins: BTreeMap::new(),
        extension_diagnostics: Vec::new(),
        session_tools: Vec::new(),
        read_only: true,
        is_child: header.spawn.is_some(),
    })
}

fn message_text(message: &axo_ai::Message) -> impl Iterator<Item = &str> {
    let (user, assistant) = match message {
        axo_ai::Message::User(user) => (Some(user), None),
        axo_ai::Message::Assistant(assistant) => (None, Some(assistant)),
        axo_ai::Message::System(_) | axo_ai::Message::ToolResult(_) => (None, None),
    };
    user.into_iter()
        .flat_map(|user| &user.content)
        .filter_map(|block| match block {
            axo_ai::UserBlock::Text { text } => Some(text.as_str()),
            axo_ai::UserBlock::Image { .. } => None,
        })
        .chain(
            assistant
                .into_iter()
                .flat_map(|assistant| &assistant.content)
                .filter_map(|block| match block {
                    axo_ai::AssistantBlock::Text { text, .. } => Some(text.as_str()),
                    axo_ai::AssistantBlock::Thinking { .. }
                    | axo_ai::AssistantBlock::ToolCall(_) => None,
                }),
        )
}

fn parse_axo_row(info: &axo::SessionInfo, skipped: &mut usize) -> Option<HistoryRow> {
    let read = || -> std::io::Result<Option<HistoryRow>> {
        let mut entries = Vec::new();
        let mut notices = HashSet::new();
        let mut row = HistoryRow {
            message_count: Some(0),
            ..HistoryRow::default()
        };
        let mut header = true;
        history::each_line(std::fs::File::open(&info.path)?, |line| {
            if std::mem::take(&mut header)
                || ![
                    "\"type\":\"submission\"",
                    "\"type\":\"message\"",
                    "\"type\":\"session_info\"",
                ]
                .iter()
                .any(|kind| line.contains(kind))
            {
                return ControlFlow::Continue(());
            }
            let Ok(entry) = serde_json::from_str::<Entry>(line) else {
                return ControlFlow::Continue(());
            };
            // Count stored user/assistant messages with text, not submissions,
            // tool-only replies, system messages, or branch/navigation records.
            match entry {
                Entry::Submission {
                    submission_id,
                    kind: axo::SubmissionKind::Quiet | axo::SubmissionKind::Mail,
                    ..
                } => {
                    notices.insert(submission_id);
                }
                Entry::Message {
                    message: axo_agent::AgentMessage::Llm(message @ axo_ai::Message::User(_)),
                    submission_id,
                    ..
                } if !submission_id
                    .as_ref()
                    .is_some_and(|id| notices.contains(id)) =>
                {
                    row.push_message(MessageRole::User, message_text(&message))
                }
                Entry::Message {
                    message: axo_agent::AgentMessage::Llm(message @ axo_ai::Message::Assistant(_)),
                    ..
                } => {
                    if let axo_ai::Message::Assistant(assistant) = &message {
                        row.model = Some(format!("{}/{}", assistant.provider, assistant.model));
                    }
                    row.push_message(MessageRole::Agent, message_text(&message));
                }
                entry @ (Entry::Submission { .. } | Entry::SessionInfo { .. }) => {
                    entries.push(entry)
                }
                Entry::Message { .. }
                | Entry::ThinkingLevelChange { .. }
                | Entry::ServiceTierChange { .. }
                | Entry::ModelChange { .. }
                | Entry::AgentChange { .. }
                | Entry::ModeChange { .. }
                | Entry::JobStart { .. }
                | Entry::ChildStart { .. }
                | Entry::JobEnd { .. }
                | Entry::Usage { .. }
                | Entry::Compaction { .. }
                | Entry::BranchSummary { .. }
                | Entry::Custom { .. }
                | Entry::CustomMessage { .. }
                | Entry::ContextEdit { .. }
                | Entry::Label { .. }
                | Entry::Leaf { .. }
                | Entry::RunStart { .. }
                | Entry::RunEnd { .. }
                | Entry::RunResumed { .. }
                | Entry::SubmissionWithdrawn { .. }
                | Entry::Unknown(_) => {}
            }
            ControlFlow::Continue(())
        })?;
        row.title = convert::title_of(entries.iter());
        Ok(Some(row))
    };
    let Some(Some(mut row)) = history::readable(
        std::fs::metadata(&info.path)
            .and_then(|metadata| history::parsed("axo", &info.path, &metadata, read)),
        skipped,
    ) else {
        return None;
    };
    row.id = info.id.clone();
    row.cwd = Some(info.cwd.to_string_lossy().into_owned());
    row.created_at = Some(info.timestamp.clone());
    row.updated_at = DateTime::<Utc>::from_timestamp_millis(info.modified_at)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true));
    Some(row)
}

struct Held<'a> {
    service: &'a AxoService,
    live: Arc<LiveSession>,
}

impl Deref for Held<'_> {
    type Target = LiveSession;

    fn deref(&self) -> &Self::Target {
        &self.live
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.live.unpin();
        self.service.maybe_unload(&self.live.id);
    }
}
