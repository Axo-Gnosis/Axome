//! Ghost-text prediction for the prompt composer, learned from submitted prompts.
//! Its operations are host-global: the desktop always asks its own host, so typed
//! text stays on the machine of the person typing.

use crate::errors::ApiError;
use crate::services::settings_resolution::resolve_effective_settings_for_folder_path;
use crate::HostDirs;
use axo_models::ModelState;
use axo_predict::{Config, ModelFiles, Options, Predictor, SMOLLM2};
use axome_protocol::chat::ChatPart;
use axome_protocol::predict::{PredictModelState, PredictStatus, PredictSuggestion};
use axome_protocol::settings::PredictSettings;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::AbortHandle;

// ponytail: persist at most every 5 min and at shutdown; a crash loses that window
const PERSIST_EVERY: Duration = Duration::from_secs(5 * 60);

// ponytail: UI polls predict_status while downloading; add a host event if polling is not enough
pub struct PredictService {
    dirs: HostDirs,
    http: reqwest::Client,
    runtime: OnceLock<Handle>,
    /// What the model files are doing: missing, downloading, on disk, failed.
    model_state: watch::Sender<ModelState>,
    state: Mutex<State>,
    /// Held by learning, reset and a whole model load. A load builds a second
    /// predictor beside the live one, so typing keeps its n-gram answers; this
    /// keeps anything from being learned into the one about to be replaced.
    learning: Mutex<()>,
}

struct State {
    settings: PredictSettings,
    predictor: Option<Predictor>,
    /// `open` failed once; do not retry until reset.
    broken: bool,
    last_persist: Instant,
    download: Option<AbortHandle>,
    /// A model load is running beside the live predictor.
    loading: bool,
    /// Downloading or loading the model failed; do not retry until reset.
    model_failed: bool,
}

impl PredictService {
    pub fn new(dirs: HostDirs) -> Arc<Self> {
        let settings = resolve_effective_settings_for_folder_path(&dirs, None)
            .settings
            .predict;
        Arc::new(Self {
            dirs,
            http: reqwest::Client::new(),
            runtime: OnceLock::new(),
            model_state: watch::channel(ModelState::Missing).0,
            state: Mutex::new(State {
                settings,
                predictor: None,
                broken: false,
                last_persist: Instant::now(),
                download: None,
                loading: false,
                model_failed: false,
            }),
            learning: Mutex::new(()),
        })
    }

    fn runtime(&self) -> Option<&Handle> {
        if let Some(handle) = self.runtime.get() {
            return Some(handle);
        }
        let handle = Handle::try_current().ok()?;
        Some(self.runtime.get_or_init(|| handle))
    }

    fn read_settings(&self) -> PredictSettings {
        resolve_effective_settings_for_folder_path(&self.dirs, None)
            .settings
            .predict
    }

    fn settings(&self) -> PredictSettings {
        self.state.lock().settings.clone()
    }

    fn state_dir(&self) -> PathBuf {
        self.dirs.data.join("predict")
    }

    fn models_root(&self) -> PathBuf {
        self.dirs.data.join("models")
    }

    pub fn complete(self: &Arc<Self>, before: &str, after: &str) -> Option<PredictSuggestion> {
        let settings = self.settings();
        self.ensure_model(&settings);
        if !settings.enabled {
            return None;
        }
        let suggestion = self
            .state
            .lock()
            .predictor(&self.state_dir())?
            .complete(before, after)?;
        Some(PredictSuggestion {
            text: suggestion.text,
            confidence: suggestion.confidence,
        })
    }

    /// Learns the typed text of a submitted prompt. Each `Text` part is its own
    /// run, so a chip between two never makes a false word pair; pastes, files
    /// and attachments are not typed and are skipped.
    pub fn observe(self: &Arc<Self>, parts: &[ChatPart]) {
        let settings = self.settings();
        self.ensure_model(&settings);
        if !settings.enabled {
            return;
        }
        let texts: Vec<String> = parts
            .iter()
            .filter_map(|part| match part {
                ChatPart::Text { text } if !text.trim().is_empty() => Some(text.clone()),
                _ => None,
            })
            .collect();
        if texts.is_empty() {
            return;
        }
        let service = Arc::clone(self);
        let learn = move || {
            let _learning = service.learning.lock();
            let pending = {
                let mut state = service.state.lock();
                let Some(predictor) = state.predictor(&service.state_dir()) else {
                    return;
                };
                let pending = predictor.observe_deferred(&texts);
                if state.last_persist.elapsed() >= PERSIST_EVERY {
                    state.persist();
                }
                pending
            };
            if let Some(update) = pending {
                match update.compute() {
                    Ok(computed) => {
                        if let Some(predictor) = service.state.lock().predictor.as_mut() {
                            predictor.apply_head(computed);
                        }
                    }
                    Err(error) => {
                        tracing::warn!("predict: updating language model head failed: {error}");
                    }
                }
            }
        };
        match self.runtime() {
            Some(runtime) => drop(runtime.spawn_blocking(learn)),
            None => learn(),
        }
    }

    pub fn status(self: &Arc<Self>) -> PredictStatus {
        let settings = self.settings();
        self.ensure_model(&settings);
        let model = if !settings.enabled || !settings.model {
            PredictModelState::Off
        } else if self.state.lock().has_model() {
            PredictModelState::Ready
        } else {
            match &*self.model_state.borrow() {
                ModelState::Missing => PredictModelState::Missing,
                &ModelState::Downloading { received, total } => {
                    PredictModelState::Downloading { received, total }
                }
                // On disk, still loading.
                ModelState::Ready => {
                    let total = SMOLLM2.files.iter().map(|file| file.size).sum();
                    PredictModelState::Downloading {
                        received: total,
                        total,
                    }
                }
                ModelState::Failed(error) => PredictModelState::Failed {
                    error: error.clone(),
                },
            }
        };
        PredictStatus {
            enabled: settings.enabled,
            model,
        }
    }

    /// Forgets everything learned, and retries a model that failed.
    pub fn reset(self: &Arc<Self>) -> Result<(), ApiError> {
        let settings = {
            let _learning = self.learning.lock();
            let mut state = self.state.lock();
            if let Some(download) = state.download.take() {
                download.abort();
            }
            self.model_state.send_replace(ModelState::Missing);
            state.predictor = None;
            state.model_failed = false;
            axo_predict::reset_dir(&self.state_dir())
                .map_err(|error| ApiError::Io(error.to_string()))?;
            state.broken = false;
            state.settings.clone()
        };
        self.ensure_model(&settings);
        Ok(())
    }

    /// Applies changed settings: loads or drops the model, releases the
    /// predictor once prediction is off.
    pub fn refresh(self: &Arc<Self>) {
        let settings = self.read_settings();
        self.state.lock().settings = settings.clone();
        self.ensure_model(&settings);
        let mut state = self.state.lock();
        if settings.enabled {
            let config = predictor_config(&settings);
            if let Some(predictor) = &mut state.predictor {
                predictor.set_config(config);
            }
        } else {
            state.persist();
            state.predictor = None;
        }
    }

    pub fn shutdown(&self) {
        self.state.lock().persist();
    }

    /// Brings the model in line with settings: drops it when unwanted, otherwise
    /// downloads and loads it in the background unless that is done, running or
    /// failed.
    fn ensure_model(self: &Arc<Self>, settings: &PredictSettings) {
        let Some(runtime) = self.runtime() else {
            return;
        };
        let mut state = self.state.lock();
        if !settings.enabled || !settings.model {
            if let Some(download) = state.download.take() {
                download.abort();
                self.model_state.send_replace(ModelState::Missing);
            }
            if let Some(predictor) = state.predictor.as_mut().filter(|p| p.has_model()) {
                if let Err(error) = predictor.set_model(None) {
                    tracing::warn!("predict: unloading the language model failed: {error}");
                }
            }
            return;
        }
        if state.has_model() || state.download.is_some() || state.loading || state.model_failed {
            return;
        }
        let root = self.models_root();
        if SMOLLM2.is_complete(&root) {
            self.model_state.send_replace(ModelState::Ready);
            state.loading = true;
            let service = Arc::clone(self);
            drop(runtime.spawn_blocking(move || service.load_model()));
            return;
        }
        let total = SMOLLM2.files.iter().map(|file| file.size).sum();
        self.model_state
            .send_replace(ModelState::Downloading { received: 0, total });
        // The task's first lock waits until the handle is stored below.
        let service = Arc::clone(self);
        let task = runtime.spawn(async move {
            let result = SMOLLM2
                .download(&service.http, &root, &service.model_state)
                .await;
            let mut state = service.state.lock();
            state.download = None;
            match result {
                Ok(()) => {
                    state.loading = true;
                    drop(state);
                    let loader = Arc::clone(&service);
                    drop(tokio::task::spawn_blocking(move || loader.load_model()));
                }
                Err(error) => {
                    tracing::warn!("predict: model download failed: {error}");
                    state.model_failed = true;
                }
            }
        });
        state.download = Some(task.abort_handle());
    }

    /// Loads the model into a fresh predictor opened from the just-persisted
    /// state, then swaps it in. The live predictor keeps answering meanwhile;
    /// learning waits on `learning` so the swap loses nothing.
    fn load_model(&self) {
        let _learning = self.learning.lock();
        let initial_config = {
            let mut state = self.state.lock();
            state.persist();
            predictor_config(&state.settings)
        };
        let files = ModelFiles::smollm2(&SMOLLM2.path(&self.models_root()));
        let loaded = Predictor::open(Options {
            state_dir: self.state_dir(),
            config: initial_config,
        })
        .and_then(|mut predictor| predictor.set_model(Some(&files)).map(|()| predictor));
        let settings = self.settings();
        let mut state = self.state.lock();
        state.loading = false;
        match loaded {
            Ok(mut predictor) if settings.enabled && settings.model => {
                predictor.set_config(predictor_config(&settings));
                state.predictor = Some(predictor);
                state.broken = false;
            }
            // Turned off while loading.
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("predict: loading the language model failed: {error}");
                state.model_failed = true;
                self.model_state
                    .send_replace(ModelState::Failed(error.to_string()));
            }
        }
    }
}

impl State {
    /// The predictor, opened on first use.
    fn predictor(&mut self, state_dir: &Path) -> Option<&mut Predictor> {
        if self.predictor.is_none() && !self.broken {
            self.predictor = open(state_dir, predictor_config(&self.settings));
            self.broken = self.predictor.is_none();
        }
        self.predictor.as_mut()
    }

    fn has_model(&self) -> bool {
        self.predictor.as_ref().is_some_and(Predictor::has_model)
    }

    fn persist(&mut self) {
        if let Some(predictor) = &mut self.predictor {
            if let Err(error) = predictor.persist() {
                tracing::warn!("predict: saving learned state failed: {error}");
            }
        }
        self.last_persist = Instant::now();
    }
}

fn predictor_config(settings: &PredictSettings) -> Config {
    Config {
        next_words: settings.next_words,
        next_word_threshold: settings.next_word_threshold,
        ..Config::default()
    }
}

/// Opens the learned state, starting over once if it is unreadable.
fn open(state_dir: &Path, config: Config) -> Option<Predictor> {
    let options = || Options {
        state_dir: state_dir.to_owned(),
        config: config.clone(),
    };
    match Predictor::open(options()) {
        Ok(predictor) => return Some(predictor),
        Err(error) => tracing::warn!("predict: learned state unreadable, starting over: {error}"),
    }
    let reopened = axo_predict::reset_dir(state_dir).and_then(|()| Predictor::open(options()));
    reopened
        .inspect_err(|error| tracing::warn!("predict: disabled until reset: {error}"))
        .ok()
}
