use std::ffi::OsStr;
#[cfg(unix)]
use std::fs::DirBuilder;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

const APP_DIR: &str = "axome";
pub(crate) const SETTINGS_FILE: &str = "settings.jsonc";
const SHORTCUTS_FILE: &str = "shortcuts.jsonc";
const MODELS_FILE: &str = "models.jsonc";
pub(crate) const FOLDER_CONFIG_DIR: &str = ".axome";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl HostDirs {
    /// `$XDG_{CONFIG,DATA,CACHE}_HOME/axome`, falling back to the home directory.
    pub fn from_env() -> Result<Self, String> {
        let home = std::env::var_os("HOME");
        Ok(Self {
            config: directory(
                std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                home.as_deref(),
                ".config",
            )?,
            data: directory(
                std::env::var_os("XDG_DATA_HOME").as_deref(),
                home.as_deref(),
                ".local/share",
            )?,
            cache: directory(
                std::env::var_os("XDG_CACHE_HOME").as_deref(),
                home.as_deref(),
                ".cache",
            )?,
        })
    }

    /// Isolated `root/{config,data,cache}` directories for tests.
    pub fn under(root: &Path) -> Self {
        Self {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.config.join(SETTINGS_FILE)
    }

    pub fn models_path(&self) -> PathBuf {
        self.config.join(MODELS_FILE)
    }

    pub fn shortcuts_path(&self) -> PathBuf {
        self.config.join(SHORTCUTS_FILE)
    }

    /// Downloaded model files, shared by memory and predictive text.
    pub fn local_models_dir(&self) -> PathBuf {
        self.data.join("models")
    }
}

fn directory(xdg: Option<&OsStr>, home: Option<&OsStr>, fallback: &str) -> Result<PathBuf, String> {
    let base = match xdg.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(
            home.filter(|value| !value.is_empty())
                .ok_or_else(|| format!("HOME is required to resolve {fallback} path"))?,
        )
        .join(fallback),
    };
    Ok(base.join(APP_DIR))
}

pub fn folder_settings_path(folder: &Path) -> PathBuf {
    folder.join(FOLDER_CONFIG_DIR).join(SETTINGS_FILE)
}

/// Create `dir` (and any missing ancestors) with mode 0o700 set
/// atomically at creation, so the leaf directory is never world-
/// traversable even briefly. Falls back to a chmod after creation when
/// the dir already exists or on non-unix targets.
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        if dir.is_dir() {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            return Ok(());
        }
        let mut builder = DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(dir)?;
        // Recursive creation only applies the mode to the leaf dir on
        // some platforms; tighten the leaf explicitly so callers always
        // see 0o700 regardless of which ancestor segments were new.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_preserve_xdg_and_home_resolution() {
        for fallback in [".config", ".local/share", ".cache"] {
            assert_eq!(
                directory(Some(OsStr::new("/tmp/xdg")), None, fallback).unwrap(),
                PathBuf::from("/tmp/xdg/axome")
            );
            assert_eq!(
                directory(Some(OsStr::new("relative")), None, fallback).unwrap(),
                PathBuf::from("relative/axome")
            );
            for xdg in [None, Some(OsStr::new(""))] {
                assert_eq!(
                    directory(xdg, Some(OsStr::new("/home/test")), fallback).unwrap(),
                    Path::new("/home/test").join(fallback).join("axome")
                );
                assert!(directory(xdg, None, fallback).is_err());
                assert!(directory(xdg, Some(OsStr::new("")), fallback).is_err());
            }
        }
    }

    #[test]
    fn settings_and_shortcuts_share_config_directory() {
        let dirs = HostDirs::under(Path::new("/tmp/host"));
        assert_eq!(
            dirs.models_path(),
            PathBuf::from("/tmp/host/config/models.jsonc")
        );
        assert_eq!(
            dirs.settings_path(),
            PathBuf::from("/tmp/host/config/settings.jsonc")
        );
        assert_eq!(
            dirs.shortcuts_path(),
            PathBuf::from("/tmp/host/config/shortcuts.jsonc")
        );
    }
}
