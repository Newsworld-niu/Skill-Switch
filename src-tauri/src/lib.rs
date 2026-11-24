mod backup_sync;
mod commands;
mod domain;
mod git;
mod github_auth;
mod legacy;
mod marketplace;
mod mcp;
mod plugins;
mod registry;
mod repo_sources;
mod secrets;
mod ssh_sync;
mod store;
mod updater;

use commands::{
    apply_environment_restore, apply_install_refresh, apply_project_profile, apply_source_update,
    backup_source_bootstrap, backup_source_connect, backup_source_pull, backup_source_push,
    backup_source_status, backup_sync_retry, backup_sync_state_get, capture_project_changes,
    check_install_updates, check_source_updates, github_token_configured, github_token_set,
    library_export_to_zip, library_import_from_zip, marketplace_load_feed, mcp_check_status,
    mcp_create, mcp_delete, mcp_get, mcp_import_external, mcp_install_global,
    mcp_install_to_project, mcp_list, mcp_list_global_enabled_apps, mcp_uninstall_from_project,
    mcp_uninstall_global, mcp_update, plugin_check_status, plugin_delete, plugin_discard_staged,
    plugin_get, plugin_import_from_dialog, plugin_import_from_folder, plugin_import_from_git,
    plugin_import_from_zip, plugin_import_staged, plugin_install_global, plugin_install_to_project,
    plugin_list, plugin_list_global_enabled_apps, plugin_uninstall_from_project,
    plugin_uninstall_global, plugin_update_config, preview_capture_project_changes,
    preview_environment_restore, preview_project_apply, preview_source_update,
    project_binding_list, project_create, project_delete, project_list, project_profile_list,
    project_remove_cli_folders, project_update, read_external_skill_content,
    read_external_slash_command_content, registry_fetch_content, registry_install, registry_search,
    repo_connect, repo_preflight, repo_pull, repo_push, repo_source_delete,
    repo_source_list_skills, repo_source_sync, repo_status, repo_sync, resource_list,
    scan_external_mcp_servers, scan_external_skills, scan_external_slash_commands,
    scan_global_environment, scan_project_state, settings_get, settings_set, show_in_finder,
    skill_check_symlink_status, skill_create, skill_delete, skill_export_to_zip, skill_get,
    skill_import_from_dialog, skill_import_from_folder, skill_import_from_market,
    skill_import_from_repo_source, skill_import_from_zip, skill_install_global,
    skill_install_to_project, skill_list, skill_list_directory, skill_list_global_enabled_apps,
    skill_open_in_editor, skill_read_file, skill_repair_broken_symlinks, skill_search,
    skill_show_in_finder, skill_source_dir_path, skill_sync_from_source,
    skill_uninstall_from_project, skill_uninstall_global, skill_update,
    slash_command_check_symlink_status, slash_command_create, slash_command_delete,
    slash_command_export_to_zip, slash_command_get, slash_command_import_from_dialog,
    slash_command_import_from_folder, slash_command_import_from_zip, slash_command_install_global,
    slash_command_install_to_project, slash_command_list, slash_command_list_directory,
    slash_command_open_in_editor, slash_command_read_file, slash_command_search,
    slash_command_show_in_finder, slash_command_source_dir_path, slash_command_sync_from_source,
    slash_command_uninstall_from_project, slash_command_uninstall_global, slash_command_update,
    ssh_calibrate_target, ssh_discover_hosts, ssh_sync_all_enabled, ssh_test_connection,
};
use updater::{check_app_update, download_and_install_update, get_current_version};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();

            match store::get_settings(&handle) {
                Ok(settings) => {
                    store::apply_theme_preference(&handle, settings.theme.as_str());
                }
                Err(error) => {
                    println!("[SkillSwitch] Failed to load theme preference: {}", error);
                }
            }

            // On startup: pull the backup source in the background (never block the window)
            backup_sync::spawn_startup_sync(&handle);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            repo_preflight,
            repo_connect,
            repo_status,
            repo_pull,
            repo_push,
            repo_sync,
            backup_source_status,
            backup_source_connect,
            backup_source_pull,
            backup_source_push,
            backup_sync_state_get,
            backup_sync_retry,
            resource_list,
            project_profile_list,
            project_binding_list,
            scan_project_state,
            preview_project_apply,
            apply_project_profile,
            preview_capture_project_changes,
            capture_project_changes,
            apply_install_refresh,
            scan_global_environment,
            preview_environment_restore,
            apply_environment_restore,
            check_source_updates,
            check_install_updates,
            preview_source_update,
            apply_source_update,
            skill_list,
            skill_get,
            skill_create,
            skill_update,
            skill_delete,
            skill_search,
            skill_install_to_project,
            skill_uninstall_from_project,
            skill_install_global,
            skill_uninstall_global,
            skill_check_symlink_status,
            skill_list_global_enabled_apps,
            skill_repair_broken_symlinks,
            skill_import_from_dialog,
            skill_import_from_folder,
            skill_import_from_zip,
            skill_export_to_zip,
            library_export_to_zip,
            library_import_from_zip,
            skill_list_directory,
            skill_read_file,
            project_remove_cli_folders,
            project_list,
            project_create,
            project_update,
            project_delete,
            show_in_finder,
            skill_show_in_finder,
            skill_open_in_editor,
            slash_command_open_in_editor,
            scan_external_skills,
            read_external_skill_content,
            settings_get,
            settings_set,
            repo_source_sync,
            repo_source_delete,
            repo_source_list_skills,
            marketplace_load_feed,
            skill_import_from_market,
            skill_source_dir_path,
            skill_sync_from_source,
            slash_command_list,
            slash_command_get,
            slash_command_create,
            slash_command_update,
            slash_command_delete,
            slash_command_search,
            slash_command_install_to_project,
            slash_command_uninstall_from_project,
            slash_command_install_global,
            slash_command_uninstall_global,
            slash_command_check_symlink_status,
            slash_command_import_from_dialog,
            slash_command_import_from_folder,
            slash_command_import_from_zip,
            slash_command_export_to_zip,
            slash_command_list_directory,
            slash_command_read_file,
            slash_command_show_in_finder,
            slash_command_source_dir_path,
            slash_command_sync_from_source,
            scan_external_slash_commands,
            read_external_slash_command_content,
            registry_search,
            registry_fetch_content,
            github_token_configured,
            github_token_set,
            registry_install,
            backup_source_bootstrap,
            skill_import_from_repo_source,
            mcp_list,
            mcp_get,
            mcp_create,
            mcp_update,
            mcp_delete,
            mcp_install_global,
            mcp_uninstall_global,
            mcp_install_to_project,
            mcp_uninstall_from_project,
            mcp_check_status,
            mcp_list_global_enabled_apps,
            scan_external_mcp_servers,
            mcp_import_external,
            plugin_list,
            plugin_get,
            plugin_import_from_folder,
            plugin_import_from_dialog,
            plugin_import_from_zip,
            plugin_import_from_git,
            plugin_import_staged,
            plugin_discard_staged,
            plugin_update_config,
            plugin_delete,
            plugin_install_global,
            plugin_uninstall_global,
            plugin_install_to_project,
            plugin_uninstall_from_project,
            plugin_check_status,
            plugin_list_global_enabled_apps,
            ssh_discover_hosts,
            ssh_test_connection,
            ssh_calibrate_target,
            ssh_sync_all_enabled,
            check_app_update,
            download_and_install_update,
            get_current_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
