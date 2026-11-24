//! Installing skills/commands into CLI app directories (global and project) via symlinks.

use super::*;

// ─── Project skill install functions ────────────────────────────────────────────

/// Install a skill to a project for specified CLI apps using symbolic links
/// Each CLI has its own directory structure:
/// - Claude Code: .claude/skills/{slug} -> {app_data}/skill-sources/{slug}/
/// - Codex CLI: .codex/skills/{slug} -> {app_data}/skill-sources/{slug}/
/// - Cursor: .cursor/skills/{slug} -> {app_data}/skill-sources/{slug}/
pub fn install_skill_to_project(
    app: &tauri::AppHandle,
    input: &InstallSkillToProjectInput,
) -> Result<InstallSkillToProjectResult, String> {
    // Get the skill
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    // Normalize project path
    let project_path = normalize_user_path(&input.project_path)?;
    if !project_path.exists() {
        return Err(format!(
            "project path does not exist: {}",
            input.project_path
        ));
    }

    let slug = &skill.slug;
    let source_dir = ensure_skill_source_for_skill(app, &skill)?;
    let custom_apps = load_custom_apps(app);

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        let result = preferred_project_skill_path(&project_path, app_id, slug, &custom_apps)
            .and_then(|link_path| create_skill_symlink(&link_path, &source_dir));

        if result.is_ok() {
            installed_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSkillToProjectResult {
        installed_apps,
        failed_apps,
    })
}

/// Uninstall a skill from a project for specified CLI apps
/// Handles both symlinks and legacy copied directories
pub fn uninstall_skill_from_project(
    app: &tauri::AppHandle,
    input: &InstallSkillToProjectInput,
) -> Result<InstallSkillToProjectResult, String> {
    // Get the skill
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    // Normalize project path
    let project_path = normalize_user_path(&input.project_path)?;
    if !project_path.exists() {
        return Err(format!(
            "project path does not exist: {}",
            input.project_path
        ));
    }

    let slug = &skill.slug;
    let custom_apps = load_custom_apps(app);

    let mut uninstalled_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        let result =
            project_skill_paths(&project_path, app_id, slug, &custom_apps).and_then(|paths| {
                for path in paths {
                    remove_symlink(&path)?;
                }
                Ok(())
            });

        if result.is_ok() {
            uninstalled_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSkillToProjectResult {
        installed_apps: uninstalled_apps,
        failed_apps,
    })
}

pub fn install_slash_command_to_project(
    app: &tauri::AppHandle,
    input: &InstallSlashCommandToProjectInput,
) -> Result<InstallSlashCommandToProjectResult, String> {
    let command = get_slash_command(app, &input.command_id)?
        .ok_or_else(|| format!("slash command {} not found", input.command_id))?;

    let project_path = normalize_user_path(&input.project_path)?;
    if !project_path.exists() {
        return Err(format!(
            "project path does not exist: {}",
            input.project_path
        ));
    }

    let source_dir = ensure_slash_command_source_for_command(app, &command)?;
    let custom_apps = load_custom_apps(app);
    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        let result =
            preferred_project_command_path(&project_path, app_id, &command.slug, &custom_apps)
                .and_then(|link_path| create_skill_symlink(&link_path, &source_dir));

        if result.is_ok() {
            installed_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSlashCommandToProjectResult {
        installed_apps,
        failed_apps,
    })
}

pub fn uninstall_slash_command_from_project(
    app: &tauri::AppHandle,
    input: &InstallSlashCommandToProjectInput,
) -> Result<InstallSlashCommandToProjectResult, String> {
    let command = get_slash_command(app, &input.command_id)?
        .ok_or_else(|| format!("slash command {} not found", input.command_id))?;

    let project_path = normalize_user_path(&input.project_path)?;
    if !project_path.exists() {
        return Err(format!(
            "project path does not exist: {}",
            input.project_path
        ));
    }

    let custom_apps = load_custom_apps(app);
    let mut uninstalled_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        let result = project_command_paths(&project_path, app_id, &command.slug, &custom_apps)
            .and_then(|paths| {
                for path in paths {
                    remove_symlink(&path)?;
                }
                Ok(())
            });

        if result.is_ok() {
            uninstalled_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSlashCommandToProjectResult {
        installed_apps: uninstalled_apps,
        failed_apps,
    })
}

// =============================================================================
// Symlink Utilities
// =============================================================================

/// Check if a path is a symbolic link
pub fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

/// Create a symbolic link from target to source (directory symlink)
/// On Unix: uses std::os::unix::fs::symlink
/// On Windows: uses std::os::windows::fs::symlink_dir (requires Developer Mode or admin)
pub fn create_skill_symlink(link_path: &Path, source_path: &Path) -> Result<(), String> {
    // Remove existing symlink or directory if it exists
    if link_path.exists() || is_symlink(link_path) {
        if is_symlink(link_path) {
            // Remove symlink
            #[cfg(unix)]
            fs::remove_file(link_path)
                .map_err(|e| format!("Failed to remove existing symlink: {}", e))?;
            #[cfg(windows)]
            fs::remove_dir(link_path)
                .map_err(|e| format!("Failed to remove existing symlink: {}", e))?;
        } else {
            // A real file/directory occupies the path (e.g. the user's own skill with
            // the same name). Never delete it: move it aside so it can be recovered.
            move_aside(link_path)?;
        }
    }

    // Create parent directory if it doesn't exist
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
    }

    // Create the symlink
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source_path, link_path)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;
    }

    #[cfg(windows)]
    symlink_dir_windows(source_path, link_path)?;

    Ok(())
}

/// Creates a directory symlink, falling back to an NTFS junction when symlinks
/// need privileges we do not have (Developer Mode off, not elevated). Junctions
/// need no special rights and behave the same for CLI tools reading the folder.
#[cfg(windows)]
pub(crate) fn symlink_dir_windows(source_path: &Path, link_path: &Path) -> Result<(), String> {
    const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
    match std::os::windows::fs::symlink_dir(source_path, link_path) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) => {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link_path)
                .arg(source_path)
                .output()
                .map_err(|e| format!("Failed to create junction: {}", e))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Failed to create symlink or junction: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ))
            }
        }
        Err(error) => Err(format!("Failed to create symlink: {}", error)),
    }
}

/// Where real directories found at link paths are moved instead of being deleted.
fn replaced_items_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("com.skill-switch.app")
        .join("replaced")
}

/// Moves `path` into a timestamped folder under the "replaced" root, returning the
/// new location. Fails (without deleting anything) if the move is not possible.
fn move_aside(path: &Path) -> Result<PathBuf, String> {
    move_aside_into(path, &replaced_items_root())
}

pub(crate) fn move_aside_into(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".into());
    let parent_label = path
        .parent()
        .map(|p| p.to_string_lossy().replace(['/', '\\', ':'], "_"))
        .unwrap_or_default();
    let destination = root
        .join(chrono::Utc::now().format("%Y%m%d-%H%M%S%3f").to_string())
        .join(parent_label.trim_start_matches('_'))
        .join(&name);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法创建备份目录：{}", e))?;
    }
    fs::rename(path, &destination).map_err(|e| {
        format!(
            "{} 已存在且不是 SkillSwitch 创建的链接，移动到备份目录失败（未做任何删除）：{}",
            path.display(),
            e
        )
    })?;
    Ok(destination)
}

/// Remove a symbolic link without affecting the source
pub fn remove_symlink(link_path: &Path) -> Result<(), String> {
    if is_symlink(link_path) {
        #[cfg(unix)]
        fs::remove_file(link_path).map_err(|e| format!("Failed to remove symlink: {}", e))?;

        #[cfg(windows)]
        fs::remove_dir(link_path).map_err(|e| format!("Failed to remove symlink: {}", e))?;

        Ok(())
    } else if link_path.exists() {
        // A real directory, not our symlink: move it aside instead of deleting it.
        move_aside(link_path).map(|_| ())
    } else {
        // Doesn't exist, nothing to do
        Ok(())
    }
}

// =============================================================================
// Symlink Status Commands
// =============================================================================

/// Check the symlink status of a skill installation
pub fn check_symlink_status(
    app: &tauri::AppHandle,
    input: &CheckSymlinkStatusInput,
) -> Result<CheckSymlinkStatusResult, String> {
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    let slug = &skill.slug;
    let custom_apps = load_custom_apps(app);
    let is_project = input.scope == "project";

    let base_path = if is_project {
        let project_path = input
            .project_path
            .as_ref()
            .ok_or("project_path is required for project scope")?;
        normalize_user_path(project_path)?
    } else {
        app.path().home_dir().map_err(|e| e.to_string())?
    };

    let apps = all_configured_app_ids(&custom_apps);
    let ssh_ledgers = if !is_project {
        load_local_state(app)
            .map(|s| s.ssh_ledgers)
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let mut statuses = Vec::new();

    for app_id in apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(&app_id, &custom_apps) {
            if is_project {
                continue;
            }
            let exists = ssh_ledgers
                .get(&app_id)
                .map(|ledger| ledger.skills.contains(slug))
                .unwrap_or(false);
            let remote_target = exists.then(|| {
                let host = custom
                    .ssh
                    .as_ref()
                    .map(|s| s.host.as_str())
                    .unwrap_or("ssh");
                format!(
                    "{}:{}/{}",
                    host,
                    custom.global_skill_dir.trim_end_matches('/'),
                    slug
                )
            });
            statuses.push(SkillSymlinkStatus {
                app_id,
                is_symlink: false,
                is_broken: false,
                target_path: remote_target,
                exists,
            });
            continue;
        }

        let install_paths = if is_project {
            match project_skill_paths(&base_path, &app_id, slug, &custom_apps) {
                Ok(paths) => paths,
                Err(_) => continue,
            }
        } else {
            match global_skill_paths(&base_path, &app_id, slug, &custom_apps) {
                Ok(paths) => paths,
                Err(_) => continue,
            }
        };

        let mut has_symlink = false;
        let mut exists = false;
        let mut is_broken = false;
        let mut target_path = None;

        for install_path in install_paths {
            let path_is_symlink = is_symlink(&install_path);
            let path_exists = install_path.exists();

            if path_is_symlink {
                has_symlink = true;
                if target_path.is_none() {
                    target_path = install_path
                        .read_link()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string());
                }
            }

            if path_exists {
                exists = true;
            }
            if path_is_symlink && !path_exists {
                is_broken = true;
            }
        }

        statuses.push(SkillSymlinkStatus {
            app_id,
            is_symlink: has_symlink,
            is_broken,
            target_path,
            exists,
        });
    }

    Ok(CheckSymlinkStatusResult {
        skill_id: input.skill_id.clone(),
        statuses,
    })
}

/// Return globally enabled app IDs for every managed skill in one pass
pub fn list_all_skills_global_enabled_apps(
    app: &tauri::AppHandle,
) -> Result<HashMap<String, Vec<String>>, String> {
    let skills = match list_legacy_skills(app) {
        Ok(items) => items,
        Err(_) => return Ok(HashMap::new()),
    };
    let home_dir = app.path().home_dir().map_err(|e| e.to_string())?;
    let custom_apps = load_custom_apps(app);
    let app_ids = all_configured_app_ids(&custom_apps);
    let ssh_ledgers = load_local_state(app)
        .map(|s| s.ssh_ledgers)
        .unwrap_or_default();

    let mut result = HashMap::with_capacity(skills.len());
    for skill in skills {
        let mut enabled = Vec::new();
        for app_id in &app_ids {
            if crate::ssh_sync::find_ssh_app(app_id, &custom_apps).is_some() {
                if ssh_ledgers
                    .get(app_id)
                    .map(|ledger| ledger.skills.contains(&skill.slug))
                    .unwrap_or(false)
                {
                    enabled.push(app_id.clone());
                }
                continue;
            }
            if let Ok(paths) = global_skill_paths(&home_dir, app_id, &skill.slug, &custom_apps) {
                if paths.iter().any(|path| path.exists()) {
                    enabled.push(app_id.clone());
                }
            }
        }
        result.insert(skill.id, enabled);
    }

    Ok(result)
}

/// Check the symlink status of a slash command installation
pub fn check_slash_command_symlink_status(
    app: &tauri::AppHandle,
    input: &CheckSlashCommandSymlinkInput,
) -> Result<CheckSlashCommandSymlinkResult, String> {
    let command = get_slash_command(app, &input.command_id)?
        .ok_or_else(|| format!("slash command {} not found", input.command_id))?;

    let slug = &command.slug;
    let custom_apps = load_custom_apps(app);
    let is_project = input.scope == "project";

    let base_path = if is_project {
        let project_path = input
            .project_path
            .as_ref()
            .ok_or("project_path is required for project scope")?;
        normalize_user_path(project_path)?
    } else {
        app.path().home_dir().map_err(|e| e.to_string())?
    };

    let apps = all_configured_app_ids(&custom_apps);
    let ssh_ledgers = if !is_project {
        load_local_state(app)
            .map(|s| s.ssh_ledgers)
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let mut statuses = Vec::new();

    for app_id in apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(&app_id, &custom_apps) {
            if is_project {
                continue;
            }
            let Some(cmd_dir) = custom
                .global_command_dir
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let exists = ssh_ledgers
                .get(&app_id)
                .map(|ledger| ledger.commands.contains(slug))
                .unwrap_or(false);
            let remote_target = exists.then(|| {
                let host = custom
                    .ssh
                    .as_ref()
                    .map(|s| s.host.as_str())
                    .unwrap_or("ssh");
                format!("{}:{}/{}.md", host, cmd_dir.trim_end_matches('/'), slug)
            });
            statuses.push(SkillSymlinkStatus {
                app_id,
                is_symlink: false,
                is_broken: false,
                target_path: remote_target,
                exists,
            });
            continue;
        }

        let install_paths = if is_project {
            match project_command_paths(&base_path, &app_id, slug, &custom_apps) {
                Ok(paths) if !paths.is_empty() => paths,
                _ => continue,
            }
        } else {
            match global_command_paths(&base_path, &app_id, slug, &custom_apps) {
                Ok(paths) if !paths.is_empty() => paths,
                _ => continue,
            }
        };

        let mut has_symlink = false;
        let mut exists = false;
        let mut is_broken = false;
        let mut target_path = None;

        for install_path in install_paths {
            let path_is_symlink = is_symlink(&install_path);
            let path_exists = install_path.exists();

            if path_is_symlink {
                has_symlink = true;
                if target_path.is_none() {
                    target_path = install_path
                        .read_link()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string());
                }
            }

            if path_exists {
                exists = true;
            }
            if path_is_symlink && !path_exists {
                is_broken = true;
            }
        }

        statuses.push(SkillSymlinkStatus {
            app_id,
            is_symlink: has_symlink,
            is_broken,
            target_path,
            exists,
        });
    }

    Ok(CheckSlashCommandSymlinkResult {
        command_id: input.command_id.clone(),
        statuses,
    })
}

/// Repair broken symlinks for a skill
pub fn repair_broken_symlinks(
    app: &tauri::AppHandle,
    input: &CheckSymlinkStatusInput,
) -> Result<RepairBrokenSymlinksResult, String> {
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    let slug = &skill.slug;
    let custom_apps = load_custom_apps(app);
    let is_project = input.scope == "project";

    let base_path = if is_project {
        let project_path = input
            .project_path
            .as_ref()
            .ok_or("project_path is required for project scope")?;
        normalize_user_path(project_path)?
    } else {
        app.path().home_dir().map_err(|e| e.to_string())?
    };

    let mut removed_symlinks = Vec::new();
    let mut errors = Vec::new();

    for app_id in all_configured_app_ids(&custom_apps) {
        if crate::ssh_sync::is_ssh_app(&app_id, &custom_apps) {
            continue;
        }
        let paths = if is_project {
            project_skill_paths(&base_path, &app_id, slug, &custom_apps).unwrap_or_default()
        } else {
            global_skill_paths(&base_path, &app_id, slug, &custom_apps).unwrap_or_default()
        };

        for install_path in paths {
            if is_symlink(&install_path) && !install_path.exists() {
                match remove_symlink(&install_path) {
                    Ok(()) => {
                        removed_symlinks.push(install_path.to_string_lossy().to_string());
                    }
                    Err(e) => {
                        errors.push(format!(
                            "Failed to remove broken symlink at {}: {}",
                            install_path.display(),
                            e
                        ));
                    }
                }
            }
        }
    }

    Ok(RepairBrokenSymlinksResult {
        removed_symlinks,
        errors,
    })
}

/// Non-destructively clean up SkillSwitch-managed symlinks inside a project's CLI folders.
/// Preserves user configuration files (`config.toml`, `mcp.json`, `settings.json`, `rules/`, etc.)
/// and removes `skills` / `commands` / CLI directories ONLY when they become completely empty.
pub(crate) fn cleanup_managed_cli_dirs_in_project(
    project_path: &Path,
    app_id: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<(), String> {
    let mut subdirs_to_scan = Vec::new();
    if let Ok(skill_dirs) = project_skill_dirs_for_app(project_path, app_id, custom_apps) {
        subdirs_to_scan.extend(skill_dirs);
    }
    if let Ok(cmd_dirs) = project_command_dirs_for_app(project_path, app_id, custom_apps) {
        subdirs_to_scan.extend(cmd_dirs);
    }

    for subdir in &subdirs_to_scan {
        if !subdir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(subdir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if is_symlink(&entry_path) {
                    remove_symlink(&entry_path)?;
                }
            }
        }
        // Remove subdir ONLY if it is now completely empty (`fs::remove_dir` refuses non-empty dirs)
        let _ = fs::remove_dir(subdir);
    }

    if let Ok(cli_dirs) = app_cli_dirs(project_path, app_id) {
        for cli_dir in cli_dirs {
            if cli_dir.exists() {
                let _ = fs::remove_dir(&cli_dir);
            }
        }
    }

    Ok(())
}

/// Remove SkillSwitch-managed CLI skill/command symlinks from a project directory
/// without destroying host configuration files (`config.toml`, `mcp.json`, `settings.json`, `rules/`, etc.).
pub fn remove_project_cli_folders(
    app: &tauri::AppHandle,
    input: &RemoveProjectCliInput,
) -> Result<RemoveProjectCliResult, String> {
    let project_path = normalize_user_path(&input.project_path)?;
    if !project_path.exists() {
        return Err(format!(
            "project path does not exist: {}",
            input.project_path
        ));
    }

    let custom_apps = load_custom_apps(app);
    let mut removed_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        match cleanup_managed_cli_dirs_in_project(&project_path, app_id, &custom_apps) {
            Ok(()) => removed_apps.push(app_id.clone()),
            Err(_) => failed_apps.push(app_id.clone()),
        }
    }

    Ok(RemoveProjectCliResult {
        removed_apps,
        failed_apps,
    })
}

// ─── Global skill install functions ─────────────────────────────────────────────

/// Install a skill globally for specified CLI apps using symbolic links
/// Each CLI has its own directory structure in the home directory:
/// - Claude Code: ~/.claude/skills/{slug} -> {app_data}/skill-sources/{slug}/
/// - Codex CLI: ~/.codex/skills/{slug} -> {app_data}/skill-sources/{slug}/
/// - Cursor: ~/.cursor/skills/{slug} -> {app_data}/skill-sources/{slug}/
pub fn install_skill_global(
    app: &tauri::AppHandle,
    input: &InstallSkillGlobalInput,
) -> Result<InstallSkillGlobalResult, String> {
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    let home_dir = app.path().home_dir().map_err(|error| error.to_string())?;

    let slug = &skill.slug;
    let source_dir = ensure_skill_source_for_skill(app, &skill)?;
    let custom_apps = load_custom_apps(app);

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::install_skill_ssh(app, custom, slug, &source_dir) {
                Ok(()) => installed_apps.push(app_id.clone()),
                Err(err) => {
                    if input.apps.len() == 1 {
                        return Err(err);
                    }
                    failed_apps.push(app_id.clone());
                }
            }
            continue;
        }

        let result = preferred_global_skill_path(&home_dir, app_id, slug, &custom_apps)
            .and_then(|link_path| create_skill_symlink(&link_path, &source_dir));

        if result.is_ok() {
            installed_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSkillGlobalResult {
        installed_apps,
        failed_apps,
    })
}

pub fn uninstall_skill_global(
    app: &tauri::AppHandle,
    input: &InstallSkillGlobalInput,
) -> Result<InstallSkillGlobalResult, String> {
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    let home_dir = app.path().home_dir().map_err(|error| error.to_string())?;

    let slug = &skill.slug;
    let custom_apps = load_custom_apps(app);

    let mut uninstalled_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::uninstall_skill_ssh(app, custom, slug) {
                Ok(()) => uninstalled_apps.push(app_id.clone()),
                Err(err) => {
                    if input.apps.len() == 1 {
                        return Err(err);
                    }
                    failed_apps.push(app_id.clone());
                }
            }
            continue;
        }

        let result = global_skill_paths(&home_dir, app_id, slug, &custom_apps).and_then(|paths| {
            for path in paths {
                remove_symlink(&path)?;
            }
            Ok(())
        });

        if result.is_ok() {
            uninstalled_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSkillGlobalResult {
        installed_apps: uninstalled_apps,
        failed_apps,
    })
}

pub fn install_slash_command_global(
    app: &tauri::AppHandle,
    input: &InstallSlashCommandGlobalInput,
) -> Result<InstallSlashCommandGlobalResult, String> {
    let command = get_slash_command(app, &input.command_id)?
        .ok_or_else(|| format!("slash command {} not found", input.command_id))?;

    let home_dir = app.path().home_dir().map_err(|error| error.to_string())?;
    let source_dir = ensure_slash_command_source_for_command(app, &command)?;
    let cmd_md_path = source_dir.join("COMMAND.md");
    let custom_apps = load_custom_apps(app);

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::install_command_ssh(app, custom, &command.slug, &cmd_md_path) {
                Ok(()) => installed_apps.push(app_id.clone()),
                Err(err) => {
                    if input.apps.len() == 1 {
                        return Err(err);
                    }
                    failed_apps.push(app_id.clone());
                }
            }
            continue;
        }

        let result = preferred_global_command_path(&home_dir, app_id, &command.slug, &custom_apps)
            .and_then(|link_path| create_skill_symlink(&link_path, &source_dir));

        if result.is_ok() {
            installed_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSlashCommandGlobalResult {
        installed_apps,
        failed_apps,
    })
}

pub fn uninstall_slash_command_global(
    app: &tauri::AppHandle,
    input: &InstallSlashCommandGlobalInput,
) -> Result<InstallSlashCommandGlobalResult, String> {
    let command = get_slash_command(app, &input.command_id)?
        .ok_or_else(|| format!("slash command {} not found", input.command_id))?;

    let home_dir = app.path().home_dir().map_err(|error| error.to_string())?;
    let custom_apps = load_custom_apps(app);

    let mut uninstalled_apps = Vec::new();
    let mut failed_apps = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::uninstall_command_ssh(app, custom, &command.slug) {
                Ok(()) => uninstalled_apps.push(app_id.clone()),
                Err(err) => {
                    if input.apps.len() == 1 {
                        return Err(err);
                    }
                    failed_apps.push(app_id.clone());
                }
            }
            continue;
        }

        let result = global_command_paths(&home_dir, app_id, &command.slug, &custom_apps).and_then(
            |paths| {
                for path in paths {
                    remove_symlink(&path)?;
                }
                Ok(())
            },
        );

        if result.is_ok() {
            uninstalled_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallSlashCommandGlobalResult {
        installed_apps: uninstalled_apps,
        failed_apps,
    })
}
