use std::path::Path;
use std::sync::Arc;

use axo::{Axo, JsonlStore};
use axo_ai::auth::CredentialUpdate;
use axo_ai::{
    Credential, CredentialStore, FileCredentialStore, Models, ModelsConfig, ModelsOptions,
};

use super::extensions::ExtensionService;
use super::{
    axo_error,
    config::{read_optional, AxomeConfig},
};
use crate::{errors::ApiError, services::db::DatabaseService, HostDirs};

pub(crate) struct Engine {
    pub axo: Arc<Axo>,
    pub models: Arc<Models>,
    pub(super) extensions: ExtensionService,
}

struct ProviderCredentials(Arc<FileCredentialStore>);

#[async_trait::async_trait]
impl CredentialStore for ProviderCredentials {
    async fn get(&self, slot: &str) -> Result<Option<Credential>, axo_ai::Error> {
        self.0.get(slot).await
    }

    async fn set(&self, slot: &str, credential: Credential) -> Result<(), axo_ai::Error> {
        self.0.set(slot, credential).await
    }

    async fn remove(&self, slot: &str) -> Result<(), axo_ai::Error> {
        // Distinguish a failed credential removal from axo's later cache-removal I/O.
        self.0.remove(slot).await.map_err(|error| match error {
            axo_ai::Error::Io(error) => {
                axo_ai::Error::Auth(format!("Could not remove stored credential: {error}"))
            }
            error => error,
        })
    }

    async fn update(
        &self,
        slot: &str,
        update: CredentialUpdate,
    ) -> Result<Option<Credential>, axo_ai::Error> {
        self.0.update(slot, update).await
    }
}

/// The credential owner of the Axo-Extras subscription providers.
const EXTRAS_OWNER: &str = "axo-extras";

/// Registers the subscription providers Axome ships: ChatGPT Codex and the Anthropic
/// subscription (`anthropic-auth`).
pub(super) fn register_providers(models: &Models) -> Result<(), axo_ai::Error> {
    models.register(EXTRAS_OWNER, axo_codex::provider(reqwest::Client::new()))?;
    models.register(EXTRAS_OWNER, axo_anthropic_auth::provider()?)
}

pub(super) fn model(models: &Models, selector: &str) -> Option<Arc<axo_ai::Model>> {
    let (provider, id) = selector.split_once('/')?;
    models.lookup(provider, id)
}

impl Engine {
    /// Builds the engine; the `Option<String>` is the `models.jsonc` error when the file was
    /// rejected and built-in models were loaded instead.
    pub(crate) async fn build(
        dirs: &HostDirs,
        db: &Arc<DatabaseService>,
        credentials: Arc<FileCredentialStore>,
        supervision: axo::Supervision,
        extended_context: bool,
        bun: Option<&Path>,
    ) -> Result<(Self, Option<String>), ApiError> {
        let data = dirs.data.join("axo");
        let cache = dirs.cache.join("axo");
        for path in [&data, &cache] {
            tokio::fs::create_dir_all(path).await.map_err(|source| {
                axo_error(axo::Error::Io {
                    path: path.clone(),
                    source,
                })
            })?;
        }
        let config = AxomeConfig {
            dirs: dirs.clone(),
            db: db.clone(),
        };
        let parsed = read_models_config(dirs)?;
        let credentials: Arc<dyn CredentialStore> = Arc::new(ProviderCredentials(credentials));
        let load = |config| {
            Models::load(ModelsOptions {
                cache_dir: Some(cache.clone()),
                extended_context,
                config,
                credentials: credentials.clone(),
                http_client: None,
            })
        };
        let loaded = match parsed {
            Ok(config) => load(config).await,
            Err(error) => Err(error),
        };
        let (models, models_error) = match loaded {
            Ok(models) => (models, None),
            Err(error) => {
                let message = error.to_string();
                tracing::warn!(path = %dirs.models_path().display(), %message, "Loading built-in models after model configuration failure");
                (
                    load(ModelsConfig::default())
                        .await
                        .map_err(|error| axo_error(error.into()))?,
                    Some(message),
                )
            }
        };
        register_providers(&models).map_err(|error| axo_error(error.into()))?;
        let models = Arc::new(models);
        let axo = Arc::new(
            Axo::builder(
                models.clone(),
                Arc::new(JsonlStore::new(data.join("sessions"))),
            )
            .config(Arc::new(config))
            .cache_dir(cache)
            .supervision(supervision)
            .build(),
        );
        let extensions = ExtensionService::new(axo.clone(), dirs, bun).await?;
        Ok((
            Self {
                axo,
                models,
                extensions,
            },
            models_error,
        ))
    }
}

/// Applies `models.jsonc` and `extendedContext` to the live registry. `Some(message)` when the
/// file is invalid or the reload fails; the registry then keeps its last good state.
pub(super) async fn reload(
    models: &Models,
    dirs: &HostDirs,
    extended_context: bool,
) -> Result<Option<String>, ApiError> {
    let config = match read_models_config(dirs)? {
        Ok(config) => config,
        Err(error) => return Ok(Some(error.to_string())),
    };
    Ok(models
        .reload(config, extended_context)
        .await
        .err()
        .map(|error| error.to_string()))
}

/// Reads and parses `models.jsonc`; a missing file is the default config. The outer error is
/// I/O, the inner one a parse error.
fn read_models_config(dirs: &HostDirs) -> Result<Result<ModelsConfig, axo_ai::Error>, ApiError> {
    Ok(
        match read_optional(&dirs.models_path()).map_err(axo_error)? {
            Some(text) => ModelsConfig::parse_jsonc(&text),
            None => Ok(ModelsConfig::default()),
        },
    )
}

pub(super) fn read_extended_context(dirs: &HostDirs) -> Result<bool, ApiError> {
    Ok(super::config::read_settings(
        &dirs.settings_path(),
        axome_protocol::settings::SettingsLayerKind::Global,
    )
    .map_err(axo_error)?
    .as_ref()
    .and_then(|settings| settings.get("extendedContext"))
    .and_then(serde_json::Value::as_bool)
    .unwrap_or(false))
}
