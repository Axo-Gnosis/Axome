use std::sync::Arc;

use axo::{Axo, JsonlStore};
use axo_ai::{FileCredentialStore, Models, ModelsConfig, ModelsOptions};

use super::{
    axo_error,
    config::{read_optional, AxomeConfig},
};
use crate::{errors::ApiError, services::db::DatabaseService, HostDirs};

pub(crate) struct Engine {
    pub axo: Arc<Axo>,
    pub models: Arc<Models>,
    pub models_error: Option<String>,
}

impl Engine {
    pub(super) fn context_window(&self, selector: Option<&str>) -> u64 {
        selector
            .and_then(|selector| self.models.find(selector))
            .map_or(0, |model| model.context_window)
    }

    pub(crate) async fn build(
        dirs: &HostDirs,
        db: &Arc<DatabaseService>,
        credentials: Arc<FileCredentialStore>,
        supervision: axo::Supervision,
    ) -> Result<Self, ApiError> {
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
        let extended_context = super::config::read_settings(
            &dirs.settings_path(),
            axome_protocol::settings::SettingsLayerKind::Global,
        )
        .map_err(axo_error)?
        .as_ref()
        .and_then(|settings| settings.get("extendedContext"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
        let models_path = dirs.models_path();
        let parsed = match read_optional(&models_path).map_err(axo_error)? {
            Some(text) => ModelsConfig::parse_jsonc(&text),
            None => Ok(ModelsConfig::default()),
        };
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
                tracing::warn!(path = %models_path.display(), %message, "Loading built-in models after model configuration failure");
                (
                    load(ModelsConfig::default())
                        .await
                        .map_err(|error| axo_error(error.into()))?,
                    Some(message),
                )
            }
        };
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
        Ok(Self {
            axo,
            models,
            models_error,
        })
    }
}
