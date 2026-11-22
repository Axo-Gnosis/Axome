use crate::events::{deliver, EventSink, HostEvent};
use crate::services::settings_resolution::resolve_effective_settings_for_folder_path;
use axome_protocol::settings::{SettingsChangedEvent, SettingsLayerKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

struct FolderSettingsWatcher {
    watcher: RecommendedWatcher,
    watching_axome_dir: bool,
    directory_generation: Arc<AtomicU64>,
    registered_generation: u64,
}

pub struct SettingsWatcherService {
    dirs: crate::HostDirs,
    events: EventSink<HostEvent>,
    generation: Arc<AtomicU64>,
    on_models_file: Arc<dyn Fn() + Send + Sync>,
    on_global_settings: Arc<dyn Fn() + Send + Sync>,
    global_watcher: Mutex<Option<RecommendedWatcher>>,
    folder_watchers: Mutex<HashMap<PathBuf, Arc<Mutex<Option<FolderSettingsWatcher>>>>>,
}

impl SettingsWatcherService {
    pub fn new(
        events: EventSink<HostEvent>,
        dirs: crate::HostDirs,
        on_models_file: Arc<dyn Fn() + Send + Sync>,
        on_global_settings: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            events,
            dirs,
            generation: Arc::new(AtomicU64::new(0)),
            on_models_file,
            on_global_settings,
            global_watcher: Mutex::new(None),
            folder_watchers: Mutex::new(HashMap::new()),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn notify(&self, layer: SettingsLayerKind, folder: Option<&Path>) {
        if matches!(layer, SettingsLayerKind::Global) {
            (self.on_global_settings)();
        }
        emit_settings_changed(&self.dirs, &self.events, &self.generation, layer, folder);
    }

    pub fn is_watching(&self, folder: &Path) -> bool {
        if !self
            .global_watcher
            .try_lock()
            .is_some_and(|slot| slot.is_some())
        {
            return false;
        }
        self.folder_watchers.lock().get(folder).is_some_and(|slot| {
            slot.try_lock().is_some_and(|watcher| {
                watcher.as_ref().is_some_and(|watcher| {
                    watcher.registered_generation
                        == watcher.directory_generation.load(Ordering::Acquire)
                })
            })
        })
    }

    pub fn watch_global(&self) -> Result<(), String> {
        let mut slot = self.global_watcher.lock();
        if slot.is_some() {
            return Ok(());
        }

        let settings_file = self.dirs.settings_path();
        let watch_path = existing_watch_parent(&settings_file)
            .ok_or_else(|| format!("No existing parent for {}", settings_file.display()))?;
        let events = Arc::clone(&self.events);
        let generation = Arc::clone(&self.generation);
        let settings_file_for_events = settings_file.clone();
        let dirs = self.dirs.clone();
        let models_file = self.dirs.models_path();
        let on_models_file = Arc::clone(&self.on_models_file);
        let on_global_settings = Arc::clone(&self.on_global_settings);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if event.paths.iter().any(|path| path == &models_file) {
                on_models_file();
            }
            if !event
                .paths
                .iter()
                .any(|path| is_settings_event_path(path, &settings_file_for_events))
            {
                return;
            }
            on_global_settings();
            emit_settings_changed(&dirs, &events, &generation, SettingsLayerKind::Global, None);
        })
        .map_err(|error| format!("Failed to create global settings watcher: {error}"))?;

        watcher
            .watch(&watch_path, RecursiveMode::Recursive)
            .map_err(|error| format!("Failed to watch {}: {error}", watch_path.display()))?;
        *slot = Some(watcher);
        Ok(())
    }

    pub fn watch_folder(&self, folder_path: &Path) -> Result<(), String> {
        let slot = Arc::clone(
            self.folder_watchers
                .lock()
                .entry(folder_path.to_path_buf())
                .or_default(),
        );
        let mut slot = slot.lock();
        if let Some(folder_watcher) = slot.as_mut() {
            let generation = folder_watcher.directory_generation.load(Ordering::Acquire);
            if folder_watcher.registered_generation == generation {
                return Ok(());
            }
            folder_watcher.watching_axome_dir = false;
            ensure_axome_watch(
                folder_watcher,
                &folder_path.join(crate::dirs::FOLDER_CONFIG_DIR),
            )?;
            folder_watcher.registered_generation = generation;
            return Ok(());
        }
        let folder_path = folder_path.to_path_buf();
        let axome_dir = folder_path.join(crate::dirs::FOLDER_CONFIG_DIR);

        let settings_file = crate::dirs::folder_settings_path(&folder_path);
        let events = Arc::clone(&self.events);
        let generation = Arc::clone(&self.generation);
        let settings_file_for_events = settings_file.clone();
        let folder_path_for_events = folder_path.clone();
        let directory_generation = Arc::new(AtomicU64::new(0));
        let watch_generation = Arc::clone(&directory_generation);
        let watched_dir = axome_dir.clone();
        let dirs = self.dirs.clone();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if event.paths.iter().any(|path| path == &watched_dir) {
                // Deletion/recreation invalidates the old inotify watch even
                // when the directory exists again before the next request.
                watch_generation.fetch_add(1, Ordering::Release);
            }
            if !event
                .paths
                .iter()
                .any(|path| is_settings_event_path(path, &settings_file_for_events))
            {
                return;
            }
            emit_settings_changed(
                &dirs,
                &events,
                &generation,
                SettingsLayerKind::Folder,
                Some(folder_path_for_events.as_path()),
            );
        })
        .map_err(|error| format!("Failed to create folder settings watcher: {error}"))?;

        watcher
            .watch(&folder_path, RecursiveMode::NonRecursive)
            .map_err(|error| format!("Failed to watch {}: {error}", folder_path.display()))?;

        let mut folder_watcher = FolderSettingsWatcher {
            watcher,
            watching_axome_dir: false,
            directory_generation,
            registered_generation: 0,
        };
        ensure_axome_watch(&mut folder_watcher, &axome_dir)?;
        *slot = Some(folder_watcher);
        Ok(())
    }

    pub fn stop(&self, folder_path: &Path) {
        let watcher = self.folder_watchers.lock().remove(folder_path);
        drop(watcher);
    }
}

fn ensure_axome_watch(watcher: &mut FolderSettingsWatcher, axome_dir: &Path) -> Result<(), String> {
    if watcher.watching_axome_dir || !axome_dir.exists() {
        return Ok(());
    }

    watcher
        .watcher
        .watch(axome_dir, RecursiveMode::NonRecursive)
        .map_err(|error| format!("Failed to watch .axome/: {error}"))?;
    watcher.watching_axome_dir = true;
    Ok(())
}

pub(crate) fn existing_watch_parent(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

fn is_settings_event_path(path: &Path, settings_file: &Path) -> bool {
    path == settings_file || Some(path) == settings_file.parent()
}

fn emit_settings_changed(
    dirs: &crate::HostDirs,
    events: &EventSink<HostEvent>,
    generation: &AtomicU64,
    layer: SettingsLayerKind,
    folder_path: Option<&Path>,
) {
    let generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
    let diagnostics = resolve_effective_settings_for_folder_path(dirs, folder_path).diagnostics;
    deliver(
        events,
        HostEvent::SettingsChanged(SettingsChangedEvent {
            layer,
            folder_path: folder_path.map(|path| path.to_string_lossy().into_owned()),
            generation,
            diagnostics,
        }),
    );
}
