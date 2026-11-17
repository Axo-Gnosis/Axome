use std::{path::Path, sync::Arc};

use axo::{ConfigLayer, ConfigSource, PromptOverride, UserConfig, PROMPT_NAMES};
use axome_protocol::settings::SettingsLayerKind;

use crate::{
    services::{
        app_state_kv::AppStateKvService,
        db::DatabaseService,
        extensions::ExtensionService,
        settings_resolution::{load_settings_layer, validated_layer},
    },
    HostDirs,
};

pub(crate) struct AxomeConfig {
    pub dirs: HostDirs,
    pub db: Arc<DatabaseService>,
}

impl AxomeConfig {
    fn layer(&self, root: &Path, kind: SettingsLayerKind) -> Result<ConfigLayer, axo::Error> {
        let path = root.join(crate::dirs::SETTINGS_FILE);
        let settings = read_settings(&path, kind)?;
        let mut layer = ConfigLayer {
            origin: path.display().to_string(),
            settings,
            skill_roots: vec![root.join("skills")],
            agent_roots: vec![root.join("agents")],
            mode_roots: vec![root.join("modes")],
            keyword_roots: vec![root.join("keywords")],
            ..Default::default()
        };
        for name in PROMPT_NAMES {
            let path = root.join("prompts").join(format!("{name}.md"));
            if let Some(text) = read_optional(&path)? {
                layer.prompts.insert(
                    name.to_owned(),
                    PromptOverride {
                        origin: path.display().to_string(),
                        text,
                    },
                );
            }
        }
        Ok(layer)
    }
}

impl ConfigSource for AxomeConfig {
    fn user(&self) -> Result<UserConfig, axo::Error> {
        let mut layer = self.layer(&self.dirs.config, SettingsLayerKind::Global)?;
        let last_model = AppStateKvService::get(self.db.read().conn(), "axo.lastExplicitModel")
            .map_err(|error| axo::Error::Host(error.into()))?
            .map(|json| serde_json::from_str::<String>(&json))
            .transpose()
            .map_err(|error| axo::Error::Host(error.into()))?;
        fill_roles(&mut layer.settings, last_model.as_deref());
        Ok(UserConfig {
            layer,
            context_files: vec![self.dirs.config.join("AGENTS.md")],
        })
    }

    fn project(&self, cwd: &Path) -> Result<ConfigLayer, axo::Error> {
        self.layer(
            &cwd.join(crate::dirs::FOLDER_CONFIG_DIR),
            SettingsLayerKind::Folder,
        )
    }
}

pub(super) fn read_settings(
    path: &Path,
    kind: SettingsLayerKind,
) -> Result<Option<serde_json::Value>, axo::Error> {
    // Preserve IO errors; the settings loader reports malformed JSON through diagnostics.
    read_optional(path)?;
    let catalog = ExtensionService::current();
    Ok(validated_layer(
        kind,
        load_settings_layer(kind, path.to_owned()),
        &catalog.settings_ids,
    )
    .and_then(|mut layer| layer.get_mut("axo").map(serde_json::Value::take)))
}

/// An Axome-owned key of the "axo" settings object: the folder layer, else the global one.
pub(super) fn host_setting(
    dirs: &HostDirs,
    folder: &Path,
    key: &str,
) -> Result<Option<serde_json::Value>, axo::Error> {
    for (path, kind) in [
        (
            crate::dirs::folder_settings_path(folder),
            SettingsLayerKind::Folder,
        ),
        (dirs.settings_path(), SettingsLayerKind::Global),
    ] {
        let value = read_settings(&path, kind)?
            .and_then(|mut settings| settings.get_mut(key).map(serde_json::Value::take))
            .filter(|value| !value.is_null());
        if value.is_some() {
            return Ok(value);
        }
    }
    Ok(None)
}

/// The folder's `plansDir` as configured (relative to the folder, or absolute), else the default.
pub(super) fn plans_dir(dirs: &HostDirs, folder: &Path) -> Result<String, axo::Error> {
    Ok(host_setting(dirs, folder, "plansDir")?
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .filter(|dir| !dir.is_empty())
        .unwrap_or(axome_protocol::axo_settings::DEFAULT_PLANS_DIR)
        .to_owned())
}

fn fill_roles(settings: &mut Option<serde_json::Value>, last_model: Option<&str>) {
    let fallback = settings
        .as_ref()
        .and_then(|settings| settings.get("modelRoles"))
        .and_then(|roles| roles.get("default"))
        .and_then(serde_json::Value::as_str)
        .or(last_model)
        .map(str::to_owned);
    let Some(fallback) = fallback else { return };
    let settings = settings.get_or_insert_with(|| serde_json::json!({}));
    let roles = settings
        .as_object_mut()
        .expect("validated axo settings")
        .entry("modelRoles")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("validated model roles");
    for name in ["default", "reason", "work", "dash", "conduct"] {
        roles.entry(name).or_insert_with(|| fallback.clone().into());
    }
}

pub(super) fn read_optional(path: &Path) -> Result<Option<String>, axo::Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(axo::Error::Io {
            path: path.to_owned(),
            source,
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::fill_roles;
    use serde_json::json;

    #[test]
    fn role_fallback_preserves_explicit_roles() {
        let mut configured = Some(json!({
            "modelRoles": {"default": "provider/default", "dash": "provider/fast"},
            "defaultThinkingLevel": "high"
        }));
        fill_roles(&mut configured, Some("provider/last"));
        assert_eq!(
            configured,
            Some(json!({
                "modelRoles": {
                    "default": "provider/default", "reason": "provider/default",
                    "work": "provider/default", "dash": "provider/fast", "conduct": "provider/default"
                },
                "defaultThinkingLevel": "high"
            }))
        );

        let original =
            Some(json!({"modelRoles": {"custom": "provider/custom", "work": "provider/work"}}));
        let mut unchanged = original.clone();
        fill_roles(&mut unchanged, None);
        assert_eq!(unchanged, original);
        let mut from_last = original;
        fill_roles(&mut from_last, Some("provider/last"));
        assert_eq!(
            from_last,
            Some(json!({"modelRoles": {
                "custom": "provider/custom", "work": "provider/work",
                "default": "provider/last", "reason": "provider/last",
                "dash": "provider/last", "conduct": "provider/last"
            }}))
        );

        let mut absent = None;
        fill_roles(&mut absent, None);
        assert_eq!(absent, None);
        fill_roles(&mut absent, Some("provider/last"));
        assert_eq!(
            absent,
            Some(json!({"modelRoles": {
                "default": "provider/last", "reason": "provider/last", "work": "provider/last",
                "dash": "provider/last", "conduct": "provider/last"
            }}))
        );
    }
}
