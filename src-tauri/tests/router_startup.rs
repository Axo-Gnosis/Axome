use crate::common;

use axome_core::{events::EventSink, Host};
use axome_lib::host_events::DesktopEvent;
use axome_lib::router::WorkspaceRouter;
use std::sync::Arc;
use tempfile::tempdir;

/// Tauri builds the router inside `setup`, which runs on the main thread with
/// no Tokio runtime. Binding a QUIC socket there panics, so construction must
/// stay runtime-free. Deliberately not a `#[tokio::test]`.
#[test]
fn router_constructs_without_a_tokio_runtime() -> anyhow::Result<()> {
    let Some(_) =
        common::isolated_home("router_startup::router_constructs_without_a_tokio_runtime")
    else {
        return Ok(());
    };
    let temporary = tempdir()?;

    let events: EventSink<DesktopEvent> = Arc::new(|_| Ok(()));
    let desktop = Arc::clone(&events);
    let host = Arc::new(Host::new(
        axome_core::HostOptions::new(axome_core::HostDirs::under(temporary.path()), "test.db"),
        Arc::new(move |event| desktop(DesktopEvent::Host(event))),
    )?);
    let router = WorkspaceRouter::with_events(host, events);
    assert!(router.server_for_run("no-such-run").is_none());
    Ok(())
}
