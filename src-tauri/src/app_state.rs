use crate::host_events::{desktop_event_sink, DesktopEvent};
use crate::router::WorkspaceRouter;
use crate::windows::WindowCoordinatorService;
use axome_core::{Host, HostDirs, HostOptions};
use std::sync::Arc;

/// Desktop ownership and IPC adapters around the local host.
pub struct AppState {
    pub host: Arc<Host>,
    pub router: WorkspaceRouter,
    pub windows: Arc<WindowCoordinatorService>,
}

pub fn is_dev_app(app: &tauri::AppHandle) -> bool {
    app.config().identifier.ends_with(".debug")
}

impl AppState {
    pub fn new(app_handle: &tauri::AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let windows = Arc::new(WindowCoordinatorService::new());
        let events = desktop_event_sink(app_handle.clone(), Arc::clone(&windows));
        let desktop = Arc::clone(&events);
        let db_name = if is_dev_app(app_handle) {
            "axome-dev.db"
        } else {
            "axome.db"
        };
        let host = Arc::new(Host::new(
            HostOptions::new(HostDirs::from_env()?, db_name),
            Arc::new(move |event| desktop(DesktopEvent::Host(event))),
        )?);
        let router = WorkspaceRouter::with_events(Arc::clone(&host), events);
        Ok(Self {
            host,
            router,
            windows,
        })
    }
}
