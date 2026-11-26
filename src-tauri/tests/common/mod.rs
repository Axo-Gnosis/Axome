#![allow(dead_code)] // each test binary uses a subset

use axome_remote::Identity;
use axome_server::{auth::append_authorized, serve, ServeOptions, ServerHandle};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

// Each isolated test spawns its own subprocess so HOME/git remain isolated for its lifetime.
pub fn isolated_home(test_path: &str) -> Option<std::path::PathBuf> {
    const MARKER: &str = "AXOME_ISOLATED_TEST";
    if std::env::var(MARKER).as_deref() == Ok(test_path) {
        return Some(std::path::PathBuf::from(
            std::env::var_os("HOME").expect("isolated HOME missing"),
        ));
    }
    let root = tempfile::tempdir().expect("scratch home");
    let home = root.path().join("home");
    let git_config = root.path().join("gitconfig");
    fs::create_dir_all(&home).expect("create scratch home");
    fs::write(&git_config, "").expect("scratch git config");
    let status = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", test_path, "--nocapture"])
        .env(MARKER, test_path)
        .env("HOME", &home)
        .env("GIT_CONFIG_GLOBAL", &git_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CACHE_HOME")
        .status()
        .expect("launch isolated test");
    assert!(
        status.success(),
        "isolated test {test_path} failed: {status}"
    );
    None
}

pub fn init_repo(path: &Path) -> anyhow::Result<()> {
    let git = Command::new("git")
        .args(["-c", "init.defaultBranch=main", "init"])
        .arg(path)
        .output()?;
    anyhow::ensure!(
        git.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&git.stderr)
    );
    fs::write(path.join("hello.txt"), "sentinel\n")?;
    Ok(())
}

pub async fn start_loop(root: &Path) -> anyhow::Result<(ServerHandle, Identity, Value)> {
    let server_config = root.join("server-config");
    let server = serve(ServeOptions {
        dirs: axome_core::HostDirs {
            config: server_config.clone(),
            data: root.join("server-data"),
            cache: root.join("server-cache"),
        },
        listen: Some("127.0.0.1:0".parse()?),
        config_file: None,
        web_assets_dir: None,
        bun: None,
    })
    .await?;
    let desktop_config = axome_core::HostDirs::under(root).config;
    fs::create_dir_all(&desktop_config)?;
    let remotes = json!({
        "loop": {
            "address": format!("localhost:{}", server.local_addr.port()),
            "fingerprint": server.fingerprint.to_string(),
        }
    });
    fs::write(
        desktop_config.join("settings.jsonc"),
        serde_json::to_vec(&json!({ "remotes": remotes }))?,
    )?;
    let identity = Identity::load_or_generate(&desktop_config, "client")?;
    append_authorized(&server_config, identity.fingerprint(), "router-test")?;
    Ok((server, identity, remotes))
}
