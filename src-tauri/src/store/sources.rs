//! Managed source directories (`skill-sources/`, `command-sources/`) for skills and commands.

use super::*;

// =============================================================================
// Skill Source Directory Management
// =============================================================================

/// Get the persistent clone directory (primary storage when backup source is configured).
pub(crate) fn persistent_clone_dir(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let config = backup_source_settings(app)?;
    match config {
        Some(config) => {
            let local_path = config
                .local_path
                .ok_or_else(|| "backup source local_path is missing".to_string())?;
            Ok(Some(PathBuf::from(local_path)))
        }
        None => Ok(None),
    }
}

/// Get the directory where skill source files are stored.
/// When a backup source is configured and the persistent clone exists, returns that directory.
/// Otherwise falls back to the legacy skill-sources directory.
pub(crate) fn skill_sources_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(clone_dir) = persistent_clone_dir(app)? {
        if clone_dir.exists() {
            return Ok(clone_dir);
        }
    }
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(SKILL_SOURCES_DIR))
}

/// Get the source directory for a specific skill
pub(crate) fn skill_source_dir(app: &tauri::AppHandle, slug: &str) -> Result<PathBuf, String> {
    skill_sources_dir(app).map(|dir| dir.join(slug))
}

/// Get the source directory for a skill by its ID (public, for commands layer)
/// For RepoSource skills, returns the managed source directory (repo-sources/<repo_id>/<skill_path_parent>)
/// For other skills, returns the skill-sources directory
pub fn skill_source_dir_by_id(app: &tauri::AppHandle, skill_id: &str) -> Result<PathBuf, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let resource = library
        .resources
        .iter()
        .find(|r| r.id == skill_id && r.kind == ResourceKind::Skill)
        .ok_or_else(|| format!("skill {} not found", skill_id))?;

    // For RepoSource provenance, return the managed source directory
    if resource.provenance.kind == ProvenanceKind::RepoSource {
        let source_id = resource
            .provenance
            .source_id
            .as_ref()
            .ok_or_else(|| "RepoSource skill missing source_id".to_string())?;
        let source_path = resource
            .provenance
            .source_path
            .as_ref()
            .ok_or_else(|| "RepoSource skill missing source_path".to_string())?;

        // source_path is the SKILL.md file path, we need its parent directory
        let repo_local_path = crate::repo_sources::repo_local_path(app, source_id)?;
        let skill_md_path = repo_local_path.join(source_path);
        let skill_dir = skill_md_path
            .parent()
            .ok_or_else(|| "Cannot get parent directory of SKILL.md path".to_string())?;

        return Ok(skill_dir.to_path_buf());
    }

    // For other skills, return the skill-sources directory
    skill_source_dir(app, &resource.slug)
}

pub(crate) fn validate_standard_skill_directories(
    directories: &[String],
) -> Result<Vec<String>, String> {
    let mut validated = Vec::new();
    let mut seen = HashSet::new();

    for directory in directories {
        let trimmed = directory.trim();
        let normalized = trimmed.trim_matches(|ch| ch == '/' || ch == '\\');

        if normalized.is_empty() {
            return Err("标准 Skill 目录不能为空".to_string());
        }

        if !STANDARD_SKILL_DIRECTORIES.contains(&normalized) {
            return Err(format!("不支持的标准 Skill 目录：{}", directory));
        }

        if seen.insert(normalized.to_string()) {
            validated.push(normalized.to_string());
        }
    }

    Ok(validated)
}

/// Ensure the skill source file exists in the app data directory
/// Returns the path to the skill source directory
pub(crate) fn ensure_skill_source(
    app: &tauri::AppHandle,
    slug: &str,
    content: &str,
) -> Result<PathBuf, String> {
    ensure_skill_source_with_directories(app, slug, content, &[])
}

pub(crate) fn ensure_skill_source_with_directories(
    app: &tauri::AppHandle,
    slug: &str,
    content: &str,
    directories: &[String],
) -> Result<PathBuf, String> {
    let sources_root = skill_sources_dir(app)?;
    ensure_skill_source_in_root(&sources_root, slug, content, directories)
}

pub(crate) fn ensure_skill_source_for_skill(
    app: &tauri::AppHandle,
    skill: &LegacySkillDto,
) -> Result<PathBuf, String> {
    ensure_skill_source(app, &skill.slug, &skill.content)
}

pub(crate) fn ensure_skill_source_in_root(
    root: &Path,
    slug: &str,
    content: &str,
    directories: &[String],
) -> Result<PathBuf, String> {
    let directories = validate_standard_skill_directories(directories)?;
    let source_dir = root.join(slug);
    let skill_file = source_dir.join("SKILL.md");

    fs::create_dir_all(&source_dir)
        .map_err(|e| format!("Failed to create skill source directory: {}", e))?;

    let should_write = if skill_file.exists() {
        let existing = fs::read_to_string(&skill_file).unwrap_or_default();
        existing != content
    } else {
        true
    };

    if should_write {
        write_file_atomic(&skill_file, content)?;
    }

    for directory in directories {
        fs::create_dir_all(source_dir.join(directory))
            .map_err(|e| format!("Failed to create skill source directory: {}", e))?;
    }

    Ok(source_dir)
}

pub(crate) fn slash_command_sources_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(clone_dir) = persistent_clone_dir(app)? {
        if clone_dir.exists() {
            return Ok(clone_dir.join(COMMAND_SOURCES_DIR));
        }
    }
    app.path()
        .app_data_dir()
        .map_err(|error| error.to_string())
        .map(|path| path.join(COMMAND_SOURCES_DIR))
}

pub(crate) fn slash_command_source_dir(
    app: &tauri::AppHandle,
    slug: &str,
) -> Result<PathBuf, String> {
    slash_command_sources_dir(app).map(|dir| dir.join(slug))
}

pub fn slash_command_source_dir_by_id(
    app: &tauri::AppHandle,
    command_id: &str,
) -> Result<PathBuf, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library(&repo_root)?;
    let resource = library
        .resources
        .iter()
        .find(|resource| resource.id == command_id && resource.kind == ResourceKind::SlashCommand)
        .ok_or_else(|| format!("slash command {} not found", command_id))?;

    slash_command_source_dir(app, &resource.slug)
}

pub(crate) fn ensure_slash_command_source(
    app: &tauri::AppHandle,
    slug: &str,
    content: &str,
) -> Result<PathBuf, String> {
    ensure_slash_command_source_with_directories(app, slug, content, &[])
}

pub(crate) fn ensure_slash_command_source_with_directories(
    app: &tauri::AppHandle,
    slug: &str,
    content: &str,
    directories: &[String],
) -> Result<PathBuf, String> {
    let sources_root = slash_command_sources_dir(app)?;
    let directories = validate_standard_skill_directories(directories)?;
    let source_dir = sources_root.join(slug);
    let command_file = source_dir.join("COMMAND.md");

    fs::create_dir_all(&source_dir)
        .map_err(|e| format!("Failed to create slash command source directory: {}", e))?;

    let should_write = if command_file.exists() {
        let existing = fs::read_to_string(&command_file).unwrap_or_default();
        existing != content
    } else {
        true
    };

    if should_write {
        write_file_atomic(&command_file, content)?;
    }

    for directory in directories {
        fs::create_dir_all(source_dir.join(directory))
            .map_err(|e| format!("Failed to create slash command source directory: {}", e))?;
    }

    Ok(source_dir)
}

pub(crate) fn ensure_slash_command_source_for_command(
    app: &tauri::AppHandle,
    command: &SlashCommandDto,
) -> Result<PathBuf, String> {
    ensure_slash_command_source(app, &command.slug, &command.content)
}

pub(crate) fn write_directory_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    current: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    write_directory_to_zip_under(zip, "", root, current, options)
}

/// Like [`write_directory_to_zip`], with every entry placed under `prefix`
/// (e.g. `"skills/"`), which must be empty or end with `/`.
pub(crate) fn write_directory_to_zip_under(
    zip: &mut zip::ZipWriter<fs::File>,
    prefix: &str,
    root: &Path,
    current: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    use std::io::Write;

    for entry in fs::read_dir(current).map_err(|e| format!("读取目录失败：{}", e))? {
        let entry = entry.map_err(|e| format!("读取目录条目失败：{}", e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map_err(|e| format!("计算相对路径失败：{}", e))?
            .to_string_lossy()
            .replace('\\', "/");
        let zip_path = format!(
            "{}{}/{}",
            prefix,
            root.file_name().and_then(|n| n.to_str()).unwrap_or("skill"),
            relative
        );

        if path.is_dir() {
            zip.add_directory(format!("{}/", zip_path), options)
                .map_err(|e| format!("写入 ZIP 目录失败：{}", e))?;
            write_directory_to_zip_under(zip, prefix, root, &path, options)?;
        } else {
            zip.start_file(&zip_path, options)
                .map_err(|e| format!("写入 ZIP 失败：{}", e))?;
            let bytes = fs::read(&path).map_err(|e| format!("读取导出文件失败：{}", e))?;
            zip.write_all(&bytes)
                .map_err(|e| format!("写入 ZIP 文件失败：{}", e))?;
        }
    }

    Ok(())
}
