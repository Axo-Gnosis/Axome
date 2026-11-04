mod convert;
mod env;
mod live;
#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock, Weak,
    },
    time::Duration,
};

use axo::store::{Entry, EntryId};
use axo::{Axo, NavigateOptions, RecoveryPolicy, SessionOptions};
use axo_ai::ThinkingLevel;
use axome_protocol::chat::*;
use axome_protocol::session::MessageRole;
use chrono::{DateTime, SecondsFormat, Utc};
use parking_lot::Mutex;
use tokio::runtime::Handle;

use crate::{
    errors::ApiError,
    events::EventSink,
    services::{
        db::DatabaseService,
        history::{self, HistoryRow, Listing},
        session_index::{SessionIndexService, SourceId},
    },
};
use live::LiveSession;

pub(crate) struct AxoService {
    this: Weak<Self>,
    db: Arc<DatabaseService>,
    runtime: OnceLock<Handle>,
    axo: tokio::sync::OnceCell<Arc<Axo>>,
    sessions: Mutex<HashMap<String, Arc<LiveSession>>>,
    // Reopening must wait for the retiring Session to release its file lock.
    retiring: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    // ponytail: one open/create at a time; per-id locks if opens contend.
    opening: tokio::sync::Mutex<()>,
    index: OnceLock<Weak<SessionIndexService>>,
    closed: AtomicBool,
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
        | axo::Error::InvalidInput(_) => ApiError::InvalidArgument(error.to_string()),
        axo::Error::EntryNotFound(_) | axo::Error::NoPendingQuestion(_) => {
            ApiError::NotFound(error.to_string())
        }
        axo::Error::Ai(_)
        | axo::Error::Agent(_)
        | axo::Error::Io { .. }
        | axo::Error::Config { .. }
        | axo::Error::Scheme(_)
        | axo::Error::NoAxoDir
        | axo::Error::Compaction(_)
        | axo::Error::MaintenanceAborted
        | axo::Error::ContextChanged
        | axo::Error::InvalidHistory(_) => ApiError::Internal(format!("axo: {error}")),
    }
}

/// How long a session with no file yet stays loaded while unattached: long enough for a
/// reconnect or a window move to reattach, since a retired unwritten session cannot be reopened.
const UNWRITTEN_GRACE: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(60)
};

impl AxoService {
    pub(crate) fn new(db: Arc<DatabaseService>) -> Arc<Self> {
        Arc::new_cyclic(|this| Self {
            this: this.clone(),
            db,
            runtime: OnceLock::new(),
            axo: tokio::sync::OnceCell::new(),
            sessions: Mutex::new(HashMap::new()),
            retiring: Mutex::new(HashMap::new()),
            opening: tokio::sync::Mutex::new(()),
            index: OnceLock::new(),
            closed: AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_axo(db: Arc<DatabaseService>, axo: Arc<Axo>) -> Arc<Self> {
        let service = Self::new(db);
        let _ = service.axo.set(axo);
        service.bind_runtime();
        service
    }

    fn runtime(&self) -> Option<&Handle> {
        if let Some(handle) = self.runtime.get() {
            return Some(handle);
        }
        let handle = Handle::try_current().ok()?;
        Some(self.runtime.get_or_init(|| handle))
    }

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

    async fn axo(&self) -> Result<&Arc<Axo>, ApiError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ApiError::Internal("Axome is shutting down".into()));
        }
        self.axo
            .get_or_try_init(|| async {
                Axo::builder()
                    .build()
                    .await
                    .map(Arc::new)
                    .map_err(axo_error)
            })
            .await
    }

    fn options(
        &self,
        folder: &Path,
        model: Option<String>,
        thinking: Option<ThinkingLevel>,
    ) -> SessionOptions {
        SessionOptions {
            cwd: folder.to_owned(),
            model,
            thinking,
            ask_enabled: true,
            project_access: None,
            exec_env: Some(Arc::new(env::FolderExecEnv::new(
                self.db.clone(),
                folder.to_owned(),
            ))),
            sections: Vec::new(),
            recovery: RecoveryPolicy::Resume,
            ..Default::default()
        }
    }

    async fn live(&self, folder: &Path, id: &str) -> Result<Arc<LiveSession>, ApiError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ApiError::Internal("Axome is shutting down".into()));
        }
        if let Some(live) = self.sessions.lock().get(id).cloned() {
            return if live.folder == folder {
                Ok(live)
            } else {
                Err(ApiError::NotFound(format!("axo session {id}")))
            };
        }
        let _opening = self.opening.lock().await;
        if let Some(live) = self.sessions.lock().get(id).cloned() {
            return if live.folder == folder {
                Ok(live)
            } else {
                Err(ApiError::NotFound(format!("axo session {id}")))
            };
        }
        let retiring = self.retiring.lock().remove(id);
        if let Some(retiring) = retiring {
            if let Err(error) = retiring.await {
                tracing::warn!(%error, session_id = id, "axo retiring session task failed");
            }
        }
        let axo = self.axo().await?;
        let info = axo
            .list_sessions(folder)
            .await
            .map_err(axo_error)?
            .into_iter()
            .find(|info| info.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("axo session {id}")))?;
        let session = axo
            .open_session(&info.path, self.options(folder, None, None))
            .await
            .map_err(axo_error)?;
        self.insert(folder.to_owned(), session).await
    }

    async fn insert(
        &self,
        folder: PathBuf,
        session: axo::Session,
    ) -> Result<Arc<LiveSession>, ApiError> {
        let live = LiveSession::start(self.this.clone(), folder, session).await;
        {
            let mut sessions = self.sessions.lock();
            if !self.closed.load(Ordering::Acquire) {
                sessions.insert(live.id.clone(), live.clone());
                return Ok(live);
            }
        }
        live.shutdown().await;
        Err(ApiError::Internal("Axome is shutting down".into()))
    }

    fn schedule_refresh(&self, folder: &Path) {
        if let Some(index) = self.index.get().and_then(Weak::upgrade) {
            index.schedule(folder, SourceId::Axo, None);
        }
    }

    fn context_window(&self, selector: Option<&str>) -> u64 {
        self.axo
            .get()
            .and_then(|axo| selector.and_then(|s| axo.models().find(s)))
            .map_or(0, |m| m.context_window)
    }

    fn maybe_unload(&self, id: &str) {
        let mut sessions = self.sessions.lock();
        let Some(live) = sessions.get(id) else { return };
        if !live.idle_and_unattached() {
            return;
        }
        if live.session.path().exists() {
            if let Some(live) = sessions.remove(id) {
                self.retire(live);
            }
            return;
        }
        drop(sessions);
        // axo creates the session file on the first submission; a session without one cannot be reopened once retired, so its owner gets UNWRITTEN_GRACE to reattach.
        self.retire_unwritten_later(id);
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
            let mut sessions = service.sessions.lock();
            if sessions
                .get(&id)
                .is_some_and(|live| live.idle_and_unattached())
            {
                if let Some(live) = sessions.remove(&id) {
                    service.retire(live);
                }
            }
        });
    }
    // Called with the registry locked so reopen cannot miss the retirement barrier.
    fn retire(&self, live: Arc<LiveSession>) {
        live.abort_relay();
        if let Some(runtime) = self.runtime() {
            let id = live.id.clone();
            let task = runtime.spawn(async move {
                live.shutdown().await;
            });
            self.retiring.lock().insert(id, task);
        }
    }

    fn crashed(&self, id: &str, message: String) {
        if let Some(live) = self.sessions.lock().remove(id) {
            live.broadcast_closed(format!("axo session stopped unexpectedly: {message}"));
            self.retire(live);
        }
    }

    pub(crate) async fn catalog(&self) -> Result<ChatModelCatalog, ApiError> {
        let axo = self.axo().await?;
        let mut checked = HashSet::new();
        let mut available = HashSet::new();
        for model in axo.models().all() {
            if checked.insert(model.provider.clone())
                && axo.models().auth(model, None).await.is_ok()
            {
                available.insert(model.provider.clone());
            }
        }
        let mut models: Vec<_> = axo
            .models()
            .all()
            .filter(|m| available.contains(&m.provider))
            .map(|m| ChatModel {
                selector: format!("{}/{}", m.provider, m.id),
                provider: m.provider.clone(),
                name: m.name.clone(),
                thinking: axo_ai::thinking::supported_levels(m)
                    .into_iter()
                    .map(convert::thinking_to_wire)
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
        Ok(ChatModelCatalog {
            default_model: axo
                .settings()
                .default_model
                .clone()
                .filter(|s| models.iter().any(|m| &m.selector == s)),
            default_thinking: convert::thinking_to_wire(axo.settings().default_thinking_level),
            models,
        })
    }

    pub(crate) async fn create(
        &self,
        folder: &Path,
        model: String,
        thinking: ChatThinking,
    ) -> Result<String, ApiError> {
        let _opening = self.opening.lock().await;
        let session = self
            .axo()
            .await?
            .create_session(self.options(
                folder,
                Some(model),
                Some(convert::thinking_from_wire(thinking)),
            ))
            .await
            .map_err(axo_error)?;
        let id = self.insert(folder.to_owned(), session).await?.id.clone();
        self.maybe_unload(&id);
        Ok(id)
    }

    pub(crate) async fn attach(
        &self,
        folder: &Path,
        id: &str,
        attach_id: String,
        sink: EventSink<ChatEvent>,
    ) -> Result<(), ApiError> {
        loop {
            let live = self.live(folder, id).await?;
            let sessions = self.sessions.lock();
            if sessions
                .get(id)
                .is_some_and(|current| Arc::ptr_eq(current, &live))
            {
                return live.subscribe(attach_id, sink);
            }
        }
    }

    pub(crate) fn detach(&self, id: &str, attach_id: &str) {
        if let Some(live) = self.sessions.lock().get(id) {
            live.unsubscribe(attach_id);
        }
        self.maybe_unload(id);
    }

    pub(crate) async fn prompt(
        &self,
        folder: &Path,
        id: &str,
        request_id: String,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        self.live(folder, id).await?.prompt(request_id, parts).await
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
    pub(crate) async fn abort(&self, folder: &Path, id: &str) -> Result<(), ApiError> {
        self.live(folder, id).await?.session.abort().await;
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
    pub(crate) async fn run_command(
        &self,
        folder: &Path,
        id: &str,
        name: String,
        args: serde_json::Value,
    ) -> Result<(), ApiError> {
        let command = axo::Command::decode(&name, args).map_err(axo_error)?;
        self.live(folder, id).await?.run_command(command).await
    }

    pub(crate) async fn resolve_link(
        &self,
        folder: &Path,
        id: &str,
        url: String,
    ) -> Result<ChatLinkTarget, ApiError> {
        let live = self.sessions.lock().get(id).cloned();
        let unloaded;
        let schemes = if let Some(live) = &live {
            if live.folder != folder {
                return Err(ApiError::NotFound(format!("axo session {id}")));
            }
            live.session.schemes()
        } else {
            let axo = self.axo().await?;
            let info = axo
                .list_sessions(folder)
                .await
                .map_err(axo_error)?
                .into_iter()
                .find(|info| info.id == id)
                .ok_or_else(|| ApiError::NotFound(format!("axo session {id}")))?;
            unloaded = axo
                .schemes(&info.path, None, &[])
                .await
                .map_err(axo_error)?;
            &unloaded
        };
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
                NavigateOptions { summarize: false },
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
            .navigate(Some(target), NavigateOptions { summarize: false })
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
        let fork = live
            .session
            .fork(&EntryId::from(entry_id))
            .await
            .map_err(axo_error)?;
        let _opening = self.opening.lock().await;
        let session = self.insert(folder.to_owned(), fork.session).await?;
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
        let mut listing = Listing::default();
        for info in infos {
            let read = || -> std::io::Result<Option<HistoryRow>> {
                let mut entries = Vec::new();
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
                        Entry::Message {
                            message: axo_agent::AgentMessage::Llm(axo_ai::Message::User(user)),
                            ..
                        } => row.push_message(
                            MessageRole::User,
                            user.content.iter().filter_map(|block| match block {
                                axo_ai::UserBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            }),
                        ),
                        Entry::Message {
                            message:
                                axo_agent::AgentMessage::Llm(axo_ai::Message::Assistant(assistant)),
                            ..
                        } => {
                            row.model = Some(format!("{}/{}", assistant.provider, assistant.model));
                            row.push_message(
                                MessageRole::Agent,
                                assistant.content.iter().filter_map(|block| match block {
                                    axo_ai::AssistantBlock::Text { text, .. } => {
                                        Some(text.as_str())
                                    }
                                    _ => None,
                                }),
                            );
                        }
                        entry @ (Entry::Submission { .. } | Entry::SessionInfo { .. }) => {
                            entries.push(entry)
                        }
                        Entry::Message { .. }
                        | Entry::ThinkingLevelChange { .. }
                        | Entry::ModelChange { .. }
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
                &mut listing.skipped,
            ) else {
                continue;
            };
            row.id = info.id;
            row.cwd = Some(info.cwd.to_string_lossy().into_owned());
            row.created_at = Some(info.timestamp);
            row.updated_at = Some(
                DateTime::<Utc>::from(info.modified).to_rfc3339_opts(SecondsFormat::Millis, true),
            );
            listing.rows.push(row);
        }
        listing.rows = history::normalize(std::mem::take(&mut listing.rows), Some(folder), None);
        Ok(listing)
    }

    pub(crate) fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .drain()
            .map(|(_, live)| {
                live.abort_relay();
                live
            })
            .collect();
        let retiring: Vec<_> = self.retiring.lock().drain().map(|(_, task)| task).collect();
        if sessions.is_empty() && retiring.is_empty() {
            return;
        }
        let Some(runtime) = self.runtime().cloned() else {
            return;
        };
        let result = std::thread::spawn(move || {
            runtime.block_on(async move {
                let mut tasks = tokio::task::JoinSet::new();
                for live in sessions {
                    tasks.spawn(async move {
                        live.shutdown().await;
                    });
                }
                for task in retiring {
                    tasks.spawn(async move {
                        if let Err(error) = task.await {
                            tracing::warn!(%error, "axo retiring session task failed");
                        }
                    });
                }
                tokio::time::timeout(Duration::from_secs(10), async {
                    while let Some(result) = tasks.join_next().await {
                        if let Err(error) = result {
                            tracing::warn!(%error, "axo shutdown task failed");
                        }
                    }
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
