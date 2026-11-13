use crate::app_state::AppState;
use axome_core::errors::ApiError;
use axome_protocol::settings::SettingsFileResult;

#[tauri::command]
pub async fn settings_open_global_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SettingsFileResult, ApiError> {
    let result = state.router.settings_create_global_file().await?;
    super::open_file(&app, result, "global settings file")
}

#[tauri::command]
pub async fn axo_open_models_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SettingsFileResult, ApiError> {
    let result = state.host.axo_create_models_file()?;
    super::open_file(&app, result, "models file")
}
