use crate::app_state::AppState;
use axome_core::errors::ApiError;
use axome_protocol::settings::SettingsFileResult;

#[tauri::command]
pub async fn shortcuts_open_global_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SettingsFileResult, ApiError> {
    let result = state.router.shortcuts_create_global_file().await?;
    super::open_file(&app, result, "shortcuts file")
}
