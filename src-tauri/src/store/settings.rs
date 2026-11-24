//! App settings persistence and backup-source configuration helpers.

use super::*;

// ─── Backup functions ──────────────────────────────────────────────────────────

pub(crate) const BACKUPS_DIR: &str = "backups";
pub(crate) const SETTINGS_FILE: &str = "settings.json";

pub(crate) fn backups_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(BACKUPS_DIR))
}

pub(crate) fn resolved_backup_path(app: &tauri::AppHandle) -> Result<String, String> {
    backups_dir(app).map(|path| path.to_string_lossy().into_owned())
}

pub(crate) fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(SETTINGS_FILE))
}

pub(crate) fn remove_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        fs::remove_file(path).map_err(|error| error.to_string())
    }
}

pub(crate) fn backup_source_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(BACKUP_SOURCE_DIR))
}

pub(crate) fn backup_source_repo_dir(
    app: &tauri::AppHandle,
    repo: &str,
) -> Result<PathBuf, String> {
    let repo_slug = slugify(repo);
    backup_source_dir(app).map(|path| path.join(repo_slug))
}

pub(crate) fn backup_source_remote_url(repo: &str) -> String {
    format!("git@github.com:{repo}.git")
}

pub(crate) fn parse_repo_from_remote_url(remote_url: &str) -> Option<String> {
    let trimmed = remote_url.trim().trim_end_matches('/');
    if let Some(repo) = trimmed.strip_prefix("git@github.com:") {
        return Some(repo.trim_end_matches(".git").to_string());
    }

    if let Some(repo) = trimmed.strip_prefix("https://github.com/") {
        let mut parts = repo.split('/');
        let owner = parts.next()?;
        let repo = parts.next()?;
        return Some(
            format!("{owner}/{repo}")
                .trim_end_matches(".git")
                .to_string(),
        );
    }

    None
}

pub(crate) fn normalize_backup_source(
    app: &tauri::AppHandle,
    source: &BackupSourceConfig,
) -> Result<BackupSourceConfig, String> {
    let repo = {
        let trimmed = source.repo.trim();
        if !trimmed.is_empty() {
            trimmed.trim_end_matches(".git").to_string()
        } else if let Some(repo) = parse_repo_from_remote_url(&source.remote_url) {
            repo
        } else {
            return Err("backup source repo is required".into());
        }
    };

    let label = {
        let trimmed = source.label.trim();
        if trimmed.is_empty() {
            repo.clone()
        } else {
            trimmed.to_string()
        }
    };

    let remote_url = {
        let trimmed = source.remote_url.trim();
        if trimmed.is_empty() {
            backup_source_remote_url(&repo)
        } else {
            trimmed.to_string()
        }
    };

    let branch = {
        let trimmed = source.branch.trim();
        if trimmed.is_empty() {
            "main".to_string()
        } else {
            trimmed.to_string()
        }
    };

    let local_path = Some(
        backup_source_repo_dir(app, &repo)?
            .to_string_lossy()
            .into_owned(),
    );

    Ok(BackupSourceConfig {
        repo,
        label,
        remote_url,
        branch,
        local_path,
        last_synced_at: source.last_synced_at,
        last_synced_commit: source.last_synced_commit.clone(),
        last_error: source.last_error.clone(),
    })
}

pub(crate) fn backup_source_settings(
    app: &tauri::AppHandle,
) -> Result<Option<BackupSourceConfig>, String> {
    let settings = load_settings(app)?;
    settings
        .backup_source
        .as_ref()
        .map(|source| normalize_backup_source(app, source))
        .transpose()
}

pub(crate) fn normalize_optional_dir(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn normalize_ssh_config(
    ssh: &Option<crate::domain::SshTargetConfig>,
) -> Option<crate::domain::SshTargetConfig> {
    let cfg = ssh.as_ref()?;
    let host = cfg.host.trim().to_string();
    if host.is_empty() {
        return None;
    }
    Some(crate::domain::SshTargetConfig {
        host,
        user: normalize_optional_dir(&cfg.user),
        port: cfg.port.filter(|&p| p > 0),
        identity_file: normalize_optional_dir(&cfg.identity_file),
        remote_app_preset: normalize_optional_dir(&cfg.remote_app_preset),
        global_plugin_dir: normalize_optional_dir(&cfg.global_plugin_dir),
        inject_secrets: cfg.inject_secrets,
    })
}

pub(crate) fn normalize_custom_apps(apps: &[CustomAppConfig]) -> Vec<CustomAppConfig> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for item in apps {
        let id = item.id.trim().to_string();
        let label = item.label.trim().to_string();
        let global_skill_dir = item.global_skill_dir.trim().to_string();
        let ssh = normalize_ssh_config(&item.ssh);
        if id.is_empty()
            || label.is_empty()
            || global_skill_dir.is_empty()
            || BUILTIN_APP_IDS.contains(&id.as_str())
            || (item.ssh.is_some() && ssh.is_none())
            || !seen.insert(id.clone())
        {
            continue;
        }
        let accent_color = if item.accent_color.trim().is_empty() {
            "#6366f1".to_string()
        } else {
            item.accent_color.trim().to_string()
        };
        let is_ssh = ssh.is_some();
        normalized.push(CustomAppConfig {
            id,
            label,
            accent_color,
            global_skill_dir,
            project_skill_dir: if is_ssh {
                None
            } else {
                normalize_optional_dir(&item.project_skill_dir)
            },
            global_command_dir: normalize_optional_dir(&item.global_command_dir),
            project_command_dir: if is_ssh {
                None
            } else {
                normalize_optional_dir(&item.project_command_dir)
            },
            global_mcp_config_path: normalize_optional_dir(&item.global_mcp_config_path),
            project_mcp_config_path: if is_ssh {
                None
            } else {
                normalize_optional_dir(&item.project_mcp_config_path)
            },
            mcp_format: normalize_optional_dir(&item.mcp_format),
            mcp_root_key: normalize_optional_dir(&item.mcp_root_key),
            ssh,
        });
    }
    normalized
}

pub(crate) fn dedupe_string_list(items: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in items {
        let trimmed = raw.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

pub(crate) fn load_settings(app: &tauri::AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(AppSettings {
            backup_path: Some(resolved_backup_path(app)?),
            backup_source: None,
            third_party_repos: crate::repo_sources::default_repos(app)?,
            ..Default::default()
        });
    }

    let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut settings: AppSettings =
        serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    if settings.third_party_repos.is_empty() {
        settings.third_party_repos = crate::repo_sources::default_repos(app)?;
    } else {
        settings.third_party_repos =
            crate::repo_sources::normalize_repo_list(app, &settings.third_party_repos)?;
    }
    settings.custom_apps = normalize_custom_apps(&settings.custom_apps);
    settings.hidden_app_ids = dedupe_string_list(&settings.hidden_app_ids);
    settings.default_install_app_ids = dedupe_string_list(&settings.default_install_app_ids);
    settings.backup_path = Some(resolved_backup_path(app)?);
    Ok(settings)
}

pub(crate) fn save_settings(app: &tauri::AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut next_settings = settings.clone();
    next_settings.backup_path = Some(resolved_backup_path(app)?);
    next_settings.backup_source = next_settings
        .backup_source
        .as_ref()
        .map(|source| normalize_backup_source(app, source))
        .transpose()?;
    next_settings.third_party_repos =
        crate::repo_sources::normalize_repo_list(app, &next_settings.third_party_repos)?;
    next_settings.custom_apps = normalize_custom_apps(&next_settings.custom_apps);
    next_settings.hidden_app_ids = dedupe_string_list(&next_settings.hidden_app_ids);
    next_settings.default_install_app_ids =
        dedupe_string_list(&next_settings.default_install_app_ids);

    let content =
        serde_json::to_string_pretty(&next_settings).map_err(|error| error.to_string())?;
    write_file_atomic(&path, content)
}

// ─── Settings functions ────────────────────────────────────────────────────────

pub fn get_settings(app: &tauri::AppHandle) -> Result<AppSettings, String> {
    let mut settings = load_settings(app)?;
    settings.backup_source = settings
        .backup_source
        .as_ref()
        .map(|source| normalize_backup_source(app, source))
        .transpose()?;
    Ok(settings)
}

pub fn apply_theme_preference(app: &tauri::AppHandle, theme: &str) {
    let theme = match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };

    app.set_theme(theme);
}

pub fn set_settings(app: &tauri::AppHandle, settings: &AppSettings) -> Result<(), String> {
    save_settings(app, settings)
}
