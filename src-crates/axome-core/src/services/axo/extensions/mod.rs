pub(super) mod install;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsStr,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::Duration,
};

use axo::ProjectAccess;
use axo_ai::{Models, ProviderOrigin};
use axo_extensions::{
    BridgeError, EnabledExtension, ExtensionEvent, ExtensionOptions, ExtensionPackage,
    ExtensionPhase, ExtensionProblem, ExtensionScope, ExtensionStatus, Extensions, Hidden,
    InstallError, InstalledExtension, PolicyHook, ProblemKind, ProviderCollision, RunnerMemory,
    ScopeOptions,
};
use axome_protocol::{axo::*, axo_settings::AxoProjectAccess, settings::SettingsLayerKind};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::{
    sync::{broadcast, watch, Mutex, Notify},
    task::JoinHandle,
};

use crate::{
    dirs::create_private_dir,
    errors::ApiError,
    events::{deliver, HostEvent},
    services::{
        env::{resolve_bun, EnvironmentService},
        folders::{normalize_absolute_path, resolve_folder},
        settings_resolution::{load_settings_layer, SettingsLayerLoad},
    },
    HostDirs,
};

use super::{project_access, AxoService, LiveSession};

#[derive(Clone)]
pub(super) enum SessionExtensions {
    Host,
    Project(PathBuf),
}

pub(super) enum ExtensionOwner<'a> {
    Host(&'a Extensions),
    Project(ExtensionScope),
}

impl ExtensionOwner<'_> {
    pub(super) fn compose(&self, options: axo::SessionOptions) -> axo::SessionOptions {
        match self {
            Self::Host(manager) => manager.compose(options),
            Self::Project(scope) => scope.compose(options),
        }
    }

    pub(super) async fn attach(&self, session: &Arc<axo::Session>) {
        match self {
            Self::Host(manager) => manager.attach(session).await,
            Self::Project(scope) => scope.attach(session).await,
        }
    }

    pub(super) fn detach(&self, id: &str) {
        match self {
            Self::Host(manager) => manager.detach(id),
            Self::Project(scope) => scope.detach(id),
        }
    }

    pub(super) fn session_diagnostics(&self, id: &str) -> Vec<String> {
        match self {
            Self::Host(manager) => manager.session_diagnostics(id),
            Self::Project(scope) => scope.session_diagnostics(id),
        }
    }
}

pub(super) struct ProviderExtensionOrigin {
    pub id: String,
    pub project: bool,
    pub hides: Option<AxoHiddenProvider>,
}

pub(super) struct ExtensionService {
    pub(super) manager: Extensions,
    pub(super) mutation: Mutex<ExtensionState>,
    dirty: Arc<Notify>,
    published: watch::Sender<()>,
    options: ExtensionOptions,
    scope_data: PathBuf,
    staging: PathBuf,
    // Keep this until the manager has reaped its runners.
    runtime: parking_lot::Mutex<Option<RuntimeDirectory>>,
    coalescer: parking_lot::Mutex<Option<WatchTask>>,
}

#[derive(Default)]
pub(super) struct ExtensionState {
    scopes: HashMap<PathBuf, ScopeState>,
    enabled: Vec<AxoEnabledExtension>,
    projects: BTreeMap<String, Vec<AxoEnabledExtension>>,
    bun: Option<PathBuf>,
    searched: Vec<PathBuf>,
    stages: HashMap<String, install::Stage>,
    failures: HashMap<(Option<PathBuf>, String), AxoExtensionFailure>,
    diagnostics: Vec<AxoExtensionDiagnostic>,
    phases: HashMap<Option<PathBuf>, HashMap<String, ExtensionPhase>>,
    host_watch: Option<WatchTask>,
}

struct ScopeState {
    scope: ExtensionScope,
    active: bool,
    enabled: Vec<AxoEnabledExtension>,
    watches: [WatchTask; 2],
}

struct WatchTask(JoinHandle<()>);
impl Drop for WatchTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct RuntimeDirectory(PathBuf);

impl Drop for RuntimeDirectory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            tracing::warn!(%error, "extension runtime cleanup failed");
        }
    }
}

fn private_runtime_parent(path: &Path) -> bool {
    // SAFETY: getting this process's effective uid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    std::fs::symlink_metadata(path)
        .is_ok_and(|meta| meta.is_dir() && meta.uid() == uid && meta.mode() & 0o077 == 0)
}

fn resolve_runtime() -> Result<(RuntimeDirectory, PathBuf), ApiError> {
    let name = format!("ext-{}", std::process::id());
    let fits = |path: &Path| {
        path.is_absolute()
            && path.as_os_str().as_encoded_bytes().len()
                + "/scopes/18446744073709551615/axo-ext-XXXXXX/rpc".len()
                <= 107
    };
    if let Some(base) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        let parent = base.join("axome");
        let path = parent.join(&name);
        if fits(&path) && private_runtime_parent(&base) {
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            if (builder.create(&parent).is_ok() || private_runtime_parent(&parent))
                && builder.create(&path).is_ok()
            {
                return Ok((RuntimeDirectory(path.clone()), path));
            }
        }
    }
    let parent = tempfile::Builder::new()
        .prefix("axome-")
        .tempdir_in("/tmp")?;
    let path = parent.path().join(name);
    create_private_dir(&path)?;
    Ok((RuntimeDirectory(parent.keep()), path))
}

pub(crate) fn validate_enabled(
    enabled: &[AxoEnabledExtension],
    project: bool,
) -> Result<Vec<EnabledExtension>, ApiError> {
    if project && enabled.iter().any(|entry| entry.id.contains('/')) {
        return Err(ApiError::InvalidArgument(
            "Project extensions must use authored extension ids".into(),
        ));
    }
    let enabled: Vec<_> = enabled
        .iter()
        .map(|entry| EnabledExtension {
            id: entry.id.clone(),
            group: entry.group.clone(),
        })
        .collect();
    EnabledExtension::validate_all(&enabled)
        .map_err(|error| ApiError::InvalidArgument(error.to_string()))?;
    Ok(enabled)
}

pub(crate) fn validate_project_entries<'a>(
    projects: impl IntoIterator<Item = (&'a String, &'a Vec<AxoEnabledExtension>)>,
) -> Result<(), ApiError> {
    for (folder, enabled) in projects {
        let path = Path::new(folder);
        if !path.is_absolute() || normalize_absolute_path(path).as_os_str() != path.as_os_str() {
            return Err(ApiError::InvalidArgument(format!(
                "Project extension path must be canonical and absolute: {folder}"
            )));
        }
        match path.canonicalize() {
            Ok(actual) if actual.as_os_str() != path.as_os_str() || !actual.is_dir() => {
                return Err(ApiError::InvalidArgument(format!(
                    "Project extension path must be a canonical folder: {folder}"
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        validate_enabled(enabled, true)?;
    }
    Ok(())
}

pub(crate) fn scope_folder(folder: &str) -> Result<PathBuf, ApiError> {
    if !Path::new(folder).is_absolute() {
        return Err(ApiError::InvalidArgument(
            "Extension scope must be an absolute host path".into(),
        ));
    }
    let path = resolve_folder(folder)?;
    folder_key(&path)?;
    Ok(path)
}

fn folder_key(path: &Path) -> Result<&str, ApiError> {
    path.to_str()
        .ok_or_else(|| ApiError::InvalidArgument("Extension scope path must be UTF-8".into()))
}

fn read_setting<T: serde::de::DeserializeOwned + Default>(
    value: Option<&Value>,
) -> Result<T, ApiError> {
    value
        .map(|value| {
            serde::Deserialize::deserialize(value)
                .map_err(|error: serde_json::Error| ApiError::InvalidArgument(error.to_string()))
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn project_root(folder: &Path) -> PathBuf {
    folder
        .join(crate::dirs::FOLDER_CONFIG_DIR)
        .join("extensions")
}

fn bun_path(explicit: Option<&Path>) -> (Option<PathBuf>, Vec<PathBuf>) {
    resolve_bun(
        explicit,
        std::env::current_exe().ok().as_deref(),
        &EnvironmentService::current().base,
    )
}

impl ExtensionService {
    pub(super) async fn new(
        axo: Arc<axo::Axo>,
        dirs: &HostDirs,
        explicit_bun: Option<&Path>,
    ) -> Result<Self, ApiError> {
        let root = dirs.config.join("extensions");
        let base = dirs.data.join("axo/extensions");
        let data_root = base.join("data");
        let store = base.join("packages");
        let scope_data = base.join("scopes");
        // An authored root can be a symlink into the user's source tree.
        // Create only a new root; never chmod an existing package tree.
        if matches!(std::fs::symlink_metadata(&root), Err(error)
            if error.kind() == std::io::ErrorKind::NotFound)
        {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true).mode(0o700).create(&root)?;
        }
        for path in [&data_root, &store, &scope_data] {
            create_private_dir(path)?;
        }
        let staging = install::prepare(&dirs.cache).await?;
        let (runtime, runtime_dir) = resolve_runtime()?;
        let (bun, searched) = bun_path(explicit_bun);
        let base_env = &EnvironmentService::current().base;
        let mut env: axo::EnvMap = base_env
            .iter()
            .filter(|(key, _)| !key.starts_with("AXOME_"))
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        if let Some(parent) = bun.as_deref().and_then(Path::parent) {
            match std::env::join_paths(
                std::iter::once(parent.to_owned()).chain(
                    base_env
                        .get("PATH")
                        .into_iter()
                        .flat_map(|value| std::env::split_paths(OsStr::new(value))),
                ),
            ) {
                Ok(path) => {
                    env.insert("PATH".into(), path);
                }
                Err(error) => {
                    tracing::warn!(%error, "Bun directory cannot be put on the extension runner PATH")
                }
            }
        }
        let options = ExtensionOptions {
            bun: bun.clone(),
            roots: vec![root],
            data_root,
            runtime_dir,
            env: Arc::new(env),
            store: Some(store),
            auto_install: true,
        };
        Ok(Self {
            manager: Extensions::new(axo, options.clone()),
            options,
            scope_data,
            staging,
            dirty: Arc::new(Notify::new()),
            runtime: parking_lot::Mutex::new(Some(runtime)),
            published: watch::channel(()).0,
            coalescer: parking_lot::Mutex::new(None),
            mutation: Mutex::new(ExtensionState {
                bun,
                searched,
                ..Default::default()
            }),
        })
    }

    pub(super) async fn initialize(&self, service: &AxoService) -> Result<(), ApiError> {
        let mut state = self.mutation.lock().await;
        state.host_watch = Some(self.watch(service.this.clone(), None, self.manager.subscribe()));
        self.manager.configure(&[]).await.map_err(bridge_error)?;
        self.apply_settings(&mut state, &service.dirs).await?;
        let signal = self.dirty.clone();
        let service = service.this.clone();
        let task = tokio::spawn(async move {
            loop {
                signal.notified().await;
                tokio::time::sleep(Duration::from_millis(100)).await;
                let Some(service) = service.upgrade() else {
                    break;
                };
                let Some(engine) = service.engine.lock().await.engine.clone() else {
                    break;
                };
                let state = engine.extensions.mutation.lock().await;
                if service.ensure_open().is_err() {
                    break;
                }
                let roots: Vec<_> = service
                    .sessions
                    .lock()
                    .values()
                    .filter(|live| live.root_id.is_none())
                    .cloned()
                    .collect();
                for live in roots {
                    match engine
                        .extensions
                        .recorded_owner(&state, &live.extension_owner)
                    {
                        Ok(owner) => {
                            live.refresh_extension_catalog(owner.session_diagnostics(&live.id))
                        }
                        Err(error) => {
                            tracing::error!(%error, session_id = %live.id, "could not refresh extension catalog")
                        }
                    }
                }
                deliver(&service.events, HostEvent::ExtensionsChanged);
                engine.extensions.published.send_replace(());
            }
        });
        *self.coalescer.lock() = Some(WatchTask(task));
        Ok(())
    }

    fn invalidate(&self) {
        self.dirty.notify_one();
    }

    async fn apply_settings(
        &self,
        state: &mut ExtensionState,
        dirs: &HostDirs,
    ) -> Result<(), ApiError> {
        let mut diagnostics = Vec::new();
        let diagnostic = |id: &str, message| AxoExtensionDiagnostic {
            id: id.into(),
            message,
        };
        match load_settings_layer(SettingsLayerKind::Global, dirs.settings_path()) {
            SettingsLayerLoad::Invalid { message, .. } => {
                diagnostics.push(diagnostic("axo", message))
            }
            SettingsLayerLoad::Loaded(layer) => {
                let axo = layer.value.get("axo");
                if axo.is_some_and(|value| !value.is_object()) {
                    diagnostics.push(diagnostic("axo", "Expected an object".into()));
                } else {
                    let host = read_setting::<Vec<AxoEnabledExtension>>(
                        axo.and_then(|v| v.get("extensions")),
                    )
                    .and_then(|list| Ok((validate_enabled(&list, false)?, list)));
                    match host {
                        Ok((typed, list)) if state.enabled != list => {
                            self.manager.configure(&typed).await.map_err(bridge_error)?;
                            state.enabled = list;
                            state.prune_phases(None);
                            self.invalidate();
                        }
                        Ok(_) => {}
                        Err(error) => {
                            diagnostics.push(diagnostic("axo.extensions", error.to_string()))
                        }
                    }
                    let projects = read_setting::<BTreeMap<String, Vec<AxoEnabledExtension>>>(
                        axo.and_then(|v| v.get("projectExtensions")),
                    )
                    .and_then(|projects| {
                        validate_project_entries(&projects)?;
                        Ok(projects)
                    });
                    match projects {
                        Ok(projects) if projects != state.projects => {
                            state.projects = projects;
                            self.invalidate();
                        }
                        Ok(_) => {}
                        Err(error) => {
                            diagnostics.push(diagnostic("axo.projectExtensions", error.to_string()))
                        }
                    }
                }
            }
        }
        for (folder, slot) in &mut state.scopes {
            if !slot.active {
                continue;
            }
            let eligible =
                project_access(folder) == ProjectAccess::Trust && project_root(folder).is_dir();
            let enabled = state
                .projects
                .get(folder_key(folder)?)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if !eligible {
                slot.scope.configure(&[]).await.map_err(bridge_error)?;
                slot.active = false;
                slot.enabled.clear();
                state.phases.remove(&Some(folder.clone()));
                self.invalidate();
            } else if slot.enabled != enabled {
                slot.scope
                    .configure(&validate_enabled(enabled, true)?)
                    .await
                    .map_err(bridge_error)?;
                slot.enabled = enabled.to_owned();
                if let Some(phases) = state.phases.get_mut(&Some(folder.clone())) {
                    phases.retain(|id, _| enabled.iter().any(|entry| entry.id == *id));
                }
                self.invalidate();
            }
        }
        if state.diagnostics != diagnostics {
            state.diagnostics = diagnostics;
            self.invalidate();
        }
        Ok(())
    }

    pub(super) fn recorded_owner<'a>(
        &'a self,
        state: &ExtensionState,
        recorded: &SessionExtensions,
    ) -> Result<ExtensionOwner<'a>, ApiError> {
        match recorded {
            SessionExtensions::Host => Ok(ExtensionOwner::Host(&self.manager)),
            SessionExtensions::Project(folder) => {
                let slot = state.scopes.get(folder).ok_or_else(|| {
                    ApiError::Internal(format!(
                        "Recorded extension scope {} is missing",
                        folder.display()
                    ))
                })?;
                Ok(ExtensionOwner::Project(slot.scope.clone()))
            }
        }
    }

    pub(super) async fn scope(
        &self,
        state: &mut ExtensionState,
        service: &AxoService,
        folder: &Path,
    ) -> Result<Option<ExtensionScope>, ApiError> {
        if project_access(folder) != ProjectAccess::Trust || !project_root(folder).is_dir() {
            self.release_scope(state, folder).await?;
            return Ok(None);
        }
        let folder_name = folder_key(folder)?;
        let enabled = state
            .projects
            .get(folder_name)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if let Some(slot) = state.scopes.get_mut(folder) {
            if !slot.active {
                slot.scope
                    .configure(&validate_enabled(enabled, true)?)
                    .await
                    .map_err(bridge_error)?;
                slot.active = true;
                slot.enabled = enabled.to_owned();
                self.invalidate();
            }
            return Ok(Some(slot.scope.clone()));
        }
        let key = scope_key(folder_name);
        if state.scopes.values().any(|slot| slot.scope.key() == key) {
            return Err(ApiError::Internal(format!(
                "Extension scope key collision for {folder_name}"
            )));
        }
        let data_root = self.scope_data.join(&key);
        create_private_dir(&data_root)?;
        let scope = self
            .manager
            .scope(ScopeOptions {
                key,
                roots: vec![project_root(folder)],
                data_root,
                auto_install: true,
            })
            .map_err(bridge_error)?;
        let watches = [
            self.watch(
                service.this.clone(),
                Some(folder.to_owned()),
                scope.subscribe(),
            ),
            WatchTask(service.watch_models(&scope.models())),
        ];
        scope
            .configure(&validate_enabled(enabled, true)?)
            .await
            .map_err(bridge_error)?;
        state.scopes.insert(
            folder.to_owned(),
            ScopeState {
                scope: scope.clone(),
                active: true,
                enabled: enabled.to_owned(),
                watches,
            },
        );
        self.invalidate();
        Ok(Some(scope))
    }

    async fn release_scope(
        &self,
        state: &mut ExtensionState,
        folder: &Path,
    ) -> Result<(), ApiError> {
        if let Some(slot) = state.scopes.get_mut(folder).filter(|slot| slot.active) {
            slot.scope.configure(&[]).await.map_err(bridge_error)?;
            slot.active = false;
            slot.enabled.clear();
            state.phases.remove(&Some(folder.to_owned()));
            self.invalidate();
        }
        Ok(())
    }

    fn watch(
        &self,
        service: Weak<AxoService>,
        folder: Option<PathBuf>,
        mut events: broadcast::Receiver<ExtensionEvent>,
    ) -> WatchTask {
        WatchTask(tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => Some(event),
                    Err(broadcast::error::RecvError::Lagged(_)) => None,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let Some(service) = service.upgrade() else {
                    break;
                };
                let Some(engine) = service.engine.lock().await.engine.clone() else {
                    break;
                };
                let ext = &engine.extensions;
                if matches!(event, None | Some(ExtensionEvent::StateChanged(_))) {
                    let mut state = ext.mutation.lock().await;
                    let ExtensionState {
                        scopes,
                        enabled,
                        phases,
                        ..
                    } = &mut *state;
                    let scope = folder.as_ref().and_then(|folder| scopes.get(folder));
                    let enabled = match scope {
                        Some(slot) => &slot.enabled,
                        None if folder.is_none() => enabled,
                        None => continue,
                    };
                    let mut publish = |item: ExtensionStatus| {
                        if !enabled.iter().any(|entry| entry.id == item.id) {
                            return;
                        }
                        let phases = phases.entry(folder.clone()).or_default();
                        let previous = match phases.get_mut(&item.id) {
                            Some(phase) => Some(std::mem::replace(phase, item.phase)),
                            None => {
                                phases.insert(item.id.clone(), item.phase);
                                None
                            }
                        };
                        if previous.is_some_and(|p| p != ExtensionPhase::Disabled)
                            && item.phase == ExtensionPhase::Disabled
                        {
                            let Some(problem) = item.problem.filter(|problem| {
                                matches!(problem.kind, ProblemKind::Load | ProblemKind::Crash)
                            }) else {
                                return;
                            };
                            deliver(
                                &service.events,
                                HostEvent::ExtensionStopped {
                                    id: item.id,
                                    folder: folder
                                        .as_ref()
                                        .and_then(|path| path.to_str())
                                        .map(str::to_owned),
                                    problem: project_problem(problem),
                                },
                            );
                        }
                    };
                    if let Some(ExtensionEvent::StateChanged(status)) = event {
                        publish(status);
                    } else {
                        let current =
                            scope.map_or_else(|| ext.manager.status(), |slot| slot.scope.status());
                        for status in current {
                            publish(status);
                        }
                    }
                }
                ext.invalidate();
            }
        }))
    }

    pub(super) async fn shutdown(&self) {
        let (stages, mut watches) = {
            let mut state = self.mutation.lock().await;
            self.manager.shutdown().await;
            self.published.send_replace(());
            let mut watches = Vec::new();
            watches.extend(self.coalescer.lock().take());
            watches.extend(state.host_watch.take());
            for (_, slot) in state.scopes.drain() {
                watches.extend(slot.watches);
            }
            (std::mem::take(&mut state.stages), watches)
        };
        for task in &watches {
            task.0.abort();
        }
        for task in &mut watches {
            let _ = (&mut task.0).await;
        }
        for (_, stage) in stages {
            discard(stage).await;
        }
        drop(self.runtime.lock().take());
    }
}

impl ExtensionState {
    fn prune_phases(&mut self, folder: Option<PathBuf>) {
        if let Some(phases) = self.phases.get_mut(&folder) {
            phases.retain(|id, _| self.enabled.iter().any(|entry| entry.id == *id));
        }
    }
}

fn bridge_error(error: BridgeError) -> ApiError {
    let message = error.to_string();
    match error {
        BridgeError::InvalidConfig(_)
        | BridgeError::InvalidOptions(_)
        | BridgeError::InstalledDependencies(_) => ApiError::InvalidArgument(message),
        BridgeError::UnknownExtension(_) => ApiError::NotFound(message),
        BridgeError::DependencyInstall(_) | BridgeError::Registry(_) | BridgeError::ShutDown => {
            ApiError::Internal(message)
        }
    }
}

fn install_error(error: InstallError) -> ApiError {
    let message = error.to_string();
    match error {
        InstallError::InvalidOptions(_)
        | InstallError::InvalidPackage(_)
        | InstallError::SdkMismatch { .. }
        | InstallError::MissingLockfile(_)
        | InstallError::Enabled(_)
        | InstallError::NoPrevious(_)
        | InstallError::OriginChanged { .. } => ApiError::InvalidArgument(message),
        InstallError::NotInstalled(_) => ApiError::NotFound(message),
        InstallError::Io(_) => ApiError::Io(message),
        InstallError::Bun(_)
        | InstallError::NoStore
        | InstallError::Busy(_)
        | InstallError::InstallFailed { .. }
        | InstallError::ShutDown => ApiError::Internal(message),
    }
}

fn project_problem(problem: ExtensionProblem) -> AxoExtensionProblem {
    let kind = match problem.kind {
        ProblemKind::Bun => AxoExtensionProblemKind::Bun,
        ProblemKind::NotFound => AxoExtensionProblemKind::NotFound,
        ProblemKind::Invalid => AxoExtensionProblemKind::Invalid,
        ProblemKind::DependenciesMissing => AxoExtensionProblemKind::DependenciesMissing,
        ProblemKind::DependencyInstall => AxoExtensionProblemKind::DependencyInstall,
        ProblemKind::Load => AxoExtensionProblemKind::Load,
        ProblemKind::Crash => AxoExtensionProblemKind::Crash,
        ProblemKind::Setup => AxoExtensionProblemKind::Setup,
    };
    AxoExtensionProblem {
        kind,
        message: problem.message,
    }
}

fn project_phase(phase: ExtensionPhase) -> AxoExtensionPhase {
    match phase {
        ExtensionPhase::Cold => AxoExtensionPhase::Cold,
        ExtensionPhase::Installing => AxoExtensionPhase::Installing,
        ExtensionPhase::Starting => AxoExtensionPhase::Starting,
        ExtensionPhase::Ready => AxoExtensionPhase::Ready,
        ExtensionPhase::Restarting => AxoExtensionPhase::Restarting,
        ExtensionPhase::Disabled => AxoExtensionPhase::Disabled,
        ExtensionPhase::Unavailable => AxoExtensionPhase::Unavailable,
    }
}

fn project_collision(value: ProviderCollision) -> AxoProviderCollision {
    AxoProviderCollision {
        provider: value.provider,
        winner: value.winner,
        hidden: match value.hidden {
            Hidden::Extension(id) => AxoHiddenProvider::Extension { id },
            Hidden::Host(ProviderOrigin::Loaded) => AxoHiddenProvider::Loaded,
            Hidden::Host(ProviderOrigin::Registered) => AxoHiddenProvider::Registered,
            Hidden::Host(ProviderOrigin::Layer) => AxoHiddenProvider::Layer,
        },
    }
}

fn project_adds(package: &ExtensionPackage) -> Result<AxoExtensionAdds, ApiError> {
    Ok(AxoExtensionAdds {
        providers: package
            .providers
            .iter()
            .map(|provider| {
                Ok(AxoExtensionProvider {
                    id: provider.id.clone(),
                    login: provider.login,
                    models: u32::try_from(provider.models.len())
                        .map_err(|error| ApiError::Internal(error.to_string()))?,
                })
            })
            .collect::<Result<_, ApiError>>()?,
        tools: package
            .session
            .tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect(),
        commands: package
            .session
            .commands
            .iter()
            .map(|command| command.name.clone())
            .collect(),
        tool_policy: package
            .session
            .tool_policy
            .iter()
            .map(|hook| match hook {
                PolicyHook::Before => "before".into(),
                PolicyHook::After => "after".into(),
            })
            .collect(),
        spawn_hook: package.session.spawn_hook,
    })
}

fn project_install(record: InstalledExtension) -> AxoExtensionInstall {
    let (revision, git_ref, url) =
        install::parse_revision_label(record.current.revision.as_deref());
    AxoExtensionInstall {
        origin: record.origin,
        revision,
        git_ref,
        url,
        installed_at: Some(record.current.installed_at),
        previous: record.previous.map(|previous| AxoExtensionCopy {
            revision: install::parse_revision_label(previous.revision.as_deref()).0,
            installed_at: Some(previous.installed_at),
        }),
    }
}

fn project_memory(value: RunnerMemory, project: bool) -> AxoRunnerMemory {
    AxoRunnerMemory {
        members: value.members,
        group: value.group,
        pss_bytes: value.pss_bytes,
        project,
    }
}

async fn discard(stage: install::Stage) {
    if let Err(error) = stage.discard().await {
        tracing::warn!(%error, "extension stage cleanup failed");
    }
}

impl ExtensionService {
    async fn project_items(
        &self,
        state: &ExtensionState,
        owner: Option<ExtensionOwner<'_>>,
        folder: Option<&PathBuf>,
        enabled: &[AxoEnabledExtension],
        installed: Vec<InstalledExtension>,
    ) -> Result<(Vec<AxoExtension>, Vec<AxoExtensionDiagnostic>), ApiError> {
        let mut installed: HashMap<_, _> = installed
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();
        let (authored, status, diagnostics) = match &owner {
            Some(ExtensionOwner::Host(manager)) => (
                manager.authored().await,
                manager.status(),
                manager.diagnostics(),
            ),
            Some(ExtensionOwner::Project(scope)) => {
                (scope.authored().await, scope.status(), scope.diagnostics())
            }
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        let authored: BTreeSet<_> = authored.into_iter().collect();
        let mut status: HashMap<_, _> = status
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect();
        let mut diagnostics: Vec<_> = diagnostics
            .into_iter()
            .map(|item| AxoExtensionDiagnostic {
                id: item.id,
                message: item.message,
            })
            .collect();
        let mut off: BTreeSet<&str> = authored
            .iter()
            .chain(installed.keys())
            .map(String::as_str)
            .collect();
        for entry in enabled {
            off.remove(entry.id.as_str());
        }
        let ids: Vec<_> = enabled
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.id.clone(), Some((i, entry.group.clone()))))
            .chain(off.into_iter().map(|id| (id.to_owned(), None)))
            .collect();
        let mut items = Vec::with_capacity(ids.len());
        for (id, entry) in ids {
            let installed = installed.remove(&id).map(project_install);
            let status: Option<ExtensionStatus> = status.remove(&id);
            let origin = if status
                .as_ref()
                .and_then(|status| status.problem.as_ref())
                .is_some_and(|p| p.kind == ProblemKind::NotFound)
            {
                AxoExtensionOrigin::Missing
            } else if id.contains('/') {
                AxoExtensionOrigin::Installed
            } else if authored.contains(&id) {
                if folder.is_some() {
                    AxoExtensionOrigin::ProjectFolder
                } else {
                    AxoExtensionOrigin::UserFolder
                }
            } else {
                AxoExtensionOrigin::Missing
            };
            let mut problem = status.as_ref().and_then(|status| status.problem.clone());
            let inspected = match &owner {
                Some(ExtensionOwner::Host(manager)) => Some(manager.inspect(&id).await),
                Some(ExtensionOwner::Project(scope)) => Some(scope.inspect(&id).await),
                None => None,
            };
            let adds = match inspected {
                Some(Ok(package)) => {
                    let adds = project_adds(&package)?;
                    for message in package.diagnostics.into_iter().chain(package.entry_error) {
                        if !diagnostics.iter().any(|item: &AxoExtensionDiagnostic| {
                            item.id == id && item.message == message
                        }) {
                            diagnostics.push(AxoExtensionDiagnostic {
                                id: id.clone(),
                                message,
                            });
                        }
                    }
                    adds
                }
                Some(Err(error)) => {
                    if status.is_none() {
                        problem = Some(error);
                    } else if problem.as_ref() != Some(&error) {
                        diagnostics.push(AxoExtensionDiagnostic {
                            id: id.clone(),
                            message: error.message,
                        });
                    }
                    AxoExtensionAdds::default()
                }
                None => AxoExtensionAdds::default(),
            };
            let data_dir = match folder {
                Some(folder) => self
                    .scope_data
                    .join(scope_key(folder_key(folder)?))
                    .join(&id),
                None => self.options.data_dir(&id),
            };
            let position = entry
                .as_ref()
                .map(|(i, _)| u32::try_from(i + 1))
                .transpose()
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            let failure = state.failures.get(&(folder.cloned(), id.clone())).cloned();
            items.push(AxoExtension {
                id,
                origin,
                enabled: entry.is_some(),
                position,
                group: entry.and_then(|(_, group)| group),
                phase: status.map(|status| project_phase(status.phase)),
                problem: problem.map(project_problem),
                adds,
                install: installed,
                data_dir: data_dir.to_string_lossy().into_owned(),
                failure,
            });
        }
        Ok((items, diagnostics))
    }
}

fn scope_key(folder: &str) -> String {
    format!("{:x}", Sha256::digest(folder.as_bytes()))[..16].to_owned()
}

impl AxoService {
    pub(crate) fn global_settings_changed(&self) {
        let (Some(runtime), Some(service)) = (self.runtime(), self.this.upgrade()) else {
            return;
        };
        runtime.spawn(async move {
            let Some(engine) = service.engine.lock().await.engine.clone() else {
                return;
            };
            let ext = &engine.extensions;
            let mut state = ext.mutation.lock().await;
            if service.ensure_open().is_ok() {
                if let Err(error) = ext.apply_settings(&mut state, &service.dirs).await {
                    tracing::warn!(%error, "extension configuration failed");
                }
            }
        });
    }

    pub(crate) fn release_extension_scope(&self, folder: &Path) {
        // Folder teardown is called from a blocking context. Finish withdrawing
        // the scope before the caller releases the folder's ownership gate.
        let Some(runtime) = self.runtime.get() else {
            return;
        };
        // The claimed folder path is already canonical and remains its key if
        // the directory has been deleted or replaced since the claim.
        runtime.block_on(async {
            let Some(engine) = self.engine.lock().await.engine.clone() else { return };
            let ext = &engine.extensions;
            let mut state = ext.mutation.lock().await;
            if self.ensure_open().is_ok() {
                if let Err(error) = ext.release_scope(&mut state, folder).await {
                    tracing::warn!(%error, folder = %folder.display(), "extension scope release failed");
                }
            }
        });
    }

    pub(super) async fn owner_for_session<'a>(
        &self,
        live: &'a LiveSession,
    ) -> Result<ExtensionOwner<'a>, ApiError> {
        let ext = &live.engine.extensions;
        if matches!(live.extension_owner, SessionExtensions::Host) {
            return Ok(ExtensionOwner::Host(&ext.manager));
        }
        let state = ext.mutation.lock().await;
        ext.recorded_owner(&state, &live.extension_owner)
    }

    pub(super) async fn models_for_folder(
        &self,
        folder: Option<&Path>,
    ) -> Result<Arc<Models>, ApiError> {
        let engine = self.engine().await?;
        let Some(folder) = folder else {
            return Ok(engine.models.clone());
        };
        let folder = canonical_folder(folder)?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        Ok(ext
            .scope(&mut state, self, &folder)
            .await?
            .map_or_else(|| engine.models.clone(), |scope| scope.models()))
    }

    pub(super) fn check_active_project_provider(
        state: &ExtensionState,
        folder: &Path,
        provider: &str,
    ) -> Result<(), ApiError> {
        let slot = state
            .scopes
            .get(folder)
            .filter(|slot| slot.active)
            .ok_or_else(|| ApiError::NotFound(format!("Project extension provider {provider}")))?;
        check_project_provider(&slot.scope, provider)
    }

    pub(super) async fn provider_models(
        &self,
        engine: &super::Engine,
        state: &mut ExtensionState,
        folder: Option<&Path>,
        provider: &str,
    ) -> Result<Arc<Models>, ApiError> {
        let Some(folder) = folder else {
            return Ok(engine.models.clone());
        };
        let scope = engine
            .extensions
            .scope(state, self, folder)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Project extension provider {provider}")))?;
        check_project_provider(&scope, provider)?;
        Ok(scope.models())
    }

    // Select the registry and its attribution under the same scope mutation guard.
    pub(super) async fn provider_origins_for_folder(
        &self,
        folder: Option<&Path>,
    ) -> Result<(Arc<Models>, HashMap<String, ProviderExtensionOrigin>), ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        let scope = match folder {
            Some(folder) => {
                ext.scope(&mut state, self, &canonical_folder(folder)?)
                    .await?
            }
            None => None,
        };
        let mut origins = HashMap::new();
        if let Some(scope) = &scope {
            origins.extend(scope.providers().into_iter().map(|p| {
                (
                    p.provider,
                    ProviderExtensionOrigin {
                        id: p.extension,
                        project: true,
                        hides: None,
                    },
                )
            }));
        }
        for provider in ext.manager.providers() {
            origins
                .entry(provider.provider)
                .or_insert(ProviderExtensionOrigin {
                    id: provider.extension,
                    project: false,
                    hides: None,
                });
        }
        let collisions = scope
            .as_ref()
            .into_iter()
            .flat_map(|scope| scope.collisions().into_iter().map(|c| (true, c)))
            .chain(ext.manager.collisions().into_iter().map(|c| (false, c)));
        for (project, collision) in collisions {
            if let Some(origin) = origins
                .get_mut(&collision.provider)
                .filter(|origin| origin.project == project && origin.id == collision.winner)
            {
                let hidden = project_collision(collision).hidden;
                if origin.hides.is_none() || matches!(hidden, AxoHiddenProvider::Loaded) {
                    origin.hides = Some(hidden);
                }
            }
        }
        let models = scope.map_or_else(|| engine.models.clone(), |scope| scope.models());
        let providers = models.providers();
        origins.retain(|id, _| {
            providers
                .iter()
                .any(|p| p.id == *id && p.origin == ProviderOrigin::Layer)
        });
        Ok((models, origins))
    }

    pub(crate) async fn extensions_get(
        &self,
        folder: Option<PathBuf>,
    ) -> Result<AxoExtensions, ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        let project = if let Some(folder) = folder.as_ref() {
            let folder_name = folder_key(folder)?.to_owned();
            let access = project_access(folder);
            let root_exists = project_root(folder).is_dir();
            let scope = ext.scope(&mut state, self, folder).await?;
            let retained = root_exists.then(|| state.scopes.get(folder)).flatten();
            let scope = scope.or_else(|| retained.map(|slot| slot.scope.clone()));
            let collisions = scope
                .as_ref()
                .map_or_else(Vec::new, |scope| scope.collisions());
            let enabled = state
                .projects
                .get(&folder_name)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let (items, diagnostics) = ext
                .project_items(
                    &state,
                    scope.map(ExtensionOwner::Project),
                    Some(folder),
                    enabled,
                    Vec::new(),
                )
                .await?;
            Some(AxoProjectExtensions {
                folder: folder_name,
                access: match access {
                    ProjectAccess::Ignore => AxoProjectAccess::Ignore,
                    ProjectAccess::Read => AxoProjectAccess::Read,
                    ProjectAccess::Trust => AxoProjectAccess::Trust,
                },
                root_exists,
                items,
                collisions: collisions.into_iter().map(project_collision).collect(),
                diagnostics,
            })
        } else {
            None
        };
        let installed = ext.manager.installed().await.map_err(install_error)?;
        let (host, mut diagnostics) = ext
            .project_items(
                &state,
                Some(ExtensionOwner::Host(&ext.manager)),
                None,
                &state.enabled,
                installed,
            )
            .await?;
        diagnostics.extend(state.diagnostics.iter().cloned());
        Ok(AxoExtensions {
            bun: AxoBun {
                path: state
                    .bun
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                searched: state
                    .searched
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
            },
            host,
            project,
            collisions: ext
                .manager
                .collisions()
                .into_iter()
                .map(project_collision)
                .collect(),
            diagnostics,
        })
    }

    pub(crate) async fn apply_extension_settings(
        &self,
        folder: Option<&Path>,
        persist: impl FnOnce() -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        self.ensure_open()?;
        persist()?;
        ext.apply_settings(&mut state, &self.dirs).await?;
        if let Some(folder) = folder {
            ext.scope(&mut state, self, folder).await?;
        }
        // The subscriber is registered under the mutation gate, so a refresh
        // that already ran cannot acknowledge this completed configuration.
        let mut published = ext.published.subscribe();
        ext.invalidate();
        drop(state);
        let _ = published.changed().await;
        self.ensure_open()?;
        Ok(())
    }

    async fn rearm_extension(
        &self,
        id: String,
        folder: Option<PathBuf>,
        action: ExtensionAction,
    ) -> Result<(), ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        self.ensure_open()?;
        let owner = if let Some(folder) = &folder {
            let scope = ext.scope(&mut state, self, folder).await?.ok_or_else(|| {
                ApiError::NotFound(format!("No eligible extension scope: {}", folder.display()))
            })?;
            ExtensionOwner::Project(scope)
        } else {
            ExtensionOwner::Host(&ext.manager)
        };
        let result = match (owner, action) {
            (ExtensionOwner::Host(manager), ExtensionAction::Retry) => manager.retry(&id).await,
            (ExtensionOwner::Host(manager), ExtensionAction::Reload) => manager.reload(&id).await,
            (ExtensionOwner::Host(manager), ExtensionAction::Dependencies) => {
                manager.install_dependencies(&id).await
            }
            (ExtensionOwner::Project(scope), ExtensionAction::Retry) => scope.retry(&id).await,
            (ExtensionOwner::Project(scope), ExtensionAction::Reload) => scope.reload(&id).await,
            (ExtensionOwner::Project(scope), ExtensionAction::Dependencies) => {
                scope.install_dependencies(&id).await
            }
        };
        if matches!(action, ExtensionAction::Dependencies) {
            let key = (folder, id);
            match &result {
                Ok(()) => {
                    state.failures.remove(&key);
                }
                Err(BridgeError::DependencyInstall(output)) => {
                    state.failures.insert(
                        key,
                        AxoExtensionFailure {
                            title: "Dependency installation failed".into(),
                            output: Some(output.clone()),
                        },
                    );
                }
                Err(_) => {}
            }
        }
        ext.invalidate();
        result.map_err(bridge_error)
    }

    pub(crate) async fn extension_retry(
        &self,
        id: String,
        folder: Option<PathBuf>,
    ) -> Result<(), ApiError> {
        self.rearm_extension(id, folder, ExtensionAction::Retry)
            .await
    }
    pub(crate) async fn extension_reload(
        &self,
        id: String,
        folder: Option<PathBuf>,
    ) -> Result<(), ApiError> {
        self.rearm_extension(id, folder, ExtensionAction::Reload)
            .await
    }
    pub(crate) async fn extension_install_dependencies(
        &self,
        id: String,
        folder: Option<PathBuf>,
    ) -> Result<(), ApiError> {
        self.rearm_extension(id, folder, ExtensionAction::Dependencies)
            .await
    }

    pub(crate) async fn extension_inspect(
        &self,
        source: AxoExtensionSource,
    ) -> Result<AxoExtensionStage, ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let source = match source {
            AxoExtensionSource::Folder { path } => install::Source::Folder(path.into()),
            AxoExtensionSource::Git { url, git_ref } => install::Source::Git { url, git_ref },
        };
        let stage = install::inspect(&ext.staging, source).await?;
        let mut state = ext.mutation.lock().await;
        let response = async {
            self.ensure_open()?;
            let id = &stage.package.id;
            let installed_origin = ext
                .manager
                .installed()
                .await
                .map_err(install_error)?
                .into_iter()
                .find(|entry| entry.id == *id)
                .map(|entry| entry.origin);
            Ok(AxoExtensionStage {
                stage_id: stage.id().to_owned(),
                id: id.clone(),
                compatible: true,
                sdk: stage.package.sdk.clone(),
                adds: project_adds(&stage.package)?,
                origin: stage.origin.clone(),
                revision: stage.revision.as_ref().map(|git| git.commit.clone()),
                subject: stage.revision.as_ref().map(|git| git.subject.clone()),
                installed_origin,
            })
        }
        .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                drop(state);
                discard(stage).await;
                return Err(error);
            }
        };
        state.stages.insert(response.stage_id.clone(), stage);
        Ok(response)
    }

    pub(crate) async fn extension_stage_discard(&self, stage_id: String) -> Result<(), ApiError> {
        if !install::valid_stage_id(&stage_id) {
            return Err(ApiError::InvalidArgument(
                "Invalid extension stage ID".into(),
            ));
        }
        let engine = self.engine().await?;
        let stage = engine
            .extensions
            .mutation
            .lock()
            .await
            .stages
            .remove(&stage_id);
        if let Some(stage) = stage {
            stage.discard().await?;
        }
        Ok(())
    }

    pub(crate) async fn extension_install(
        &self,
        stage_id: String,
        replace_origin: bool,
    ) -> Result<AxoInstallOutcome, ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        self.ensure_open()?;
        let stage = state
            .stages
            .get(&stage_id)
            .ok_or_else(|| ApiError::NotFound("Extension stage".into()))?;
        let id = stage.package.id.clone();
        let revision = stage.revision_label()?;
        let result = ext
            .manager
            .install(
                &stage.package.package,
                &stage.origin,
                revision.as_deref(),
                replace_origin,
            )
            .await;
        ext.invalidate();
        if let Err(InstallError::OriginChanged {
            installed, offered, ..
        }) = result
        {
            return Ok(AxoInstallOutcome::OriginChanged { installed, offered });
        }
        if let Some(stage) = state.stages.remove(&stage_id) {
            discard(stage).await;
        }
        let key = (None, id.clone());
        let installed = ext.manager.installed().await;
        match result {
            Ok(record) => {
                state.failures.remove(&key);
                let installed = match installed {
                    Ok(records) => records,
                    Err(error) => {
                        tracing::warn!(%error, "extension installed but store could not be reread");
                        vec![record]
                    }
                };
                let (items, _) = ext
                    .project_items(
                        &state,
                        Some(ExtensionOwner::Host(&ext.manager)),
                        None,
                        &state.enabled,
                        installed,
                    )
                    .await?;
                let extension = items
                    .into_iter()
                    .find(|item| item.id == id)
                    .ok_or_else(|| {
                        ApiError::Internal(format!("Installed extension {id} missing from store"))
                    })?;
                Ok(AxoInstallOutcome::Installed { extension })
            }
            Err(error) => {
                let title = error.to_string();
                let output = match error {
                    InstallError::InstallFailed { output, .. } => Some(output),
                    _ => None,
                };
                if let Err(error) = installed {
                    tracing::warn!(%error, "extension store could not be reread");
                }
                state.failures.insert(
                    key,
                    AxoExtensionFailure {
                        title: title.clone(),
                        output: output.clone(),
                    },
                );
                Ok(AxoInstallOutcome::Failed { title, output })
            }
        }
    }

    pub(crate) async fn extension_rollback(&self, id: String) -> Result<(), ApiError> {
        self.change_installed(id, InstalledAction::Rollback).await
    }

    pub(crate) async fn extension_uninstall(
        &self,
        id: String,
        delete_data: bool,
        disable: impl FnOnce(&str) -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        self.change_installed(
            id,
            InstalledAction::Uninstall {
                delete_data,
                disable: Box::new(disable),
            },
        )
        .await
    }

    async fn change_installed(
        &self,
        id: String,
        action: InstalledAction<'_>,
    ) -> Result<(), ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        self.ensure_open()?;
        let result = match action {
            InstalledAction::Rollback => ext.manager.rollback(&id).await.map(|_| ()),
            InstalledAction::Uninstall {
                delete_data,
                disable,
            } => {
                if !id.contains('/') {
                    return Err(ApiError::InvalidArgument(
                        "Expected an installed extension ID".into(),
                    ));
                }
                validate_enabled(
                    &[AxoEnabledExtension {
                        id: id.clone(),
                        group: None,
                    }],
                    false,
                )?;
                disable(&id)?;
                ext.apply_settings(&mut state, &self.dirs).await?;
                ext.manager.uninstall(&id, delete_data).await
            }
        };
        if result.is_ok() {
            state.failures.remove(&(None, id));
        }
        if let Err(error) = ext.manager.installed().await {
            tracing::warn!(%error, "extension store could not be reread");
        }
        ext.invalidate();
        result.map_err(install_error)
    }

    pub(crate) async fn extensions_memory(
        &self,
        folder: Option<PathBuf>,
    ) -> Result<Vec<AxoRunnerMemory>, ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        let mut result: Vec<_> = ext
            .manager
            .memory()
            .await
            .into_iter()
            .map(|row| project_memory(row, false))
            .collect();
        if let Some(folder) = folder {
            if let Some(scope) = ext.scope(&mut state, self, &folder).await? {
                result.extend(
                    scope
                        .memory()
                        .await
                        .into_iter()
                        .map(|row| project_memory(row, true)),
                );
            }
        }
        Ok(result)
    }

    pub(crate) async fn extensions_check_bun(&self) -> Result<(), ApiError> {
        let engine = self.engine().await?;
        let ext = &engine.extensions;
        let mut state = ext.mutation.lock().await;
        self.ensure_open()?;
        let (bun, searched) = bun_path(self.bun.as_deref());
        state.bun = bun.clone();
        state.searched = searched;
        let result = ext.manager.set_bun(bun).await;
        ext.invalidate();
        result.map_err(bridge_error)
    }
}

pub(super) fn canonical_folder(path: &Path) -> Result<PathBuf, ApiError> {
    scope_folder(folder_key(path)?)
}

#[derive(Clone, Copy)]
enum ExtensionAction {
    Retry,
    Reload,
    Dependencies,
}

enum InstalledAction<'a> {
    Rollback,
    Uninstall {
        delete_data: bool,
        disable: Box<dyn FnOnce(&str) -> Result<(), ApiError> + 'a>,
    },
}

fn check_project_provider(scope: &ExtensionScope, provider: &str) -> Result<(), ApiError> {
    if scope
        .providers()
        .iter()
        .any(|item| item.provider == provider)
        && scope
            .models()
            .providers()
            .iter()
            .any(|item| item.id == provider && item.origin == ProviderOrigin::Layer)
    {
        Ok(())
    } else {
        Err(ApiError::NotFound(format!(
            "Project extension provider {provider}"
        )))
    }
}
