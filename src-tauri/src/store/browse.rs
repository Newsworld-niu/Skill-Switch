//! Browsing managed source directories and scanning CLI app directories for external items.

use super::*;

// =============================================================================
// Skill Directory Browsing
// =============================================================================

/// List the contents of a skill directory
/// Returns the directory listing including files and subdirectories
pub fn list_skill_directory(
    app: &tauri::AppHandle,
    input: &crate::domain::SkillDirectoryInput,
) -> Result<crate::domain::SkillDirectoryListing, String> {
    use crate::domain::{SkillDirectoryEntry, SkillDirectoryListing, SkillEntryKind};

    // Get the skill to find its slug
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    // Ensure the managed source directory exists before browsing it
    let source_dir = ensure_skill_source_for_skill(app, &skill)?;

    // Determine the target directory
    let target_dir = if let Some(sub_path) = &input.sub_path {
        // Sanitize the sub_path to prevent directory traversal
        let sanitized = sub_path
            .trim_start_matches('/')
            .trim_start_matches('\\')
            .replace("..", "");
        let full_path = source_dir.join(&sanitized);

        if !full_path.exists() || !full_path.is_dir() {
            return Err(format!("目录不存在或不是文件夹: {}", sub_path));
        }

        full_path
    } else {
        source_dir.clone()
    };

    // Calculate relative path
    let current_path = input.sub_path.clone().unwrap_or_default();
    let parent_path = if current_path.is_empty() {
        None
    } else {
        let parts: Vec<&str> = current_path.rsplitn(2, '/').collect();
        if parts.len() > 1 {
            Some(parts[1].to_string())
        } else {
            Some(String::new())
        }
    };

    // Read directory entries
    let mut entries: Vec<SkillDirectoryEntry> = Vec::new();

    if target_dir.exists() && target_dir.is_dir() {
        for entry in fs::read_dir(&target_dir).map_err(|e| format!("读取目录失败：{}", e))? {
            let entry = entry.map_err(|e| format!("读取目录条目失败：{}", e))?;
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();

            // Skip hidden files
            if name.starts_with('.') {
                continue;
            }

            let metadata = entry.metadata().ok();
            let is_dir = path.is_dir();
            let kind = if is_dir {
                SkillEntryKind::Directory
            } else {
                SkillEntryKind::File
            };

            let entry_path = if current_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", current_path, name)
            };

            let extension = if is_dir {
                None
            } else {
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_lowercase())
            };

            entries.push(SkillDirectoryEntry {
                name,
                kind,
                path: entry_path,
                extension,
                size: metadata.as_ref().map(|m| m.len()),
            });
        }

        // Sort: directories first, then files, alphabetically
        entries.sort_by(|a, b| match (&a.kind, &b.kind) {
            (SkillEntryKind::Directory, SkillEntryKind::File) => std::cmp::Ordering::Less,
            (SkillEntryKind::File, SkillEntryKind::Directory) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
    }

    Ok(SkillDirectoryListing {
        skill_id: input.skill_id.clone(),
        skill_slug: skill.slug,
        root_path: source_dir.to_string_lossy().to_string(),
        current_path,
        parent_path,
        entries,
    })
}

/// Read the content of a file in a skill directory
pub fn read_skill_file(
    app: &tauri::AppHandle,
    input: &crate::domain::SkillFileInput,
) -> Result<crate::domain::SkillFileContent, String> {
    // Get the skill to find its slug
    let skill = get_legacy_skill(app, &input.skill_id)?
        .ok_or_else(|| format!("skill {} not found", input.skill_id))?;

    // Ensure the managed source directory exists before reading files
    let source_dir = ensure_skill_source_for_skill(app, &skill)?;

    // Sanitize the file path to prevent directory traversal
    let sanitized = input
        .file_path
        .trim_start_matches('/')
        .trim_start_matches('\\')
        .replace("..", "");

    let file_path = source_dir.join(&sanitized);

    if !file_path.exists() {
        return Err(format!("文件不存在: {}", input.file_path));
    }

    if !file_path.is_file() {
        return Err(format!("路径不是文件: {}", input.file_path));
    }

    // Check file size (limit to 1MB for text files)
    let metadata = fs::metadata(&file_path).map_err(|e| format!("读取文件信息失败：{}", e))?;
    let size = metadata.len();

    if size > 1024 * 1024 {
        return Err("文件太大，超过 1MB 限制".to_string());
    }

    // Read file content
    let content = fs::read_to_string(&file_path).map_err(|e| format!("读取文件失败：{}", e))?;

    Ok(crate::domain::SkillFileContent {
        skill_id: input.skill_id.clone(),
        path: input.file_path.clone(),
        content,
        size,
    })
}

pub fn list_slash_command_directory(
    app: &tauri::AppHandle,
    input: &crate::domain::SlashCommandDirectoryInput,
) -> Result<crate::domain::SlashCommandDirectoryListing, String> {
    use crate::domain::{SkillDirectoryEntry, SkillEntryKind, SlashCommandDirectoryListing};

    let command = get_slash_command(app, &input.command_id)?
        .ok_or_else(|| format!("slash command {} not found", input.command_id))?;

    let source_dir = ensure_slash_command_source_for_command(app, &command)?;

    let target_dir = if let Some(sub_path) = &input.sub_path {
        let sanitized = sub_path
            .trim_start_matches('/')
            .trim_start_matches('\\')
            .replace("..", "");
        let full_path = source_dir.join(&sanitized);

        if !full_path.exists() || !full_path.is_dir() {
            return Err(format!("目录不存在或不是文件夹: {}", sub_path));
        }

        full_path
    } else {
        source_dir.clone()
    };

    let current_path = input.sub_path.clone().unwrap_or_default();
    let parent_path = if current_path.is_empty() {
        None
    } else {
        let parts: Vec<&str> = current_path.rsplitn(2, '/').collect();
        if parts.len() > 1 {
            Some(parts[1].to_string())
        } else {
            Some(String::new())
        }
    };

    let mut entries: Vec<SkillDirectoryEntry> = Vec::new();

    if target_dir.exists() && target_dir.is_dir() {
        for entry in fs::read_dir(&target_dir).map_err(|e| format!("读取目录失败：{}", e))? {
            let entry = entry.map_err(|e| format!("读取目录条目失败：{}", e))?;
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?")
                .to_string();

            if name.starts_with('.') {
                continue;
            }

            let metadata = entry.metadata().ok();
            let is_dir = path.is_dir();
            let kind = if is_dir {
                SkillEntryKind::Directory
            } else {
                SkillEntryKind::File
            };

            let entry_path = if current_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", current_path, name)
            };

            let extension = if is_dir {
                None
            } else {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_lowercase())
            };

            entries.push(SkillDirectoryEntry {
                name,
                kind,
                path: entry_path,
                extension,
                size: metadata.as_ref().map(|metadata| metadata.len()),
            });
        }

        entries.sort_by(|left, right| match (&left.kind, &right.kind) {
            (SkillEntryKind::Directory, SkillEntryKind::File) => std::cmp::Ordering::Less,
            (SkillEntryKind::File, SkillEntryKind::Directory) => std::cmp::Ordering::Greater,
            _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
        });
    }

    Ok(SlashCommandDirectoryListing {
        command_id: input.command_id.clone(),
        command_slug: command.slug,
        root_path: source_dir.to_string_lossy().to_string(),
        current_path,
        parent_path,
        entries,
    })
}

pub fn read_slash_command_file(
    app: &tauri::AppHandle,
    input: &crate::domain::SlashCommandFileInput,
) -> Result<crate::domain::SlashCommandFileContent, String> {
    let command = get_slash_command(app, &input.command_id)?
        .ok_or_else(|| format!("slash command {} not found", input.command_id))?;

    let source_dir = ensure_slash_command_source_for_command(app, &command)?;
    let sanitized = input
        .file_path
        .trim_start_matches('/')
        .trim_start_matches('\\')
        .replace("..", "");

    let file_path = source_dir.join(&sanitized);

    if !file_path.exists() {
        return Err(format!("文件不存在: {}", input.file_path));
    }

    if !file_path.is_file() {
        return Err(format!("路径不是文件: {}", input.file_path));
    }

    let metadata = fs::metadata(&file_path).map_err(|e| format!("读取文件信息失败：{}", e))?;
    let size = metadata.len();

    if size > 1024 * 1024 {
        return Err("文件太大，超过 1MB 限制".to_string());
    }

    let content = fs::read_to_string(&file_path).map_err(|e| format!("读取文件失败：{}", e))?;

    Ok(crate::domain::SlashCommandFileContent {
        command_id: input.command_id.clone(),
        path: input.file_path.clone(),
        content,
        size,
    })
}

// =============================================================================
// External App Skills Scanning
// =============================================================================

/// Scan skills from an external app directory (e.g., ~/.claude/skills/)
/// Returns a list of skills found, with info about whether they're managed by SkillSwitch
pub fn scan_external_app_skills(
    app: &tauri::AppHandle,
    app_id: &str,
) -> Result<Vec<crate::domain::ExternalSkillDto>, String> {
    use crate::domain::ExternalSkillDto;

    // Get the home directory
    let home_dir = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = load_custom_apps(app);
    if crate::ssh_sync::is_ssh_app(app_id, &custom_apps) {
        return Ok(Vec::new());
    }

    let skills_dirs = global_skill_dirs_for_app(&home_dir, app_id, &custom_apps)?;

    // Get the SkillSwitch skill-sources directory to check symlinks
    let skill_sources = skill_sources_dir(app)?;
    let _skill_sources_str = skill_sources.to_string_lossy().to_string();

    let mut skills: Vec<ExternalSkillDto> = Vec::new();
    let mut seen_slugs = HashSet::new();

    for skills_dir in skills_dirs {
        if !skills_dir.exists() {
            continue;
        }

        let entries = fs::read_dir(&skills_dir).map_err(|e| format!("读取目录失败：{}", e))?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            let slug = match path.file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                None => continue,
            };

            if !seen_slugs.insert(slug.clone()) {
                continue;
            }

            let content = match fs::read_to_string(&skill_file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (name, description) = parse_skill_metadata(&content);
            if name.is_empty() {
                continue;
            }

            let metadata = fs::symlink_metadata(&path);
            let is_symlink = metadata
                .as_ref()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);

            let symlink_target = if is_symlink {
                fs::read_link(&path)
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
            } else {
                None
            };

            skills.push(ExternalSkillDto {
                slug,
                name,
                description,
                app_id: app_id.to_string(),
                path: path.to_string_lossy().to_string(),
                is_symlink,
                symlink_target: symlink_target.clone(),
            });
        }
    }

    // Sort by name
    skills.sort_by_key(|a| a.name.to_lowercase());

    Ok(skills)
}

/// Read the SKILL.md content from an external skill directory
pub fn read_external_skill_content(path: &str) -> Result<String, String> {
    let skill_path = PathBuf::from(path);
    let skill_file = skill_path.join("SKILL.md");

    if !skill_file.exists() {
        return Err("SKILL.md 文件不存在".to_string());
    }

    fs::read_to_string(&skill_file).map_err(|e| format!("读取 SKILL.md 失败：{}", e))
}

pub fn scan_external_app_slash_commands(
    app: &tauri::AppHandle,
    app_id: &str,
) -> Result<Vec<crate::domain::ExternalSlashCommandDto>, String> {
    use crate::domain::ExternalSlashCommandDto;

    let home_dir = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = load_custom_apps(app);
    if crate::ssh_sync::is_ssh_app(app_id, &custom_apps) {
        return Ok(Vec::new());
    }
    let command_dirs = global_command_dirs_for_app(&home_dir, app_id, &custom_apps)?;

    let mut commands: Vec<ExternalSlashCommandDto> = Vec::new();
    let mut seen_slugs = HashSet::new();

    for command_dir in command_dirs {
        if !command_dir.exists() {
            continue;
        }

        let entries = fs::read_dir(&command_dir).map_err(|e| format!("读取目录失败：{}", e))?;

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let path = entry.path();

            // Case 1: subdirectory with COMMAND.md (e.g. ~/.claude/commands/my-command/COMMAND.md)
            if path.is_dir() {
                let command_file = path.join("COMMAND.md");
                if !command_file.exists() {
                    continue;
                }

                let slug = match path.file_name() {
                    Some(name) => name.to_string_lossy().to_string(),
                    None => continue,
                };

                if !seen_slugs.insert(slug.clone()) {
                    continue;
                }

                let content = match fs::read_to_string(&command_file) {
                    Ok(content) => content,
                    Err(_) => continue,
                };

                let (name, description) = parse_skill_metadata(&content);
                if name.is_empty() {
                    continue;
                }

                let metadata = fs::symlink_metadata(&path);
                let is_symlink = metadata
                    .as_ref()
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false);

                let symlink_target = if is_symlink {
                    fs::read_link(&path)
                        .ok()
                        .map(|target| target.to_string_lossy().to_string())
                } else {
                    None
                };

                commands.push(ExternalSlashCommandDto {
                    slug,
                    name,
                    description,
                    app_id: app_id.to_string(),
                    path: path.to_string_lossy().to_string(),
                    is_symlink,
                    symlink_target,
                });
            }
            // Case 2: direct .md file (e.g. ~/.claude/commands/my-command.md)
            else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let slug = match path.file_stem() {
                    Some(stem) => stem.to_string_lossy().to_string(),
                    None => continue,
                };

                if !seen_slugs.insert(slug.clone()) {
                    continue;
                }

                let content = match fs::read_to_string(&path) {
                    Ok(content) => content,
                    Err(_) => continue,
                };

                let (name, description) = parse_skill_metadata(&content);
                if name.is_empty() {
                    continue;
                }

                // .md files are never symlinks (the symlink would be the file itself)
                let is_symlink = fs::symlink_metadata(&path)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);

                let symlink_target = if is_symlink {
                    fs::read_link(&path)
                        .ok()
                        .map(|target| target.to_string_lossy().to_string())
                } else {
                    None
                };

                commands.push(ExternalSlashCommandDto {
                    slug,
                    name,
                    description,
                    app_id: app_id.to_string(),
                    path: path.to_string_lossy().to_string(),
                    is_symlink,
                    symlink_target,
                });
            }
        }
    }

    commands.sort_by_key(|a| a.name.to_lowercase());

    Ok(commands)
}

pub fn read_external_slash_command_content(path: &str) -> Result<String, String> {
    let command_path = PathBuf::from(path);
    if command_path.is_file() {
        return fs::read_to_string(&command_path).map_err(|e| format!("读取命令文件失败：{}", e));
    }

    let command_file = command_path.join("COMMAND.md");
    if !command_file.exists() {
        return Err("COMMAND.md 文件不存在".to_string());
    }

    fs::read_to_string(&command_file).map_err(|e| format!("读取 COMMAND.md 失败：{}", e))
}
