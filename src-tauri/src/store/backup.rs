//! Backup-source git operations: connect, pull, push, bootstrap and post-mutation commit.

use super::*;

pub(crate) fn sync_after_mutation(app: &tauri::AppHandle, message: &str) -> BackupSyncResult {
    let clone_dir = match persistent_clone_dir(app) {
        Ok(Some(d)) if d.exists() => d,
        Ok(Some(_)) | Ok(None) => {
            return BackupSyncResult {
                status: "skipped".into(),
                attempts: 0,
                message: Some("备份仓库未配置或未连接".into()),
                last_error: None,
            }
        }
        Err(e) => {
            return BackupSyncResult {
                status: "failed".into(),
                attempts: 0,
                message: Some("读取备份源配置失败".into()),
                last_error: Some(e),
            }
        }
    };

    // git add -A
    if let Err(e) = git::add_all(&clone_dir) {
        return BackupSyncResult {
            status: "failed".into(),
            attempts: 1,
            message: Some("git add 失败".into()),
            last_error: Some(e),
        };
    }

    // git commit
    match git::commit(&clone_dir, message) {
        Ok(false) => {} // nothing to commit
        Ok(true) => {}  // committed
        Err(e) => {
            return BackupSyncResult {
                status: "failed".into(),
                attempts: 1,
                message: Some("git commit 失败".into()),
                last_error: Some(e),
            }
        }
    }

    // Push happens on the background worker so the UI never waits on the network.
    crate::backup_sync::schedule_push(app);
    BackupSyncResult {
        status: "queued".into(),
        attempts: 0,
        message: Some("已提交到本地备份仓库，正在后台推送".into()),
        last_error: None,
    }
}

/// Persists the outcome of a background push into the backup source settings.
pub(crate) fn record_backup_push_result(
    app: &tauri::AppHandle,
    pushed_commit: Option<String>,
    error: Option<String>,
) -> Result<(), String> {
    let mut settings = load_settings(app)?;
    let Some(source) = settings.backup_source.as_mut() else {
        return Ok(());
    };
    match error {
        Some(error) => source.last_error = Some(error),
        None => {
            source.last_error = None;
            source.last_synced_at = Some(now_ms());
            if pushed_commit.is_some() {
                source.last_synced_commit = pushed_commit;
            }
        }
    }
    save_settings(app, &settings)
}

pub fn repo_pull(app: &tauri::AppHandle) -> Result<RepoStatus, String> {
    let repo_root = connected_repo_root(app)?;
    git::pull(&repo_root)?;
    let library = load_repo_library(&repo_root)?;
    build_repo_status_from_state(&repo_root, &library, None)
}

pub fn repo_push(app: &tauri::AppHandle) -> Result<RepoStatus, String> {
    let repo_root = connected_repo_root(app)?;
    git::push(&repo_root)?;
    let library = load_repo_library(&repo_root)?;
    build_repo_status_from_state(&repo_root, &library, None)
}

pub fn repo_sync(app: &tauri::AppHandle) -> Result<RepoStatus, String> {
    let repo_root = connected_repo_root(app)?;
    git::pull(&repo_root)?;
    let library = load_repo_library(&repo_root)?;
    build_repo_status_from_state(&repo_root, &library, None)
}

pub fn backup_source_status(app: &tauri::AppHandle) -> Result<BackupSourceStatus, String> {
    match backup_source_settings(app)? {
        Some(_) => build_backup_source_status_from_clone(app),
        None => Ok(BackupSourceStatus {
            configured: false,
            repo: String::new(),
            label: String::new(),
            remote_url: String::new(),
            branch: "main".to_string(),
            local_path: String::new(),
            last_synced_at: None,
            last_synced_commit: None,
            connected: false,
            git_available: git::git_available(),
            is_git_repo: false,
            head: None,
            ahead: 0,
            behind: 0,
            dirty: false,
            last_error: None,
            notice: None,
        }),
    }
}

pub(crate) fn build_backup_source_status_from_clone(
    app: &tauri::AppHandle,
) -> Result<BackupSourceStatus, String> {
    let settings = load_settings(app)?;
    let config = settings
        .backup_source
        .ok_or_else(|| "backup source is not configured".to_string())?;
    let normalized = normalize_backup_source(app, &config)?;

    let clone_dir = PathBuf::from(
        normalized
            .local_path
            .as_ref()
            .ok_or_else(|| "backup source local_path is missing".to_string())?,
    );

    let clone_exists = clone_dir.exists();
    let is_git = clone_exists && git::is_git_repo(&clone_dir);

    let (ahead, behind) = if is_git {
        git::ahead_behind(&clone_dir)
    } else {
        (0, 0)
    };

    Ok(BackupSourceStatus {
        configured: true,
        repo: normalized.repo,
        label: normalized.label,
        remote_url: normalized.remote_url,
        branch: normalized.branch,
        local_path: clone_dir.to_string_lossy().into_owned(),
        last_synced_at: normalized.last_synced_at,
        last_synced_commit: normalized.last_synced_commit,
        connected: is_git,
        git_available: git::git_available(),
        is_git_repo: is_git,
        head: if is_git { git::head(&clone_dir) } else { None },
        ahead,
        behind,
        dirty: if is_git {
            git::dirty(&clone_dir)
        } else {
            false
        },
        last_error: normalized.last_error,
        notice: None,
    })
}

pub fn backup_source_connect(app: &tauri::AppHandle) -> Result<BackupSourceStatus, String> {
    let mut settings = load_settings(app)?;
    let source = settings
        .backup_source
        .clone()
        .ok_or_else(|| "backup source is not configured".to_string())?;
    let normalized = normalize_backup_source(app, &source)?;

    if !git::git_available() {
        return Err("git is not available on this machine".into());
    }

    // Validate remote accessibility
    let _ = git::remote_branch_exists(&normalized.remote_url, &normalized.branch)
        .map_err(|error| format!("无法访问远程仓库: {}", error))?;

    let clone_dir = PathBuf::from(
        normalized
            .local_path
            .as_ref()
            .ok_or_else(|| "backup source local_path is missing".to_string())?,
    );

    let remote_has_branch = git::remote_branch_exists(&normalized.remote_url, &normalized.branch)?;

    if !clone_dir.exists() {
        if let Some(parent) = clone_dir.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        if remote_has_branch {
            git::clone_repository(&normalized.remote_url, &clone_dir, Some(&normalized.branch))?;
        } else {
            fs::create_dir_all(&clone_dir).map_err(|e| e.to_string())?;
            git::init_repository(&clone_dir)?;
            git::configure_identity(&clone_dir)?;
            git::checkout_branch(&clone_dir, &normalized.branch)?;
            git::add_remote(&clone_dir, "origin", &normalized.remote_url)?;
        }
    } else {
        // Verify remote matches, fetch, pull
        if let Ok(Some(current_remote)) = git::remote_url(&clone_dir, "origin") {
            if current_remote != normalized.remote_url {
                git::set_remote_url(&clone_dir, "origin", &normalized.remote_url)?;
            }
        }
        if remote_has_branch {
            let _ = git::pull(&clone_dir); // Best effort
        }
    }

    git::configure_identity(&clone_dir)?;

    // Migrate library from old location if persistent clone has no library yet
    let clone_manifest = repo_manifest_path(&clone_dir);
    if !clone_manifest.exists() {
        let old_root = default_library_repo_root(app)?;
        let old_manifest = repo_manifest_path(&old_root);
        if old_manifest.exists() {
            if let Some(parent) = clone_manifest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&old_manifest, &clone_manifest).map_err(|e| e.to_string())?;
        } else {
            let library = RepoLibrary {
                version: "2".into(),
                ..Default::default()
            };
            save_repo_library(&clone_dir, &library)?;
        }
    }

    // Migrate skill files from old skill-sources to persistent clone
    let old_sources = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(SKILL_SOURCES_DIR);
    if old_sources.exists() && old_sources != clone_dir {
        migrate_skill_sources_to_clone(&old_sources, &clone_dir)?;
    }

    reconcile_managed_skills_from_backup_clone(&clone_dir)?;

    // Commit any changes from migration
    let _ = git::add_all(&clone_dir);
    let _ = git::commit(&clone_dir, "SkillSwitch: initial sync");

    // Try to push (best effort)
    let _ = git::push_branch(&clone_dir, "origin", &normalized.branch);

    // Update settings
    let new_commit = git::head(&clone_dir);
    let mut updated = normalized.clone();
    updated.last_synced_at = Some(now_ms());
    updated.last_synced_commit = new_commit;
    updated.last_error = None;
    settings.backup_source = Some(updated);
    save_settings(app, &settings)?;

    build_backup_source_status_from_clone(app)
}

pub fn backup_source_pull(app: &tauri::AppHandle) -> Result<BackupSourceStatus, String> {
    let mut settings = load_settings(app)?;
    let source = settings
        .backup_source
        .clone()
        .ok_or_else(|| "backup source is not configured".to_string())?;
    let normalized = normalize_backup_source(app, &source)?;

    let clone_dir = PathBuf::from(
        normalized
            .local_path
            .as_ref()
            .ok_or_else(|| "backup source local_path is missing".to_string())?,
    );

    if !clone_dir.exists() || !git::is_git_repo(&clone_dir) {
        return Err("persistent clone does not exist, please connect first".into());
    }

    git::pull(&clone_dir)?;
    reconcile_managed_skills_from_backup_clone(&clone_dir)?;

    // Update settings
    let new_commit = git::head(&clone_dir);
    let mut next_source = normalized.clone();
    next_source.last_synced_at = Some(now_ms());
    next_source.last_synced_commit = new_commit;
    next_source.last_error = None;
    settings.backup_source = Some(next_source);
    save_settings(app, &settings)?;

    build_backup_source_status_from_clone(app)
}

pub fn backup_source_push(app: &tauri::AppHandle) -> Result<BackupSourceStatus, String> {
    let mut settings = load_settings(app)?;
    let source = settings
        .backup_source
        .clone()
        .ok_or_else(|| "backup source is not configured".to_string())?;
    let normalized = normalize_backup_source(app, &source)?;

    let clone_dir = PathBuf::from(
        normalized
            .local_path
            .as_ref()
            .ok_or_else(|| "backup source local_path is missing".to_string())?,
    );

    if !clone_dir.exists() || !git::is_git_repo(&clone_dir) {
        return Err("persistent clone does not exist, please connect first".into());
    }

    // Stage, commit, push
    git::add_all(&clone_dir)?;
    let commit_message = format!(
        "Backup skill sources {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M")
    );
    let committed = git::commit(&clone_dir, &commit_message)?;
    if committed {
        git::push_branch(&clone_dir, "origin", &normalized.branch)?;
    }

    // Update settings
    let new_commit = git::head(&clone_dir);
    let mut next_source = normalized.clone();
    next_source.last_synced_at = Some(now_ms());
    next_source.last_synced_commit = new_commit;
    next_source.last_error = None;
    settings.backup_source = Some(next_source);
    save_settings(app, &settings)?;

    build_backup_source_status_from_clone(app)
}

pub fn backup_source_bootstrap(
    app: &tauri::AppHandle,
    input: &crate::domain::BootstrapBackupInput,
) -> Result<BackupSourceStatus, String> {
    let branch = input.branch.as_deref().unwrap_or("main");
    let remote_url = input.remote_url.trim();

    if remote_url.is_empty() {
        return Err("远程仓库地址不能为空".into());
    }

    // Derive repo identifier from URL
    let repo = parse_repo_from_remote_url(remote_url)
        .ok_or_else(|| "无法识别仓库地址，请使用 SSH 或 HTTPS 格式".to_string())?;

    let config = BackupSourceConfig {
        repo: repo.clone(),
        label: repo.clone(),
        remote_url: remote_url.to_string(),
        branch: branch.to_string(),
        local_path: None,
        last_synced_at: None,
        last_synced_commit: None,
        last_error: None,
    };

    let normalized = normalize_backup_source(app, &config)?;

    // Save config first so connect can read it
    let mut settings = load_settings(app)?;
    settings.backup_source = Some(normalized);
    save_settings(app, &settings)?;

    // Now connect
    backup_source_connect(app)
}

/// Returns whether the clone has local commits that still need pushing.
pub fn backup_source_startup_sync(app: &tauri::AppHandle) -> Result<bool, String> {
    let settings = load_settings(app)?;
    if settings.backup_source.is_none() {
        return Ok(false);
    }

    if let Some(clone_dir) = persistent_clone_dir(app)? {
        if clone_dir.exists() && git::is_git_repo(&clone_dir) {
            // Best effort pull on startup
            let _ = git::pull(&clone_dir);
            let _ = reconcile_managed_skills_from_backup_clone(&clone_dir);
            let (ahead, _) = git::ahead_behind(&clone_dir);
            return Ok(ahead > 0);
        }
    }

    Ok(false)
}
