use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;
use uuid::Uuid;

use crate::domain::{
    AppSettings, BackupSourceConfig, BackupSourceStatus, BackupSyncResult,
    CheckSlashCommandSymlinkInput, CheckSlashCommandSymlinkResult, CheckSymlinkStatusInput,
    CheckSymlinkStatusResult, CreateLegacySkillResult, CreateProjectInput, CreateSkillInput,
    CreateSlashCommandInput, CreateSlashCommandResult, CustomAppConfig, InstallRecord,
    InstallSkillGlobalInput, InstallSkillGlobalResult, InstallSkillToProjectInput,
    InstallSkillToProjectResult, InstallSlashCommandGlobalInput, InstallSlashCommandGlobalResult,
    InstallSlashCommandToProjectInput, InstallSlashCommandToProjectResult, InstallStatus,
    InstallTarget, InstallTargetKind, LegacyProjectDto, LegacySkillDto, LibraryExportResult,
    LibraryImportResult, LocalProjectBinding, LocalState, PreviewDecisionAction, PreviewPlan,
    PreviewPlanKind, ProjectFileEntry, ProjectFileStatus, ProjectPreviewInput, ProjectProfile,
    ProjectScanResult, ProjectScanSummary, Provenance, ProvenanceKind, RecoveryEntry,
    RecoveryScanResult, RemoveProjectCliInput, RemoveProjectCliResult, RepairBrokenSymlinksResult,
    RepoConfig, RepoLibrary, RepoStatus, Resource, ResourceKind, ResourceListFilter,
    ResourceOrigin, ResourceScope, SkillMutationResult, SkillSymlinkStatus, SlashCommandDto,
    SourceStatus, SyncStatus, UpdateItem, UpdateProjectInput, UpdateSkillInput,
    UpdateSlashCommandInput,
};
use crate::git;
use crate::legacy;

mod backup;
mod browse;
mod crud;
mod import_export;
mod install;
mod settings;
mod sources;

pub use backup::*;
pub use browse::*;
pub use crud::*;
pub use import_export::*;
pub use install::*;
pub use settings::*;
pub use sources::*;

const LOCAL_STATE_FILE: &str = "local-state.json";
const LEGACY_STORE_FILE: &str = "skills.json";
const LIBRARY_DIR: &str = ".skill-switch";
const LIBRARY_FILE: &str = "library.json";
const BACKUP_SOURCE_DIR: &str = "backup-source";
const SKILL_SOURCES_DIR: &str = "skill-sources";
const COMMAND_SOURCES_DIR: &str = "command-sources";
pub(crate) const PLUGIN_SOURCES_DIR: &str = "plugin-sources";
const DEFAULT_LIBRARY_REPO_DIR: &str = "library-repo";
const STANDARD_SKILL_DIRECTORIES: &[&str] = &["scripts", "references", "assets"];

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in value.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            _ => Some('-'),
        };

        if let Some(mapped) = mapped {
            if mapped == '-' {
                if !slug.is_empty() && !prev_dash {
                    slug.push(mapped);
                    prev_dash = true;
                }
            } else {
                slug.push(mapped);
                prev_dash = false;
            }
        }
    }

    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        format!("resource-{}", now_ms())
    } else {
        trimmed
    }
}

pub fn compute_revision(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{:02x}", byte)).collect()
}

pub fn normalize_user_path(path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    if candidate.exists() {
        return candidate.canonicalize().map_err(|error| error.to_string());
    }

    if candidate.is_absolute() {
        Ok(candidate)
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())
            .map(|cwd| cwd.join(candidate))
    }
}

pub(crate) const BUILTIN_APP_IDS: &[&str] = &[
    "claude",
    "codex",
    "cursor",
    "antigravity",
    "gemini",
    "windsurf",
    "t3",
];

fn app_cli_dir_aliases(app_id: &str) -> Option<&'static [&'static str]> {
    match app_id {
        "claude" => Some(&[".claude"]),
        "codex" => Some(&[".codex"]),
        "cursor" => Some(&[".cursor"]),
        "antigravity" => Some(&[".agents", ".agent"]),
        "gemini" => Some(&[".gemini"]),
        "windsurf" => Some(&[".windsurf"]),
        "t3" => Some(&[".agents"]),
        _ => None,
    }
}

fn app_cli_dirs(base_path: &Path, app_id: &str) -> Result<Vec<PathBuf>, String> {
    let aliases = app_cli_dir_aliases(app_id).ok_or_else(|| format!("未知的应用: {}", app_id))?;
    Ok(aliases.iter().map(|alias| base_path.join(alias)).collect())
}

fn preferred_app_cli_dir(base_path: &Path, app_id: &str) -> Result<PathBuf, String> {
    let dirs = app_cli_dirs(base_path, app_id)?;
    dirs.into_iter()
        .next()
        .ok_or_else(|| format!("未知的应用: {}", app_id))
}

fn app_skill_dirs(base_path: &Path, app_id: &str) -> Result<Vec<PathBuf>, String> {
    Ok(app_cli_dirs(base_path, app_id)?
        .into_iter()
        .map(|dir| dir.join("skills"))
        .collect())
}

pub(crate) fn expand_home_dir(home: &Path, raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        return home.join(rest);
    }
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        home.join(candidate)
    }
}

pub(crate) fn resolve_project_subpath(project_root: &Path, raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("项目相对路径不能为空".to_string());
    }
    let cleaned = trimmed
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .trim_start_matches('/')
        .trim_start_matches('\\');
    if cleaned.is_empty() {
        return Err("项目相对路径无效".to_string());
    }
    let rel_path = Path::new(cleaned);
    for component in rel_path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(format!("项目相对路径不允许越界: {}", raw));
        }
    }
    Ok(project_root.join(rel_path))
}

pub(crate) fn load_custom_apps(app: &tauri::AppHandle) -> Vec<CustomAppConfig> {
    load_settings(app)
        .map(|settings| settings.custom_apps)
        .unwrap_or_default()
}

pub(crate) fn all_configured_app_ids(custom_apps: &[CustomAppConfig]) -> Vec<String> {
    let mut ids: Vec<String> = BUILTIN_APP_IDS.iter().map(|id| (*id).to_string()).collect();
    for custom in custom_apps {
        if !ids.iter().any(|existing| existing == &custom.id) {
            ids.push(custom.id.clone());
        }
    }
    ids
}

pub(crate) fn global_skill_dirs_for_app(
    home: &Path,
    app_id: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    match app_id {
        "claude" => Ok(vec![home.join(".claude").join("skills")]),
        "codex" => Ok(vec![home.join(".codex").join("skills")]),
        "cursor" => Ok(vec![home.join(".cursor").join("skills")]),
        "antigravity" => Ok(vec![
            home.join(".gemini").join("config").join("skills"),
            home.join(".gemini").join("antigravity").join("skills"),
        ]),
        "gemini" => Ok(vec![home.join(".gemini").join("skills")]),
        "windsurf" => Ok(vec![
            home.join(".codeium").join("windsurf").join("skills"),
            home.join(".windsurf").join("skills"),
        ]),
        "t3" => Ok(vec![home.join(".agents").join("skills")]),
        _ => {
            let custom = custom_apps
                .iter()
                .find(|item| item.id == app_id)
                .ok_or_else(|| format!("未知的应用: {}", app_id))?;
            if crate::ssh_sync::is_ssh_app(app_id, custom_apps) {
                return Ok(Vec::new());
            }
            Ok(vec![expand_home_dir(home, &custom.global_skill_dir)])
        }
    }
}

pub(crate) fn project_skill_dirs_for_app(
    project_root: &Path,
    app_id: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    match app_id {
        "claude" => Ok(vec![project_root.join(".claude").join("skills")]),
        "codex" => Ok(vec![project_root.join(".codex").join("skills")]),
        "cursor" => Ok(vec![project_root.join(".cursor").join("skills")]),
        "antigravity" => Ok(vec![
            project_root.join(".agents").join("skills"),
            project_root.join(".agent").join("skills"),
        ]),
        "gemini" => Ok(vec![project_root.join(".gemini").join("skills")]),
        "windsurf" => Ok(vec![project_root.join(".windsurf").join("skills")]),
        "t3" => Ok(vec![project_root.join(".agents").join("skills")]),
        _ => {
            let custom = custom_apps
                .iter()
                .find(|item| item.id == app_id)
                .ok_or_else(|| format!("未知的应用: {}", app_id))?;
            if crate::ssh_sync::is_ssh_app(app_id, custom_apps) {
                return Err(format!(
                    "SSH 远程目标端 {} 不支持本地项目级目录挂载",
                    custom.label
                ));
            }
            let rel = custom
                .project_skill_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| format!("应用 {} 未配置项目级 Skills 目录", custom.label))?;
            Ok(vec![resolve_project_subpath(project_root, rel)?])
        }
    }
}

fn preferred_global_skill_path(
    home: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<PathBuf, String> {
    global_skill_dirs_for_app(home, app_id, custom_apps)?
        .into_iter()
        .next()
        .map(|dir| dir.join(slug))
        .ok_or_else(|| format!("未知的应用: {}", app_id))
}

fn global_skill_paths(
    home: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    Ok(global_skill_dirs_for_app(home, app_id, custom_apps)?
        .into_iter()
        .map(|dir| dir.join(slug))
        .collect())
}

fn preferred_project_skill_path(
    project_root: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<PathBuf, String> {
    project_skill_dirs_for_app(project_root, app_id, custom_apps)?
        .into_iter()
        .next()
        .map(|dir| dir.join(slug))
        .ok_or_else(|| format!("未知的应用: {}", app_id))
}

fn project_skill_paths(
    project_root: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    Ok(
        project_skill_dirs_for_app(project_root, app_id, custom_apps)?
            .into_iter()
            .map(|dir| dir.join(slug))
            .collect(),
    )
}

pub(crate) fn global_command_dirs_for_app(
    home: &Path,
    app_id: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    match app_id {
        "claude" => Ok(vec![home.join(".claude").join("commands")]),
        "codex" => Ok(vec![home.join(".codex").join("commands")]),
        "cursor" => Ok(vec![home.join(".cursor").join("commands")]),
        "gemini" => Ok(vec![home.join(".gemini").join("commands")]),
        "windsurf" => Ok(vec![
            home.join(".codeium").join("windsurf").join("workflows"),
            home.join(".windsurf").join("workflows"),
        ]),
        "antigravity" | "t3" => Ok(Vec::new()),
        _ => {
            let custom = custom_apps
                .iter()
                .find(|item| item.id == app_id)
                .ok_or_else(|| format!("未知的应用: {}", app_id))?;
            if crate::ssh_sync::is_ssh_app(app_id, custom_apps) {
                return Ok(Vec::new());
            }
            match custom
                .global_command_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                Some(dir) => Ok(vec![expand_home_dir(home, dir)]),
                None => Ok(Vec::new()),
            }
        }
    }
}

pub(crate) fn project_command_dirs_for_app(
    project_root: &Path,
    app_id: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    match app_id {
        "claude" => Ok(vec![project_root.join(".claude").join("commands")]),
        "codex" => Ok(vec![project_root.join(".codex").join("commands")]),
        "cursor" => Ok(vec![project_root.join(".cursor").join("commands")]),
        "gemini" => Ok(vec![project_root.join(".gemini").join("commands")]),
        "windsurf" => Ok(vec![project_root.join(".windsurf").join("workflows")]),
        "antigravity" | "t3" => Ok(Vec::new()),
        _ => {
            let custom = custom_apps
                .iter()
                .find(|item| item.id == app_id)
                .ok_or_else(|| format!("未知的应用: {}", app_id))?;
            if crate::ssh_sync::is_ssh_app(app_id, custom_apps) {
                return Ok(Vec::new());
            }
            match custom
                .project_command_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                Some(rel) => Ok(vec![resolve_project_subpath(project_root, rel)?]),
                None => Ok(Vec::new()),
            }
        }
    }
}

fn preferred_global_command_path(
    home: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<PathBuf, String> {
    global_command_dirs_for_app(home, app_id, custom_apps)?
        .into_iter()
        .next()
        .map(|dir| dir.join(slug))
        .ok_or_else(|| format!("应用 {} 未配置全局 Commands 目录", app_id))
}

fn global_command_paths(
    home: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    Ok(global_command_dirs_for_app(home, app_id, custom_apps)?
        .into_iter()
        .map(|dir| dir.join(slug))
        .collect())
}

fn preferred_project_command_path(
    project_root: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<PathBuf, String> {
    project_command_dirs_for_app(project_root, app_id, custom_apps)?
        .into_iter()
        .next()
        .map(|dir| dir.join(slug))
        .ok_or_else(|| format!("应用 {} 未配置项目级 Commands 目录", app_id))
}

fn project_command_paths(
    project_root: &Path,
    app_id: &str,
    slug: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Vec<PathBuf>, String> {
    Ok(
        project_command_dirs_for_app(project_root, app_id, custom_apps)?
            .into_iter()
            .map(|dir| dir.join(slug))
            .collect(),
    )
}

fn local_state_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(LOCAL_STATE_FILE))
}

pub fn legacy_store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(LEGACY_STORE_FILE))
}

pub fn repo_manifest_path(repo_root: &Path) -> PathBuf {
    repo_root.join(LIBRARY_DIR).join(LIBRARY_FILE)
}

pub fn load_local_state(app: &tauri::AppHandle) -> Result<LocalState, String> {
    let path = local_state_path(app)?;
    if !path.exists() {
        return Ok(LocalState {
            version: "2".into(),
            ..Default::default()
        });
    }

    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

/// Serializes every state-mutating operation (library/local-state read-modify-write,
/// symlink changes, backup git operations). Not reentrant: acquire it only at the
/// command boundary, never from inside store/business functions called under it.
static MUTATION_LOCK: Mutex<()> = Mutex::new(());

pub fn with_mutation_lock<T>(task: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _guard = MUTATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    task()
}

/// Writes `contents` to a sibling temp file and renames it over `path`, so a crash
/// mid-write never leaves a truncated file behind.
pub fn write_file_atomic(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    use std::io::Write;

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("invalid file path: {}", path.display()))?;
    let tmp_path = path.with_file_name(format!(".{}.{}.tmp", file_name, std::process::id()));

    let result = (|| {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        fs::rename(&tmp_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result.map_err(|error| format!("写入文件失败 {}: {}", path.display(), error))
}

pub fn save_local_state(app: &tauri::AppHandle, state: &LocalState) -> Result<(), String> {
    let path = local_state_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let raw = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
    write_file_atomic(&path, raw)
}

pub fn load_repo_library(repo_root: &Path) -> Result<RepoLibrary, String> {
    let path = repo_manifest_path(repo_root);
    if !path.exists() {
        return Ok(RepoLibrary {
            version: "2".into(),
            ..Default::default()
        });
    }

    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

pub fn save_repo_library(repo_root: &Path, library: &RepoLibrary) -> Result<(), String> {
    let path = repo_manifest_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let raw = serde_json::to_string_pretty(library).map_err(|error| error.to_string())?;
    write_file_atomic(&path, raw)
}

pub fn connected_repo_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    // Try persistent clone first (when backup source is configured)
    if let Some(clone_dir) = persistent_clone_dir(app)? {
        if clone_dir.exists() {
            // Ensure .skill-switch/library.json exists
            let manifest = repo_manifest_path(&clone_dir);
            if !manifest.exists() {
                if let Some(parent) = manifest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let library = RepoLibrary {
                    version: "2".into(),
                    ..Default::default()
                };
                save_repo_library(&clone_dir, &library)?;
            }
            return Ok(clone_dir);
        }
    }
    // Fallback to old library-repo
    let mut state = load_local_state(app)?;
    ensure_local_library_repo_root(app, &mut state)
}

pub fn ensure_repo_connection(
    app: &tauri::AppHandle,
    path: &str,
    remote_url: Option<&str>,
    branch: Option<&str>,
    initialize_if_missing: bool,
) -> Result<RepoStatus, String> {
    let normalized = normalize_user_path(path)?;

    if !normalized.exists() {
        if let Some(remote_url) = remote_url {
            if !git::git_available() {
                return Err("git is not available on this machine".into());
            }
            if let Some(parent) = normalized.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            git::clone_repository(remote_url, &normalized, branch)?;
        } else if !initialize_if_missing {
            return Err(format!(
                "repo path does not exist: {}",
                normalized.display()
            ));
        } else {
            fs::create_dir_all(&normalized).map_err(|error| error.to_string())?;
        }
    }

    if !normalized.is_dir() {
        return Err(format!(
            "repo path is not a directory: {}",
            normalized.display()
        ));
    }

    let mut local_state = load_local_state(app)?;
    local_state.repo = Some(RepoConfig {
        path: normalized.to_string_lossy().into_owned(),
        connected_at: now_ms(),
    });

    let mut library = load_repo_library(&normalized)?;
    let mut migration = None;

    if !local_state.migrated_legacy_store {
        if let Some(legacy_store) = legacy::read_legacy_store(&legacy_store_path(app)?)? {
            let report = legacy::migrate_legacy_store(
                legacy_store,
                &mut library,
                &mut local_state.project_bindings,
            );
            if report.migrated {
                local_state.migrated_legacy_store = true;
                migration = Some(report);
            }
        }
    }

    save_repo_library(&normalized, &library)?;
    save_local_state(app, &local_state)?;

    build_repo_status_from_state(&normalized, &library, migration)
}

pub fn build_repo_status(app: &tauri::AppHandle) -> Result<RepoStatus, String> {
    let mut local_state = load_local_state(app)?;
    let repo_root = ensure_local_library_repo_root(app, &mut local_state)?;
    let library = load_repo_library(&repo_root)?;
    build_repo_status_from_state(&repo_root, &library, None)
}

fn build_repo_status_from_state(
    repo_root: &Path,
    library: &RepoLibrary,
    migration: Option<crate::domain::MigrationReport>,
) -> Result<RepoStatus, String> {
    let (ahead, behind) = git::ahead_behind(repo_root);
    Ok(RepoStatus {
        connected: true,
        path: Some(repo_root.to_string_lossy().into_owned()),
        manifest_exists: repo_manifest_path(repo_root).exists(),
        git_available: git::git_available(),
        is_git_repo: git::is_git_repo(repo_root),
        branch: git::branch(repo_root),
        head: git::head(repo_root),
        ahead,
        behind,
        dirty: git::dirty(repo_root),
        resources_count: library.resources.len(),
        project_profiles_count: library.project_profiles.len(),
        migration,
    })
}

pub fn list_resources(
    app: &tauri::AppHandle,
    filter: Option<ResourceListFilter>,
) -> Result<Vec<Resource>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let filtered = library
        .resources
        .into_iter()
        .filter(|resource| {
            filter
                .as_ref()
                .and_then(|filter| filter.kind)
                .map(|kind| resource.kind == kind)
                .unwrap_or(true)
                && filter
                    .as_ref()
                    .and_then(|filter| filter.scope)
                    .map(|scope| resource.scope == scope)
                    .unwrap_or(true)
                && filter
                    .as_ref()
                    .and_then(|filter| filter.project_id.as_ref())
                    .map(|project_id| resource.project_id.as_deref() == Some(project_id.as_str()))
                    .unwrap_or(true)
        })
        .collect();

    Ok(filtered)
}

pub fn list_project_profiles(app: &tauri::AppHandle) -> Result<Vec<ProjectProfile>, String> {
    let repo_root = connected_repo_root(app)?;
    Ok(load_repo_library(&repo_root)?.project_profiles)
}

pub fn list_project_bindings(app: &tauri::AppHandle) -> Result<Vec<LocalProjectBinding>, String> {
    Ok(load_local_state(app)?.project_bindings)
}

pub fn scan_project_state(
    app: &tauri::AppHandle,
    project_path: &str,
) -> Result<ProjectScanResult, String> {
    let normalized = normalize_user_path(project_path)?;
    if !normalized.exists() || !normalized.is_dir() {
        return Err(format!("project path is invalid: {}", normalized.display()));
    }

    let local_state = load_local_state(app)?;
    let library = match connected_repo_root(app) {
        Ok(repo_root) => load_repo_library(&repo_root)?,
        Err(_) => RepoLibrary {
            version: "2".into(),
            ..Default::default()
        },
    };

    let binding = local_state
        .project_bindings
        .iter()
        .find(|binding| normalize_user_path(&binding.path).ok().as_ref() == Some(&normalized))
        .cloned();

    let profile = binding
        .as_ref()
        .and_then(|binding| {
            library
                .project_profiles
                .iter()
                .find(|profile| profile.id == binding.project_id)
        })
        .cloned();

    let items = scan_files(&normalized, &local_state.install_records)?;
    Ok(ProjectScanResult {
        project_path: normalized.to_string_lossy().into_owned(),
        repo_root: git::git_root(&normalized).map(|path| path.to_string_lossy().into_owned()),
        is_git_repo: git::is_git_repo(&normalized),
        profile,
        binding,
        summary: summarize_items(&items),
        items,
    })
}

pub fn preview_project_apply(
    app: &tauri::AppHandle,
    input: &ProjectPreviewInput,
) -> Result<crate::domain::PreviewPlan, String> {
    let normalized = normalize_user_path(&input.path)?;
    let local_state = load_local_state(app)?;
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;

    let profile = library
        .project_profiles
        .iter()
        .find(|profile| profile.id == input.project_id)
        .cloned()
        .ok_or_else(|| format!("project profile {} not found", input.project_id))?;

    let resources = expected_project_resources(&library, &profile);
    let mut items = Vec::new();
    for resource in resources {
        if let Some(item) = build_apply_item(&normalized, &resource, &local_state.install_records)?
        {
            items.push(item);
        }
    }

    Ok(crate::domain::PreviewPlan {
        kind: crate::domain::PreviewPlanKind::Apply,
        project_id: profile.id,
        project_path: normalized.to_string_lossy().into_owned(),
        summary: summarize_items(&items),
        items,
    })
}

pub fn preview_capture_project_changes(
    app: &tauri::AppHandle,
    input: &ProjectPreviewInput,
) -> Result<crate::domain::PreviewPlan, String> {
    let normalized = normalize_user_path(&input.path)?;
    let local_state = load_local_state(app)?;
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;

    let profile = library
        .project_profiles
        .iter()
        .find(|profile| profile.id == input.project_id)
        .cloned()
        .ok_or_else(|| format!("project profile {} not found", input.project_id))?;

    let mut items = scan_files(&normalized, &local_state.install_records)?;

    let expected_resources = expected_project_resources(&library, &profile);
    for resource in expected_resources {
        let (absolute_path, relative_path) = project_target_path(&normalized, &resource);
        if absolute_path.exists() {
            continue;
        }

        items.push(ProjectFileEntry {
            relative_path,
            absolute_path: absolute_path.to_string_lossy().into_owned(),
            status: ProjectFileStatus::Missing,
            resource_id: Some(resource.id),
            install_record_id: local_state
                .install_records
                .iter()
                .find(|record| record.target.path == absolute_path.to_string_lossy())
                .map(|record| record.id.clone()),
            tracked: false,
            current_revision: None,
            expected_revision: Some(resource.revision),
            note: Some("expected by project profile but missing in project".into()),
        });
    }

    Ok(crate::domain::PreviewPlan {
        kind: crate::domain::PreviewPlanKind::Capture,
        project_id: profile.id,
        project_path: normalized.to_string_lossy().into_owned(),
        summary: summarize_items(&items),
        items,
    })
}

pub fn list_legacy_skills(app: &tauri::AppHandle) -> Result<Vec<LegacySkillDto>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    Ok(library
        .resources
        .iter()
        .filter(|resource| resource.kind == ResourceKind::Skill)
        .map(|resource| resource_to_legacy_skill(resource, &library))
        .collect())
}

pub fn get_legacy_skill(
    app: &tauri::AppHandle,
    id: &str,
) -> Result<Option<LegacySkillDto>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    Ok(library
        .resources
        .iter()
        .find(|resource| resource.kind == ResourceKind::Skill && resource.id == id)
        .map(|resource| resource_to_legacy_skill(resource, &library)))
}

pub fn list_slash_commands(app: &tauri::AppHandle) -> Result<Vec<SlashCommandDto>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    Ok(library
        .resources
        .iter()
        .filter(|resource| resource.kind == ResourceKind::SlashCommand)
        .map(|resource| resource_to_slash_command(resource, &library))
        .collect())
}

pub fn get_slash_command(
    app: &tauri::AppHandle,
    id: &str,
) -> Result<Option<SlashCommandDto>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    Ok(library
        .resources
        .iter()
        .find(|resource| resource.kind == ResourceKind::SlashCommand && resource.id == id)
        .map(|resource| resource_to_slash_command(resource, &library)))
}

fn is_managed_skill_resource(resource: &Resource) -> bool {
    resource.kind == ResourceKind::Skill
        && resource.scope == ResourceScope::Global
        && resource.project_id.is_none()
}

fn is_managed_slash_command_resource(resource: &Resource) -> bool {
    resource.kind == ResourceKind::SlashCommand
        && resource.scope == ResourceScope::Global
        && resource.project_id.is_none()
}

fn load_repo_library_for_legacy_skills(
    _app: &tauri::AppHandle,
    repo_root: &Path,
) -> Result<RepoLibrary, String> {
    load_repo_library(repo_root)
}

fn reconcile_managed_skills_from_backup_clone(clone_dir: &Path) -> Result<bool, String> {
    if !clone_dir.exists() || !clone_dir.is_dir() {
        return Ok(false);
    }

    let mut library = load_repo_library(clone_dir)?;
    let mut source_skills = Vec::new();

    for entry in fs::read_dir(clone_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let slug = entry.file_name().to_string_lossy().into_owned();
        if slug.starts_with('.') || slug == COMMAND_SOURCES_DIR || slug == PLUGIN_SOURCES_DIR {
            continue;
        }

        let skill_file = path.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }

        let content = match fs::read_to_string(&skill_file) {
            Ok(content) => content,
            Err(_) => continue,
        };

        source_skills.push((slug, content));
    }

    let now = now_ms();
    let mut changed = false;
    let mut seen_slugs = HashSet::new();

    for (slug, content) in source_skills {
        seen_slugs.insert(slug.clone());

        if let Some(resource) = library
            .resources
            .iter_mut()
            .find(|resource| is_managed_skill_resource(resource) && resource.slug == slug)
        {
            let (next_name, next_description, next_tags) =
                derive_skill_source_metadata(&slug, &content, Some(resource.tags.as_slice()));
            let next_description = next_description.or_else(|| resource.description.clone());
            let next_revision = compute_revision(&content);

            if resource.title != next_name
                || resource.description != next_description
                || resource.content != content
                || resource.tags != next_tags
                || resource.revision != next_revision
            {
                resource.title = next_name;
                resource.description = next_description;
                resource.content = content;
                resource.tags = next_tags;
                resource.revision = next_revision;
                resource.updated_at = now;
                changed = true;
            }

            continue;
        }

        let (name, description, tags) = derive_skill_source_metadata(&slug, &content, None);
        library.resources.push(Resource {
            id: Uuid::new_v4().to_string(),
            slug,
            title: name,
            description,
            kind: ResourceKind::Skill,
            scope: ResourceScope::Global,
            origin: ResourceOrigin::Private,
            source_status: SourceStatus::LocalOnly,
            project_id: None,
            tags,
            revision: compute_revision(&content),
            content,
            source_url: None,
            source_ref: None,
            source_path: None,
            upstream_revision: None,
            forked_from: None,
            created_at: now,
            updated_at: now,
            provenance: Default::default(),
        });
        changed = true;
    }

    let removed_ids: Vec<String> = library
        .resources
        .iter()
        .filter(|resource| {
            is_managed_skill_resource(resource) && !seen_slugs.contains(&resource.slug)
        })
        .map(|resource| resource.id.clone())
        .collect();

    if !removed_ids.is_empty() {
        let removed_ids: HashSet<String> = removed_ids.into_iter().collect();
        library
            .resources
            .retain(|resource| !removed_ids.contains(&resource.id));
        for resource_id in &removed_ids {
            detach_resource_from_all_profiles(&mut library.project_profiles, resource_id);
        }
        changed = true;
    }

    if changed {
        save_repo_library(clone_dir, &library)?;
    }

    Ok(changed)
}

pub(crate) fn parse_skill_front_matter(
    content: &str,
) -> (Option<String>, Option<String>, Vec<String>) {
    let normalized = content.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return (None, None, vec![]);
    };

    let Some((front_matter, _)) = rest
        .split_once("\n---\n")
        .or_else(|| rest.strip_suffix("\n---").map(|fm| (fm, "")))
    else {
        return (None, None, vec![]);
    };

    let mut name = None;
    let mut description = None;
    let mut tags = Vec::new();

    let lines: Vec<&str> = front_matter.lines().collect();
    let mut idx = 0;

    while idx < lines.len() {
        let line = lines[idx];

        if let Some(raw_val) = line.strip_prefix("name:") {
            let cleaned = raw_val.trim().trim_matches('"').trim_matches('\'').trim();
            if !cleaned.is_empty() {
                name = Some(cleaned.to_string());
            }
            idx += 1;
            continue;
        }

        if let Some(raw_val) = line.strip_prefix("description:") {
            let trimmed = raw_val.trim();
            if trimmed == ">"
                || trimmed == ">-"
                || trimmed == "|+"
                || trimmed == "|-"
                || trimmed == "|"
                || trimmed.is_empty()
            {
                let preserve_newlines = trimmed.starts_with('|');
                let mut block_lines = Vec::new();
                idx += 1;
                while idx < lines.len() {
                    let next_line = lines[idx];
                    if next_line.starts_with(' ') || next_line.starts_with('\t') {
                        block_lines.push(next_line.trim());
                        idx += 1;
                    } else if next_line.trim().is_empty() {
                        if preserve_newlines && !block_lines.is_empty() {
                            block_lines.push("");
                        }
                        idx += 1;
                    } else {
                        break;
                    }
                }
                let joined = if preserve_newlines {
                    block_lines.join("\n").trim().to_string()
                } else {
                    block_lines
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                if !joined.is_empty() {
                    description = Some(joined);
                }
                continue;
            } else {
                let cleaned = trimmed.trim_matches('"').trim_matches('\'').trim();
                if !cleaned.is_empty() {
                    description = Some(cleaned.to_string());
                }
                idx += 1;
                continue;
            }
        }

        if let Some(raw_val) = line.strip_prefix("tags:") {
            let trimmed = raw_val.trim();
            if trimmed.is_empty() {
                idx += 1;
                while idx < lines.len() {
                    let next_line = lines[idx].trim();
                    if let Some(item) = next_line.strip_prefix('-') {
                        let cleaned = item.trim().trim_matches('"').trim_matches('\'').trim();
                        if !cleaned.is_empty() {
                            tags.push(cleaned.to_string());
                        }
                        idx += 1;
                    } else if next_line.is_empty() {
                        idx += 1;
                    } else {
                        break;
                    }
                }
                continue;
            } else {
                let inner = trimmed.trim_start_matches('[').trim_end_matches(']');
                if !inner.is_empty() {
                    tags = inner
                        .split(',')
                        .map(|tag| tag.trim().trim_matches('"').trim_matches('\''))
                        .filter(|tag| !tag.is_empty())
                        .map(|tag| tag.to_string())
                        .collect();
                }
                idx += 1;
                continue;
            }
        }

        idx += 1;
    }

    (name, description, tags)
}

fn derive_skill_source_metadata(
    slug: &str,
    content: &str,
    existing_tags: Option<&[String]>,
) -> (String, Option<String>, Vec<String>) {
    let (front_name, front_description, front_tags) = parse_skill_front_matter(content);
    let (parsed_name, parsed_description) = parse_skill_metadata(content);

    let name = front_name
        .or_else(|| (!parsed_name.is_empty()).then_some(parsed_name))
        .unwrap_or_else(|| slug.to_string());
    let description = front_description.or(parsed_description);
    let tags = if front_tags.is_empty() {
        existing_tags.map(|tags| tags.to_vec()).unwrap_or_default()
    } else {
        front_tags
    };

    (name, description, tags)
}

fn default_library_repo_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(DEFAULT_LIBRARY_REPO_DIR))
}

fn ensure_local_library_repo_root(
    app: &tauri::AppHandle,
    local_state: &mut LocalState,
) -> Result<PathBuf, String> {
    let default_root = default_library_repo_root(app)?;
    let backup_root = skill_sources_dir(app)?;

    if let Some(repo) = local_state.repo.as_ref() {
        let normalized = normalize_user_path(&repo.path)?;
        if normalized != backup_root {
            return Ok(normalized);
        }

        let library = load_repo_library(&normalized)?;
        save_repo_library(&default_root, &library)?;
        local_state.repo = Some(RepoConfig {
            path: default_root.to_string_lossy().into_owned(),
            connected_at: now_ms(),
        });
        save_local_state(app, local_state)?;
        let embedded_library_dir = backup_root.join(LIBRARY_DIR);
        if embedded_library_dir.exists() {
            remove_path(&embedded_library_dir)?;
        }
        let git_dir = backup_root.join(".git");
        if git_dir.exists() {
            remove_path(&git_dir)?;
        }
        return Ok(default_root);
    }

    let library = load_repo_library(&default_root)?;
    save_repo_library(&default_root, &library)?;
    local_state.repo = Some(RepoConfig {
        path: default_root.to_string_lossy().into_owned(),
        connected_at: now_ms(),
    });
    save_local_state(app, local_state)?;
    Ok(default_root)
}

/// Migrate skill source directories from old skill-sources to the persistent clone.
fn migrate_skill_sources_to_clone(old_sources: &Path, clone_dir: &Path) -> Result<(), String> {
    if !old_sources.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(old_sources).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let slug = entry.file_name().to_string_lossy().into_owned();
        if slug.starts_with('.') || !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }
        let target = clone_dir.join(&slug);
        if !target.exists() {
            copy_dir_all(&path, &target)?;
        }
    }
    Ok(())
}

pub fn list_legacy_projects(app: &tauri::AppHandle) -> Result<Vec<LegacyProjectDto>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let state = load_local_state(app)?;
    let binding_map = state
        .project_bindings
        .iter()
        .map(|binding| (binding.project_id.clone(), binding.path.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    Ok(library
        .project_profiles
        .iter()
        .map(|profile| LegacyProjectDto {
            id: profile.id.clone(),
            name: profile.name.clone(),
            path: binding_map.get(&profile.id).cloned(),
            color: profile.color.clone(),
            created_at: profile.created_at,
            updated_at: profile.updated_at,
        })
        .collect())
}

pub fn create_legacy_project(
    app: &tauri::AppHandle,
    input: &CreateProjectInput,
) -> Result<LegacyProjectDto, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    let mut state = load_local_state(app)?;
    let now = now_ms();
    let profile = ProjectProfile {
        id: Uuid::new_v4().to_string(),
        slug: slugify(&input.name),
        name: input.name.clone(),
        description: None,
        color: input.color.clone(),
        agents_resource_id: None,
        attached_resource_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    if let Some(path) = &input.path {
        upsert_project_binding(&mut state.project_bindings, &profile.id, path);
    }
    let result = LegacyProjectDto {
        id: profile.id.clone(),
        name: profile.name.clone(),
        path: input.path.clone(),
        color: profile.color.clone(),
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    };
    library.project_profiles.push(profile);
    save_repo_library(&repo_root, &library)?;
    save_local_state(app, &state)?;
    Ok(result)
}

pub fn update_legacy_project(
    app: &tauri::AppHandle,
    input: &UpdateProjectInput,
) -> Result<LegacyProjectDto, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    let mut state = load_local_state(app)?;
    let profile = library
        .project_profiles
        .iter_mut()
        .find(|profile| profile.id == input.id)
        .ok_or_else(|| format!("project {} not found", input.id))?;

    if let Some(name) = &input.name {
        profile.name = name.clone();
        profile.slug = slugify(name);
    }
    if let Some(color) = &input.color {
        profile.color = color.clone();
    }
    profile.updated_at = now_ms();

    if let Some(path) = &input.path {
        match path {
            Some(path) => upsert_project_binding(&mut state.project_bindings, &profile.id, path),
            None => state
                .project_bindings
                .retain(|binding| binding.project_id != profile.id),
        }
    }

    let result = LegacyProjectDto {
        id: profile.id.clone(),
        name: profile.name.clone(),
        path: state
            .project_bindings
            .iter()
            .find(|binding| binding.project_id == profile.id)
            .map(|binding| binding.path.clone()),
        color: profile.color.clone(),
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    };
    save_repo_library(&repo_root, &library)?;
    save_local_state(app, &state)?;
    Ok(result)
}

pub fn delete_legacy_project(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    let mut state = load_local_state(app)?;
    let before = library.project_profiles.len();
    let mut scoped_resource_ids = HashSet::new();
    for resource in &library.resources {
        if resource.project_id.as_deref() == Some(id) {
            scoped_resource_ids.insert(resource.id.clone());
        }
    }

    library.project_profiles.retain(|profile| profile.id != id);
    if before == library.project_profiles.len() {
        return Err(format!("project {} not found", id));
    }

    library.resources.retain(|resource| {
        resource.project_id.as_deref() != Some(id) && !scoped_resource_ids.contains(&resource.id)
    });
    state
        .project_bindings
        .retain(|binding| binding.project_id != id);
    state
        .install_records
        .retain(|record| record.target.project_id.as_deref() != Some(id));
    save_repo_library(&repo_root, &library)?;
    save_local_state(app, &state)
}

fn expected_project_resources(library: &RepoLibrary, profile: &ProjectProfile) -> Vec<Resource> {
    let mut resources = Vec::new();
    let mut seen = HashSet::new();

    if let Some(agents_resource_id) = &profile.agents_resource_id {
        if let Some(resource) = library
            .resources
            .iter()
            .find(|resource| resource.id == *agents_resource_id)
        {
            seen.insert(resource.id.clone());
            resources.push(resource.clone());
        }
    }

    for resource in &library.resources {
        let attached = profile
            .attached_resource_ids
            .iter()
            .any(|id| id == &resource.id);
        let project_scoped = resource.project_id.as_deref() == Some(profile.id.as_str());
        if (attached || project_scoped) && seen.insert(resource.id.clone()) {
            resources.push(resource.clone());
        }
    }

    resources
}

fn project_target_path(project_root: &Path, resource: &Resource) -> (PathBuf, String) {
    match resource.kind {
        ResourceKind::Agents => {
            let relative = "AGENTS.md".to_string();
            (project_root.join(&relative), relative)
        }
        ResourceKind::Skill => {
            let relative = format!(".codex/skills/{}/SKILL.md", resource.slug);
            (project_root.join(&relative), relative)
        }
        ResourceKind::SlashCommand => {
            let relative = format!(".codex/commands/{}.md", resource.slug);
            (project_root.join(&relative), relative)
        }
        ResourceKind::Prompt => {
            let relative = format!(".codex/prompts/{}.md", resource.slug);
            (project_root.join(&relative), relative)
        }
        ResourceKind::McpServer => {
            let relative = ".codex/config.toml".to_string();
            (project_root.join(&relative), relative)
        }
        ResourceKind::Plugin => {
            let relative = format!(".codex/plugins/{}/plugin.json", resource.slug);
            (project_root.join(&relative), relative)
        }
    }
}

fn build_apply_item(
    project_root: &Path,
    resource: &Resource,
    install_records: &[InstallRecord],
) -> Result<Option<ProjectFileEntry>, String> {
    let (absolute_path, relative_path) = project_target_path(project_root, resource);
    let install_record = install_records
        .iter()
        .find(|record| record.target.path == absolute_path.to_string_lossy());

    let tracked = git::tracked(project_root, &absolute_path);
    if tracked {
        return Ok(Some(ProjectFileEntry {
            relative_path,
            absolute_path: absolute_path.to_string_lossy().into_owned(),
            status: ProjectFileStatus::TrackedConflict,
            resource_id: Some(resource.id.clone()),
            install_record_id: install_record.map(|record| record.id.clone()),
            tracked: true,
            current_revision: read_revision(&absolute_path)?,
            expected_revision: Some(resource.revision.clone()),
            note: Some("target file is already tracked by git".into()),
        }));
    }

    if !absolute_path.exists() {
        return Ok(Some(ProjectFileEntry {
            relative_path,
            absolute_path: absolute_path.to_string_lossy().into_owned(),
            status: ProjectFileStatus::New,
            resource_id: Some(resource.id.clone()),
            install_record_id: install_record.map(|record| record.id.clone()),
            tracked: false,
            current_revision: None,
            expected_revision: Some(resource.revision.clone()),
            note: Some("file will be created".into()),
        }));
    }

    let current_revision = read_revision(&absolute_path)?;
    let status = if current_revision.as_deref() == Some(resource.revision.as_str()) {
        ProjectFileStatus::Unchanged
    } else if install_record
        .map(|record| record.revision != current_revision.clone().unwrap_or_default())
        .unwrap_or(false)
    {
        ProjectFileStatus::Diverged
    } else {
        ProjectFileStatus::Modified
    };

    if status == ProjectFileStatus::Unchanged {
        return Ok(None);
    }

    Ok(Some(ProjectFileEntry {
        relative_path,
        absolute_path: absolute_path.to_string_lossy().into_owned(),
        status,
        resource_id: Some(resource.id.clone()),
        install_record_id: install_record.map(|record| record.id.clone()),
        tracked: false,
        current_revision,
        expected_revision: Some(resource.revision.clone()),
        note: Some("file exists and would change during apply".into()),
    }))
}

fn scan_files(
    project_root: &Path,
    install_records: &[InstallRecord],
) -> Result<Vec<ProjectFileEntry>, String> {
    let mut items = Vec::new();
    let agents_path = project_root.join("AGENTS.md");

    if agents_path.exists() {
        items.push(classify_existing_file(
            project_root,
            &agents_path,
            install_records,
        )?);
    }

    for skills_root in app_skill_dirs(project_root, "codex")? {
        if skills_root.exists() {
            for entry in fs::read_dir(skills_root).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                let skill_file = entry.path().join("SKILL.md");
                if skill_file.exists() {
                    items.push(classify_existing_file(
                        project_root,
                        &skill_file,
                        install_records,
                    )?);
                }
            }
        }
    }

    for record in install_records {
        let target_path = Path::new(&record.target.path);
        if record.target.project_id.is_some()
            && target_path.starts_with(project_root)
            && !target_path.exists()
        {
            items.push(ProjectFileEntry {
                relative_path: record.target.relative_path.clone(),
                absolute_path: record.target.path.clone(),
                status: ProjectFileStatus::Missing,
                resource_id: Some(record.resource_id.clone()),
                install_record_id: Some(record.id.clone()),
                tracked: false,
                current_revision: None,
                expected_revision: Some(record.revision.clone()),
                note: Some("install record exists but file is missing".into()),
            });
        }
    }

    Ok(items)
}

fn classify_existing_file(
    project_root: &Path,
    absolute_path: &Path,
    install_records: &[InstallRecord],
) -> Result<ProjectFileEntry, String> {
    let relative_path = absolute_path
        .strip_prefix(project_root)
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .into_owned();
    let install_record = install_records
        .iter()
        .find(|record| record.target.path == absolute_path.to_string_lossy());
    let current_revision = read_revision(absolute_path)?;
    let tracked = git::tracked(project_root, absolute_path);

    let status = if tracked {
        ProjectFileStatus::TrackedConflict
    } else if let Some(record) = install_record {
        if current_revision.as_deref() == Some(record.revision.as_str()) {
            ProjectFileStatus::Unchanged
        } else {
            ProjectFileStatus::Diverged
        }
    } else {
        ProjectFileStatus::New
    };

    Ok(ProjectFileEntry {
        relative_path,
        absolute_path: absolute_path.to_string_lossy().into_owned(),
        status,
        resource_id: install_record.map(|record| record.resource_id.clone()),
        install_record_id: install_record.map(|record| record.id.clone()),
        tracked,
        current_revision,
        expected_revision: install_record.map(|record| record.revision.clone()),
        note: None,
    })
}

fn read_revision(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(Some(compute_revision(&content)))
}

fn summarize_items(items: &[ProjectFileEntry]) -> ProjectScanSummary {
    ProjectScanSummary {
        total_items: items.len(),
        tracked_conflicts: items
            .iter()
            .filter(|item| item.status == ProjectFileStatus::TrackedConflict)
            .count(),
        divergent_items: items
            .iter()
            .filter(|item| item.status == ProjectFileStatus::Diverged)
            .count(),
    }
}

fn item_is_ignored(input: &ProjectPreviewInput, item: &ProjectFileEntry) -> bool {
    input
        .decisions
        .iter()
        .find(|decision| decision.path == item.absolute_path || decision.path == item.relative_path)
        .map(|decision| decision.action == PreviewDecisionAction::Ignore)
        .unwrap_or(false)
}

pub fn apply_project_profile(
    app: &tauri::AppHandle,
    input: &ProjectPreviewInput,
) -> Result<(), String> {
    let preview = preview_project_apply(app, input)?;
    let selected_items = preview
        .items
        .iter()
        .filter(|item| !item_is_ignored(input, item))
        .collect::<Vec<_>>();

    if selected_items
        .iter()
        .any(|item| item.status == ProjectFileStatus::TrackedConflict)
    {
        return Err("Cannot apply while target files are tracked by git.".into());
    }
    if selected_items
        .iter()
        .any(|item| item.status == ProjectFileStatus::Diverged)
    {
        return Err(
            "Cannot apply while project files have diverged from installed revisions.".into(),
        );
    }

    let selected_paths = selected_items
        .iter()
        .map(|item| item.absolute_path.clone())
        .collect::<HashSet<_>>();

    let normalized = normalize_user_path(&input.path)?;
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let mut state = load_local_state(app)?;
    let profile = library
        .project_profiles
        .iter()
        .find(|profile| profile.id == input.project_id)
        .cloned()
        .ok_or_else(|| format!("project profile {} not found", input.project_id))?;

    let resources = expected_project_resources(&library, &profile);
    for resource in resources {
        let (absolute_path, relative_path) = project_target_path(&normalized, &resource);
        let absolute_path_string = absolute_path.to_string_lossy().into_owned();
        if !input.decisions.is_empty() && !selected_paths.contains(&absolute_path_string) {
            continue;
        }
        write_resource_to_path(&absolute_path, &resource.content)?;
        ensure_git_exclude(&normalized)?;
        upsert_install_record(
            &mut state.install_records,
            &resource,
            InstallTarget {
                kind: target_kind_for_resource(&resource),
                project_id: Some(profile.id.clone()),
                path: absolute_path_string,
                relative_path,
            },
        );
    }

    upsert_project_binding(
        &mut state.project_bindings,
        &profile.id,
        &normalized.to_string_lossy(),
    );
    state.last_active_project_id = Some(profile.id.clone());
    remember_recent_project(&mut state.recent_project_ids, &profile.id);
    save_local_state(app, &state)
}

pub fn capture_project_changes(
    app: &tauri::AppHandle,
    input: &ProjectPreviewInput,
) -> Result<(), String> {
    let preview = preview_capture_project_changes(app, input)?;
    let selected_items = preview
        .items
        .iter()
        .filter(|item| !item_is_ignored(input, item))
        .collect::<Vec<_>>();
    let selected_paths = selected_items
        .iter()
        .map(|item| item.absolute_path.clone())
        .collect::<HashSet<_>>();
    let selected_resource_ids = selected_items
        .iter()
        .filter_map(|item| item.resource_id.clone())
        .collect::<HashSet<_>>();

    let normalized = normalize_user_path(&input.path)?;
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    let mut state = load_local_state(app)?;
    let profile_index = library
        .project_profiles
        .iter()
        .position(|profile| profile.id == input.project_id)
        .ok_or_else(|| format!("project profile {} not found", input.project_id))?;
    let project_id = library.project_profiles[profile_index].id.clone();
    let project_name = library.project_profiles[profile_index].name.clone();

    let agents_path = normalized.join("AGENTS.md");
    let agents_path_string = agents_path.to_string_lossy().into_owned();
    let agents_selected =
        input.decisions.is_empty() || selected_paths.contains(&agents_path_string);

    if agents_selected && agents_path.exists() {
        let content = fs::read_to_string(&agents_path).map_err(|error| error.to_string())?;
        let resource_id = upsert_project_resource(
            &mut library.resources,
            Some(project_id.clone()),
            ResourceKind::Agents,
            &format!("{} agents", project_name),
            "agents",
            content,
        );
        library.project_profiles[profile_index].agents_resource_id = Some(resource_id);
    } else if agents_selected {
        if let Some(agents_resource_id) = library.project_profiles[profile_index]
            .agents_resource_id
            .clone()
        {
            library
                .resources
                .retain(|resource| resource.id != agents_resource_id);
        }
        library.project_profiles[profile_index].agents_resource_id = None;
    }

    let mut seen_skill_resource_ids = HashSet::new();
    for skills_root in app_skill_dirs(&normalized, "codex")? {
        if skills_root.exists() {
            for entry in fs::read_dir(&skills_root).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                let folder_path = entry.path();
                let skill_file = folder_path.join("SKILL.md");
                if !skill_file.exists() {
                    continue;
                }
                let skill_path_string = skill_file.to_string_lossy().into_owned();
                if !input.decisions.is_empty() && !selected_paths.contains(&skill_path_string) {
                    continue;
                }
                let slug = entry.file_name().to_string_lossy().into_owned();
                let content = fs::read_to_string(&skill_file).map_err(|error| error.to_string())?;
                let resource_id = upsert_project_resource(
                    &mut library.resources,
                    Some(project_id.clone()),
                    ResourceKind::Skill,
                    &slug,
                    &slug,
                    content,
                );
                seen_skill_resource_ids.insert(resource_id.clone());
                if !library.project_profiles[profile_index]
                    .attached_resource_ids
                    .contains(&resource_id)
                {
                    library.project_profiles[profile_index]
                        .attached_resource_ids
                        .push(resource_id);
                }
            }
        }
    }

    library.project_profiles[profile_index]
        .attached_resource_ids
        .retain(|resource_id| {
            library
                .resources
                .iter()
                .find(|resource| resource.id == *resource_id)
                .map(|resource| {
                    if resource.project_id.as_deref() != Some(project_id.as_str()) {
                        return true;
                    }
                    if resource.kind != ResourceKind::Skill {
                        return true;
                    }
                    if input.decisions.is_empty() || selected_resource_ids.contains(resource_id) {
                        seen_skill_resource_ids.contains(resource_id)
                    } else {
                        true
                    }
                })
                .unwrap_or(false)
        });

    let current_agents_resource_id = library.project_profiles[profile_index]
        .agents_resource_id
        .clone();
    library.resources.retain(|resource| {
        if resource.project_id.as_deref() != Some(project_id.as_str()) {
            return true;
        }
        if resource.kind == ResourceKind::Agents {
            return !agents_selected
                || current_agents_resource_id.as_deref() == Some(resource.id.as_str());
        }
        if input.decisions.is_empty() || selected_resource_ids.contains(&resource.id) {
            return seen_skill_resource_ids.contains(&resource.id);
        }
        true
    });

    upsert_project_binding(
        &mut state.project_bindings,
        &project_id,
        &normalized.to_string_lossy(),
    );
    state.last_active_project_id = Some(project_id.clone());
    remember_recent_project(&mut state.recent_project_ids, &project_id);
    save_repo_library(&repo_root, &library)?;
    save_local_state(app, &state)
}

pub fn apply_install_refresh(app: &tauri::AppHandle, record_id: &str) -> Result<(), String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let mut state = load_local_state(app)?;
    let record = state
        .install_records
        .iter_mut()
        .find(|record| record.id == record_id)
        .ok_or_else(|| format!("install record {} not found", record_id))?;
    let resource = library
        .resources
        .iter()
        .find(|resource| resource.id == record.resource_id)
        .cloned()
        .ok_or_else(|| format!("resource {} not found", record.resource_id))?;

    write_resource_to_path(Path::new(&record.target.path), &resource.content)?;
    if let Some(project_id) = &record.target.project_id {
        if let Some(binding) = state
            .project_bindings
            .iter()
            .find(|binding| binding.project_id == *project_id)
        {
            ensure_git_exclude(Path::new(&binding.path))?;
        }
    }

    record.revision = resource.revision;
    record.status = InstallStatus::InSync;
    record.last_scanned_at = Some(now_ms());
    record.updated_at = now_ms();
    save_local_state(app, &state)
}

pub fn scan_global_environment(app: &tauri::AppHandle) -> Result<RecoveryScanResult, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let home_dir = app.path().home_dir().map_err(|error| error.to_string())?;
    let codex_home = preferred_app_cli_dir(&home_dir, "codex")?;

    let mut global_items = Vec::new();
    let mut known_paths = HashSet::new();

    for resource in library
        .resources
        .iter()
        .filter(|resource| resource.scope == ResourceScope::Global)
        .filter(|resource| {
            resource.kind == ResourceKind::Skill
                || resource.kind == ResourceKind::SlashCommand
                || resource.kind == ResourceKind::Prompt
        })
    {
        let path = global_target_path(&codex_home, resource);
        known_paths.insert(path.clone());
        let local_revision = read_revision(&path)?;
        let status = match local_revision.as_deref() {
            None => "repo-only",
            Some(revision) if revision == resource.revision => "same",
            Some(_) => "different",
        };
        global_items.push(RecoveryEntry {
            id: resource.id.clone(),
            kind: match resource.kind {
                ResourceKind::Skill => "skill",
                ResourceKind::SlashCommand => "slash-command",
                ResourceKind::Prompt => "prompt",
                ResourceKind::Agents => "agents",
                ResourceKind::McpServer => "mcp-server",
                ResourceKind::Plugin => "plugin",
            }
            .into(),
            name: resource.title.clone(),
            path: path.to_string_lossy().into_owned(),
            status: status.into(),
            repo_revision: Some(resource.revision.clone()),
            local_revision,
        });
    }

    for local_path in discover_local_codex_files(&codex_home)? {
        if known_paths.contains(&local_path) {
            continue;
        }
        global_items.push(RecoveryEntry {
            id: local_path.to_string_lossy().into_owned(),
            kind: if local_path
                .components()
                .any(|component| component.as_os_str() == "skills")
            {
                "skill".into()
            } else {
                "prompt".into()
            },
            name: local_path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "local item".into()),
            path: local_path.to_string_lossy().into_owned(),
            status: "local-only".into(),
            repo_revision: None,
            local_revision: read_revision(&local_path)?,
        });
    }

    let project_items = library
        .project_profiles
        .iter()
        .map(|profile| RecoveryEntry {
            id: profile.id.clone(),
            kind: "project".into(),
            name: profile.name.clone(),
            path: profile.slug.clone(),
            status: "repo-only".into(),
            repo_revision: None,
            local_revision: None,
        })
        .collect::<Vec<_>>();

    Ok(RecoveryScanResult {
        summary: format!(
            "{} global item(s) and {} project profile(s) detected",
            global_items.len(),
            project_items.len()
        ),
        global_items,
        project_items,
    })
}

pub fn preview_environment_restore(app: &tauri::AppHandle) -> Result<PreviewPlan, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let home_dir = app.path().home_dir().map_err(|error| error.to_string())?;
    let codex_home = preferred_app_cli_dir(&home_dir, "codex")?;

    let mut items = Vec::new();
    for resource in library
        .resources
        .iter()
        .filter(|resource| resource.scope == ResourceScope::Global)
        .filter(|resource| {
            resource.kind == ResourceKind::Skill || resource.kind == ResourceKind::Prompt
        })
    {
        let target_path = global_target_path(&codex_home, resource);
        let current_revision = read_revision(&target_path)?;
        let status = match current_revision.as_deref() {
            None => ProjectFileStatus::New,
            Some(revision) if revision == resource.revision => ProjectFileStatus::Unchanged,
            Some(_) => ProjectFileStatus::Modified,
        };

        if status == ProjectFileStatus::Unchanged {
            continue;
        }

        let relative_path = target_path
            .strip_prefix(&home_dir)
            .unwrap_or(&target_path)
            .to_string_lossy()
            .into_owned();
        items.push(ProjectFileEntry {
            relative_path,
            absolute_path: target_path.to_string_lossy().into_owned(),
            status,
            resource_id: Some(resource.id.clone()),
            install_record_id: None,
            tracked: false,
            current_revision,
            expected_revision: Some(resource.revision.clone()),
            note: Some("global Codex item will be restored from the library repo".into()),
        });
    }

    Ok(PreviewPlan {
        kind: PreviewPlanKind::Apply,
        project_id: "global-environment".into(),
        project_path: codex_home.to_string_lossy().into_owned(),
        summary: summarize_items(&items),
        items,
    })
}

pub fn apply_environment_restore(app: &tauri::AppHandle) -> Result<(), String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let home_dir = app.path().home_dir().map_err(|error| error.to_string())?;
    let codex_home = preferred_app_cli_dir(&home_dir, "codex")?;
    let mut state = load_local_state(app)?;

    for resource in library
        .resources
        .iter()
        .filter(|resource| resource.scope == ResourceScope::Global)
        .filter(|resource| {
            resource.kind == ResourceKind::Skill || resource.kind == ResourceKind::Prompt
        })
    {
        let target_path = global_target_path(&codex_home, resource);
        write_resource_to_path(&target_path, &resource.content)?;
        let relative_path = target_path
            .strip_prefix(&home_dir)
            .unwrap_or(&target_path)
            .to_string_lossy()
            .into_owned();
        upsert_install_record(
            &mut state.install_records,
            resource,
            InstallTarget {
                kind: target_kind_for_resource(resource),
                project_id: None,
                path: target_path.to_string_lossy().into_owned(),
                relative_path,
            },
        );
    }

    save_local_state(app, &state)
}

pub fn check_install_updates(app: &tauri::AppHandle) -> Result<Vec<UpdateItem>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let state = load_local_state(app)?;

    let mut updates = Vec::new();
    for record in &state.install_records {
        let Some(resource) = library
            .resources
            .iter()
            .find(|resource| resource.id == record.resource_id)
        else {
            continue;
        };

        let path = Path::new(&record.target.path);
        let current_revision = read_revision(path)?;
        let install_status = if current_revision.is_none() {
            InstallStatus::Missing
        } else if current_revision.as_deref() != Some(record.revision.as_str()) {
            InstallStatus::Diverged
        } else if record.revision != resource.revision {
            InstallStatus::Stale
        } else {
            InstallStatus::InSync
        };

        if install_status == InstallStatus::InSync {
            continue;
        }

        updates.push(UpdateItem {
            id: record.id.clone(),
            resource_id: resource.id.clone(),
            resource_name: resource.title.clone(),
            project_id: record.target.project_id.clone(),
            project_name: library
                .project_profiles
                .iter()
                .find(|profile| Some(profile.id.as_str()) == record.target.project_id.as_deref())
                .map(|profile| profile.name.clone()),
            target_path: Some(record.target.path.clone()),
            origin: resource.origin,
            source_status: resource.source_status,
            install_status,
            current_revision,
            next_revision: Some(resource.revision.clone()),
            summary: "Installed file is out of sync with the library source.".into(),
        });
    }

    Ok(updates)
}

pub fn check_source_updates(app: &tauri::AppHandle) -> Result<Vec<UpdateItem>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;

    let mut updates = Vec::new();
    for resource in &library.resources {
        if !matches!(
            resource.origin,
            ResourceOrigin::Vendor | ResourceOrigin::ForkedVendor
        ) {
            continue;
        }
        let Some(source_url) = resource.source_url.as_deref() else {
            continue;
        };
        let upstream_head = match git::ls_remote_head(source_url, resource.source_ref.as_deref()) {
            Ok(head) => head,
            Err(_) => continue,
        };

        if resource.upstream_revision.as_deref() == Some(upstream_head.as_str()) {
            continue;
        }

        updates.push(UpdateItem {
            id: resource.id.clone(),
            resource_id: resource.id.clone(),
            resource_name: resource.title.clone(),
            project_id: resource.project_id.clone(),
            project_name: None,
            target_path: None,
            origin: resource.origin,
            source_status: if resource.source_status == SourceStatus::MergeBlocked {
                SourceStatus::MergeBlocked
            } else {
                SourceStatus::UpstreamAvailable
            },
            install_status: InstallStatus::NotInstalled,
            current_revision: resource.upstream_revision.clone(),
            next_revision: Some(upstream_head),
            summary: "Upstream source has a newer revision available.".into(),
        });
    }

    Ok(updates)
}

pub fn preview_source_update(
    app: &tauri::AppHandle,
    resource_id: &str,
) -> Result<PreviewPlan, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let resource = library
        .resources
        .iter()
        .find(|resource| resource.id == resource_id)
        .cloned()
        .ok_or_else(|| format!("resource {} not found", resource_id))?;
    let source_url = resource
        .source_url
        .clone()
        .ok_or_else(|| "resource does not have an upstream source".to_string())?;
    let next_revision = git::ls_remote_head(&source_url, resource.source_ref.as_deref())?;

    Ok(PreviewPlan {
        kind: PreviewPlanKind::Apply,
        project_id: resource.id.clone(),
        project_path: source_url.clone(),
        summary: ProjectScanSummary {
            total_items: 1,
            tracked_conflicts: 0,
            divergent_items: 0,
        },
        items: vec![ProjectFileEntry {
            relative_path: resource
                .source_path
                .clone()
                .unwrap_or_else(|| default_source_path_for_resource(&resource)),
            absolute_path: source_url,
            status: ProjectFileStatus::Modified,
            resource_id: Some(resource.id),
            install_record_id: None,
            tracked: false,
            current_revision: resource.upstream_revision.clone(),
            expected_revision: Some(next_revision),
            note: Some("Previewing upstream source update.".into()),
        }],
    })
}

pub fn apply_source_update(
    app: &tauri::AppHandle,
    resource_id: &str,
) -> Result<UpdateItem, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    let resource_index = library
        .resources
        .iter()
        .position(|resource| resource.id == resource_id)
        .ok_or_else(|| format!("resource {} not found", resource_id))?;
    let resource_snapshot = library.resources[resource_index].clone();
    let source_url = resource_snapshot
        .source_url
        .clone()
        .ok_or_else(|| "resource does not have an upstream source".to_string())?;
    let source_path = resource_snapshot
        .source_path
        .clone()
        .unwrap_or_else(|| default_source_path_for_resource(&resource_snapshot));

    let temp_dir = std::env::temp_dir().join(format!("skill-switch-source-{}", Uuid::new_v4()));
    git::clone_repository(
        &source_url,
        &temp_dir,
        resource_snapshot.source_ref.as_deref(),
    )?;
    let next_revision =
        git::head(&temp_dir).ok_or_else(|| "failed to read upstream HEAD".to_string())?;
    let upstream_file = temp_dir.join(&source_path);
    let upstream_content = fs::read_to_string(&upstream_file).map_err(|error| error.to_string())?;

    let source_status = SourceStatus::Current;
    if resource_snapshot.origin == ResourceOrigin::ForkedVendor {
        if let Some(base_revision) = resource_snapshot.upstream_revision.clone() {
            let base_content = git::read_file_at_revision(&temp_dir, &base_revision, &source_path)?;
            let merge_dir =
                std::env::temp_dir().join(format!("skill-switch-merge-{}", Uuid::new_v4()));
            fs::create_dir_all(&merge_dir).map_err(|error| error.to_string())?;
            let base_path = merge_dir.join("base.txt");
            let local_path = merge_dir.join("local.txt");
            let remote_path = merge_dir.join("remote.txt");
            fs::write(&base_path, base_content).map_err(|error| error.to_string())?;
            fs::write(&local_path, &resource_snapshot.content)
                .map_err(|error| error.to_string())?;
            fs::write(&remote_path, &upstream_content).map_err(|error| error.to_string())?;
            let (merged, clean) = git::merge_file(&base_path, &local_path, &remote_path)?;
            if clean {
                let resource = &mut library.resources[resource_index];
                resource.content = merged;
            } else {
                let blocked_resource = &mut library.resources[resource_index];
                blocked_resource.source_status = SourceStatus::MergeBlocked;
                let response = UpdateItem {
                    id: blocked_resource.id.clone(),
                    resource_id: blocked_resource.id.clone(),
                    resource_name: blocked_resource.title.clone(),
                    project_id: blocked_resource.project_id.clone(),
                    project_name: None,
                    target_path: None,
                    origin: blocked_resource.origin,
                    source_status: SourceStatus::MergeBlocked,
                    install_status: InstallStatus::NotInstalled,
                    current_revision: blocked_resource.upstream_revision.clone(),
                    next_revision: Some(next_revision),
                    summary: "Automatic upstream merge produced conflicts.".into(),
                };
                save_repo_library(&repo_root, &library)?;
                return Ok(response);
            }
            let _ = fs::remove_dir_all(&merge_dir);
        } else {
            let resource = &mut library.resources[resource_index];
            resource.content = upstream_content;
        }
    } else {
        let resource = &mut library.resources[resource_index];
        resource.content = upstream_content;
    }

    let response = {
        let resource = &mut library.resources[resource_index];
        resource.source_path = Some(source_path);
        resource.upstream_revision = Some(next_revision.clone());
        resource.source_status = source_status;
        resource.revision = compute_revision(&resource.content);
        resource.updated_at = now_ms();

        UpdateItem {
            id: resource.id.clone(),
            resource_id: resource.id.clone(),
            resource_name: resource.title.clone(),
            project_id: resource.project_id.clone(),
            project_name: None,
            target_path: None,
            origin: resource.origin,
            source_status,
            install_status: InstallStatus::NotInstalled,
            current_revision: resource.upstream_revision.clone(),
            next_revision: Some(next_revision.clone()),
            summary: "Library source updated from upstream.".into(),
        }
    };

    save_repo_library(&repo_root, &library)?;
    let _ = fs::remove_dir_all(&temp_dir);
    Ok(response)
}

fn resource_to_legacy_skill(resource: &Resource, library: &RepoLibrary) -> LegacySkillDto {
    let mut project_ids = library
        .project_profiles
        .iter()
        .filter(|profile| profile.attached_resource_ids.contains(&resource.id))
        .map(|profile| profile.id.clone())
        .collect::<Vec<_>>();
    if let Some(project_id) = &resource.project_id {
        if !project_ids.contains(project_id) {
            project_ids.push(project_id.clone());
        }
    }

    LegacySkillDto {
        id: resource.id.clone(),
        slug: resource.slug.clone(),
        name: resource.title.clone(),
        description: resource.description.clone(),
        content: resource.content.clone(),
        tags: resource.tags.clone(),
        project_ids,
        created_at: resource.created_at,
        updated_at: resource.updated_at,
        provenance: resource.provenance.clone(),
    }
}

fn resource_to_slash_command(resource: &Resource, library: &RepoLibrary) -> SlashCommandDto {
    let mut project_ids = library
        .project_profiles
        .iter()
        .filter(|profile| profile.attached_resource_ids.contains(&resource.id))
        .map(|profile| profile.id.clone())
        .collect::<Vec<_>>();
    if let Some(project_id) = &resource.project_id {
        if !project_ids.contains(project_id) {
            project_ids.push(project_id.clone());
        }
    }

    SlashCommandDto {
        id: resource.id.clone(),
        slug: resource.slug.clone(),
        name: resource.title.clone(),
        description: resource.description.clone(),
        content: resource.content.clone(),
        tags: resource.tags.clone(),
        project_ids,
        created_at: resource.created_at,
        updated_at: resource.updated_at,
        provenance: resource.provenance.clone(),
    }
}

fn attach_resource_to_profiles(
    profiles: &mut [ProjectProfile],
    resource_id: &str,
    project_ids: &[String],
) {
    for profile in profiles {
        if project_ids.contains(&profile.id)
            && !profile
                .attached_resource_ids
                .contains(&resource_id.to_string())
        {
            profile.attached_resource_ids.push(resource_id.to_string());
        }
    }
}

fn detach_resource_from_all_profiles(profiles: &mut [ProjectProfile], resource_id: &str) {
    for profile in profiles {
        profile.attached_resource_ids.retain(|id| id != resource_id);
        if profile.agents_resource_id.as_deref() == Some(resource_id) {
            profile.agents_resource_id = None;
        }
    }
}

fn upsert_project_binding(bindings: &mut Vec<LocalProjectBinding>, project_id: &str, path: &str) {
    if let Some(binding) = bindings
        .iter_mut()
        .find(|binding| binding.project_id == project_id)
    {
        binding.path = path.to_string();
        binding.updated_at = now_ms();
    } else {
        bindings.push(LocalProjectBinding {
            project_id: project_id.to_string(),
            path: path.to_string(),
            detected_repo_root: git::git_root(Path::new(path))
                .map(|root| root.to_string_lossy().into_owned()),
            updated_at: now_ms(),
        });
    }
}

fn upsert_install_record(
    install_records: &mut Vec<InstallRecord>,
    resource: &Resource,
    target: InstallTarget,
) {
    if let Some(record) = install_records
        .iter_mut()
        .find(|record| record.target.path == target.path)
    {
        record.resource_id = resource.id.clone();
        record.target = target;
        record.revision = resource.revision.clone();
        record.status = InstallStatus::InSync;
        record.last_scanned_at = Some(now_ms());
        record.updated_at = now_ms();
    } else {
        install_records.push(InstallRecord {
            id: Uuid::new_v4().to_string(),
            resource_id: resource.id.clone(),
            target,
            revision: resource.revision.clone(),
            status: InstallStatus::InSync,
            last_scanned_at: Some(now_ms()),
            updated_at: now_ms(),
        });
    }
}

fn target_kind_for_resource(resource: &Resource) -> InstallTargetKind {
    match resource.kind {
        ResourceKind::Agents => InstallTargetKind::ProjectAgents,
        ResourceKind::Prompt => InstallTargetKind::GlobalCodexPrompt,
        ResourceKind::SlashCommand => {
            if resource.scope == ResourceScope::Project {
                InstallTargetKind::ProjectCodexSlashCommand
            } else {
                InstallTargetKind::GlobalCodexSlashCommand
            }
        }
        ResourceKind::Skill | ResourceKind::McpServer | ResourceKind::Plugin => {
            if resource.scope == ResourceScope::Project {
                InstallTargetKind::ProjectCodexSkill
            } else {
                InstallTargetKind::GlobalCodexSkill
            }
        }
    }
}

fn write_resource_to_path(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, content).map_err(|error| error.to_string())
}

fn ensure_git_exclude(project_root: &Path) -> Result<(), String> {
    let Some(repo_root) = git::git_root(project_root) else {
        return Ok(());
    };
    let relative_root = project_root
        .strip_prefix(&repo_root)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = if relative_root.is_empty() {
        String::new()
    } else {
        format!("{relative_root}/")
    };
    let exclude_path = repo_root.join(".git").join("info").join("exclude");
    if let Some(parent) = exclude_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let existing = if exclude_path.exists() {
        fs::read_to_string(&exclude_path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };
    let mut next = existing;
    for pattern in [format!("{prefix}AGENTS.md"), format!("{prefix}.codex/")] {
        if !next.lines().any(|line| line.trim() == pattern) {
            if !next.ends_with('\n') && !next.is_empty() {
                next.push('\n');
            }
            next.push_str(&pattern);
            next.push('\n');
        }
    }
    fs::write(exclude_path, next).map_err(|error| error.to_string())
}

fn upsert_project_resource(
    resources: &mut Vec<Resource>,
    project_id: Option<String>,
    kind: ResourceKind,
    title: &str,
    slug: &str,
    content: String,
) -> String {
    if let Some(resource) = resources.iter_mut().find(|resource| {
        resource.project_id == project_id && resource.kind == kind && resource.slug == slug
    }) {
        resource.title = title.to_string();
        resource.content = content;
        resource.revision = compute_revision(&resource.content);
        resource.updated_at = now_ms();
        return resource.id.clone();
    }

    let resource = Resource {
        id: Uuid::new_v4().to_string(),
        slug: slugify(slug),
        title: title.to_string(),
        description: None,
        kind,
        scope: ResourceScope::Project,
        origin: ResourceOrigin::Private,
        source_status: SourceStatus::LocalOnly,
        project_id,
        tags: Vec::new(),
        revision: compute_revision(&content),
        content,
        source_url: None,
        source_ref: None,
        source_path: None,
        upstream_revision: None,
        forked_from: None,
        created_at: now_ms(),
        updated_at: now_ms(),
        provenance: Default::default(),
    };
    let id = resource.id.clone();
    resources.push(resource);
    id
}

fn global_target_path(codex_home: &Path, resource: &Resource) -> PathBuf {
    match resource.kind {
        ResourceKind::Skill => codex_home
            .join("skills")
            .join(&resource.slug)
            .join("SKILL.md"),
        ResourceKind::SlashCommand => codex_home
            .join("commands")
            .join(format!("{}.md", resource.slug)),
        ResourceKind::Prompt => codex_home
            .join("prompts")
            .join(format!("{}.md", resource.slug)),
        ResourceKind::Agents => codex_home.join("AGENTS.md"),
        ResourceKind::McpServer => codex_home.join("config.toml"),
        ResourceKind::Plugin => codex_home
            .join("plugins")
            .join(&resource.slug)
            .join("plugin.json"),
    }
}

fn discover_local_codex_files(codex_home: &Path) -> Result<Vec<PathBuf>, String> {
    let mut items = Vec::new();
    let skills_root = codex_home.join("skills");
    if skills_root.exists() {
        for entry in fs::read_dir(&skills_root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file = entry.path().join("SKILL.md");
            if file.exists() {
                items.push(file);
            }
        }
    }

    let prompts_root = codex_home.join("prompts");
    if prompts_root.exists() {
        for entry in fs::read_dir(&prompts_root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file = entry.path();
            if file.extension().and_then(|extension| extension.to_str()) == Some("md") {
                items.push(file);
            }
        }
    }

    let commands_root = codex_home.join("commands");
    if commands_root.exists() {
        for entry in fs::read_dir(&commands_root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file = entry.path();
            if file.extension().and_then(|ext| ext.to_str()) == Some("md") {
                items.push(file);
            }
        }
    }

    Ok(items)
}

fn remember_recent_project(recent_project_ids: &mut Vec<String>, project_id: &str) {
    recent_project_ids.retain(|id| id != project_id);
    recent_project_ids.insert(0, project_id.to_string());
    recent_project_ids.truncate(5);
}

fn default_source_path_for_resource(resource: &Resource) -> String {
    match resource.kind {
        ResourceKind::Skill => "SKILL.md".into(),
        ResourceKind::SlashCommand => "COMMAND.md".into(),
        ResourceKind::Prompt => format!("{}.md", resource.slug),
        ResourceKind::Agents => "AGENTS.md".into(),
        ResourceKind::McpServer => "mcp.json".into(),
        ResourceKind::Plugin => "plugin.json".into(),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn library_zip_entries_are_prefixed() {
        let base = std::env::temp_dir().join(format!("ss-libzip-{}", uuid::Uuid::new_v4()));
        let skill = base.join("demo");
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::write(skill.join("SKILL.md"), "# Demo").unwrap();
        fs::write(skill.join("scripts").join("run.sh"), "echo").unwrap();

        let zip_path = base.join("library.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        write_directory_to_zip_under(&mut zip, "skills/", &skill, &skill, options).unwrap();
        zip.finish().unwrap();

        let archive = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        let mut names: Vec<_> = archive.file_names().map(str::to_string).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "skills/demo/SKILL.md",
                "skills/demo/scripts/",
                "skills/demo/scripts/run.sh"
            ]
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn move_aside_preserves_real_directory() {
        let base = std::env::temp_dir().join(format!("ss-aside-{}", uuid::Uuid::new_v4()));
        let user_skill = base.join("skills").join("demo");
        fs::create_dir_all(&user_skill).unwrap();
        fs::write(user_skill.join("SKILL.md"), "mine").unwrap();

        let moved = install::move_aside_into(&user_skill, &base.join("replaced")).unwrap();

        assert!(!user_skill.exists());
        assert_eq!(fs::read_to_string(moved.join("SKILL.md")).unwrap(), "mine");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn write_file_atomic_replaces_contents_without_leftovers() {
        let dir = std::env::temp_dir().join(format!("ss-atomic-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("library.json");
        fs::write(&target, "old").unwrap();

        write_file_atomic(&target, "new").unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipArchive, ZipWriter};

    fn make_temp_path(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "skill-switch-{}-{}-{}",
            name,
            std::process::id(),
            timestamp
        ))
    }

    #[test]
    fn rejects_invalid_standard_skill_directory() {
        let error =
            validate_standard_skill_directories(&["scripts".to_string(), "agents".to_string()])
                .expect_err("invalid directory should fail validation");

        assert!(error.contains("agents"));
    }

    #[test]
    fn creates_selected_standard_skill_directories() {
        let root = make_temp_path("skill-source");

        let result = ensure_skill_source_in_root(
            &root,
            "demo-skill",
            "# Demo",
            &[
                "scripts".to_string(),
                "references".to_string(),
                "assets".to_string(),
            ],
        );

        assert!(result.is_ok());
        assert_eq!(
            fs::read_to_string(root.join("demo-skill").join("SKILL.md"))
                .expect("skill content should exist"),
            "# Demo"
        );
        assert!(root.join("demo-skill").join("scripts").is_dir());
        assert!(root.join("demo-skill").join("references").is_dir());
        assert!(root.join("demo-skill").join("assets").is_dir());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn zip_export_helper_keeps_empty_directories() {
        let root = make_temp_path("zip-export");
        let skill_root = root.join("demo-skill");
        let zip_path = root.join("demo-skill.zip");

        fs::create_dir_all(skill_root.join("scripts")).expect("scripts directory should exist");
        fs::write(skill_root.join("SKILL.md"), "# Demo").expect("skill file should exist");

        let file = fs::File::create(&zip_path).expect("zip file should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        write_directory_to_zip(&mut zip, &skill_root, &skill_root, options)
            .expect("zip helper should succeed");
        zip.finish().expect("zip should finish");

        let file = fs::File::open(&zip_path).expect("zip file should open");
        let archive = ZipArchive::new(file).expect("zip archive should open");
        let names: Vec<String> = archive.file_names().map(str::to_string).collect();

        assert!(names.contains(&"demo-skill/SKILL.md".to_string()));
        assert!(names.contains(&"demo-skill/scripts/".to_string()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reconcile_backup_clone_imports_updates_and_removes_managed_skills() {
        let root = make_temp_path("reconcile-backup-clone");
        fs::create_dir_all(&root).expect("temp root should be created");

        let legacy_skill_dir = root.join("legacy-skill");
        fs::create_dir_all(&legacy_skill_dir).expect("legacy skill dir should exist");
        fs::write(
            legacy_skill_dir.join("SKILL.md"),
            "---\nname: Legacy Skill\ndescription: Pulled from backup\ntags: [backup]\n---\n# Legacy Skill\n\nPulled from backup\n",
        )
        .expect("legacy skill should be written");

        let stale_skill = Resource {
            id: "stale-id".into(),
            slug: "stale-skill".into(),
            title: "Stale Skill".into(),
            description: Some("stale".into()),
            kind: ResourceKind::Skill,
            scope: ResourceScope::Global,
            origin: ResourceOrigin::Private,
            source_status: SourceStatus::LocalOnly,
            project_id: None,
            tags: vec!["old".into()],
            content: "# Stale Skill".into(),
            revision: "old-revision".into(),
            source_url: None,
            source_ref: None,
            source_path: None,
            upstream_revision: None,
            forked_from: None,
            created_at: 1,
            updated_at: 1,
            provenance: Default::default(),
        };

        let kept_skill = Resource {
            id: "kept-id".into(),
            slug: "legacy-skill".into(),
            title: "Old Name".into(),
            description: Some("old description".into()),
            kind: ResourceKind::Skill,
            scope: ResourceScope::Global,
            origin: ResourceOrigin::Private,
            source_status: SourceStatus::LocalOnly,
            project_id: None,
            tags: vec!["old".into()],
            content: "# Old Name".into(),
            revision: "old-revision".into(),
            source_url: None,
            source_ref: None,
            source_path: None,
            upstream_revision: None,
            forked_from: None,
            created_at: 1,
            updated_at: 1,
            provenance: Default::default(),
        };

        let mut library = RepoLibrary {
            version: "2".into(),
            ..Default::default()
        };
        library.resources.push(stale_skill);
        library.resources.push(kept_skill);
        save_repo_library(&root, &library).expect("library should be saved");

        let changed =
            reconcile_managed_skills_from_backup_clone(&root).expect("reconcile should succeed");
        assert!(changed, "reconcile should detect backup-driven changes");

        let next_library = load_repo_library(&root).expect("library should reload");
        assert_eq!(
            next_library.resources.len(),
            1,
            "stale resource should be removed"
        );

        let resource = next_library
            .resources
            .iter()
            .find(|resource| resource.slug == "legacy-skill")
            .expect("legacy skill should be present");
        assert_eq!(resource.title, "Legacy Skill");
        assert_eq!(resource.description.as_deref(), Some("Pulled from backup"));
        assert_eq!(resource.tags, vec!["backup".to_string()]);
        assert!(resource.content.contains("Pulled from backup"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_frontmatter_only_multiline_skill_and_crlf() {
        let raw = "---\r\nname: browser-debugger\r\ndescription: >-\r\n  Automates headless checks\r\n  across web previews.\r\ntags:\r\n  - browser\r\n  - debug\r\n---\r\nStep 1: Open the page.\r\n";
        let (name, desc, tags) = parse_skill_front_matter(raw);
        assert_eq!(name.as_deref(), Some("browser-debugger"));
        assert_eq!(
            desc.as_deref(),
            Some("Automates headless checks across web previews.")
        );
        assert_eq!(tags, vec!["browser".to_string(), "debug".to_string()]);

        let (meta_name, meta_desc) = parse_skill_metadata(raw);
        assert_eq!(meta_name, "browser-debugger");
        assert_eq!(
            meta_desc.as_deref(),
            Some("Automates headless checks across web previews.")
        );
    }

    #[test]
    fn reconcile_ignores_plugin_and_command_sources_dirs() {
        let root = make_temp_path("reconcile-ignores-containers");
        fs::create_dir_all(root.join("plugin-sources").join("SKILL.md-fake")).unwrap();
        fs::write(
            root.join("plugin-sources").join("SKILL.md"),
            "# Not a top-level skill",
        )
        .unwrap();
        fs::create_dir_all(root.join("command-sources")).unwrap();
        fs::write(
            root.join("command-sources").join("SKILL.md"),
            "# Not a skill",
        )
        .unwrap();

        let library = RepoLibrary {
            version: "2".into(),
            ..Default::default()
        };
        save_repo_library(&root, &library).unwrap();

        let changed = reconcile_managed_skills_from_backup_clone(&root).unwrap();
        assert!(!changed);
        let reloaded = load_repo_library(&root).unwrap();
        assert!(reloaded.resources.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_managed_cli_dirs_preserves_host_configs_and_user_files() {
        let project = make_temp_path("safe-cli-cleanup");
        let skill_src = project.join("_canonical_skill");
        fs::create_dir_all(&skill_src).unwrap();
        fs::write(skill_src.join("SKILL.md"), "# Demo").unwrap();

        let codex_dir = project.join(".codex");
        fs::create_dir_all(codex_dir.join("skills")).unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "# user config\n[mcp_servers.demo]\ncommand = \"npx\"\n",
        )
        .unwrap();
        create_skill_symlink(&codex_dir.join("skills").join("demo"), &skill_src).unwrap();

        cleanup_managed_cli_dirs_in_project(&project, "codex", &[]).unwrap();

        assert!(
            !codex_dir.join("skills").join("demo").exists(),
            "managed symlink should be removed"
        );
        assert!(
            codex_dir.join("config.toml").exists(),
            "host config.toml must NEVER be deleted"
        );

        let _ = fs::remove_dir_all(project);
    }
    #[test]
    fn normalize_custom_apps_sanitizes_ssh_and_prevents_local_tilde_expansion() {
        let valid_ssh = CustomAppConfig {
            id: "  cage-claude  ".to_string(),
            label: "  Cage Claude  ".to_string(),
            accent_color: "".to_string(),
            global_skill_dir: "  ~/.claude/skills  ".to_string(),
            project_skill_dir: Some(".claude/skills".to_string()),
            global_command_dir: Some(" ~/.claude/commands ".to_string()),
            project_command_dir: Some(".claude/commands".to_string()),
            global_mcp_config_path: Some(" ~/.claude.json ".to_string()),
            project_mcp_config_path: Some(".mcp.json".to_string()),
            mcp_format: Some(" json ".to_string()),
            mcp_root_key: Some(" mcpServers ".to_string()),
            ssh: Some(crate::domain::SshTargetConfig {
                host: "  lima-claude-cage  ".to_string(),
                user: Some("  cage  ".to_string()),
                port: Some(0),
                identity_file: Some("  ~/.ssh/id_ed25519  ".to_string()),
                remote_app_preset: Some(" claude ".to_string()),
                global_plugin_dir: Some(" ~/.claude/plugins/local ".to_string()),
                inject_secrets: true,
            }),
        };
        let empty_host_ssh = CustomAppConfig {
            id: "broken-ssh".to_string(),
            label: "Broken".to_string(),
            accent_color: "#000000".to_string(),
            global_skill_dir: "~/.claude/skills".to_string(),
            ssh: Some(crate::domain::SshTargetConfig {
                host: "   ".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let normalized = normalize_custom_apps(&[valid_ssh, empty_host_ssh]);
        assert_eq!(normalized.len(), 1);
        let app = &normalized[0];
        assert_eq!(app.id, "cage-claude");
        assert_eq!(app.project_skill_dir, None);
        assert_eq!(app.project_command_dir, None);
        assert_eq!(app.project_mcp_config_path, None);
        let ssh = app.ssh.as_ref().expect("ssh config preserved");
        assert_eq!(ssh.host, "lima-claude-cage");
        assert_eq!(ssh.user.as_deref(), Some("cage"));
        assert_eq!(ssh.port, None);
        assert!(ssh.inject_secrets);

        let local_home = Path::new("/Users/local-mac");
        assert!(
            global_skill_dirs_for_app(local_home, "cage-claude", &normalized)
                .unwrap()
                .is_empty(),
            "SSH app must NEVER expand remote ~ against local Mac $HOME"
        );
        assert!(
            global_command_dirs_for_app(local_home, "cage-claude", &normalized)
                .unwrap()
                .is_empty()
        );
    }
}
