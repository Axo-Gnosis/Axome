use std::{fs, future::Future, process::Command, task::Poll};

pub fn isolated(test_path: &str) -> bool {
    const MARKER: &str = "AXOME_ISOLATED_TEST";
    if std::env::var(MARKER).as_deref() == Ok(test_path) {
        return true;
    }
    let home = tempfile::tempdir().expect("scratch home");
    let git_config = home.path().join("gitconfig");
    fs::write(&git_config, "").expect("scratch git config");
    let status = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", test_path, "--nocapture"])
        .env(MARKER, test_path)
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", git_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("launch isolated test");
    assert!(
        status.success(),
        "isolated test {test_path} failed: {status}"
    );
    false
}

pub async fn pending(future: &mut (impl Future + Unpin)) -> bool {
    std::future::poll_fn(|cx| Poll::Ready(std::pin::Pin::new(&mut *future).poll(cx).is_pending()))
        .await
}
