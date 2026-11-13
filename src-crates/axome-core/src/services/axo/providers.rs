use std::collections::HashMap;

use axo_ai::{Credential, CredentialSource, CredentialStore};
use axome_protocol::axo::*;

use super::AxoService;
use crate::errors::ApiError;

impl AxoService {
    pub(crate) async fn providers(&self) -> Result<AxoProvidersPayload, ApiError> {
        let engine = self.engine().await?;
        let mut counts = HashMap::<&str, u32>::new();
        for model in engine.models.all() {
            *counts.entry(&model.provider).or_default() += 1;
        }
        let mut providers = Vec::new();
        for provider in engine.models.providers() {
            let source = match engine
                .models
                .credential_source(&provider.id)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?
            {
                CredentialSource::Config => AxoCredentialSource::Config,
                CredentialSource::ApiKey => AxoCredentialSource::ApiKey,
                CredentialSource::Oauth => AxoCredentialSource::Oauth,
                CredentialSource::Env(name) => AxoCredentialSource::Env { name },
                CredentialSource::None => AxoCredentialSource::None,
            };
            providers.push(AxoProvider {
                models: counts.get(provider.id.as_str()).copied().unwrap_or(0),
                id: provider.id,
                env: provider.env,
                requires_key: provider.requires_key,
                oauth: provider.oauth,
                source,
            });
        }
        Ok(AxoProvidersPayload {
            providers,
            models_file: AxoModelsFile {
                path: self.dirs.models_path().to_string_lossy().into_owned(),
                error: engine.models_error.clone(),
            },
        })
    }

    pub(crate) async fn provider_set_key(
        &self,
        provider: &str,
        key: String,
    ) -> Result<(), ApiError> {
        if key.trim().is_empty() {
            return Err(ApiError::InvalidArgument(
                "API key must not be empty".into(),
            ));
        }
        self.credentials
            .set(provider, Credential::ApiKey { key })
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.invalidate().await;
        Ok(())
    }

    pub(crate) async fn provider_remove(&self, provider: &str) -> Result<(), ApiError> {
        self.credentials
            .remove(provider)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.invalidate().await;
        Ok(())
    }
}
