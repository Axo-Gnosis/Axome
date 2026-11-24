//! Socket path resolution.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(super) const SOCKET_PATH_LIMIT: usize = 100;

pub(super) fn socket_path_for(project_path: &Path) -> Result<PathBuf, String> {
    let file = format!(
        "{}.sock",
        short_hash(project_path.to_string_lossy().as_ref())
    );
    let primary = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("axome/issues")
        .join(&file);
    if primary.as_os_str().len() <= SOCKET_PATH_LIMIT {
        return Ok(primary);
    }

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    let fallback = std::env::temp_dir()
        .join(format!("axome-{}", user))
        .join("issues")
        .join(file);
    if fallback.as_os_str().len() > SOCKET_PATH_LIMIT {
        return Err(format!(
            "Issue bridge socket path too long: {}",
            fallback.display()
        ));
    }
    Ok(fallback)
}

fn short_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    digest[..6].iter().map(|b| format!("{b:02x}")).collect()
}
