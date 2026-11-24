use std::collections::HashMap;
use std::path::Path;

use axo_ai::{Credential, CredentialSource, CredentialStore};
use axome_protocol::axo::*;

use super::AxoService;
use crate::errors::ApiError;

impl AxoService {
    pub(crate) async fn providers(
        &self,
        folder: Option<&Path>,
    ) -> Result<AxoProvidersPayload, ApiError> {
        let (models, mut origins) = self.provider_origins_for_folder(folder).await?;
        let all = models.all();
        let mut counts = HashMap::<&str, u32>::new();
        for model in all.iter() {
            *counts.entry(&model.provider).or_default() += 1;
        }
        let mut auth = Self::provider_auth(&models).await;
        let mut providers = Vec::new();
        for provider in models.providers() {
            let source = match models
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
            let status = match auth.remove(&provider.id) {
                Some(Ok(())) => AxoProviderStatus::Ready,
                Some(Err(axo_ai::Error::NoApiKey { .. })) => AxoProviderStatus::NeedsCredentials,
                Some(Err(error)) => AxoProviderStatus::Error {
                    message: error.to_string(),
                },
                // No models to check with: judge by the credential alone.
                None if provider.requires_key && matches!(source, AxoCredentialSource::None) => {
                    AxoProviderStatus::NeedsCredentials
                }
                None => AxoProviderStatus::Ready,
            };
            let (origin, origin_project, hides) = origins
                .remove(&provider.id)
                .map_or((None, false, None), |origin| {
                    (Some(origin.id), origin.project, origin.hides)
                });
            providers.push(AxoProvider {
                models: counts.get(provider.id.as_str()).copied().unwrap_or(0),
                origin_project,
                hides,
                origin,
                id: provider.id,
                env: provider.env,
                requires_key: provider.requires_key,
                login: provider.login,
                source,
                status,
            });
        }
        Ok(AxoProvidersPayload {
            providers,
            models_file: AxoModelsFile {
                path: self.dirs.models_path().to_string_lossy().into_owned(),
                error: self.models_error().await,
            },
        })
    }

    pub(crate) async fn provider_set_key(
        &self,
        provider: &str,
        key: String,
        folder: Option<&Path>,
    ) -> Result<(), ApiError> {
        if key.trim().is_empty() {
            return Err(ApiError::InvalidArgument(
                "API key must not be empty".into(),
            ));
        }
        let engine = self.engine().await?;
        let mut state = engine.extensions.mutation.lock().await;
        self.ensure_open()?;
        let models = self
            .provider_models(&engine, &mut state, folder, provider)
            .await?;
        let slot = models
            .credential_slot(provider)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.credentials
            .set(&slot, Credential::ApiKey { key })
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        drop(state);
        self.sync_models_after_credential_change().await;
        Ok(())
    }

    pub(crate) async fn provider_remove(
        &self,
        provider: &str,
        folder: Option<&Path>,
    ) -> Result<Option<String>, ApiError> {
        let engine = self.engine().await?;
        let mut state = engine.extensions.mutation.lock().await;
        self.ensure_open()?;
        let models = self
            .provider_models(&engine, &mut state, folder, provider)
            .await?;
        let warning = match models.logout(provider).await {
            Ok(()) => None,
            Err(axo_ai::Error::Io(error)) => {
                tracing::warn!(%provider, %error, "Could not remove cached model list after logout");
                Some(format!(
                    "Logged out, but the cached model list could not be removed: {error}"
                ))
            }
            Err(error) => return Err(ApiError::Internal(error.to_string())),
        };
        drop(state);
        self.sync_models_after_credential_change().await;
        Ok(warning)
    }
}
