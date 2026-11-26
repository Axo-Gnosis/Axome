use crate::{
    errors::ApiError,
    services::{
        env::EnvironmentService,
        harnesses,
        integrations::IntegrationService,
        settings::SettingsService,
        settings_resolution::{
            harness_config_record, load_settings_layer, parse_error_diagnostic,
            resolve_effective_settings_for_folder_path, SettingsLayerLoad,
        },
    },
    Host,
};
use axome_protocol::settings::{
    EffectiveSettingsPayload, FormattingSettings, HarnessConfigRecord, HarnessSettingsEntry,
    MemorySettings, NixSettings, PatchSettingsSectionInput, PredictSettings, SettingsFileResult,
    SettingsLayerKind, SettingsLayerPayload, SettingsPayload, ShellConfigRecord,
    TypographySettings, WindowSettings,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

impl Host {
    pub fn settings_get(&self) -> Result<SettingsPayload, ApiError> {
        self.watch_settings_paths(None);
        let resolved = resolve_effective_settings_for_folder_path(&self.dirs, None);
        let harnesses = harnesses::detect_all(
            &IntegrationService::current(),
            &resolved.settings,
            &EnvironmentService::current().base,
        )
        .into_iter()
        .map(|harness| {
            let config = harness_config_record(&resolved.settings, &harness.id);
            HarnessSettingsEntry { harness, config }
        })
        .collect();
        let lsp = resolved.settings.lsp;

        Ok(SettingsPayload {
            axo: resolved.settings.axo,
            memory: resolved.settings.memory,
            predict: resolved.settings.predict,
            window: resolved.settings.window,
            typography: resolved.settings.typography,
            nix: resolved.settings.nix,
            formatting: resolved.settings.formatting,
            lsp,
            harnesses,
            shell: resolved.settings.shell,
        })
    }

    pub fn settings_get_effective(
        &self,
        folder_path: Option<String>,
    ) -> Result<EffectiveSettingsPayload, ApiError> {
        // Remote workspaces have no local folder layer: the daemon owns their
        // files, and the desktop's own settings still govern this window.
        let folder_path = folder_path
            .filter(|path| !path.starts_with("axome://"))
            .map(PathBuf::from);
        self.watch_settings_paths(folder_path.as_deref());
        let resolved =
            resolve_effective_settings_for_folder_path(&self.dirs, folder_path.as_deref());
        Ok(EffectiveSettingsPayload {
            settings: resolved.settings,
            diagnostics: resolved.diagnostics,
        })
    }

    pub fn settings_get_global_layer(&self) -> Result<SettingsLayerPayload, ApiError> {
        self.watch_settings_paths(None);
        Ok(layer_payload(load_settings_layer(
            SettingsLayerKind::Global,
            self.dirs.settings_path(),
        )))
    }

    pub fn settings_patch_global_section(
        &self,
        input: PatchSettingsSectionInput,
    ) -> Result<SettingsLayerPayload, ApiError> {
        self.settings_update_global_section(&input.section, |_| Ok(input.value))
    }

    /// Atomically update a section against its latest committed value.
    pub fn settings_update_global_section(
        &self,
        section: &str,
        update: impl FnOnce(Value) -> Result<Value, ApiError>,
    ) -> Result<SettingsLayerPayload, ApiError> {
        let path = self.dirs.settings_path();
        let payload = SettingsService::update_section(&path, section, update)?;
        self.settings_watchers
            .notify(SettingsLayerKind::Global, None);
        Ok(payload)
    }

    pub fn settings_create_global_file(&self) -> Result<SettingsFileResult, ApiError> {
        let path = self.dirs.settings_path();
        SettingsService::ensure_file(&path)?;
        Ok(SettingsFileResult {
            path: path.to_string_lossy().into_owned(),
        })
    }

    pub fn settings_open_folder_file(
        &self,
        folder_path: String,
    ) -> Result<SettingsFileResult, ApiError> {
        let path = crate::dirs::folder_settings_path(Path::new(&folder_path));
        SettingsService::ensure_file(&path)?;
        Ok(SettingsFileResult {
            path: path.to_string_lossy().into_owned(),
        })
    }

    pub fn settings_set_window(
        &self,
        settings: WindowSettings,
    ) -> Result<WindowSettings, ApiError> {
        let value = serde_json::to_value(&settings)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.settings_update_global_section("window", |_| Ok(value))?;
        Ok(settings)
    }

    pub fn settings_set_typography(
        &self,
        settings: TypographySettings,
    ) -> Result<TypographySettings, ApiError> {
        let value = serde_json::to_value(&settings)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.settings_update_global_section("typography", |_| Ok(value))?;
        Ok(settings)
    }

    pub fn settings_set_nix(&self, settings: NixSettings) -> Result<NixSettings, ApiError> {
        let value = serde_json::to_value(&settings)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.settings_update_global_section("nix", |_| Ok(value))?;
        Ok(settings)
    }

    pub fn settings_set_memory(
        &self,
        settings: MemorySettings,
    ) -> Result<MemorySettings, ApiError> {
        let value = serde_json::to_value(&settings)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.settings_update_global_section("memory", |_| Ok(value))?;
        Ok(settings)
    }

    pub fn settings_set_predict(
        &self,
        settings: PredictSettings,
    ) -> Result<PredictSettings, ApiError> {
        let value = serde_json::to_value(&settings)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.settings_update_global_section("predict", |_| Ok(value))?;
        Ok(settings)
    }

    pub fn settings_set_shell_config(
        &self,
        config: ShellConfigRecord,
    ) -> Result<ShellConfigRecord, ApiError> {
        self.settings_update_global_section("shell", |mut shell| {
            if !shell.is_object() {
                shell = json!({});
            }
            shell["binaryPathOverride"] = json!(config.binary_path_override);
            shell["extraArgs"] = json!(config.extra_args);
            Ok(shell)
        })?;
        Ok(config)
    }

    pub fn settings_set_formatting(
        &self,
        formatting: FormattingSettings,
    ) -> Result<FormattingSettings, ApiError> {
        let value = serde_json::to_value(&formatting)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        self.settings_update_global_section("formatting", |_| Ok(value))?;
        Ok(formatting)
    }

    pub fn settings_set_harness_config(
        &self,
        config: HarnessConfigRecord,
    ) -> Result<HarnessConfigRecord, ApiError> {
        self.settings_update_global_section("harnesses", |harnesses| {
            let mut root = json!({ "harnesses": harnesses });
            let map = SettingsService::object_property(&mut root, "harnesses")?;
            let entry = map.entry(config.harness_id.clone()).or_insert(json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            entry["enabled"] = json!(config.enabled);
            entry["binaryPathOverride"] = json!(config.binary_path_override);
            entry["extraArgs"] = json!(config.extra_args);
            Ok(Value::Object(std::mem::take(map)))
        })?;
        Ok(config)
    }

    pub fn settings_paths_watched(&self, folder_path: &Path) -> bool {
        self.settings_watchers.is_watching(folder_path)
    }

    pub fn watch_settings_paths(&self, folder_path: Option<&Path>) {
        if let Err(error) = self.settings_watchers.watch_global() {
            tracing::warn!("settings global watcher failed: {error}");
        }

        if let Some(folder_path) = folder_path {
            if let Err(error) = self.settings_watchers.watch_folder(folder_path) {
                tracing::warn!("settings folder watcher failed: {error}");
            }
        }
    }
}

fn layer_payload(layer: SettingsLayerLoad) -> SettingsLayerPayload {
    match layer {
        SettingsLayerLoad::Loaded(layer) => SettingsLayerPayload {
            path: layer.path.to_string_lossy().into_owned(),
            loaded: layer.loaded,
            value: layer.value,
            diagnostics: Vec::new(),
        },
        SettingsLayerLoad::Invalid {
            layer,
            path,
            message,
        } => SettingsLayerPayload {
            path: path.to_string_lossy().into_owned(),
            loaded: false,
            value: json!({}),
            diagnostics: vec![parse_error_diagnostic(layer, &path, message)],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::Arc};

    #[test]
    fn shell_config_write_preserves_presets() {
        let home =
            std::env::temp_dir().join(format!("axome-shell-settings-{}", uuid::Uuid::new_v4()));
        let host = Host::new(
            crate::HostOptions::new(crate::HostDirs::under(&home), "test.db"),
            Arc::new(|_| Ok(())),
        )
        .unwrap();
        let preset = json!({
            "command": "/bin/sh",
            "args": ["-l"],
            "env": { "DEV": "1" },
            "startup": "echo ready"
        });
        host.settings_update_global_section("shell", |_| {
            Ok(json!({
                "binaryPathOverride": "/old/shell",
                "extraArgs": ["--old"],
                "presets": { "dev": preset },
                "defaultPreset": "dev",
                "future_key": true
            }))
        })
        .unwrap();
        for binary_path_override in [Some("/new/shell".to_string()), None] {
            let config = ShellConfigRecord {
                binary_path_override,
                extra_args: vec!["--new".to_string()],
            };
            assert_eq!(
                host.settings_set_shell_config(config.clone()).unwrap(),
                config
            );
            let shell = host.settings_get_global_layer().unwrap().value["shell"].clone();
            assert_eq!(shell["presets"]["dev"], preset);
            assert_eq!(shell["defaultPreset"], "dev");
            assert_eq!(shell["future_key"], true);
            assert_eq!(
                shell["binaryPathOverride"],
                json!(config.binary_path_override)
            );
            assert_eq!(shell["extraArgs"], json!(config.extra_args));
        }
        drop(host);
        fs::remove_dir_all(home).unwrap();
    }
}
