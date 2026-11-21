use axome_protocol::{
    axo::*, axo_settings::AxoSettings, chat::ChatModel, settings::SettingsFileResult,
};
use serde_json::{json, Value};

use crate::{errors::ApiError, services::settings::SettingsService, Host};

impl Host {
    pub fn axo_models(&self) -> Result<Vec<ChatModel>, ApiError> {
        self.axo.block_on(self.axo.models())?
    }

    pub fn axo_providers(&self) -> Result<AxoProvidersPayload, ApiError> {
        self.axo.block_on(self.axo.providers())?
    }

    pub fn axo_provider_set_key(&self, provider: String, key: String) -> Result<(), ApiError> {
        self.axo
            .block_on(self.axo.provider_set_key(&provider, key))?
    }

    pub fn axo_provider_remove(&self, provider: String) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.provider_remove(&provider))?
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

    pub fn axo_login_start(&self, provider: String) -> Result<AxoLoginStarted, ApiError> {
        self.axo.block_on(self.axo.login_start(provider))?
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
        self.settings_update_global_section("axo", |mut current| {
            merge_patch(&mut current, patch);
            let settings = serde_json::from_value::<AxoSettings>(current.clone())
                .map_err(|error| ApiError::InvalidArgument(error.to_string()))?;
            for (name, selector) in &settings.model_roles {
                if !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                    || !name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                    || selector.starts_with('@')
                {
                    return Err(ApiError::InvalidArgument(format!("Invalid model role {name}: use a lowercase role name and a model selector, not another role")));
                }
            }
            Ok(current)
        })?;
        Ok(self.settings_get_effective(None)?.settings.axo)
    }
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
