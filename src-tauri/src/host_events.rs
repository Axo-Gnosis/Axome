use crate::windows::WindowCoordinatorService;
use axome_core::events::{EventSink, HostEvent};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tauri::Emitter;

pub enum DesktopEvent {
    Host(HostEvent),
    RemoteStatus {
        server: String,
        connected: bool,
        last_error: Option<String>,
        state: String,
    },
    RemoteRunStatus {
        run_id: String,
        state: String,
    },
    /// `server`'s workbench registry changed; the desktop re-lists it.
    WorkbenchesChanged {
        server: String,
    },
    /// `server`'s model registry changed; the desktop refetches its model lists.
    ModelsChanged {
        server: String,
    },
    ExtensionsChanged {
        server: String,
    },
    MemoryCaptureChanged {
        server: String,
        event: axome_protocol::memory::MemoryCaptureChangedEvent,
    },
    ExtensionStopped {
        server: String,
        id: String,
        folder: Option<String>,
        problem: axome_protocol::axo::AxoExtensionProblem,
    },
}

pub fn channel_sink<T: Serialize + Send + Sync + 'static>(
    channel: tauri::ipc::Channel<T>,
) -> EventSink<T> {
    Arc::new(move |payload| channel.send(payload).map_err(|error| error.to_string()))
}

pub fn desktop_event_sink(
    app: tauri::AppHandle,
    windows: Arc<WindowCoordinatorService>,
) -> EventSink<DesktopEvent> {
    Arc::new(move |event| {
        let result = match event {
            DesktopEvent::RemoteStatus { server, connected, last_error, state } =>
                app.emit("remote-status", json!({ "server": server, "connected": connected, "last_error": last_error, "state": state })),
            DesktopEvent::WorkbenchesChanged { server } =>
                app.emit("workbenches-changed", json!({ "server": server })),
            DesktopEvent::ModelsChanged { server } =>
                app.emit("models-changed", json!({ "server": server })),
            DesktopEvent::Host(HostEvent::ModelsChanged) =>
                app.emit("models-changed", json!({ "server": null })),
            DesktopEvent::ExtensionsChanged { server } =>
                app.emit("extensions-changed", json!({ "server": server })),
            DesktopEvent::Host(HostEvent::ExtensionsChanged) =>
                app.emit("extensions-changed", json!({ "server": null })),
            DesktopEvent::MemoryCaptureChanged { server, event } =>
                app.emit("memory-capture-changed", json!({ "server": server, "capture": event.capture, "notify": event.notify })),
            DesktopEvent::Host(HostEvent::MemoryCaptureChanged(event)) =>
                app.emit("memory-capture-changed", json!({ "server": null, "capture": event.capture, "notify": event.notify })),
            DesktopEvent::ExtensionStopped { server, id, folder, problem } =>
                app.emit("extension-stopped", json!({ "server": server, "id": id, "folder": folder, "problem": problem })),
            DesktopEvent::Host(HostEvent::ExtensionStopped { id, folder, problem }) =>
                app.emit("extension-stopped", json!({ "server": null, "id": id, "folder": folder, "problem": problem })),
            DesktopEvent::RemoteRunStatus { run_id, state } =>
                app.emit("remote-run-status", json!({ "runId": run_id, "state": state })),
            DesktopEvent::Host(HostEvent::FilesChanged(payload)) => app.emit("files-changed", payload),
            DesktopEvent::Host(HostEvent::GitChanged(payload)) => app.emit("git-changed", payload),
            DesktopEvent::Host(HostEvent::SettingsChanged(payload)) => app.emit("settings-changed", payload),
            DesktopEvent::Host(HostEvent::RecentFoldersChanged(folders)) => {
                app.emit("recent-folders-changed", folders)
            }
            DesktopEvent::Host(HostEvent::TasksChanged(folder)) => app.emit("tasks-changed", folder),
            DesktopEvent::Host(HostEvent::NixChanged(folder)) => {
                app.emit("nix-changed", json!({ "folderPath": folder }))
            }
            DesktopEvent::Host(HostEvent::SessionsChanged(folder)) => {
                app.emit("sessions-changed", json!({ "folderPath": folder }))
            }
            DesktopEvent::Host(HostEvent::IssuesChanged(folder)) => {
                app.emit("issues-changed", json!({ "folderPath": folder }))
            }
            DesktopEvent::Host(HostEvent::ChatSummariesChanged(payload)) => {
                app.emit("chat-summaries-changed", payload)
            }
            DesktopEvent::Host(HostEvent::FileMoved {
                folder_path,
                old_path,
                new_path,
                replace_destination,
            }) => {
                if replace_destination && windows.release_claims_under(&new_path) > 0 {
                    let _ = app.emit(
                        "file-deleted",
                        json!({ "filePath": new_path.to_string_lossy() }),
                    );
                }
                windows.rename_claims_under(&old_path, &new_path)?;
                app.emit(
                    "file-path-changed",
                    json!({
                        "oldPath": old_path.to_string_lossy(),
                        "newPath": new_path.to_string_lossy(),
                        "folderPath": folder_path.to_string_lossy(),
                    }),
                )
            }
            DesktopEvent::Host(HostEvent::FileDeleted(path)) => {
                windows.release_claims_under(&path);
                app.emit(
                    "file-deleted",
                    json!({ "filePath": path.to_string_lossy() }),
                )
            }
        };
        result.map_err(|error| error.to_string())
    })
}
