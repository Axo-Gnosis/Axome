use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A Chromium-based browser discovered on the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedBrowser {
    pub id: String,
    pub label: String,
    pub path: String,
}

const KNOWN_BROWSERS: &[(&str, &str, &[&str])] = &[
    ("helium", "Helium", &["helium", "helium-browser"]),
    (
        "chrome",
        "Google Chrome",
        &["google-chrome", "google-chrome-stable"],
    ),
    ("chromium", "Chromium", &["chromium", "chromium-browser"]),
    ("brave", "Brave", &["brave", "brave-browser"]),
    ("vivaldi", "Vivaldi", &["vivaldi", "vivaldi-stable"]),
    (
        "edge",
        "Microsoft Edge",
        &["microsoft-edge", "microsoft-edge-stable"],
    ),
];

/// Scans `PATH` for installed Chromium-based browsers, deduplicated by binary path.
pub fn discover_browsers() -> Vec<DetectedBrowser> {
    let mut found = Vec::new();
    let mut seen_paths = HashSet::new();

    for (id, label, candidates) in KNOWN_BROWSERS {
        for bin in *candidates {
            if let Ok(path) = which::which(bin) {
                let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if seen_paths.insert(key) {
                    found.push(DetectedBrowser {
                        id: (*id).to_owned(),
                        label: (*label).to_owned(),
                        path: path.to_string_lossy().into_owned(),
                    });
                    break;
                }
            }
        }
    }

    found
}

fn first_installed_browser() -> Option<PathBuf> {
    for (_, _, candidates) in KNOWN_BROWSERS {
        for bin in *candidates {
            if let Ok(path) = which::which(bin) {
                return Some(path);
            }
        }
    }
    None
}

/// Resolves the browser executable path:
/// 1. Explicit `configured` path or command name (from `axo.browser.executablePath`)
/// 2. `AXOME_BROWSER_EXECUTABLE_PATH` environment variable
/// 3. First installed Chromium-based browser discovered on `PATH`
/// 4. `None` (lets Playwright fall back to its own cached browser, if any)
pub fn resolve_executable(configured: Option<&str>) -> Result<Option<PathBuf>, String> {
    if let Some(raw) = configured.map(str::trim).filter(|s| !s.is_empty()) {
        return resolve_explicit(raw).map(Some);
    }

    if let Ok(env_val) = std::env::var("AXOME_BROWSER_EXECUTABLE_PATH") {
        let trimmed = env_val.trim();
        if !trimmed.is_empty() {
            return resolve_explicit(trimmed).map(Some);
        }
    }

    Ok(first_installed_browser())
}

fn resolve_explicit(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if path.components().count() > 1 || path.is_absolute() {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(format!("Configured browser executable not found: {raw}"));
    }
    which::which(raw).map_err(|_| format!("Configured browser executable not found on PATH: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_missing_path_returns_error() {
        let err = resolve_explicit("/nonexistent/axome-browser-binary-xyz").unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn explicit_missing_command_returns_error() {
        let err = resolve_explicit("axome-nonexistent-browser-cmd-xyz").unwrap_err();
        assert!(err.contains("not found on PATH"));
    }
}
