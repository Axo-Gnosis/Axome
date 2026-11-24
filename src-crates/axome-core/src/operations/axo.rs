use axome_protocol::{
    axo::*, axo_settings::AxoSettings, chat::ChatModel, settings::SettingsFileResult,
};
use serde_json::{json, Value};

use crate::services::axo::extensions::{
    scope_folder as resolve_extension_scope, validate_enabled, validate_project_entries,
};
use crate::{errors::ApiError, services::settings::SettingsService, Host};
use std::path::{Path, PathBuf};

impl Host {
    pub fn axo_models(&self) -> Result<Vec<ChatModel>, ApiError> {
        self.axo.block_on(self.axo.models())?
    }

    pub fn axo_providers(
        &self,
        scope_folder: Option<String>,
    ) -> Result<AxoProvidersPayload, ApiError> {
        let folder = optional_scope(scope_folder.as_deref())?;
        self.axo.block_on(self.axo.providers(folder.as_deref()))?
    }

    pub fn axo_provider_set_key(
        &self,
        provider: String,
        key: String,
        scope_folder: Option<String>,
    ) -> Result<(), ApiError> {
        let folder = optional_scope(scope_folder.as_deref())?;
        self.axo
            .block_on(self.axo.provider_set_key(&provider, key, folder.as_deref()))?
    }

    pub fn axo_provider_remove(
        &self,
        provider: String,
        scope_folder: Option<String>,
    ) -> Result<Option<String>, ApiError> {
        let folder = optional_scope(scope_folder.as_deref())?;
        self.axo
            .block_on(self.axo.provider_remove(&provider, folder.as_deref()))?
    }

    pub fn axo_extensions_get(
        &self,
        scope_folder: Option<String>,
    ) -> Result<AxoExtensions, ApiError> {
        let scope_folder = optional_scope(scope_folder.as_deref())?;
        self.axo.block_on(self.axo.extensions_get(scope_folder))?
    }

    pub fn axo_extensions_set_enabled(
        &self,
        enabled: Vec<AxoEnabledExtension>,
    ) -> Result<(), ApiError> {
        self.axo
            .block_on(self.axo.apply_extension_settings(None, || {
                self.update_enabled_extensions(None, |_| Ok(enabled))
            }))?
    }

    pub fn axo_project_extensions_set_enabled(
        &self,
        scope_folder: String,
        enabled: Vec<AxoEnabledExtension>,
    ) -> Result<(), ApiError> {
        let scope_folder = resolve_extension_scope(&scope_folder)?;
        self.axo
            .block_on(self.axo.apply_extension_settings(Some(&scope_folder), || {
                self.update_enabled_extensions(Some(&scope_folder), |_| Ok(enabled))
            }))?
    }

    pub fn axo_extension_retry(
        &self,
        id: String,
        scope_folder: Option<String>,
    ) -> Result<(), ApiError> {
        let scope_folder = optional_scope(scope_folder.as_deref())?;
        self.axo
            .block_on(self.axo.extension_retry(id, scope_folder))?
    }

    pub fn axo_extension_reload(
        &self,
        id: String,
        scope_folder: Option<String>,
    ) -> Result<(), ApiError> {
        let scope_folder = optional_scope(scope_folder.as_deref())?;
        self.axo
            .block_on(self.axo.extension_reload(id, scope_folder))?
    }

    pub fn axo_extension_install_dependencies(
        &self,
        id: String,
        scope_folder: Option<String>,
    ) -> Result<(), ApiError> {
        let scope_folder = optional_scope(scope_folder.as_deref())?;
        self.axo
            .block_on(self.axo.extension_install_dependencies(id, scope_folder))?
    }

    pub fn axo_extension_inspect(
        &self,
        source: AxoExtensionSource,
    ) -> Result<AxoExtensionStage, ApiError> {
        self.axo.block_on(self.axo.extension_inspect(source))?
    }

    pub fn axo_extension_stage_discard(&self, stage_id: String) -> Result<(), ApiError> {
        self.axo
            .block_on(self.axo.extension_stage_discard(stage_id))?
    }

    pub fn axo_extension_install(
        &self,
        stage_id: String,
        replace_origin: bool,
    ) -> Result<AxoInstallOutcome, ApiError> {
        self.axo
            .block_on(self.axo.extension_install(stage_id, replace_origin))?
    }

    pub fn axo_extension_rollback(&self, id: String) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.extension_rollback(id))?
    }

    pub fn axo_extension_uninstall(&self, id: String, delete_data: bool) -> Result<(), ApiError> {
        self.axo
            .block_on(self.axo.extension_uninstall(id, delete_data, |id| {
                self.update_enabled_extensions(None, |value| {
                    let mut list: Vec<AxoEnabledExtension> = match value {
                        Some(value) => serde::Deserialize::deserialize(value).map_err(
                            |error: serde_json::Error| ApiError::InvalidArgument(error.to_string()),
                        )?,
                        None => Vec::new(),
                    };
                    list.retain(|entry| entry.id != id);
                    Ok(list)
                })
            }))?
    }

    pub fn axo_extensions_memory(
        &self,
        scope_folder: Option<String>,
    ) -> Result<Vec<AxoRunnerMemory>, ApiError> {
        let scope_folder = optional_scope(scope_folder.as_deref())?;
        self.axo
            .block_on(self.axo.extensions_memory(scope_folder))?
    }

    pub fn axo_extensions_check_bun(&self) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.extensions_check_bun())?
    }

    fn update_enabled_extensions(
        &self,
        folder: Option<&Path>,
        update: impl FnOnce(Option<&Value>) -> Result<Vec<AxoEnabledExtension>, ApiError>,
    ) -> Result<(), ApiError> {
        self.settings_update_global_section("axo", |mut current| {
            if current.is_null() {
                current = json!({});
            }
            let section = current.as_object_mut().ok_or_else(|| {
                ApiError::InvalidArgument("axo settings must be an object".into())
            })?;
            let (target, key) = if let Some(folder) = folder {
                let projects = section
                    .entry("projectExtensions")
                    .or_insert_with(|| json!({}));
                let projects = projects.as_object_mut().ok_or_else(|| {
                    ApiError::InvalidArgument("projectExtensions must be an object".into())
                })?;
                (
                    projects,
                    folder.to_str().expect("scope_folder validates UTF-8"),
                )
            } else {
                (&mut *section, "extensions")
            };
            let enabled = update(target.get(key))?;
            validate_enabled(&enabled, folder.is_some())?;
            target.insert(
                key.to_owned(),
                serde_json::to_value(enabled)
                    .map_err(|error| ApiError::Internal(error.to_string()))?,
            );
            if folder.is_some() {
                let projects: std::collections::BTreeMap<String, Vec<AxoEnabledExtension>> =
                    serde::Deserialize::deserialize(&section["projectExtensions"]).map_err(
                        |error: serde_json::Error| ApiError::InvalidArgument(error.to_string()),
                    )?;
                validate_project_entries(&projects)?;
            }
            Ok(current)
        })?;
        Ok(())
    }

    pub fn axo_create_models_file(&self) -> Result<SettingsFileResult, ApiError> {
        let path = self.dirs.models_path();
        SettingsService::ensure_file(&path)?;
        Ok(SettingsFileResult {
            path: path.to_string_lossy().into_owned(),
        })
    }

    pub fn axo_models_refresh(&self) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.refresh_models())?
    }

    pub fn axo_login_start(
        &self,
        provider: String,
        scope_folder: Option<String>,
    ) -> Result<AxoLoginStarted, ApiError> {
        let folder = optional_scope(scope_folder.as_deref())?;
        self.axo
            .block_on(self.axo.login_start(provider, folder.as_deref()))?
    }

    pub fn axo_login_submit(
        &self,
        login_id: String,
        question_id: u32,
        answer: String,
    ) -> Result<(), ApiError> {
        self.axo
            .block_on(self.axo.login_submit(&login_id, question_id, answer))?
    }

    pub fn axo_login_status(&self, login_id: String) -> Result<AxoLoginStatus, ApiError> {
        self.axo.block_on(self.axo.login_status(&login_id))?
    }

    pub fn axo_login_cancel(&self, login_id: String) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.login_cancel(&login_id))?
    }

    pub fn axo_keywords(&self) -> Result<Vec<AxoKeywordInfo>, ApiError> {
        self.axo.block_on(self.axo.keywords())?
    }

    pub fn axo_web_tools_status(&self) -> Result<AxoWebToolsStatusPayload, ApiError> {
        self.axo.block_on(self.axo.web_tools_status())?
    }

    pub fn axo_search_key_set(&self, provider: String, key: String) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.search_key_set(&provider, key))?
    }

    pub fn axo_search_key_remove(&self, provider: String) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.search_key_remove(&provider))?
    }

    pub fn axo_docs_refresh(&self, source: Option<String>) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.docs_refresh(source))?
    }

    pub fn axo_docs_remove(&self, source: Option<String>) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.docs_remove(source))?
    }

    pub fn settings_set_axo(&self, patch: Value) -> Result<AxoSettings, ApiError> {
        if !patch.is_object() {
            return Err(ApiError::InvalidArgument(
                "axo settings patch must be an object".into(),
            ));
        }
        let engine = self.axo.block_on(self.axo.engine())??;
        self.settings_update_global_section("axo", |mut current| {
            let previous = if current.is_null() {
                AxoSettings::default()
            } else {
                serde_json::from_value::<AxoSettings>(current.clone())
                    .map_err(|error| ApiError::InvalidArgument(error.to_string()))?
            };
            merge_patch(&mut current, patch);
            let settings = serde_json::from_value::<AxoSettings>(current.clone())
                .map_err(|error| ApiError::InvalidArgument(error.to_string()))?;
            validate_enabled(&settings.extensions, false)?;
            validate_project_entries(&settings.project_extensions)?;
            self.axo
                .validate_model_selectors(&previous, &settings, &engine.models)?;
            Ok(current)
        })?;
        Ok(self.settings_get_effective(None)?.settings.axo)
    }
}

fn optional_scope(folder: Option<&str>) -> Result<Option<PathBuf>, ApiError> {
    folder.map(resolve_extension_scope).transpose()
}

fn merge_patch(current: &mut Value, patch: Value) {
    let Value::Object(patch) = patch else {
        *current = patch;
        return;
    };
    if !current.is_object() {
        *current = json!({});
    }
    let current = current.as_object_mut().unwrap();
    for (key, value) in patch {
        if value.is_null() {
            current.remove(&key);
        } else {
            let target = current.entry(key.clone()).or_insert(Value::Null);
            merge_patch(target, value);
            if target.as_object().is_some_and(|object| object.is_empty()) {
                current.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_merges_nested_settings_and_removes_cleared_objects() {
        let mut current = json!({"bash": {"ci": true, "promoteAfterMs": 10}, "modelRoles": {"dash": "test/m"}, "defaultModel": "test/m"});
        merge_patch(
            &mut current,
            json!({"bash": {"promoteAfterMs": 20}, "modelRoles": {"dash": null}, "defaultModel": null}),
        );
        assert_eq!(current, json!({"bash": {"ci": true, "promoteAfterMs": 20}}));
    }
}
