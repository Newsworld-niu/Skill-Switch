//! Create/update/delete/search for managed skills and slash commands.

use super::*;

pub fn create_legacy_skill(
    app: &tauri::AppHandle,
    input: &CreateSkillInput,
) -> Result<CreateLegacySkillResult, String> {
    create_legacy_skill_internal(app, input, true)
}

pub(crate) fn create_legacy_skill_internal(
    app: &tauri::AppHandle,
    input: &CreateSkillInput,
    _snapshot_before_write: bool,
) -> Result<CreateLegacySkillResult, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let now = now_ms();
    let slug = slugify(&input.name);

    if library
        .resources
        .iter()
        .any(|resource| is_managed_skill_resource(resource) && resource.slug == slug)
    {
        return Err(format!("已存在同名 Skill：{}", slug));
    }

    let resource = Resource {
        id: Uuid::new_v4().to_string(),
        slug,
        title: input.name.clone(),
        description: input.description.clone(),
        kind: ResourceKind::Skill,
        scope: ResourceScope::Global,
        origin: ResourceOrigin::Private,
        source_status: SourceStatus::LocalOnly,
        project_id: None,
        tags: input.tags.clone(),
        content: input.content.clone(),
        revision: compute_revision(&input.content),
        source_url: None,
        source_ref: None,
        source_path: None,
        upstream_revision: None,
        forked_from: None,
        created_at: now,
        updated_at: now,
        provenance: Default::default(),
    };

    let resource_id = resource.id.clone();
    library.resources.push(resource.clone());
    attach_resource_to_profiles(
        &mut library.project_profiles,
        &resource_id,
        &input.project_ids,
    );
    save_repo_library(&repo_root, &library)?;
    ensure_skill_source_with_directories(
        app,
        &resource.slug,
        &resource.content,
        &input.directories,
    )?;

    let backup_sync = sync_after_mutation(app, &format!("Create skill: {}", resource.title));

    Ok(CreateLegacySkillResult {
        skill: resource_to_legacy_skill(&resource, &library),
        backup_sync,
    })
}

pub fn update_legacy_skill(
    app: &tauri::AppHandle,
    input: &UpdateSkillInput,
) -> Result<LegacySkillDto, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let parsed_front_matter = input.content.as_ref().map(|content| {
        let (name, description, _) = parse_skill_front_matter(content);
        (name, description)
    });

    {
        let resource = library
            .resources
            .iter_mut()
            .find(|resource| resource.kind == ResourceKind::Skill && resource.id == input.id)
            .ok_or_else(|| format!("skill {} not found", input.id))?;

        if let Some(name) = &input.name {
            resource.title = name.clone();
        }
        if let Some(description) = &input.description {
            resource.description = description.clone();
        }
        if let Some(content) = &input.content {
            resource.content = content.clone();
            resource.revision = compute_revision(content);

            if input.name.is_none() {
                if let Some(front_name) = parsed_front_matter
                    .as_ref()
                    .and_then(|(name, _)| name.clone())
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                {
                    resource.title = front_name;
                }
            }

            if input.description.is_none() {
                if let Some(front_description) = parsed_front_matter
                    .as_ref()
                    .and_then(|(_, description)| description.clone())
                {
                    let trimmed = front_description.trim().to_string();
                    resource.description = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    };
                }
            }
        }
        if let Some(tags) = &input.tags {
            resource.tags = tags.clone();
        }
        resource.updated_at = now_ms();
    }

    if let Some(project_ids) = &input.project_ids {
        detach_resource_from_all_profiles(&mut library.project_profiles, &input.id);
        attach_resource_to_profiles(&mut library.project_profiles, &input.id, project_ids);
    }

    let resource = library
        .resources
        .iter()
        .find(|resource| resource.kind == ResourceKind::Skill && resource.id == input.id)
        .ok_or_else(|| format!("skill {} not found", input.id))?;
    let result = resource_to_legacy_skill(resource, &library);
    save_repo_library(&repo_root, &library)?;
    let source_dir = ensure_skill_source(app, &resource.slug, &resource.content)?;
    crate::ssh_sync::resync_skill_to_enabled_ssh_apps(app, &resource.slug, &source_dir);

    // Sync update to persistent clone
    let _backup_sync = sync_after_mutation(app, &format!("Update skill: {}", result.name));

    Ok(result)
}

pub fn sync_legacy_skill_from_source(
    app: &tauri::AppHandle,
    skill_id: &str,
) -> Result<LegacySkillDto, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let resource_index = library
        .resources
        .iter()
        .position(|resource| resource.kind == ResourceKind::Skill && resource.id == skill_id)
        .ok_or_else(|| format!("skill {} not found", skill_id))?;

    // Use skill_source_dir_by_id which handles RepoSource skills correctly
    let skill_dir = skill_source_dir_by_id(app, skill_id)?;
    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.exists() {
        return Err("SKILL.md 文件不存在".to_string());
    }

    let content =
        fs::read_to_string(&skill_file).map_err(|e| format!("读取 SKILL.md 失败：{}", e))?;
    let changed = {
        let resource = &mut library.resources[resource_index];
        let (name, description, tags) =
            derive_skill_source_metadata(&resource.slug, &content, Some(resource.tags.as_slice()));
        let revision = compute_revision(&content);
        let changed = resource.title != name
            || resource.description != description
            || resource.content != content
            || resource.tags != tags
            || resource.revision != revision;

        if changed {
            resource.title = name;
            resource.description = description;
            resource.content = content;
            resource.tags = tags;
            resource.revision = revision;
            resource.updated_at = now_ms();
        }

        changed
    };

    if changed {
        save_repo_library(&repo_root, &library)?;
        let resource_name = library.resources[resource_index].title.clone();
        let _backup_sync =
            sync_after_mutation(app, &format!("Update skill from source: {}", resource_name));
    }
    let slug = library.resources[resource_index].slug.clone();
    crate::ssh_sync::resync_skill_to_enabled_ssh_apps(app, &slug, &skill_dir);

    Ok(resource_to_legacy_skill(
        &library.resources[resource_index],
        &library,
    ))
}

pub fn delete_legacy_skill(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let removed_slug = library
        .resources
        .iter()
        .find(|resource| resource.id == id)
        .map(|resource| resource.slug.clone())
        .ok_or_else(|| format!("skill {} not found", id))?;
    let before = library.resources.len();
    library.resources.retain(|resource| resource.id != id);
    debug_assert!(before > library.resources.len());
    detach_resource_from_all_profiles(&mut library.project_profiles, id);
    save_repo_library(&repo_root, &library)?;

    let source_dir = skill_source_dir(app, &removed_slug)?;
    let custom_apps = load_custom_apps(app);
    if let Ok(home_dir) = app.path().home_dir() {
        for app_id in all_configured_app_ids(&custom_apps) {
            if crate::ssh_sync::is_ssh_app(&app_id, &custom_apps) {
                continue;
            }
            if let Ok(paths) = global_skill_paths(&home_dir, &app_id, &removed_slug, &custom_apps) {
                for path in paths {
                    if is_symlink(&path) {
                        let _ = remove_symlink(&path);
                    }
                }
            }
        }
    }

    let initial_ledgers = load_local_state(app)
        .map(|s| s.ssh_ledgers)
        .unwrap_or_default();
    for (app_id, ledger) in &initial_ledgers {
        if ledger.skills.contains(&removed_slug) {
            if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
                let _ = crate::ssh_sync::uninstall_skill_ssh(app, custom, &removed_slug);
            }
        }
    }
    if let Ok(mut state) = load_local_state(app) {
        let mut ledger_changed = false;
        for ledger in state.ssh_ledgers.values_mut() {
            let before = ledger.skills.len();
            ledger.skills.retain(|s| s != &removed_slug);
            if ledger.skills.len() != before {
                ledger_changed = true;
            }
        }
        if ledger_changed {
            let _ = save_local_state(app, &state);
        }
    }

    if source_dir.exists() {
        remove_path(&source_dir)?;
    }

    // Sync deletion to persistent clone
    let _backup_sync = sync_after_mutation(app, &format!("Delete skill: {}", removed_slug));

    Ok(())
}

pub fn search_legacy_skills(
    app: &tauri::AppHandle,
    query: &str,
) -> Result<Vec<LegacySkillDto>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return list_legacy_skills(app);
    }

    Ok(library
        .resources
        .iter()
        .filter(|resource| resource.kind == ResourceKind::Skill)
        .filter(|resource| {
            resource.title.to_lowercase().contains(&needle)
                || resource
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&needle)
                || resource.content.to_lowercase().contains(&needle)
                || resource
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&needle))
        })
        .map(|resource| resource_to_legacy_skill(resource, &library))
        .collect())
}

pub fn create_slash_command(
    app: &tauri::AppHandle,
    input: &CreateSlashCommandInput,
) -> Result<CreateSlashCommandResult, String> {
    create_slash_command_internal(app, input, true)
}

pub(crate) fn create_slash_command_internal(
    app: &tauri::AppHandle,
    input: &CreateSlashCommandInput,
    _snapshot_before_write: bool,
) -> Result<CreateSlashCommandResult, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let now = now_ms();
    let slug = slugify(&input.name);

    if library
        .resources
        .iter()
        .any(|resource| is_managed_slash_command_resource(resource) && resource.slug == slug)
    {
        return Err(format!("已存在同名 Slash Command：{}", slug));
    }

    let resource = Resource {
        id: Uuid::new_v4().to_string(),
        slug,
        title: input.name.clone(),
        description: input.description.clone(),
        kind: ResourceKind::SlashCommand,
        scope: ResourceScope::Global,
        origin: ResourceOrigin::Private,
        source_status: SourceStatus::LocalOnly,
        project_id: None,
        tags: input.tags.clone(),
        content: input.content.clone(),
        revision: compute_revision(&input.content),
        source_url: None,
        source_ref: None,
        source_path: None,
        upstream_revision: None,
        forked_from: None,
        created_at: now,
        updated_at: now,
        provenance: Default::default(),
    };

    let resource_id = resource.id.clone();
    library.resources.push(resource.clone());
    attach_resource_to_profiles(
        &mut library.project_profiles,
        &resource_id,
        &input.project_ids,
    );
    save_repo_library(&repo_root, &library)?;
    ensure_slash_command_source_with_directories(
        app,
        &resource.slug,
        &resource.content,
        &input.directories,
    )?;

    let backup_sync =
        sync_after_mutation(app, &format!("Create slash command: {}", resource.title));

    Ok(CreateSlashCommandResult {
        command: resource_to_slash_command(&resource, &library),
        backup_sync,
    })
}

pub fn update_slash_command(
    app: &tauri::AppHandle,
    input: &UpdateSlashCommandInput,
) -> Result<SlashCommandDto, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let parsed_front_matter = input.content.as_ref().map(|content| {
        let (name, description, _) = parse_skill_front_matter(content);
        (name, description)
    });

    {
        let resource = library
            .resources
            .iter_mut()
            .find(|resource| resource.kind == ResourceKind::SlashCommand && resource.id == input.id)
            .ok_or_else(|| format!("slash command {} not found", input.id))?;

        if let Some(name) = &input.name {
            resource.title = name.clone();
        }
        if let Some(description) = &input.description {
            resource.description = description.clone();
        }
        if let Some(content) = &input.content {
            resource.content = content.clone();
            resource.revision = compute_revision(content);

            if input.name.is_none() {
                if let Some(front_name) = parsed_front_matter
                    .as_ref()
                    .and_then(|(name, _)| name.clone())
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                {
                    resource.title = front_name;
                }
            }

            if input.description.is_none() {
                if let Some(front_description) = parsed_front_matter
                    .as_ref()
                    .and_then(|(_, description)| description.clone())
                {
                    let trimmed = front_description.trim().to_string();
                    resource.description = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    };
                }
            }
        }
        if let Some(tags) = &input.tags {
            resource.tags = tags.clone();
        }
        resource.updated_at = now_ms();
    }

    if let Some(project_ids) = &input.project_ids {
        detach_resource_from_all_profiles(&mut library.project_profiles, &input.id);
        attach_resource_to_profiles(&mut library.project_profiles, &input.id, project_ids);
    }

    let resource = library
        .resources
        .iter()
        .find(|resource| resource.kind == ResourceKind::SlashCommand && resource.id == input.id)
        .ok_or_else(|| format!("slash command {} not found", input.id))?;
    let result = resource_to_slash_command(resource, &library);
    save_repo_library(&repo_root, &library)?;
    let source_dir = ensure_slash_command_source(app, &resource.slug, &resource.content)?;
    let cmd_md_path = source_dir.join("COMMAND.md");
    crate::ssh_sync::resync_command_to_enabled_ssh_apps(app, &resource.slug, &cmd_md_path);

    let _backup_sync = sync_after_mutation(app, &format!("Update slash command: {}", result.name));

    Ok(result)
}

pub fn sync_slash_command_from_source(
    app: &tauri::AppHandle,
    command_id: &str,
) -> Result<SlashCommandDto, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let resource_index = library
        .resources
        .iter()
        .position(|resource| {
            resource.kind == ResourceKind::SlashCommand && resource.id == command_id
        })
        .ok_or_else(|| format!("slash command {} not found", command_id))?;

    let command_dir = slash_command_source_dir_by_id(app, command_id)?;
    let command_file = command_dir.join("COMMAND.md");
    if !command_file.exists() {
        return Err("COMMAND.md 文件不存在".to_string());
    }

    let content =
        fs::read_to_string(&command_file).map_err(|e| format!("读取 COMMAND.md 失败：{}", e))?;
    let changed = {
        let resource = &mut library.resources[resource_index];
        let (name, description, tags) =
            derive_skill_source_metadata(&resource.slug, &content, Some(resource.tags.as_slice()));
        let revision = compute_revision(&content);
        let changed = resource.title != name
            || resource.description != description
            || resource.content != content
            || resource.tags != tags
            || resource.revision != revision;

        if changed {
            resource.title = name;
            resource.description = description;
            resource.content = content;
            resource.tags = tags;
            resource.revision = revision;
            resource.updated_at = now_ms();
        }

        changed
    };

    if changed {
        save_repo_library(&repo_root, &library)?;
        let resource_name = library.resources[resource_index].title.clone();
        let _backup_sync = sync_after_mutation(
            app,
            &format!("Update slash command from source: {}", resource_name),
        );
    }
    let slug = library.resources[resource_index].slug.clone();
    crate::ssh_sync::resync_command_to_enabled_ssh_apps(app, &slug, &command_file);

    Ok(resource_to_slash_command(
        &library.resources[resource_index],
        &library,
    ))
}

pub fn delete_slash_command(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let removed_slug = library
        .resources
        .iter()
        .find(|resource| resource.kind == ResourceKind::SlashCommand && resource.id == id)
        .map(|resource| resource.slug.clone())
        .ok_or_else(|| format!("slash command {} not found", id))?;
    let before = library.resources.len();
    library.resources.retain(|resource| resource.id != id);
    debug_assert!(before > library.resources.len());
    detach_resource_from_all_profiles(&mut library.project_profiles, id);
    save_repo_library(&repo_root, &library)?;

    let source_dir = slash_command_source_dir(app, &removed_slug)?;
    let custom_apps = load_custom_apps(app);
    if let Ok(home_dir) = app.path().home_dir() {
        for app_id in all_configured_app_ids(&custom_apps) {
            if crate::ssh_sync::is_ssh_app(&app_id, &custom_apps) {
                continue;
            }
            if let Ok(paths) = global_command_paths(&home_dir, &app_id, &removed_slug, &custom_apps)
            {
                for path in paths {
                    if is_symlink(&path) {
                        let _ = remove_symlink(&path);
                    }
                }
            }
        }
    }

    let initial_ledgers = load_local_state(app)
        .map(|s| s.ssh_ledgers)
        .unwrap_or_default();
    for (app_id, ledger) in &initial_ledgers {
        if ledger.commands.contains(&removed_slug) {
            if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
                let _ = crate::ssh_sync::uninstall_command_ssh(app, custom, &removed_slug);
            }
        }
    }
    if let Ok(mut state) = load_local_state(app) {
        let mut ledger_changed = false;
        for ledger in state.ssh_ledgers.values_mut() {
            let before = ledger.commands.len();
            ledger.commands.retain(|c| c != &removed_slug);
            if ledger.commands.len() != before {
                ledger_changed = true;
            }
        }
        if ledger_changed {
            let _ = save_local_state(app, &state);
        }
    }

    if source_dir.exists() {
        remove_path(&source_dir)?;
    }

    let _backup_sync = sync_after_mutation(app, &format!("Delete slash command: {}", removed_slug));

    Ok(())
}

pub fn search_slash_commands(
    app: &tauri::AppHandle,
    query: &str,
) -> Result<Vec<SlashCommandDto>, String> {
    let repo_root = connected_repo_root(app)?;
    let library = load_repo_library_for_legacy_skills(app, &repo_root)?;
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return list_slash_commands(app);
    }

    Ok(library
        .resources
        .iter()
        .filter(|resource| resource.kind == ResourceKind::SlashCommand)
        .filter(|resource| {
            resource.title.to_lowercase().contains(&needle)
                || resource
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&needle)
                || resource.content.to_lowercase().contains(&needle)
                || resource
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&needle))
        })
        .map(|resource| resource_to_slash_command(resource, &library))
        .collect())
}
