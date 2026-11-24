//! Importing skills/commands from folders, ZIPs, dialogs and repo sources; exporting to ZIP.

use super::*;

/// Fails before any file is touched if a managed skill/command already uses `slug`,
/// so a conflicting import never overwrites the existing item's source directory.
fn ensure_managed_slug_available(
    app: &tauri::AppHandle,
    kind: ResourceKind,
    slug: &str,
) -> Result<(), String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let taken = library.resources.iter().any(|resource| {
        resource.slug == slug
            && match kind {
                ResourceKind::SlashCommand => is_managed_slash_command_resource(resource),
                _ => is_managed_skill_resource(resource),
            }
    });
    if !taken {
        return Ok(());
    }
    Err(match kind {
        ResourceKind::SlashCommand => format!("已存在同名 Slash Command：{}", slug),
        _ => format!("已存在同名 Skill：{}", slug),
    })
}

// ─── Import skill functions ────────────────────────────────────────────────────

/// Recursively copy a directory and all its contents
pub(crate) fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建目录失败：{}", e))?;

    for entry in fs::read_dir(src).map_err(|e| format!("读取目录失败：{}", e))? {
        let entry = entry.map_err(|e| format!("读取目录条目失败：{}", e))?;
        let ty = entry
            .file_type()
            .map_err(|e| format!("读取文件类型失败：{}", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| format!("复制文件失败：{}", e))?;
        }
    }
    Ok(())
}

/// Pick a skill import path using tauri_plugin_dialog (works on all platforms)
pub(crate) fn pick_skill_import_path(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;

    // First try to pick a folder
    if let Some(folder) = app
        .dialog()
        .file()
        .set_title("选择 Skill 文件夹")
        .blocking_pick_folder()
    {
        return folder
            .into_path()
            .map(Some)
            .map_err(|error| format!("读取目录路径失败：{}", error));
    }

    // If folder picker was cancelled, try to pick a file (ZIP or SKILL.md)
    if let Some(file) = app
        .dialog()
        .file()
        .add_filter("Skill Import", &["zip", "md"])
        .set_title("选择 SKILL.md 或 ZIP 包")
        .blocking_pick_file()
    {
        return file
            .into_path()
            .map(Some)
            .map_err(|error| format!("读取文件路径失败：{}", error));
    }

    Ok(None)
}

pub fn import_skill_from_path(
    app: &tauri::AppHandle,
    selected_path: &Path,
) -> Result<LegacySkillDto, String> {
    if !selected_path.exists() {
        return Err("选择的路径不存在".to_string());
    }

    if selected_path.is_dir() {
        return import_skill_from_folder(app, selected_path);
    }

    if !selected_path.is_file() {
        return Err("选择的内容既不是文件夹也不是文件".to_string());
    }

    let file_name = selected_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if file_name.eq_ignore_ascii_case("SKILL.md") {
        let parent = selected_path
            .parent()
            .ok_or_else(|| "无法识别 SKILL.md 所在目录".to_string())?;
        return import_skill_from_folder(app, parent);
    }

    let extension = selected_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    if extension.eq_ignore_ascii_case("zip") {
        return import_skill_from_zip(app, selected_path);
    }

    Err("请选择包含 SKILL.md 的文件夹、SKILL.md 文件或 ZIP 包".to_string())
}

pub fn import_skill_from_dialog(app: &tauri::AppHandle) -> Result<Option<LegacySkillDto>, String> {
    let Some(path) = pick_skill_import_path(app)? else {
        return Ok(None);
    };

    with_mutation_lock(|| import_skill_from_path(app, &path)).map(Some)
}

pub(crate) fn pick_slash_command_import_path(
    app: &tauri::AppHandle,
) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;

    if let Some(folder) = app
        .dialog()
        .file()
        .set_title("选择 Slash Command 文件夹")
        .blocking_pick_folder()
    {
        return folder
            .into_path()
            .map(Some)
            .map_err(|error| format!("读取目录路径失败：{}", error));
    }

    if let Some(file) = app
        .dialog()
        .file()
        .add_filter("Slash Command Import", &["zip", "md"])
        .set_title("选择 COMMAND.md 或 ZIP 包")
        .blocking_pick_file()
    {
        return file
            .into_path()
            .map(Some)
            .map_err(|error| format!("读取文件路径失败：{}", error));
    }

    Ok(None)
}

pub fn import_slash_command_from_path(
    app: &tauri::AppHandle,
    selected_path: &Path,
) -> Result<SlashCommandDto, String> {
    if !selected_path.exists() {
        return Err("选择的路径不存在".to_string());
    }

    if selected_path.is_dir() {
        return import_slash_command_from_folder(app, selected_path);
    }

    if !selected_path.is_file() {
        return Err("选择的内容既不是文件夹也不是文件".to_string());
    }

    let file_name = selected_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if file_name.eq_ignore_ascii_case("COMMAND.md") {
        let parent = selected_path
            .parent()
            .ok_or_else(|| "无法识别 COMMAND.md 所在目录".to_string())?;
        return import_slash_command_from_folder(app, parent);
    }

    let extension = selected_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    if extension.eq_ignore_ascii_case("zip") {
        return import_slash_command_from_zip(app, selected_path);
    }

    Err("请选择包含 COMMAND.md 的文件夹、COMMAND.md 文件或 ZIP 包".to_string())
}

pub fn import_slash_command_from_dialog(
    app: &tauri::AppHandle,
) -> Result<Option<SlashCommandDto>, String> {
    let Some(path) = pick_slash_command_import_path(app)? else {
        return Ok(None);
    };

    with_mutation_lock(|| import_slash_command_from_path(app, &path)).map(Some)
}

pub fn import_slash_command_from_folder(
    app: &tauri::AppHandle,
    folder_path: &Path,
) -> Result<SlashCommandDto, String> {
    if !folder_path.exists() || !folder_path.is_dir() {
        return Err("路径不存在或不是文件夹".to_string());
    }

    let command_md_path = folder_path.join("COMMAND.md");
    if !command_md_path.exists() {
        return Err("文件夹中未找到 COMMAND.md 文件，不符合 Slash Command 规范".to_string());
    }

    let content =
        fs::read_to_string(&command_md_path).map_err(|e| format!("读取 COMMAND.md 失败：{}", e))?;
    let (name, description) = parse_skill_metadata(&content);

    if name.is_empty() {
        return Err("COMMAND.md 中未找到有效的命令名称".to_string());
    }

    let slug = slugify(&name);
    ensure_managed_slug_available(app, ResourceKind::SlashCommand, &slug)?;
    let target_dir = slash_command_source_dir(app, &slug)?;
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| format!("清理旧目录失败：{}", e))?;
    }
    copy_dir_all(folder_path, &target_dir)?;

    let input = CreateSlashCommandInput {
        name,
        description,
        content,
        directories: vec![],
        tags: vec!["imported".to_string()],
        project_ids: vec![],
    };

    let result = create_slash_command_internal(app, &input, false)?;
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    if let Some(resource) = library
        .resources
        .iter_mut()
        .find(|resource| resource.id == result.command.id)
    {
        resource.provenance = Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".to_string(),
            ..Default::default()
        };
    }
    save_repo_library(&repo_root, &library)?;
    Ok(SlashCommandDto {
        provenance: Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".into(),
            ..Default::default()
        },
        ..result.command
    })
}

/// Import a skill from a folder containing SKILL.md
/// Returns the imported skill on success, or an error message
pub fn import_skill_from_folder(
    app: &tauri::AppHandle,
    folder_path: &Path,
) -> Result<LegacySkillDto, String> {
    // Check if the folder exists
    if !folder_path.exists() || !folder_path.is_dir() {
        return Err("路径不存在或不是文件夹".to_string());
    }

    // Check for SKILL.md in the folder
    let skill_md_path = folder_path.join("SKILL.md");
    if !skill_md_path.exists() {
        return Err("文件夹中未找到 SKILL.md 文件，不符合 Skill 规范".to_string());
    }

    // Read SKILL.md content
    let content =
        fs::read_to_string(&skill_md_path).map_err(|e| format!("读取 SKILL.md 失败：{}", e))?;

    // Parse skill metadata from content
    let (name, description) = parse_skill_metadata(&content);

    if name.is_empty() {
        return Err("SKILL.md 中未找到有效的技能名称".to_string());
    }

    // Generate slug first
    let slug = slugify(&name);

    ensure_managed_slug_available(app, ResourceKind::Skill, &slug)?;

    // Copy the entire folder to skill-sources directory
    let target_dir = skill_source_dir(app, &slug)?;
    if target_dir.exists() {
        // Remove existing directory if it exists
        fs::remove_dir_all(&target_dir).map_err(|e| format!("清理旧目录失败：{}", e))?;
    }
    copy_dir_all(folder_path, &target_dir)?;

    // Create the skill using existing create_legacy_skill function
    let input = CreateSkillInput {
        name,
        description,
        content,
        directories: vec![],
        tags: vec!["imported".to_string()],
        project_ids: vec![],
    };

    let result = create_legacy_skill_internal(app, &input, false)?;
    // Update provenance for file import
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    if let Some(resource) = library
        .resources
        .iter_mut()
        .find(|r| r.id == result.skill.id)
    {
        resource.provenance = Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".to_string(),
            ..Default::default()
        };
    }
    save_repo_library(&repo_root, &library)?;
    Ok(LegacySkillDto {
        provenance: Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".into(),
            ..Default::default()
        },
        ..result.skill
    })
}

/// Import a skill from a zip file
/// The zip must contain a folder with SKILL.md at the root level or one level deep
pub fn import_skill_from_zip(
    app: &tauri::AppHandle,
    zip_path: &Path,
) -> Result<LegacySkillDto, String> {
    use std::io::Read;
    use zip::ZipArchive;

    // Check if the file exists
    if !zip_path.exists() {
        return Err("ZIP 文件不存在".to_string());
    }

    // Open the zip file
    let file = fs::File::open(zip_path).map_err(|e| format!("打开 ZIP 文件失败：{}", e))?;

    let mut archive = ZipArchive::new(file).map_err(|e| format!("解析 ZIP 文件失败：{}", e))?;

    // Find SKILL.md in the archive and determine the root folder
    let mut skill_md_content: Option<String> = None;
    let mut root_prefix: Option<String> = None;

    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败：{}", e))?;
        let name = zip_file.name().to_string();

        // Check if this is SKILL.md
        if name.ends_with("SKILL.md") {
            // Determine the root prefix (folder containing SKILL.md)
            if let Some(pos) = name.rfind("SKILL.md") {
                root_prefix = if pos == 0 {
                    None
                } else {
                    Some(name[..pos].to_string())
                };
            }

            // Read the content
            let mut content = String::new();
            zip_file
                .read_to_string(&mut content)
                .map_err(|e| format!("读取 SKILL.md 内容失败：{}", e))?;
            skill_md_content = Some(content);
        }
    }

    let content = skill_md_content
        .ok_or_else(|| "ZIP 包中未找到 SKILL.md 文件，不符合 Skill 规范".to_string())?;

    // Parse skill metadata from content
    let (name, description) = parse_skill_metadata(&content);

    if name.is_empty() {
        return Err("SKILL.md 中未找到有效的技能名称".to_string());
    }

    // Generate slug
    let slug = slugify(&name);

    ensure_managed_slug_available(app, ResourceKind::Skill, &slug)?;

    // Extract all files to skill-sources directory
    let target_dir = skill_source_dir(app, &slug)?;
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| format!("清理旧目录失败：{}", e))?;
    }
    fs::create_dir_all(&target_dir).map_err(|e| format!("创建目录失败：{}", e))?;

    // Extract all files from the archive
    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败：{}", e))?;
        let name = zip_file.name().to_string();

        // Skip the root prefix if present
        let relative_path = if let Some(ref prefix) = root_prefix {
            if name.starts_with(prefix) {
                &name[prefix.len()..]
            } else {
                &name
            }
        } else {
            &name
        };

        if relative_path.is_empty() || relative_path == "/" {
            continue;
        }

        let target_path = target_dir.join(relative_path);

        if name.ends_with('/') {
            // It's a directory
            fs::create_dir_all(&target_path).map_err(|e| format!("创建目录失败：{}", e))?;
        } else {
            // It's a file
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{}", e))?;
            }
            let mut outfile =
                fs::File::create(&target_path).map_err(|e| format!("创建文件失败：{}", e))?;
            std::io::copy(&mut zip_file, &mut outfile)
                .map_err(|e| format!("写入文件失败：{}", e))?;
        }
    }

    // Create the skill using existing create_legacy_skill function
    let input = CreateSkillInput {
        name,
        description,
        content,
        directories: vec![],
        tags: vec!["imported".to_string()],
        project_ids: vec![],
    };

    let result = create_legacy_skill_internal(app, &input, false)?;
    // Update provenance for zip import
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    if let Some(resource) = library
        .resources
        .iter_mut()
        .find(|r| r.id == result.skill.id)
    {
        resource.provenance = Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".to_string(),
            ..Default::default()
        };
    }
    save_repo_library(&repo_root, &library)?;
    Ok(LegacySkillDto {
        provenance: Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".into(),
            ..Default::default()
        },
        ..result.skill
    })
}

pub fn import_slash_command_from_zip(
    app: &tauri::AppHandle,
    zip_path: &Path,
) -> Result<SlashCommandDto, String> {
    use std::io::Read;
    use zip::ZipArchive;

    if !zip_path.exists() {
        return Err("ZIP 文件不存在".to_string());
    }

    let file = fs::File::open(zip_path).map_err(|e| format!("打开 ZIP 文件失败：{}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("解析 ZIP 文件失败：{}", e))?;

    let mut command_md_content: Option<String> = None;
    let mut root_prefix: Option<String> = None;

    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败：{}", e))?;
        let name = zip_file.name().to_string();

        if name.ends_with("COMMAND.md") {
            if let Some(pos) = name.rfind("COMMAND.md") {
                root_prefix = if pos == 0 {
                    None
                } else {
                    Some(name[..pos].to_string())
                };
            }

            let mut content = String::new();
            zip_file
                .read_to_string(&mut content)
                .map_err(|e| format!("读取 COMMAND.md 内容失败：{}", e))?;
            command_md_content = Some(content);
        }
    }

    let content = command_md_content
        .ok_or_else(|| "ZIP 包中未找到 COMMAND.md 文件，不符合 Slash Command 规范".to_string())?;

    let (name, description) = parse_skill_metadata(&content);

    if name.is_empty() {
        return Err("COMMAND.md 中未找到有效的命令名称".to_string());
    }

    let slug = slugify(&name);
    ensure_managed_slug_available(app, ResourceKind::SlashCommand, &slug)?;
    let target_dir = slash_command_source_dir(app, &slug)?;
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| format!("清理旧目录失败：{}", e))?;
    }
    fs::create_dir_all(&target_dir).map_err(|e| format!("创建目录失败：{}", e))?;

    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败：{}", e))?;
        let name = zip_file.name().to_string();

        let relative_path = if let Some(ref prefix) = root_prefix {
            if name.starts_with(prefix) {
                &name[prefix.len()..]
            } else {
                &name
            }
        } else {
            &name
        };

        if relative_path.is_empty() || relative_path == "/" {
            continue;
        }

        let target_path = target_dir.join(relative_path);

        if name.ends_with('/') {
            fs::create_dir_all(&target_path).map_err(|e| format!("创建目录失败：{}", e))?;
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{}", e))?;
            }
            let mut outfile =
                fs::File::create(&target_path).map_err(|e| format!("创建文件失败：{}", e))?;
            std::io::copy(&mut zip_file, &mut outfile)
                .map_err(|e| format!("写入文件失败：{}", e))?;
        }
    }

    let input = CreateSlashCommandInput {
        name,
        description,
        content,
        directories: vec![],
        tags: vec!["imported".to_string()],
        project_ids: vec![],
    };

    let result = create_slash_command_internal(app, &input, false)?;
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    if let Some(resource) = library
        .resources
        .iter_mut()
        .find(|resource| resource.id == result.command.id)
    {
        resource.provenance = Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".to_string(),
            ..Default::default()
        };
    }
    save_repo_library(&repo_root, &library)?;
    Ok(SlashCommandDto {
        provenance: Provenance {
            kind: ProvenanceKind::FileImport,
            label: "导入".into(),
            ..Default::default()
        },
        ..result.command
    })
}

/// Import a skill from a third-party repo source
pub fn import_skill_from_repo_source(
    app: &tauri::AppHandle,
    input: &crate::domain::ImportRepoSourceSkillInput,
) -> Result<SkillMutationResult, String> {
    // Find repo source local path
    let settings = get_settings(app)?;
    let repo = settings
        .third_party_repos
        .iter()
        .find(|r| r.id == input.repo_id)
        .ok_or_else(|| format!("repo source {} not found", input.repo_id))?
        .clone();

    let repo_local_path = PathBuf::from(
        repo.local_path
            .as_ref()
            .ok_or_else(|| "repo source has no local path, sync first".to_string())?,
    );

    // Find skill in repo source
    let source_path = repo_local_path.join(&input.skill_path);
    if !source_path.exists() || !source_path.is_dir() {
        return Err(format!(
            "skill path not found in repo source: {}",
            input.skill_path
        ));
    }

    let skill_md = source_path.join("SKILL.md");
    if !skill_md.exists() {
        return Err("SKILL.md not found in source path".into());
    }

    let content = fs::read_to_string(&skill_md).map_err(|e| e.to_string())?;
    let (name, description, _) = derive_skill_source_metadata(&input.skill_slug, &content, None);
    let slug = slugify(&name);

    // Copy entire directory to persistent clone
    let target_dir = skill_source_dir(app, &slug)?;
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| e.to_string())?;
    }
    copy_dir_all(&source_path, &target_dir)?;

    // Create skill in library
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let now = now_ms();

    let provenance = Provenance {
        kind: ProvenanceKind::RepoSource,
        label: "仓库源导入".to_string(),
        source_id: Some(repo.id.clone()),
        source_name: Some(repo.label.clone()),
        source_url: Some(repo.url.clone()),
        source_path: Some(input.skill_path.clone()),
        app_id: None,
    };

    // Check for existing skill with same slug
    if let Some(existing) = library
        .resources
        .iter_mut()
        .find(|r| is_managed_skill_resource(r) && r.slug == slug)
    {
        existing.content = content.clone();
        existing.revision = compute_revision(&content);
        existing.title = name;
        existing.description = description;
        existing.provenance = provenance.clone();
        existing.updated_at = now;
    } else {
        library.resources.push(Resource {
            id: Uuid::new_v4().to_string(),
            slug: slug.clone(),
            title: name,
            description,
            kind: ResourceKind::Skill,
            scope: ResourceScope::Global,
            origin: ResourceOrigin::Private,
            source_status: SourceStatus::LocalOnly,
            project_id: None,
            tags: vec![],
            content: content.clone(),
            revision: compute_revision(&content),
            source_url: None,
            source_ref: None,
            source_path: None,
            upstream_revision: None,
            forked_from: None,
            provenance: provenance.clone(),
            created_at: now,
            updated_at: now,
        });
    }

    save_repo_library(&repo_root, &library)?;

    let backup_sync = sync_after_mutation(app, &format!("Import from repo source: {}", slug));

    let resource = library
        .resources
        .iter()
        .find(|r| r.kind == ResourceKind::Skill && r.slug == slug)
        .ok_or("imported skill not found after save")?;

    Ok(SkillMutationResult {
        skill: resource_to_legacy_skill(resource, &library),
        sync: SyncStatus {
            status: backup_sync.status,
            message: backup_sync.message,
            last_error: backup_sync.last_error,
            ahead: 0,
            behind: 0,
        },
    })
}

/// Parse skill metadata (name, description) from SKILL.md content.
/// Prefers YAML frontmatter (`name` + `description`), falling back to the first Markdown `#` heading
/// and first paragraph when frontmatter fields are absent.
pub(crate) fn parse_skill_metadata(content: &str) -> (String, Option<String>) {
    let normalized = content.replace("\r\n", "\n");
    let (front_name, front_desc, _) = parse_skill_front_matter(&normalized);

    let body = if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some((_, after)) = rest.split_once("\n---\n") {
            after
        } else if rest.ends_with("\n---") {
            ""
        } else {
            normalized.as_str()
        }
    } else {
        normalized.as_str()
    };

    let mut heading_name = String::new();
    let mut body_description: Option<String> = None;

    let lines: Vec<&str> = body.lines().collect();
    let mut found_heading = false;
    let mut desc_lines: Vec<String> = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        if !found_heading && trimmed.is_empty() {
            continue;
        }

        if !found_heading && trimmed.starts_with('#') {
            heading_name = trimmed.trim_start_matches('#').trim().to_string();
            found_heading = true;
            continue;
        }

        if found_heading {
            if trimmed.starts_with('#') || trimmed.starts_with("```") {
                break;
            }
            if !trimmed.is_empty() {
                desc_lines.push(trimmed.to_string());
            } else if !desc_lines.is_empty() {
                break;
            }
        } else if front_name.is_some() && front_desc.is_none() {
            // Frontmatter provided `name` without `description`, and body has no `#` heading:
            // capture first paragraph of body as description
            if trimmed.starts_with("```") {
                break;
            }
            if !trimmed.is_empty() {
                desc_lines.push(trimmed.to_string());
            } else if !desc_lines.is_empty() {
                break;
            }
        }
    }

    if !desc_lines.is_empty() {
        body_description = Some(desc_lines.join(" "));
    }

    let final_name = front_name.filter(|s| !s.is_empty()).unwrap_or(heading_name);
    let mut final_desc = front_desc.or(body_description);
    if let Some(ref desc) = final_desc {
        if desc.chars().count() > 200 {
            final_desc = Some(format!("{}...", desc.chars().take(197).collect::<String>()));
        }
    }

    (final_name, final_desc)
}

/// Export a skill to a ZIP file
/// Exports the full managed source directory when available
pub fn export_skill_to_zip(
    app: &tauri::AppHandle,
    skill_id: &str,
    output_path: &Path,
) -> Result<String, String> {
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    // Get the skill
    let skill =
        get_legacy_skill(app, skill_id)?.ok_or_else(|| format!("skill {} not found", skill_id))?;

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{}", e))?;
    }

    // Create the ZIP file
    let file = fs::File::create(output_path).map_err(|e| format!("创建 ZIP 文件失败：{}", e))?;

    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let source_dir = ensure_skill_source_for_skill(app, &skill)?;
    write_directory_to_zip(&mut zip, &source_dir, &source_dir, options)?;

    // Finish the ZIP
    zip.finish().map_err(|e| format!("完成 ZIP 失败：{}", e))?;

    Ok(output_path.to_string_lossy().to_string())
}

pub fn export_slash_command_to_zip(
    app: &tauri::AppHandle,
    command_id: &str,
    output_path: &Path,
) -> Result<String, String> {
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let command = get_slash_command(app, command_id)?
        .ok_or_else(|| format!("slash command {} not found", command_id))?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{}", e))?;
    }

    let file = fs::File::create(output_path).map_err(|e| format!("创建 ZIP 文件失败：{}", e))?;

    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let source_dir = ensure_slash_command_source_for_command(app, &command)?;
    write_directory_to_zip(&mut zip, &source_dir, &source_dir, options)?;

    zip.finish().map_err(|e| format!("完成 ZIP 失败：{}", e))?;

    Ok(output_path.to_string_lossy().to_string())
}

const LIBRARY_ZIP_SKILLS_DIR: &str = "skills";
const LIBRARY_ZIP_COMMANDS_DIR: &str = "commands";

/// Exports every managed skill and slash command into one ZIP
/// (`skills/<slug>/…`, `commands/<slug>/…`) for sharing or offline backup.
pub fn export_library_to_zip(
    app: &tauri::AppHandle,
    output_path: &Path,
) -> Result<LibraryExportResult, String> {
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{}", e))?;
    }
    let file = fs::File::create(output_path).map_err(|e| format!("创建 ZIP 文件失败：{}", e))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let skills = list_legacy_skills(app)?;
    for skill in &skills {
        let source_dir = ensure_skill_source_for_skill(app, skill)?;
        let prefix = format!("{}/", LIBRARY_ZIP_SKILLS_DIR);
        write_directory_to_zip_under(&mut zip, &prefix, &source_dir, &source_dir, options)?;
    }
    let commands = list_slash_commands(app)?;
    for command in &commands {
        let source_dir = ensure_slash_command_source_for_command(app, command)?;
        let prefix = format!("{}/", LIBRARY_ZIP_COMMANDS_DIR);
        write_directory_to_zip_under(&mut zip, &prefix, &source_dir, &source_dir, options)?;
    }
    zip.finish().map_err(|e| format!("完成 ZIP 失败：{}", e))?;

    Ok(LibraryExportResult {
        path: output_path.to_string_lossy().into_owned(),
        skill_count: skills.len(),
        command_count: commands.len(),
    })
}

/// Imports a ZIP produced by [`export_library_to_zip`]. Items whose name already
/// exists in the library are skipped (never overwritten).
pub fn import_library_from_zip(
    app: &tauri::AppHandle,
    zip_path: &Path,
) -> Result<LibraryImportResult, String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("打开 ZIP 失败：{}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析 ZIP 失败：{}", e))?;
    let temp_dir = std::env::temp_dir().join(format!("skill-switch-library-{}", Uuid::new_v4()));
    // `extract` rejects entries that would escape the target directory.
    let extracted = archive
        .extract(&temp_dir)
        .map_err(|e| format!("解压 ZIP 失败：{}", e));

    let result = extracted.and_then(|()| {
        let mut result = LibraryImportResult::default();
        let subdirs = |name: &str| -> Vec<PathBuf> {
            let mut dirs: Vec<PathBuf> = fs::read_dir(temp_dir.join(name))
                .map(|entries| {
                    entries
                        .filter_map(|entry| entry.ok().map(|e| e.path()))
                        .filter(|path| path.is_dir())
                        .collect()
                })
                .unwrap_or_default();
            dirs.sort();
            dirs
        };
        let skill_dirs = subdirs(LIBRARY_ZIP_SKILLS_DIR);
        let command_dirs = subdirs(LIBRARY_ZIP_COMMANDS_DIR);
        if skill_dirs.is_empty() && command_dirs.is_empty() {
            return Err("ZIP 中没有 skills/ 或 commands/ 目录，不是 SkillSwitch 导出的库".into());
        }
        for dir in skill_dirs {
            match import_skill_from_folder(app, &dir) {
                Ok(skill) => result.imported_skills.push(skill.name),
                Err(error) => result
                    .skipped
                    .push(format!("{}：{}", dir_label(&dir), error)),
            }
        }
        for dir in command_dirs {
            match import_slash_command_from_folder(app, &dir) {
                Ok(command) => result.imported_commands.push(command.name),
                Err(error) => result
                    .skipped
                    .push(format!("{}：{}", dir_label(&dir), error)),
            }
        }
        Ok(result)
    });

    let _ = fs::remove_dir_all(&temp_dir);
    result
}

fn dir_label(dir: &Path) -> String {
    dir.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
