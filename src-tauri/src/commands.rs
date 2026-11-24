use std::path::Path;

use tauri::Manager;

use crate::domain::{
    CreateProjectInput, CreateSkillInput, CreateSlashCommandInput, ImportMarketSkillInput,
    InstallRecordIdInput, InstallSkillGlobalInput, InstallSkillToProjectInput,
    InstallSlashCommandGlobalInput, InstallSlashCommandToProjectInput, MarketplaceFeedInput,
    ProjectPathInput, ProjectPreviewInput, RemoveProjectCliInput, RepoConnectInput,
    RepoPreflightInput, RepoPreflightResult, ResourceIdInput, ResourceListFilter,
    SkillDirectoryInput, SkillFileInput, SlashCommandDirectoryInput, SlashCommandFileInput,
    ThirdPartyRepo, UpdateProjectInput, UpdateSkillInput, UpdateSlashCommandInput,
};
use crate::{git, store};

async fn run_blocking_command<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| error.to_string())?
}

/// Like `run_blocking_command`, but holds the global mutation lock while the task runs.
async fn run_locked_command<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    run_blocking_command(move || store::with_mutation_lock(task)).await
}

#[tauri::command]
pub fn repo_preflight(
    app: tauri::AppHandle,
    input: RepoPreflightInput,
) -> Result<RepoPreflightResult, String> {
    let path = input
        .local_path
        .clone()
        .or(input.path.clone())
        .ok_or_else(|| "repo path is required".to_string())?;
    let normalized = store::normalize_user_path(&path)?;
    let path_exists = normalized.exists();
    let path_is_directory = path_exists && normalized.is_dir();
    let manifest_exists = path_is_directory && store::repo_manifest_path(&normalized).exists();
    let legacy_store_exists = store::legacy_store_path(&app)?.exists();
    let codex_home = app
        .path()
        .home_dir()
        .map_err(|error| error.to_string())?
        .join(".codex")
        .to_string_lossy()
        .into_owned();

    Ok(RepoPreflightResult {
        normalized_path: normalized.to_string_lossy().into_owned(),
        path_exists,
        path_is_directory,
        git_available: git::git_available(),
        is_git_repo: path_is_directory && git::is_git_repo(&normalized),
        manifest_exists,
        legacy_store_exists,
        codex_home: Some(codex_home),
    })
}

#[tauri::command]
pub async fn repo_connect(
    app: tauri::AppHandle,
    input: RepoConnectInput,
) -> Result<crate::domain::RepoStatus, String> {
    run_locked_command(move || {
        let path = input
            .local_path
            .clone()
            .or(input.path.clone())
            .ok_or_else(|| "repo path is required".to_string())?;
        let initialize_if_missing = input
            .initialize_if_missing
            .unwrap_or_else(|| input.mode.as_deref() == Some("clone"));

        store::ensure_repo_connection(
            &app,
            &path,
            input.remote_url.as_deref(),
            input.branch.as_deref(),
            initialize_if_missing,
        )
    })
    .await
}

#[tauri::command]
pub fn repo_status(app: tauri::AppHandle) -> Result<crate::domain::RepoStatus, String> {
    store::build_repo_status(&app)
}

#[tauri::command]
pub async fn repo_pull(app: tauri::AppHandle) -> Result<crate::domain::RepoStatus, String> {
    run_locked_command(move || store::repo_pull(&app)).await
}

#[tauri::command]
pub async fn repo_push(app: tauri::AppHandle) -> Result<crate::domain::RepoStatus, String> {
    run_locked_command(move || store::repo_push(&app)).await
}

#[tauri::command]
pub async fn repo_sync(app: tauri::AppHandle) -> Result<crate::domain::RepoStatus, String> {
    run_locked_command(move || store::repo_sync(&app)).await
}

#[tauri::command]
pub fn backup_source_status(
    app: tauri::AppHandle,
) -> Result<crate::domain::BackupSourceStatus, String> {
    store::backup_source_status(&app)
}

#[tauri::command]
pub fn backup_sync_state_get() -> crate::domain::BackupSyncState {
    crate::backup_sync::current_state()
}

#[tauri::command]
pub fn backup_sync_retry(app: tauri::AppHandle) {
    crate::backup_sync::schedule_push(&app);
}

#[tauri::command]
pub async fn backup_source_connect(
    app: tauri::AppHandle,
) -> Result<crate::domain::BackupSourceStatus, String> {
    run_locked_command(move || store::backup_source_connect(&app)).await
}

#[tauri::command]
pub async fn backup_source_pull(
    app: tauri::AppHandle,
) -> Result<crate::domain::BackupSourceStatus, String> {
    run_locked_command(move || store::backup_source_pull(&app)).await
}

#[tauri::command]
pub async fn backup_source_push(
    app: tauri::AppHandle,
) -> Result<crate::domain::BackupSourceStatus, String> {
    run_locked_command(move || store::backup_source_push(&app)).await
}

#[tauri::command]
pub fn resource_list(
    app: tauri::AppHandle,
    filter: Option<ResourceListFilter>,
) -> Result<Vec<crate::domain::Resource>, String> {
    store::list_resources(&app, filter)
}

#[tauri::command]
pub fn project_profile_list(
    app: tauri::AppHandle,
) -> Result<Vec<crate::domain::ProjectProfile>, String> {
    store::list_project_profiles(&app)
}

#[tauri::command]
pub fn project_binding_list(
    app: tauri::AppHandle,
) -> Result<Vec<crate::domain::LocalProjectBinding>, String> {
    store::list_project_bindings(&app)
}

#[tauri::command]
pub fn scan_project_state(
    app: tauri::AppHandle,
    input: ProjectPathInput,
) -> Result<crate::domain::ProjectScanResult, String> {
    store::scan_project_state(&app, &input.path)
}

#[tauri::command]
pub fn preview_project_apply(
    app: tauri::AppHandle,
    input: ProjectPreviewInput,
) -> Result<crate::domain::PreviewPlan, String> {
    store::preview_project_apply(&app, &input)
}

#[tauri::command]
pub async fn apply_project_profile(
    app: tauri::AppHandle,
    input: ProjectPreviewInput,
) -> Result<(), String> {
    run_locked_command(move || store::apply_project_profile(&app, &input)).await
}

#[tauri::command]
pub fn preview_capture_project_changes(
    app: tauri::AppHandle,
    input: ProjectPreviewInput,
) -> Result<crate::domain::PreviewPlan, String> {
    if !Path::new(&input.path).exists() {
        return Err(format!("project path does not exist: {}", input.path));
    }

    store::preview_capture_project_changes(&app, &input)
}

#[tauri::command]
pub async fn capture_project_changes(
    app: tauri::AppHandle,
    input: ProjectPreviewInput,
) -> Result<(), String> {
    run_locked_command(move || store::capture_project_changes(&app, &input)).await
}

#[tauri::command]
pub async fn apply_install_refresh(
    app: tauri::AppHandle,
    input: InstallRecordIdInput,
) -> Result<(), String> {
    run_locked_command(move || store::apply_install_refresh(&app, &input.record_id)).await
}

#[tauri::command]
pub fn scan_global_environment(
    app: tauri::AppHandle,
) -> Result<crate::domain::RecoveryScanResult, String> {
    store::scan_global_environment(&app)
}

#[tauri::command]
pub fn preview_environment_restore(
    app: tauri::AppHandle,
) -> Result<crate::domain::PreviewPlan, String> {
    store::preview_environment_restore(&app)
}

#[tauri::command]
pub async fn apply_environment_restore(app: tauri::AppHandle) -> Result<(), String> {
    run_locked_command(move || store::apply_environment_restore(&app)).await
}

#[tauri::command]
pub fn check_source_updates(
    app: tauri::AppHandle,
) -> Result<Vec<crate::domain::UpdateItem>, String> {
    store::check_source_updates(&app)
}

#[tauri::command]
pub fn check_install_updates(
    app: tauri::AppHandle,
) -> Result<Vec<crate::domain::UpdateItem>, String> {
    store::check_install_updates(&app)
}

#[tauri::command]
pub fn preview_source_update(
    app: tauri::AppHandle,
    input: ResourceIdInput,
) -> Result<crate::domain::PreviewPlan, String> {
    store::preview_source_update(&app, &input.resource_id)
}

#[tauri::command]
pub async fn apply_source_update(
    app: tauri::AppHandle,
    input: ResourceIdInput,
) -> Result<crate::domain::UpdateItem, String> {
    run_locked_command(move || store::apply_source_update(&app, &input.resource_id)).await
}

#[tauri::command]
pub fn skill_list(app: tauri::AppHandle) -> Result<Vec<crate::domain::LegacySkillDto>, String> {
    store::list_legacy_skills(&app)
}

#[tauri::command]
pub fn skill_get(
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<crate::domain::LegacySkillDto>, String> {
    store::get_legacy_skill(&app, &id)
}

#[tauri::command]
pub async fn skill_create(
    app: tauri::AppHandle,
    input: CreateSkillInput,
) -> Result<crate::domain::CreateLegacySkillResult, String> {
    run_locked_command(move || store::create_legacy_skill(&app, &input)).await
}

#[tauri::command]
pub async fn skill_update(
    app: tauri::AppHandle,
    input: UpdateSkillInput,
) -> Result<crate::domain::LegacySkillDto, String> {
    run_locked_command(move || store::update_legacy_skill(&app, &input)).await
}

#[tauri::command]
pub async fn skill_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run_locked_command(move || store::delete_legacy_skill(&app, &id)).await
}

#[tauri::command]
pub fn skill_search(
    app: tauri::AppHandle,
    query: String,
) -> Result<Vec<crate::domain::LegacySkillDto>, String> {
    store::search_legacy_skills(&app, &query)
}

#[tauri::command]
pub fn project_list(app: tauri::AppHandle) -> Result<Vec<crate::domain::LegacyProjectDto>, String> {
    store::list_legacy_projects(&app)
}

#[tauri::command]
pub async fn project_create(
    app: tauri::AppHandle,
    input: CreateProjectInput,
) -> Result<crate::domain::LegacyProjectDto, String> {
    run_locked_command(move || store::create_legacy_project(&app, &input)).await
}

#[tauri::command]
pub async fn project_update(
    app: tauri::AppHandle,
    input: UpdateProjectInput,
) -> Result<crate::domain::LegacyProjectDto, String> {
    run_locked_command(move || store::update_legacy_project(&app, &input)).await
}

#[tauri::command]
pub async fn project_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run_locked_command(move || store::delete_legacy_project(&app, &id)).await
}

#[tauri::command]
pub fn show_in_finder(path: String) -> Result<(), String> {
    let target_path = Path::new(&path);
    if !target_path.exists() {
        return Err(format!("path does not exist: {}", path));
    }

    open_in_system_file_manager(target_path)
}

#[tauri::command]
pub fn skill_show_in_finder(app: tauri::AppHandle, skill_id: String) -> Result<(), String> {
    let path = store::skill_source_dir_by_id(&app, &skill_id)?;
    if !path.exists() {
        return Err("skill 目录不存在".to_string());
    }
    open_in_system_file_manager(&path)
}

#[tauri::command]
pub fn skill_open_in_editor(app: tauri::AppHandle, skill_id: String) -> Result<(), String> {
    let skill_dir = store::skill_source_dir_by_id(&app, &skill_id)?;
    let skill_md_path = skill_dir.join("SKILL.md");
    if !skill_md_path.exists() {
        return Err("SKILL.md 文件不存在".to_string());
    }
    open_file_in_editor(&app, &skill_md_path)
}

#[tauri::command]
pub fn skill_source_dir_path(app: tauri::AppHandle, skill_id: String) -> Result<String, String> {
    let skill_dir = store::skill_source_dir_by_id(&app, &skill_id)?;
    if !skill_dir.join("SKILL.md").exists() {
        return Err("SKILL.md 文件不存在".to_string());
    }

    Ok(skill_dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn skill_sync_from_source(
    app: tauri::AppHandle,
    skill_id: String,
) -> Result<crate::domain::LegacySkillDto, String> {
    run_locked_command(move || store::sync_legacy_skill_from_source(&app, &skill_id)).await
}

#[tauri::command]
pub fn slash_command_list(
    app: tauri::AppHandle,
) -> Result<Vec<crate::domain::SlashCommandDto>, String> {
    store::list_slash_commands(&app)
}

#[tauri::command]
pub fn slash_command_get(
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<crate::domain::SlashCommandDto>, String> {
    store::get_slash_command(&app, &id)
}

#[tauri::command]
pub async fn slash_command_create(
    app: tauri::AppHandle,
    input: CreateSlashCommandInput,
) -> Result<crate::domain::CreateSlashCommandResult, String> {
    run_locked_command(move || store::create_slash_command(&app, &input)).await
}

#[tauri::command]
pub async fn slash_command_update(
    app: tauri::AppHandle,
    input: UpdateSlashCommandInput,
) -> Result<crate::domain::SlashCommandDto, String> {
    run_locked_command(move || store::update_slash_command(&app, &input)).await
}

#[tauri::command]
pub async fn slash_command_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run_locked_command(move || store::delete_slash_command(&app, &id)).await
}

#[tauri::command]
pub fn slash_command_search(
    app: tauri::AppHandle,
    query: String,
) -> Result<Vec<crate::domain::SlashCommandDto>, String> {
    store::search_slash_commands(&app, &query)
}

#[tauri::command]
pub fn slash_command_show_in_finder(
    app: tauri::AppHandle,
    command_id: String,
) -> Result<(), String> {
    let path = store::slash_command_source_dir_by_id(&app, &command_id)?;
    if !path.exists() {
        return Err("Slash Command 目录不存在".to_string());
    }
    open_in_system_file_manager(&path)
}

#[tauri::command]
pub fn slash_command_open_in_editor(
    app: tauri::AppHandle,
    command_id: String,
) -> Result<(), String> {
    let command_dir = store::slash_command_source_dir_by_id(&app, &command_id)?;
    let command_md_path = command_dir.join("COMMAND.md");
    if !command_md_path.exists() {
        return Err("COMMAND.md 文件不存在".to_string());
    }
    open_file_in_editor(&app, &command_md_path)
}

/// Opens `file` in the editor configured in settings, or the system default app.
fn open_file_in_editor(app: &tauri::AppHandle, file: &Path) -> Result<(), String> {
    let editor = store::get_settings(app)?
        .external_editor
        .map(|editor| editor.trim().to_string())
        .filter(|editor| !editor.is_empty());

    let Some(editor) = editor else {
        use tauri_plugin_opener::OpenerExt;
        return app
            .opener()
            .open_path(file.to_string_lossy(), None::<&str>)
            .map_err(|e| format!("无法用默认程序打开文件：{}", e));
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        // Accepts an app name ("Typora", "Visual Studio Code") like `open -a`.
        let mut command = std::process::Command::new("open");
        command.arg("-a").arg(&editor);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        // `start` resolves .cmd/.bat shims such as `code`.
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]).arg(&editor);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new(&editor);

    command
        .arg(file)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法启动编辑器「{}」：{}", editor, e))
}

#[tauri::command]
pub fn slash_command_source_dir_path(
    app: tauri::AppHandle,
    command_id: String,
) -> Result<String, String> {
    let command_dir = store::slash_command_source_dir_by_id(&app, &command_id)?;
    if !command_dir.join("COMMAND.md").exists() {
        return Err("COMMAND.md 文件不存在".to_string());
    }

    Ok(command_dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn slash_command_sync_from_source(
    app: tauri::AppHandle,
    command_id: String,
) -> Result<crate::domain::SlashCommandDto, String> {
    run_locked_command(move || store::sync_slash_command_from_source(&app, &command_id)).await
}

fn open_in_system_file_manager(target_path: &Path) -> Result<(), String> {
    // Use system `open -R` command which is more stable than showfile crate
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(target_path)
            .spawn()
            .map_err(|e| format!("failed to open finder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(target_path)
            .spawn()
            .map_err(|e| format!("failed to open explorer: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = target_path.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("failed to open file manager: {}", e))?;
        }
    }

    Ok(())
}

// ─── Settings commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn settings_get(app: tauri::AppHandle) -> Result<crate::domain::AppSettings, String> {
    store::get_settings(&app)
}

#[tauri::command]
pub async fn settings_set(
    app: tauri::AppHandle,
    settings: crate::domain::AppSettings,
) -> Result<(), String> {
    run_locked_command(move || {
        store::set_settings(&app, &settings)?;
        store::apply_theme_preference(&app, settings.theme.as_str());
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn repo_source_sync(
    app: tauri::AppHandle,
    repo: ThirdPartyRepo,
) -> Result<crate::domain::ThirdPartyRepo, String> {
    run_locked_command(move || crate::repo_sources::sync_repo_source(&app, &repo)).await
}

#[tauri::command]
pub async fn repo_source_delete(app: tauri::AppHandle, repo: ThirdPartyRepo) -> Result<(), String> {
    run_locked_command(move || crate::repo_sources::delete_repo_source(&app, &repo)).await
}

#[tauri::command]
pub async fn repo_source_list_skills(
    app: tauri::AppHandle,
    repo: ThirdPartyRepo,
) -> Result<Vec<crate::domain::RemoteSkill>, String> {
    run_blocking_command(move || crate::repo_sources::list_repo_source_skills(&app, &repo)).await
}

// ─── Project skill install commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn skill_install_to_project(
    app: tauri::AppHandle,
    input: InstallSkillToProjectInput,
) -> Result<crate::domain::InstallSkillToProjectResult, String> {
    run_locked_command(move || store::install_skill_to_project(&app, &input)).await
}

#[tauri::command]
pub async fn skill_uninstall_from_project(
    app: tauri::AppHandle,
    input: InstallSkillToProjectInput,
) -> Result<crate::domain::InstallSkillToProjectResult, String> {
    run_locked_command(move || store::uninstall_skill_from_project(&app, &input)).await
}

#[tauri::command]
pub async fn slash_command_install_to_project(
    app: tauri::AppHandle,
    input: InstallSlashCommandToProjectInput,
) -> Result<crate::domain::InstallSlashCommandToProjectResult, String> {
    run_locked_command(move || store::install_slash_command_to_project(&app, &input)).await
}

#[tauri::command]
pub async fn slash_command_uninstall_from_project(
    app: tauri::AppHandle,
    input: InstallSlashCommandToProjectInput,
) -> Result<crate::domain::InstallSlashCommandToProjectResult, String> {
    run_locked_command(move || store::uninstall_slash_command_from_project(&app, &input)).await
}

#[tauri::command]
pub async fn project_remove_cli_folders(
    app: tauri::AppHandle,
    input: RemoveProjectCliInput,
) -> Result<crate::domain::RemoveProjectCliResult, String> {
    run_locked_command(move || store::remove_project_cli_folders(&app, &input)).await
}

// ─── Global skill install commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn skill_install_global(
    app: tauri::AppHandle,
    input: InstallSkillGlobalInput,
) -> Result<crate::domain::InstallSkillGlobalResult, String> {
    run_locked_command(move || store::install_skill_global(&app, &input)).await
}

#[tauri::command]
pub async fn skill_uninstall_global(
    app: tauri::AppHandle,
    input: InstallSkillGlobalInput,
) -> Result<crate::domain::InstallSkillGlobalResult, String> {
    run_locked_command(move || store::uninstall_skill_global(&app, &input)).await
}

#[tauri::command]
pub async fn slash_command_install_global(
    app: tauri::AppHandle,
    input: InstallSlashCommandGlobalInput,
) -> Result<crate::domain::InstallSlashCommandGlobalResult, String> {
    run_locked_command(move || store::install_slash_command_global(&app, &input)).await
}

#[tauri::command]
pub async fn slash_command_uninstall_global(
    app: tauri::AppHandle,
    input: InstallSlashCommandGlobalInput,
) -> Result<crate::domain::InstallSlashCommandGlobalResult, String> {
    run_locked_command(move || store::uninstall_slash_command_global(&app, &input)).await
}

// ─── Symlink status commands ───────────────────────────────────────────────────

#[tauri::command]
pub fn skill_check_symlink_status(
    app: tauri::AppHandle,
    input: crate::domain::CheckSymlinkStatusInput,
) -> Result<crate::domain::CheckSymlinkStatusResult, String> {
    store::check_symlink_status(&app, &input)
}

#[tauri::command]
pub fn skill_list_global_enabled_apps(
    app: tauri::AppHandle,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    store::list_all_skills_global_enabled_apps(&app)
}

#[tauri::command]
pub fn slash_command_check_symlink_status(
    app: tauri::AppHandle,
    input: crate::domain::CheckSlashCommandSymlinkInput,
) -> Result<crate::domain::CheckSlashCommandSymlinkResult, String> {
    store::check_slash_command_symlink_status(&app, &input)
}

#[tauri::command]
pub async fn skill_repair_broken_symlinks(
    app: tauri::AppHandle,
    input: crate::domain::CheckSymlinkStatusInput,
) -> Result<crate::domain::RepairBrokenSymlinksResult, String> {
    run_locked_command(move || store::repair_broken_symlinks(&app, &input)).await
}

// ─── Import skill commands ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn skill_import_from_folder(
    app: tauri::AppHandle,
    folder_path: String,
) -> Result<crate::domain::LegacySkillDto, String> {
    run_locked_command(move || {
        let path = Path::new(&folder_path);
        store::import_skill_from_folder(&app, path)
    })
    .await
}

#[tauri::command]
pub async fn skill_import_from_dialog(
    app: tauri::AppHandle,
) -> Result<Option<crate::domain::LegacySkillDto>, String> {
    run_blocking_command(move || store::import_skill_from_dialog(&app)).await
}

#[tauri::command]
pub async fn skill_import_from_zip(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<crate::domain::LegacySkillDto, String> {
    run_locked_command(move || {
        let path = Path::new(&zip_path);
        store::import_skill_from_zip(&app, path)
    })
    .await
}

#[tauri::command]
pub async fn slash_command_import_from_folder(
    app: tauri::AppHandle,
    folder_path: String,
) -> Result<crate::domain::SlashCommandDto, String> {
    run_locked_command(move || {
        let path = Path::new(&folder_path);
        store::import_slash_command_from_folder(&app, path)
    })
    .await
}

#[tauri::command]
pub async fn slash_command_import_from_dialog(
    app: tauri::AppHandle,
) -> Result<Option<crate::domain::SlashCommandDto>, String> {
    run_blocking_command(move || store::import_slash_command_from_dialog(&app)).await
}

#[tauri::command]
pub async fn slash_command_import_from_zip(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<crate::domain::SlashCommandDto, String> {
    run_locked_command(move || {
        let path = Path::new(&zip_path);
        store::import_slash_command_from_zip(&app, path)
    })
    .await
}

#[tauri::command]
pub fn skill_export_to_zip(
    app: tauri::AppHandle,
    skill_id: String,
    output_path: String,
) -> Result<String, String> {
    let path = Path::new(&output_path);
    store::export_skill_to_zip(&app, &skill_id, path)
}

#[tauri::command]
pub async fn library_export_to_zip(
    app: tauri::AppHandle,
    output_path: String,
) -> Result<crate::domain::LibraryExportResult, String> {
    run_locked_command(move || store::export_library_to_zip(&app, Path::new(&output_path))).await
}

#[tauri::command]
pub async fn library_import_from_zip(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<crate::domain::LibraryImportResult, String> {
    run_locked_command(move || store::import_library_from_zip(&app, Path::new(&zip_path))).await
}

#[tauri::command]
pub fn slash_command_export_to_zip(
    app: tauri::AppHandle,
    command_id: String,
    output_path: String,
) -> Result<String, String> {
    let path = Path::new(&output_path);
    store::export_slash_command_to_zip(&app, &command_id, path)
}

// ─── Skill directory browsing commands ─────────────────────────────────────────

#[tauri::command]
pub fn skill_list_directory(
    app: tauri::AppHandle,
    input: SkillDirectoryInput,
) -> Result<crate::domain::SkillDirectoryListing, String> {
    store::list_skill_directory(&app, &input)
}

#[tauri::command]
pub fn skill_read_file(
    app: tauri::AppHandle,
    input: SkillFileInput,
) -> Result<crate::domain::SkillFileContent, String> {
    store::read_skill_file(&app, &input)
}

#[tauri::command]
pub fn slash_command_list_directory(
    app: tauri::AppHandle,
    input: SlashCommandDirectoryInput,
) -> Result<crate::domain::SlashCommandDirectoryListing, String> {
    store::list_slash_command_directory(&app, &input)
}

#[tauri::command]
pub fn slash_command_read_file(
    app: tauri::AppHandle,
    input: SlashCommandFileInput,
) -> Result<crate::domain::SlashCommandFileContent, String> {
    store::read_slash_command_file(&app, &input)
}

// ─── External app skills scanning commands ─────────────────────────────────────

#[tauri::command]
pub fn scan_external_skills(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<Vec<crate::domain::ExternalSkillDto>, String> {
    store::scan_external_app_skills(&app, &app_id)
}

#[tauri::command]
pub fn read_external_skill_content(path: String) -> Result<String, String> {
    store::read_external_skill_content(&path)
}

#[tauri::command]
pub fn scan_external_slash_commands(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<Vec<crate::domain::ExternalSlashCommandDto>, String> {
    store::scan_external_app_slash_commands(&app, &app_id)
}

#[tauri::command]
pub fn read_external_slash_command_content(path: String) -> Result<String, String> {
    store::read_external_slash_command_content(&path)
}

// ─── Marketplace commands ─────────────────────────────────────────────────────

#[tauri::command]
pub fn marketplace_load_feed(
    app: tauri::AppHandle,
    input: MarketplaceFeedInput,
) -> Result<crate::domain::MarketplaceFeedPage, String> {
    crate::marketplace::load_marketplace_feed(&app, input)
}

#[tauri::command]
pub async fn skill_import_from_market(
    app: tauri::AppHandle,
    input: ImportMarketSkillInput,
) -> Result<crate::domain::LegacySkillDto, String> {
    run_locked_command(move || crate::marketplace::import_skill_from_market(&app, &input)).await
}

// ─── Registry commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn registry_search(
    query: String,
    limit: Option<u32>,
) -> Result<crate::domain::RegistrySearchResult, String> {
    let limit = limit.unwrap_or(30);
    crate::registry::search_registry(&query, limit)
}

#[tauri::command]
pub async fn registry_fetch_content(
    app: tauri::AppHandle,
    source: String,
    skill_id: String,
) -> Result<crate::domain::RegistrySkillContent, String> {
    run_blocking_command(move || crate::registry::fetch_registry_content(&app, &source, &skill_id))
        .await
}

#[tauri::command]
pub fn github_token_configured(app: tauri::AppHandle) -> bool {
    crate::github_auth::has_stored_token(&app)
}

#[tauri::command]
pub async fn github_token_set(app: tauri::AppHandle, token: String) -> Result<(), String> {
    run_locked_command(move || crate::github_auth::set_token(&app, &token)).await
}

#[tauri::command]
pub async fn registry_install(
    input: crate::domain::RegistryInstallInput,
) -> Result<crate::domain::RegistryInstallResult, String> {
    run_locked_command(move || crate::registry::install_registry_skill(&input)).await
}

// ─── Backup bootstrap & repo source import commands ───────────────────────────

#[tauri::command]
pub async fn backup_source_bootstrap(
    app: tauri::AppHandle,
    input: crate::domain::BootstrapBackupInput,
) -> Result<crate::domain::BackupSourceStatus, String> {
    run_locked_command(move || store::backup_source_bootstrap(&app, &input)).await
}

#[tauri::command]
pub async fn skill_import_from_repo_source(
    app: tauri::AppHandle,
    input: crate::domain::ImportRepoSourceSkillInput,
) -> Result<crate::domain::SkillMutationResult, String> {
    run_locked_command(move || store::import_skill_from_repo_source(&app, &input)).await
}

// ─── MCP Server commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn library_folders_list(
    app: tauri::AppHandle,
) -> Result<Vec<crate::domain::LibraryFolder>, String> {
    store::list_library_folders(&app)
}

#[tauri::command]
pub async fn library_folders_set(
    app: tauri::AppHandle,
    kind: crate::domain::LibraryFolderKind,
    folders: Vec<crate::domain::LibraryFolder>,
) -> Result<Vec<crate::domain::LibraryFolder>, String> {
    run_locked_command(move || store::set_library_folders(&app, kind, folders)).await
}

#[tauri::command]
pub fn mcp_list(app: tauri::AppHandle) -> Result<Vec<crate::domain::McpServerDto>, String> {
    crate::mcp::list_mcp_servers(&app)
}

#[tauri::command]
pub fn mcp_get(
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<crate::domain::McpServerDto>, String> {
    crate::mcp::get_mcp_server(&app, &id)
}

#[tauri::command]
pub async fn mcp_create(
    app: tauri::AppHandle,
    input: crate::domain::CreateMcpServerInput,
) -> Result<crate::domain::CreateMcpServerResult, String> {
    run_locked_command(move || crate::mcp::create_mcp_server(&app, &input)).await
}

#[tauri::command]
pub async fn mcp_update(
    app: tauri::AppHandle,
    input: crate::domain::UpdateMcpServerInput,
) -> Result<crate::domain::McpServerDto, String> {
    run_locked_command(move || crate::mcp::update_mcp_server(&app, &input)).await
}

#[tauri::command]
pub async fn mcp_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run_locked_command(move || crate::mcp::delete_mcp_server(&app, &id)).await
}

#[tauri::command]
pub async fn mcp_install_global(
    app: tauri::AppHandle,
    input: crate::domain::InstallMcpGlobalInput,
) -> Result<crate::domain::InstallMcpResult, String> {
    run_locked_command(move || crate::mcp::install_mcp_global(&app, &input)).await
}

#[tauri::command]
pub async fn mcp_uninstall_global(
    app: tauri::AppHandle,
    input: crate::domain::InstallMcpGlobalInput,
) -> Result<crate::domain::InstallMcpResult, String> {
    run_locked_command(move || crate::mcp::uninstall_mcp_global(&app, &input)).await
}

#[tauri::command]
pub async fn mcp_install_to_project(
    app: tauri::AppHandle,
    input: crate::domain::InstallMcpToProjectInput,
) -> Result<crate::domain::InstallMcpResult, String> {
    run_locked_command(move || crate::mcp::install_mcp_to_project(&app, &input)).await
}

#[tauri::command]
pub async fn mcp_uninstall_from_project(
    app: tauri::AppHandle,
    input: crate::domain::InstallMcpToProjectInput,
) -> Result<crate::domain::InstallMcpResult, String> {
    run_locked_command(move || crate::mcp::uninstall_mcp_from_project(&app, &input)).await
}

#[tauri::command]
pub fn mcp_check_status(
    app: tauri::AppHandle,
    input: crate::domain::CheckMcpStatusInput,
) -> Result<crate::domain::CheckMcpStatusResult, String> {
    crate::mcp::check_mcp_status(&app, &input)
}

#[tauri::command]
pub fn mcp_list_global_enabled_apps(
    app: tauri::AppHandle,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    crate::mcp::list_all_mcp_global_enabled_apps(&app)
}

#[tauri::command]
pub async fn scan_external_mcp_servers(
    app: tauri::AppHandle,
) -> Result<Vec<crate::domain::ExternalMcpServerDto>, String> {
    run_blocking_command(move || crate::mcp::scan_external_mcp_servers(&app)).await
}

#[tauri::command]
pub async fn mcp_import_external(
    app: tauri::AppHandle,
    input: crate::domain::ImportExternalMcpInput,
) -> Result<crate::domain::CreateMcpServerResult, String> {
    run_locked_command(move || crate::mcp::import_external_mcp_server(&app, &input)).await
}

// ─── Plugin / Extension commands ──────────────────────────────────────────────

#[tauri::command]
pub fn plugin_list(app: tauri::AppHandle) -> Result<Vec<crate::domain::PluginDto>, String> {
    crate::plugins::list_plugins(&app)
}

#[tauri::command]
pub fn plugin_get(
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<crate::domain::PluginDto>, String> {
    crate::plugins::get_plugin(&app, &id)
}

#[tauri::command]
pub async fn plugin_import_from_folder(
    app: tauri::AppHandle,
    input: crate::domain::ImportPluginFromFolderInput,
) -> Result<crate::domain::ImportPluginOutcome, String> {
    run_locked_command(move || crate::plugins::import_plugin_from_folder(&app, &input)).await
}

#[tauri::command]
pub async fn plugin_import_from_dialog(
    app: tauri::AppHandle,
) -> Result<Option<crate::domain::ImportPluginOutcome>, String> {
    run_blocking_command(move || crate::plugins::import_plugin_from_dialog(&app)).await
}

#[tauri::command]
pub async fn plugin_import_from_zip(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<crate::domain::ImportPluginOutcome, String> {
    run_locked_command(move || crate::plugins::import_plugin_from_zip(&app, &zip_path)).await
}

#[tauri::command]
pub async fn plugin_import_from_git(
    app: tauri::AppHandle,
    input: crate::domain::ImportPluginFromGitInput,
) -> Result<crate::domain::ImportPluginOutcome, String> {
    run_locked_command(move || crate::plugins::import_plugin_from_git(&app, &input)).await
}

#[tauri::command]
pub async fn plugin_import_staged(
    app: tauri::AppHandle,
    input: crate::domain::ImportStagedPluginsInput,
) -> Result<crate::domain::ImportPluginBatchResult, String> {
    run_locked_command(move || crate::plugins::import_staged_plugins(&app, &input)).await
}

#[tauri::command]
pub async fn plugin_discard_staged(staging_id: String) -> Result<(), String> {
    run_locked_command(move || {
        crate::plugins::discard_staged_import(&staging_id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn plugin_update_config(
    app: tauri::AppHandle,
    input: crate::domain::UpdatePluginConfigInput,
) -> Result<crate::domain::PluginDto, String> {
    run_locked_command(move || crate::plugins::update_plugin_config(&app, &input)).await
}

#[tauri::command]
pub async fn plugin_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run_locked_command(move || crate::plugins::delete_plugin(&app, &id)).await
}

#[tauri::command]
pub async fn plugin_install_global(
    app: tauri::AppHandle,
    input: crate::domain::InstallPluginGlobalInput,
) -> Result<crate::domain::InstallPluginResult, String> {
    run_locked_command(move || crate::plugins::install_plugin_global(&app, &input)).await
}

#[tauri::command]
pub async fn plugin_uninstall_global(
    app: tauri::AppHandle,
    input: crate::domain::InstallPluginGlobalInput,
) -> Result<crate::domain::InstallPluginResult, String> {
    run_locked_command(move || crate::plugins::uninstall_plugin_global(&app, &input)).await
}

#[tauri::command]
pub async fn plugin_install_to_project(
    app: tauri::AppHandle,
    input: crate::domain::InstallPluginToProjectInput,
) -> Result<crate::domain::InstallPluginResult, String> {
    run_locked_command(move || crate::plugins::install_plugin_to_project(&app, &input)).await
}

#[tauri::command]
pub async fn plugin_uninstall_from_project(
    app: tauri::AppHandle,
    input: crate::domain::InstallPluginToProjectInput,
) -> Result<crate::domain::InstallPluginResult, String> {
    run_locked_command(move || crate::plugins::uninstall_plugin_from_project(&app, &input)).await
}

#[tauri::command]
pub fn plugin_check_status(
    app: tauri::AppHandle,
    input: crate::domain::CheckPluginStatusInput,
) -> Result<crate::domain::CheckPluginStatusResult, String> {
    crate::plugins::check_plugin_status(&app, &input)
}

#[tauri::command]
pub fn plugin_list_global_enabled_apps(
    app: tauri::AppHandle,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    crate::plugins::list_all_plugins_global_enabled_apps(&app)
}

// ─── SSH Remote Target commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn ssh_discover_hosts() -> Result<Vec<crate::domain::DiscoveredSshHost>, String> {
    run_blocking_command(crate::ssh_sync::discover_ssh_hosts).await
}

#[tauri::command]
pub async fn ssh_test_connection(
    ssh: crate::domain::SshTargetConfig,
) -> Result<crate::domain::SshConnectionTestResult, String> {
    run_blocking_command(move || crate::ssh_sync::test_ssh_connection(&ssh)).await
}

#[tauri::command]
pub async fn ssh_calibrate_target(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<crate::domain::SshAppSyncLedger, String> {
    run_locked_command(move || crate::ssh_sync::calibrate_ssh_target(&app, &app_id)).await
}

#[tauri::command]
pub async fn ssh_sync_all_enabled(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<crate::domain::SshSyncSummaryResult, String> {
    run_locked_command(move || crate::ssh_sync::sync_all_enabled_to_ssh_target(&app, &app_id)).await
}
