pub mod app;
pub mod chat;
pub mod dnd;
pub mod files;
pub mod folders;
pub mod git;
pub mod lsp;
pub mod remote;
pub mod runs;
pub mod sessions;
pub mod settings;
pub mod shells;
pub mod shortcuts;
pub mod tasks;
pub mod window;
pub mod workbench;

use crate::app_state::AppState;
use axome_core::errors::ApiError;
use axome_protocol::settings::SettingsFileResult;
use tauri_plugin_opener::OpenerExt;

pub(super) fn open_file(
    app: &tauri::AppHandle,
    result: SettingsFileResult,
    what: &str,
) -> Result<SettingsFileResult, ApiError> {
    app.opener()
        .open_path(result.path.clone(), None::<&str>)
        .map_err(|error| {
            ApiError::Internal(format!("Failed to open {what} {}: {error}", result.path))
        })?;
    Ok(result)
}

/// Host-global operations run on the server that owns the given folder URI, else on the desktop.
fn target_server(folder_path: Option<&str>) -> Result<Option<String>, ApiError> {
    use crate::router::Target;
    match folder_path {
        Some(path) => match Target::parse(path)? {
            Target::Remote { server, .. } => Ok(Some(server.to_owned())),
            Target::Local => Ok(None),
        },
        None => Ok(None),
    }
}

macro_rules! forward_command {
    (#[route($route:ident)] FolderClaim => $($row:tt)*) => {};
    (#[route($route:ident)] FolderRelease => $($row:tt)*) => {};
    (#[route($route:ident)] FilesWatchDirs => $($row:tt)*) => {};
    (#[route($route:ident)] GitWatch => $($row:tt)*) => {};
    (#[route($route:ident)] GitWorktree => $($row:tt)*) => {};
    (#[route($route:ident)] SessionStart => $($row:tt)*) => {};
    (#[route($route:ident)] ShellStart => $($row:tt)*) => {};
    (#[route($route:ident)] TasksStart => $($row:tt)*) => {};
    (#[route($route:ident)] LspStart => $($row:tt)*) => {};
    (#[route($route:ident)] AppRuntimeInfo => $($row:tt)*) => {};
    (#[route($route:ident)] RunStatus => $($row:tt)*) => {};
    (#[route(connection)] $($row:tt)*) => {};
    (#[route(server)] $variant:ident => $method:ident(
        $($argument:ident: $argument_type:ty),* $(,)?
    ) -> $return_type:ty;) => {
        #[tauri::command]
        async fn $method(
            $($argument: $argument_type,)*
            folder_path: Option<String>,
            state: tauri::State<'_, AppState>,
        ) -> Result<$return_type, ApiError> {
            state
                .router
                .$method(target_server(folder_path.as_deref())?, $($argument),*)
                .await
        }
    };
    (#[route($route:ident)] $variant:ident => $method:ident(
        $($argument:ident: $argument_type:ty),* $(,)?
    ) -> $return_type:ty;) => {
        #[tauri::command]
        async fn $method(
            $($argument: $argument_type,)*
            state: tauri::State<'_, AppState>,
        ) -> Result<$return_type, ApiError> {
            state.router.$method($($argument),*).await
        }
    };
}

macro_rules! forward_commands {
    (
        $(
            #[route($route:ident)]
            $variant:ident => $method:ident(
                $($argument:ident: $argument_type:ty),* $(,)?
            ) -> $return_type:ty;
        )*
    ) => {
        $(
            forward_command! {
                #[route($route)]
                $variant => $method($($argument: $argument_type),*) -> $return_type;
            }
        )*
    };
}

axome_protocol::axome_rpc_ops!(forward_commands);

pub(crate) fn invoke_handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        crate::deep_links::deep_link_take,
        remote::pair_remote,
        remote::repair_remote,
        remote::remote_client_fingerprint,
        remote::remote_status,
        workbench::workbench_list,
        workbench::workbench_close,
        workbench::workbench_attach,
        workbench::workbench_detach,
        workbench::workbench_transfer,
        workbench::workbench_save,
        remote::remote_runs_release,
        remote::rename_remote,
        remote::remove_remote,
        files::file_read_stream,
        files::file_read_stream_cancel,
        // Activity map commands
        activity_map_get,
        activity_map_refresh,
        session_index_get,
        session_index_refresh,
        // Chat commands
        chat::chat_attach,
        chat::chat_detach,
        chat_models,
        chat_create,
        chat_prompt,
        chat_steer,
        chat_follow_up,
        chat_withdraw,
        chat_send_now,
        chat_abort,
        chat_close,
        chat_resume,
        chat_answer,
        chat_dismiss,
        chat_run_command,
        chat_resolve_link,
        chat_navigate,
        chat_select_branch,
        chat_fork,
        chat_background,
        chat_kill_job,
        chat_resume_wakes,
        chat_watch_job,
        chat_live_summaries,
        chat_context_details,
        chat_local_files,
        chat_set_agent,
        chat_set_mode,
        chat_keyword_matches,
        chat_accept_result,
        chat_revise_result,
        chat_accept_plan,
        chat_squash_changes,
        chat_turn_diff_file,
        // App commands
        app::clipboard_copy_files,
        app::clipboard_read_files,
        app_state_get,
        app_state_put,
        app_state_delete,
        app::app_runtime_info,
        // Window commands
        window::window_create,
        window::window_ready,
        window::window_open_target_finished,
        window::window_close,
        window::window_claim_file,
        window::window_release_file,
        window::window_transfer_initiate,
        window::window_transfer_source_exported,
        window::window_transfer_target_staged,
        window::window_transfer_abort,
        window::window_group_handoff,
        window::window_group_exported,
        window::window_group_staged,
        window::window_group_abort,
        window::pty_pause,
        window::pty_attach,
        // Extension commands
        extensions_get_catalog,
        extensions_get_grammars,
        extensions_reload,
        // Environment commands
        environment_reload,
        // Config schema commands (drives Monaco autocomplete for .axome/*.json)
        config_schemas_list,
        // Drag and drop commands
        dnd::dnd_save_dropped_bytes,
        // Issue commands
        issues_list,
        issues_ready,
        issues_search,
        issues_get,
        issues_create,
        issues_update,
        issues_delete,
        issue_epics_create,
        issue_epics_list,
        issue_epics_get,
        issue_epics_update,
        issue_epics_delete,
        issue_comments_add,
        issue_comments_list,
        issue_comments_update,
        issue_comments_delete,
        issue_dependencies_add,
        issue_dependencies_remove,
        issue_dependencies_list,
        issue_current_git_user,
        issue_config_list,
        // Folder commands
        folders::folder_select_directory,
        folder_resolve,
        folder_list_entries,
        folder_path_root,
        folder_home,
        folder_working_directory,
        folders::folder_open_in_terminal,
        recent_folders_list,
        recent_folders_touch,
        recent_folders_remove,
        folders::folder_claim,
        folders::folder_release,
        // Harness commands
        harness_list,
        // Settings commands
        settings_get,
        settings_get_effective,
        settings_get_global_layer,
        settings_patch_global_section,
        settings_create_global_file,
        settings::settings_open_global_file,
        settings::axo_open_models_file,
        axo_providers,
        axo_models,
        axo_provider_set_key,
        axo_provider_remove,
        axo_models_refresh,
        axo_login_start,
        axo_login_submit,
        axo_login_status,
        axo_login_cancel,
        axo_keywords,
        axo_web_tools_status,
        memory_status,
        memory_retry_download,
        memory_ensure_bank,
        axo_search_key_set,
        axo_search_key_remove,
        axo_docs_refresh,
        axo_docs_remove,
        settings_set_axo,
        settings_open_folder_file,
        settings_set_window,
        settings_set_typography,
        settings_set_nix,
        settings_set_memory,
        settings_set_formatting,
        settings_set_harness_config,
        settings_set_shell_config,
        // Shortcut commands
        shortcuts_get_global,
        shortcuts_set_global,
        shortcuts_create_global_file,
        shortcuts::shortcuts_open_global_file,
        // Formatter commands
        formatting_format,
        formatting_list_formatters,
        // Task commands (folder-scoped .axome/tasks.jsonc)
        tasks_list,
        tasks::tasks_start,
        tasks_stop,
        // Nix environment commands
        nix_detect,
        nix_select,
        nix_evaluate,
        nix_clear,
        nix_lint,
        harness_list_for_folder,
        // File commands
        app::file_reveal,
        file_read,
        file_stat,
        file_write,
        file_create_dir,
        file_rename,
        file_delete,
        file_paste,
        file_paste_collisions,
        files_read_dir,
        files_list_paths,
        files::files_watch_dirs,
        // Git commands
        git_get_summary,
        git_get_brief,
        git::git_watch,
        git_get_graph,
        git_get_branch_commits,
        git_get_commit_detail,
        diff_get_file,
        diff_get_working_index,
        // Git write commands
        git_paths,
        git_get_full_patch,
        git_get_path_patch,
        git_get_quick_diff_data,
        git_stage_file_content,
        git_commit,
        git_undo_last_commit,
        git_remote,
        git_stash,
        git_stash_count,
        git_stash_list,
        git_show_file,
        git_init,
        git_clone_in_place,
        // Git branch commands
        git_list_branches,
        git_list_remotes,
        git_list_tags,
        git_tag,
        git_remote_config,
        git_history,
        git_diff_range,
        git_merged_branches,
        git_branch,
        git_sequencer,
        git_conflicts,
        git_worktree_list,
        git_worktree_default_path,
        git::git_worktree,
        // LSP commands
        lsp_list_servers,
        lsp_set_server_config,
        lsp::lsp_start,
        lsp::lsp_send,
        lsp_stop,
        // Session commands (process-only; resume identity lives in the tab)
        sessions::session_start,
        session_stop,
        omp_resolve_uri,
        // Shell commands
        shells::shell_start,
        shell_stop,
        shell_list,
        shell_list_for_folder,
        // Run commands
        runs::run_write,
        runs::run_resize,
    ]
}
