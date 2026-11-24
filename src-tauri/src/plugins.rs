use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use toml_edit::{DocumentMut, Item, Table};
use uuid::Uuid;

use crate::domain::{
    CheckPluginStatusInput, CheckPluginStatusResult, ClaudeProjectMcpMode, CustomAppConfig,
    DiscoveredPluginCandidate, ImportPluginBatchResult, ImportPluginFromFolderInput,
    ImportPluginFromGitInput, ImportPluginOutcome, ImportStagedPluginsInput, InstallMcpGlobalInput,
    InstallMcpToProjectInput, InstallPluginGlobalInput, InstallPluginResult,
    InstallPluginToProjectInput, InstallSkillGlobalInput, InstallSkillToProjectInput,
    InstallSlashCommandGlobalInput, InstallSlashCommandToProjectInput, McpEnvVarSpec,
    McpHeaderSpec, McpServerRecord, PluginAppInstallStatus, PluginComponentSummary, PluginDto,
    PluginInstallMode, PluginManifestKind, PluginRecord, PluginUserConfigField, Provenance,
    ProvenanceKind, RepoLibrary, Resource, ResourceKind, ResourceOrigin, ResourceScope,
    SourceStatus, UpdatePluginConfigInput,
};
use crate::{git, mcp, secrets, store};

// ─── Staged Multi-Plugin Import Session Store ─────────────────────────────────

struct StagedPluginImport {
    root_dir: PathBuf,
    cleanup_dir: Option<PathBuf>,
    provenance_base: Provenance,
    created_at_ms: i64,
}

impl StagedPluginImport {
    fn cleanup_disk(&self) {
        if let Some(ref dir) = self.cleanup_dir {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

fn staged_imports_store() -> &'static Mutex<HashMap<String, StagedPluginImport>> {
    static STORE: OnceLock<Mutex<HashMap<String, StagedPluginImport>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

const STAGED_TTL_MS: i64 = 30 * 60 * 1000;

fn register_staged_import(entry: StagedPluginImport) -> String {
    let token = Uuid::new_v4().to_string();
    let now = store::now_ms();
    if let Ok(mut guard) = staged_imports_store().lock() {
        let expired: Vec<String> = guard
            .iter()
            .filter(|(_, v)| now.saturating_sub(v.created_at_ms) > STAGED_TTL_MS)
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired {
            if let Some(old) = guard.remove(&key) {
                old.cleanup_disk();
            }
        }
        guard.insert(token.clone(), entry);
    }
    token
}

fn take_staged_import(staging_id: &str) -> Option<StagedPluginImport> {
    staged_imports_store()
        .lock()
        .ok()
        .and_then(|mut guard| guard.remove(staging_id))
}

pub fn discard_staged_import(staging_id: &str) {
    if let Some(staged) = take_staged_import(staging_id) {
        staged.cleanup_disk();
    }
}

// ─── Paths & DTOs ─────────────────────────────────────────────────────────────

pub fn plugin_sources_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(clone_dir) = store::persistent_clone_dir(app)? {
        if clone_dir.exists() {
            return Ok(clone_dir.join(store::PLUGIN_SOURCES_DIR));
        }
    }
    app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())
        .map(|p| p.join(store::PLUGIN_SOURCES_DIR))
}

pub fn plugin_source_dir(app: &tauri::AppHandle, slug: &str) -> Result<PathBuf, String> {
    plugin_sources_dir(app).map(|d| d.join(slug))
}

pub fn record_to_dto(app: &tauri::AppHandle, record: &PluginRecord) -> Result<PluginDto, String> {
    let source_dir = plugin_source_dir(app, &record.slug)?;
    let configured_secret_keys =
        secrets::configured_secret_keys(app, &secrets::plugin_namespace(&record.id));
    Ok(PluginDto {
        id: record.id.clone(),
        slug: record.slug.clone(),
        name: record.name.clone(),
        version: record.version.clone(),
        author: record.author.clone(),
        description: record.description.clone(),
        homepage: record.homepage.clone(),
        repository_url: record.repository_url.clone(),
        manifest_kind: record.manifest_kind,
        source_dir_path: source_dir.to_string_lossy().into_owned(),
        user_config_schema: record.user_config_schema.clone(),
        user_config_values: record.user_config_values.clone(),
        configured_secret_keys,
        components: record.components.clone(),
        projected_skill_ids: record.projected_skill_ids.clone(),
        projected_command_ids: record.projected_command_ids.clone(),
        projected_mcp_server_ids: record.projected_mcp_server_ids.clone(),
        tags: record.tags.clone(),
        created_at: record.created_at,
        updated_at: record.updated_at,
        provenance: record.provenance.clone(),
    })
}

pub fn list_plugins(app: &tauri::AppHandle) -> Result<Vec<PluginDto>, String> {
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let mut out = Vec::new();
    for rec in &library.plugins {
        out.push(record_to_dto(app, rec)?);
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    Ok(out)
}

pub fn get_plugin(app: &tauri::AppHandle, id: &str) -> Result<Option<PluginDto>, String> {
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    match library.plugins.iter().find(|p| p.id == id) {
        Some(rec) => Ok(Some(record_to_dto(app, rec)?)),
        None => Ok(None),
    }
}

// ─── Plugin Inspection & Multi-Plugin / Marketplace Discovery ─────────────────

#[derive(Debug, Clone, Default)]
pub struct MarketplaceEntryHint {
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository_url: Option<String>,
    pub category: Option<String>,
    pub raw_entry: Option<JsonValue>,
}

#[derive(Debug)]
pub struct DiscoveredPluginBundle {
    pub manifest_kind: PluginManifestKind,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository_url: Option<String>,
    pub user_config_schema: Vec<PluginUserConfigField>,
    pub components: PluginComponentSummary,
    /// `(skill_slug, rel_dir_from_plugin_root)`
    pub skill_dirs: Vec<(String, PathBuf)>,
    /// `(cmd_slug, rel_file_from_plugin_root)`
    pub command_files: Vec<(String, PathBuf)>,
    pub inline_or_file_mcp: HashMap<String, mcp::ParsedRawMcpEntry>,
    pub warnings: Vec<String>,
}

pub fn inspect_plugin_directory(plugin_root: &Path) -> Result<DiscoveredPluginBundle, String> {
    inspect_plugin_directory_opts(plugin_root, None, false)
}

fn inspect_plugin_directory_opts(
    plugin_root: &Path,
    hint: Option<&MarketplaceEntryHint>,
    allow_root_skill_md: bool,
) -> Result<DiscoveredPluginBundle, String> {
    if !plugin_root.exists() || !plugin_root.is_dir() {
        return Err(format!("插件目录不存在: {:?}", plugin_root));
    }

    let manifest_candidates: &[(&str, PluginManifestKind)] = &[
        (
            ".claude-plugin/plugin.json",
            PluginManifestKind::ClaudePlugin,
        ),
        ("plugin.json", PluginManifestKind::AgentPlugins),
        (".codex-plugin/plugin.json", PluginManifestKind::CodexPlugin),
        (
            ".cursor-plugin/plugin.json",
            PluginManifestKind::CursorPlugin,
        ),
        (
            ".antigravity-plugin/plugin.json",
            PluginManifestKind::AntigravityPlugin,
        ),
        (
            ".gemini-plugin/plugin.json",
            PluginManifestKind::AntigravityPlugin,
        ),
        ("gemini-extension.json", PluginManifestKind::GeminiExtension),
    ];

    let mut manifest_kind = PluginManifestKind::Generic;
    let mut loaded_manifests: Vec<JsonValue> = Vec::new();

    for (rel, kind) in manifest_candidates {
        let candidate = plugin_root.join(rel);
        if candidate.is_file() {
            let raw = fs::read_to_string(&candidate)
                .map_err(|e| format!("读取插件清单 {:?} 失败: {}", candidate, e))?;
            let val: JsonValue = serde_json::from_str(&raw)
                .map_err(|e| format!("解析插件清单 {:?} 失败: {}", candidate, e))?;
            if loaded_manifests.is_empty() {
                manifest_kind = *kind;
            }
            loaded_manifests.push(val);
        }
    }

    if let Some(h) = hint {
        if let Some(ref raw_entry) = h.raw_entry {
            loaded_manifests.push(raw_entry.clone());
        }
    }

    let fallback_name = plugin_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "plugin".to_string());

    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut author: Option<String> = None;
    let mut description: Option<String> = None;
    let mut homepage: Option<String> = None;
    let mut repository_url: Option<String> = None;
    let mut user_config_schema: Vec<PluginUserConfigField> = Vec::new();
    let mut seen_config_keys: HashSet<String> = HashSet::new();
    let mut inline_or_file_mcp: HashMap<String, mcp::ParsedRawMcpEntry> = HashMap::new();
    let mut extra_skill_paths: Vec<String> = Vec::new();
    let mut extra_command_paths: Vec<String> = Vec::new();
    let mut extra_mcp_paths: Vec<String> = Vec::new();

    for mf in &loaded_manifests {
        if name.is_none() {
            name = mf
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        if version.is_none() {
            version = mf
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        if author.is_none() {
            author = extract_author_field(mf);
        }
        if description.is_none() {
            description = mf
                .get("description")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    mf.pointer("/interface/shortDescription")
                        .and_then(|v| v.as_str())
                })
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        if homepage.is_none() {
            homepage = mf
                .get("homepage")
                .and_then(|v| v.as_str())
                .or_else(|| mf.pointer("/interface/websiteURL").and_then(|v| v.as_str()))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        if repository_url.is_none() {
            repository_url = extract_repo_field(mf);
        }

        // Parse Claude / Agent Plugins `userConfig` object OR Gemini Extension `settings` array
        if let Some(uc_obj) = mf.get("userConfig").and_then(|v| v.as_object()) {
            let mut keys: Vec<&String> = uc_obj.keys().collect();
            keys.sort();
            for key in keys {
                if seen_config_keys.contains(key) {
                    continue;
                }
                if let Some(field_obj) = uc_obj.get(key).and_then(|v| v.as_object()) {
                    let sensitive = field_obj
                        .get("sensitive")
                        .or_else(|| field_obj.get("secret"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or_else(|| mcp::looks_like_secret_env(key, ""));
                    let title = field_obj
                        .get("title")
                        .or_else(|| field_obj.get("label"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(key.as_str())
                        .to_string();
                    let desc = field_obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let required = field_obj
                        .get("required")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let default_value = if sensitive {
                        None
                    } else {
                        field_obj
                            .get("default")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    };
                    seen_config_keys.insert(key.clone());
                    user_config_schema.push(PluginUserConfigField {
                        key: key.clone(),
                        title,
                        description: desc,
                        required,
                        sensitive,
                        default_value,
                    });
                }
            }
        } else if let Some(settings_arr) = mf.get("settings").and_then(|v| v.as_array()) {
            // Gemini CLI `gemini-extension.json` settings array
            for item in settings_arr {
                if let Some(obj) = item.as_object() {
                    let Some(env_var) = obj
                        .get("envVar")
                        .or_else(|| obj.get("name"))
                        .and_then(|v| v.as_str())
                    else {
                        continue;
                    };
                    if seen_config_keys.contains(env_var) {
                        continue;
                    }
                    let sensitive = obj
                        .get("sensitive")
                        .and_then(|v| v.as_bool())
                        .unwrap_or_else(|| mcp::looks_like_secret_env(env_var, ""));
                    let title = obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(env_var)
                        .to_string();
                    let desc = obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    seen_config_keys.insert(env_var.to_string());
                    user_config_schema.push(PluginUserConfigField {
                        key: env_var.to_string(),
                        title,
                        description: desc,
                        required: false,
                        sensitive,
                        default_value: None,
                    });
                }
            }
        }

        // Inline `mcpServers` inside manifest OR relative file path string/array
        if let Some(mcp_val) = mf.get("mcpServers") {
            if let Some(inline_mcp_obj) = mcp_val.as_object() {
                for (srv_key, srv_val) in inline_mcp_obj {
                    if let Some(srv_obj) = srv_val.as_object() {
                        if let Some(parsed) = mcp::parse_json_mcp_object(srv_obj) {
                            inline_or_file_mcp.entry(srv_key.clone()).or_insert(parsed);
                        }
                    }
                }
            } else {
                extra_mcp_paths.extend(collect_string_or_array(mcp_val));
            }
        }

        if let Some(skills_val) = mf.get("skills") {
            extra_skill_paths.extend(collect_string_or_array(skills_val));
        }
        if let Some(cmds_val) = mf.get("commands") {
            extra_command_paths.extend(collect_string_or_array(cmds_val));
        }
    }

    if let Some(h) = hint {
        if name.is_none() {
            name = h.name.clone();
        }
        if version.is_none() {
            version = h.version.clone();
        }
        if author.is_none() {
            author = h.author.clone();
        }
        if description.is_none() {
            description = h.description.clone();
        }
        if homepage.is_none() {
            homepage = h.homepage.clone();
        }
        if repository_url.is_none() {
            repository_url = h.repository_url.clone();
        }
    }

    // Discover MCP files: `.mcp.json` (Claude), `mcp.json` (Agent Plugins / Cursor / Codex), `mcp_config.json` (Antigravity), plus manifest paths
    let mut mcp_rel_candidates: Vec<String> = vec![
        ".mcp.json".to_string(),
        "mcp.json".to_string(),
        "mcp_config.json".to_string(),
    ];
    mcp_rel_candidates.extend(extra_mcp_paths);

    for mcp_rel_raw in mcp_rel_candidates {
        let Some(clean_rel) = sanitize_plugin_rel_subpath(&mcp_rel_raw) else {
            continue;
        };
        let mcp_file = plugin_root.join(&clean_rel);
        if mcp_file.is_file() {
            if let Ok(raw) = fs::read_to_string(&mcp_file) {
                let entries = mcp::parse_external_entries_from_raw(
                    &raw,
                    mcp::ConfigFormat::Json,
                    "mcpServers",
                );
                for (k, v) in entries {
                    inline_or_file_mcp.insert(k, v);
                }
            }
        }
    }

    let mut warnings = Vec::new();
    let mut skill_dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut seen_skill_paths: HashSet<PathBuf> = HashSet::new();

    let mut skill_search_paths = vec!["skills".to_string()];
    skill_search_paths.extend(extra_skill_paths);

    for raw_skill_path in skill_search_paths {
        let Some(rel_base) = sanitize_plugin_rel_subpath(&raw_skill_path) else {
            continue;
        };
        let full_base = plugin_root.join(&rel_base);
        if !full_base.is_dir() {
            continue;
        }
        // Direct skill folder specified in manifest (e.g. `"skills": ["./my-skill"]`)
        if rel_base != Path::new(".") && full_base.join("SKILL.md").is_file() {
            let slug = full_base
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "skill".to_string());
            push_discovered_skill(
                plugin_root,
                slug,
                rel_base,
                &mut skill_dirs,
                &mut seen_skill_paths,
                &mut warnings,
            );
            continue;
        }

        if let Ok(entries) = fs::read_dir(&full_base) {
            for entry in entries.flatten() {
                let p = entry.path();
                let child_name = entry.file_name().to_string_lossy().into_owned();
                if child_name.starts_with('.') || child_name == "node_modules" {
                    continue;
                }
                if p.is_dir() && p.join("SKILL.md").is_file() {
                    let rel_p = join_rel_clean(&rel_base, &child_name);
                    push_discovered_skill(
                        plugin_root,
                        child_name,
                        rel_p,
                        &mut skill_dirs,
                        &mut seen_skill_paths,
                        &mut warnings,
                    );
                } else if p.is_dir() {
                    // Support 1-level grouped skills (e.g. `skills/category/my-skill/SKILL.md`)
                    if let Ok(sub_entries) = fs::read_dir(&p) {
                        for sub in sub_entries.flatten() {
                            let sp = sub.path();
                            let sub_name = sub.file_name().to_string_lossy().into_owned();
                            if sub_name.starts_with('.') || sub_name == "node_modules" {
                                continue;
                            }
                            if sp.is_dir() && sp.join("SKILL.md").is_file() {
                                let rel_sp = join_rel_clean(&rel_base, &child_name).join(&sub_name);
                                push_discovered_skill(
                                    plugin_root,
                                    sub_name,
                                    rel_sp,
                                    &mut skill_dirs,
                                    &mut seen_skill_paths,
                                    &mut warnings,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Standalone single-skill root fallback when opted in
    if skill_dirs.is_empty() && allow_root_skill_md && plugin_root.join("SKILL.md").is_file() {
        let root_skill_raw = fs::read_to_string(plugin_root.join("SKILL.md")).unwrap_or_default();
        let (parsed_skill_name, parsed_skill_desc) = store::parse_skill_metadata(&root_skill_raw);
        let root_slug = if !parsed_skill_name.is_empty() {
            store::slugify(&parsed_skill_name)
        } else {
            store::slugify(&fallback_name)
        };
        if name.is_none() && !parsed_skill_name.is_empty() {
            name = Some(parsed_skill_name);
        }
        if description.is_none() {
            description = parsed_skill_desc;
        }
        push_discovered_skill(
            plugin_root,
            root_slug,
            PathBuf::from("."),
            &mut skill_dirs,
            &mut seen_skill_paths,
            &mut warnings,
        );
    }

    skill_dirs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut command_files: Vec<(String, PathBuf)> = Vec::new();
    let mut seen_cmd_paths: HashSet<PathBuf> = HashSet::new();
    let mut cmd_search_paths = vec!["commands".to_string()];
    cmd_search_paths.extend(extra_command_paths);

    for raw_cmd_path in cmd_search_paths {
        let Some(rel_cmd_base) = sanitize_plugin_rel_subpath(&raw_cmd_path) else {
            continue;
        };
        let full_cmd_base = plugin_root.join(&rel_cmd_base);
        if full_cmd_base.is_file()
            && full_cmd_base.extension().and_then(|e| e.to_str()) == Some("md")
        {
            if let Some(stem) = full_cmd_base.file_stem() {
                if seen_cmd_paths.insert(rel_cmd_base.clone()) {
                    command_files.push((stem.to_string_lossy().into_owned(), rel_cmd_base));
                }
            }
            continue;
        }
        if !full_cmd_base.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&full_cmd_base) {
            for entry in entries.flatten() {
                let p = entry.path();
                let entry_name = entry.file_name().to_string_lossy().into_owned();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Some(stem) = p.file_stem() {
                        let rel_file = join_rel_clean(&rel_cmd_base, &entry_name);
                        if seen_cmd_paths.insert(rel_file.clone()) {
                            command_files.push((stem.to_string_lossy().into_owned(), rel_file));
                        }
                    }
                } else if p.is_dir() && p.join("COMMAND.md").is_file() {
                    let rel_file = join_rel_clean(&rel_cmd_base, &entry_name).join("COMMAND.md");
                    if seen_cmd_paths.insert(rel_file.clone()) {
                        command_files.push((entry_name, rel_file));
                    }
                }
            }
        }
    }
    command_files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hooks = Vec::new();
    for hook_rel in ["hooks/hooks.json", "hooks/hooks-cursor.json", "hooks.json"] {
        let hp = plugin_root.join(hook_rel);
        if hp.is_file() {
            hooks.push(hook_rel.to_string());
        }
    }

    let mut agents = Vec::new();
    let agents_root = plugin_root.join("agents");
    if agents_root.is_dir() {
        if let Ok(entries) = fs::read_dir(&agents_root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Some(stem) = p.file_stem() {
                        agents.push(stem.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    agents.sort();

    let mut rules = Vec::new();
    let rules_root = plugin_root.join("rules");
    if rules_root.is_dir() {
        if let Ok(entries) = fs::read_dir(&rules_root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    rules.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    for root_rule in ["GEMINI.md", "AGENTS.md", "CLAUDE.md"] {
        if plugin_root.join(root_rule).is_file() {
            rules.push(root_rule.to_string());
        }
    }
    rules.sort();

    let has_manifest = !loaded_manifests.is_empty();
    if !has_manifest
        && skill_dirs.is_empty()
        && command_files.is_empty()
        && inline_or_file_mcp.is_empty()
    {
        return Err(
            "目录中未检测到有效插件清单（plugin.json / gemini-extension.json）或可管理的 skills / MCP 配置"
                .to_string(),
        );
    }

    let mut mcp_keys: Vec<String> = inline_or_file_mcp.keys().cloned().collect();
    mcp_keys.sort();

    let components = PluginComponentSummary {
        skills: skill_dirs.iter().map(|(s, _)| s.clone()).collect(),
        commands: command_files.iter().map(|(c, _)| c.clone()).collect(),
        mcp_servers: mcp_keys,
        hooks,
        agents,
        rules,
    };

    Ok(DiscoveredPluginBundle {
        manifest_kind,
        name: name.unwrap_or(fallback_name),
        version,
        author,
        description,
        homepage,
        repository_url,
        user_config_schema,
        components,
        skill_dirs,
        command_files,
        inline_or_file_mcp,
        warnings,
    })
}

fn push_discovered_skill(
    plugin_root: &Path,
    slug: String,
    rel_path: PathBuf,
    skill_dirs: &mut Vec<(String, PathBuf)>,
    seen_skill_paths: &mut HashSet<PathBuf>,
    warnings: &mut Vec<String>,
) {
    if !seen_skill_paths.insert(rel_path.clone()) {
        return;
    }
    let skill_md = if rel_path == Path::new(".") {
        plugin_root.join("SKILL.md")
    } else {
        plugin_root.join(&rel_path).join("SKILL.md")
    };
    if let Ok(skill_text) = fs::read_to_string(skill_md) {
        if skill_text.contains("mcp__plugin_") {
            warnings.push(format!(
                "技能「{}」引用了 Claude 插件专属工具前缀 `mcp__plugin_*`，跨端解包投射时已自动适配但建议检查 allowed-tools",
                slug
            ));
        }
    }
    skill_dirs.push((slug, rel_path));
}

#[derive(Debug)]
struct DiscoveredCandidateInternal {
    sub_path: String,
    category: Option<String>,
    hint: Option<MarketplaceEntryHint>,
    bundle: DiscoveredPluginBundle,
}

#[derive(Debug)]
struct DiscoveredSource {
    marketplace_name: Option<String>,
    candidates: Vec<DiscoveredCandidateInternal>,
}

/// Discover either a single plugin OR multiple plugins (via `marketplace.json` or `plugins/` monorepo layout).
fn discover_plugins_in_source(root_dir: &Path) -> Result<DiscoveredSource, String> {
    if !root_dir.exists() || !root_dir.is_dir() {
        return Err(format!("插件目录不存在: {:?}", root_dir));
    }

    let marketplace_files = [
        ".claude-plugin/marketplace.json",
        ".agents/plugins/marketplace.json",
        ".codex-plugin/marketplace.json",
        ".cursor-plugin/marketplace.json",
        "marketplace.json",
    ];

    let mut marketplace_name: Option<String> = None;
    let mut candidates: Vec<DiscoveredCandidateInternal> = Vec::new();
    let mut seen_canon_dirs: HashSet<PathBuf> = HashSet::new();
    let mut remote_only_count = 0usize;

    for mf_rel in marketplace_files {
        let mf_path = root_dir.join(mf_rel);
        if !mf_path.is_file() {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&mf_path) else {
            continue;
        };
        let Ok(mf_json) = serde_json::from_str::<JsonValue>(&raw) else {
            continue;
        };
        let Some(plugins_arr) = mf_json.get("plugins").and_then(|v| v.as_array()) else {
            continue;
        };

        marketplace_name = mf_json
            .pointer("/interface/displayName")
            .and_then(|v| v.as_str())
            .or_else(|| mf_json.get("name").and_then(|v| v.as_str()))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let plugin_root_prefix = mf_json
            .get("pluginRoot")
            .and_then(|v| v.as_str())
            .or_else(|| {
                mf_json
                    .pointer("/metadata/pluginRoot")
                    .and_then(|v| v.as_str())
            })
            .and_then(sanitize_plugin_rel_subpath);

        let owner_author = extract_author_field(&mf_json).or_else(|| {
            mf_json
                .pointer("/owner/name")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });

        for entry in plugins_arr {
            let Some(entry_obj) = entry.as_object() else {
                continue;
            };
            let hint = MarketplaceEntryHint {
                name: entry_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                version: entry_obj
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                author: extract_author_field(entry).or_else(|| owner_author.clone()),
                description: entry_obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                homepage: entry_obj
                    .get("homepage")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                repository_url: extract_repo_field(entry),
                category: entry_obj
                    .get("category")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                raw_entry: Some(entry.clone()),
            };

            let Some(local_raw_path) =
                extract_local_marketplace_source_path(entry_obj, &mut remote_only_count)
            else {
                continue;
            };

            let Some(resolved_rel) = resolve_marketplace_entry_rel_dir(
                root_dir,
                plugin_root_prefix.as_deref(),
                &local_raw_path,
            ) else {
                continue;
            };

            let full_dir = if resolved_rel == Path::new(".") {
                root_dir.to_path_buf()
            } else {
                root_dir.join(&resolved_rel)
            };
            if !full_dir.is_dir() {
                continue;
            }
            let canon = full_dir.canonicalize().unwrap_or_else(|_| full_dir.clone());
            if !seen_canon_dirs.insert(canon) {
                continue;
            }

            if let Ok(bundle) = inspect_plugin_directory_opts(&full_dir, Some(&hint), true) {
                let sub_path = path_to_slash_rel(&resolved_rel);
                candidates.push(DiscoveredCandidateInternal {
                    sub_path,
                    category: hint.category.clone(),
                    hint: Some(hint),
                    bundle,
                });
            }
        }

        if !candidates.is_empty() {
            break;
        }
    }

    // 2. Direct single-plugin check at root_dir (when not a multi-plugin marketplace)
    if candidates.is_empty() {
        if let Ok(bundle) = inspect_plugin_directory(root_dir) {
            candidates.push(DiscoveredCandidateInternal {
                sub_path: ".".to_string(),
                category: None,
                hint: None,
                bundle,
            });
        }
    }

    // 3. Monorepo / collection container scan (`plugins/*`, `extensions/*`, `external_plugins/*`, `packages/*`)
    if candidates.is_empty() {
        for container in ["plugins", "extensions", "external_plugins", "packages"] {
            let container_dir = root_dir.join(container);
            if !container_dir.is_dir() {
                continue;
            }
            for child in sorted_subdirs(&container_dir) {
                let canon = child.canonicalize().unwrap_or_else(|_| child.clone());
                if seen_canon_dirs.contains(&canon) {
                    continue;
                }
                if let Ok(bundle) = inspect_plugin_directory_opts(&child, None, true) {
                    seen_canon_dirs.insert(canon);
                    if let Ok(rel) = child.strip_prefix(root_dir) {
                        candidates.push(DiscoveredCandidateInternal {
                            sub_path: path_to_slash_rel(rel),
                            category: None,
                            hint: None,
                            bundle,
                        });
                    }
                } else {
                    // Check 1 level deeper (e.g. `plugins/group/plugin-name`)
                    for grand in sorted_subdirs(&child) {
                        let g_canon = grand.canonicalize().unwrap_or_else(|_| grand.clone());
                        if seen_canon_dirs.contains(&g_canon) {
                            continue;
                        }
                        if let Ok(bundle) = inspect_plugin_directory_opts(&grand, None, true) {
                            seen_canon_dirs.insert(g_canon);
                            if let Ok(rel) = grand.strip_prefix(root_dir) {
                                candidates.push(DiscoveredCandidateInternal {
                                    sub_path: path_to_slash_rel(rel),
                                    category: None,
                                    hint: None,
                                    bundle,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Direct child directories fallback (e.g. an unzipped bundle folder containing plugin folders)
    if candidates.is_empty() {
        for child in sorted_subdirs(root_dir) {
            let canon = child.canonicalize().unwrap_or_else(|_| child.clone());
            if seen_canon_dirs.contains(&canon) {
                continue;
            }
            if let Ok(bundle) = inspect_plugin_directory(&child) {
                seen_canon_dirs.insert(canon);
                if let Ok(rel) = child.strip_prefix(root_dir) {
                    candidates.push(DiscoveredCandidateInternal {
                        sub_path: path_to_slash_rel(rel),
                        category: None,
                        hint: None,
                        bundle,
                    });
                }
            }
        }
    }

    // 5. Root standalone `SKILL.md` fallback (single-skill repo imported via Plugins page)
    if candidates.is_empty() {
        if let Ok(bundle) = inspect_plugin_directory_opts(root_dir, None, true) {
            candidates.push(DiscoveredCandidateInternal {
                sub_path: ".".to_string(),
                category: None,
                hint: None,
                bundle,
            });
        }
    }

    if candidates.is_empty() {
        if remote_only_count > 0 {
            return Err(format!(
                "该仓库清单包含 {} 个外部远程仓库索引，当前目录内不含本地插件源码；请直接粘贴目标插件仓库地址导入",
                remote_only_count
            ));
        }
        return Err(
            "目录中未检测到有效插件清单（plugin.json / marketplace.json / gemini-extension.json）或可管理的 skills / MCP 配置"
                .to_string(),
        );
    }

    Ok(DiscoveredSource {
        marketplace_name,
        candidates,
    })
}

/// Expand `${CLAUDE_PLUGIN_ROOT}`, `${PLUGIN_ROOT}`, `${extensionPath}`, and `${user_config.<KEY>}`!
pub fn expand_plugin_macros(
    raw: &str,
    plugin_source_dir: &Path,
    user_config_values: &HashMap<String, String>,
    sensitive_keys: &HashSet<String>,
) -> String {
    let root_str = plugin_source_dir.to_string_lossy();
    let mut out = raw
        .replace("${CLAUDE_PLUGIN_ROOT}", &root_str)
        .replace("${PLUGIN_ROOT}", &root_str)
        .replace("${CURSOR_PLUGIN_ROOT}", &root_str)
        .replace("${extensionPath}", &root_str);

    for (k, v) in user_config_values {
        if !sensitive_keys.contains(k) {
            out = out.replace(&format!("${{user_config.{}}}", k), v);
        }
    }

    // Any remaining `${user_config.SECRET_KEY}` becomes `${SECRET_KEY}` so MCP secret resolution handles it safely!
    for sk in sensitive_keys {
        out = out.replace(&format!("${{user_config.{}}}", sk), &format!("${{{}}}", sk));
    }

    out
}

// ─── Git URL & Shorthand Parsing ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGitPluginSpec {
    pub repo_url: String,
    pub branch: Option<String>,
    pub sub_path: Option<String>,
    pub raw_tree_tail: Option<String>,
}

pub fn parse_git_plugin_spec(
    raw_input: &str,
    explicit_sub_path: Option<&str>,
    explicit_branch: Option<&str>,
) -> Result<ParsedGitPluginSpec, String> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return Err("Git 仓库地址不能为空".to_string());
    }

    let mut extracted_target: Option<String> = None;
    let mut cli_branch: Option<String> = None;

    if trimmed.split_whitespace().count() > 1 {
        let tokens: Vec<&str> = trimmed.split_whitespace().filter(|t| *t != "\\").collect();
        let mut i = 0;
        while i < tokens.len() {
            let tok = tokens[i];
            if matches!(tok, "--ref" | "-b" | "--branch") {
                if let Some(val) = tokens.get(i + 1) {
                    cli_branch = Some((*val).trim_matches(&['"', '\''][..]).to_string());
                }
                i += 2;
                continue;
            }
            if matches!(tok, "--sparse" | "--scope" | "--target" | "-t" | "-s") {
                i += 2;
                continue;
            }
            if tok.starts_with('-')
                || matches!(
                    tok,
                    "git"
                        | "clone"
                        | "npx"
                        | "pnpm"
                        | "dlx"
                        | "plugins"
                        | "plugin"
                        | "skills"
                        | "codex"
                        | "claude"
                        | "marketplace"
                        | "add"
                        | "install"
                )
            {
                i += 1;
                continue;
            }
            let clean_tok = tok.trim_matches(&['"', '\''][..]);
            if extracted_target.is_none()
                && (clean_tok.contains('/') || clean_tok.starts_with("git@"))
            {
                extracted_target = Some(clean_tok.to_string());
            }
            i += 1;
        }
    }

    let target_raw = extracted_target.unwrap_or_else(|| trimmed.to_string());
    let without_query = target_raw
        .split('?')
        .next()
        .unwrap_or(&target_raw)
        .split('#')
        .next()
        .unwrap_or(&target_raw)
        .trim()
        .trim_end_matches('/');

    let mut repo_url = without_query.to_string();
    let mut inferred_branch: Option<String> = None;
    let mut inferred_sub_path: Option<String> = None;
    let mut raw_tree_tail: Option<String> = None;

    let normalized_web = without_query
        .strip_prefix("github.com/")
        .or_else(|| without_query.strip_prefix("https://github.com/"))
        .or_else(|| without_query.strip_prefix("http://github.com/"))
        .map(|rest| ("https://github.com", rest))
        .or_else(|| {
            without_query
                .strip_prefix("gitlab.com/")
                .or_else(|| without_query.strip_prefix("https://gitlab.com/"))
                .map(|rest| ("https://gitlab.com", rest))
        });

    if let Some((host, rest)) = normalized_web {
        // GitLab uses `/-/tree/<branch>/<subpath>` or `/-/blob/<branch>/<subpath>`
        if let Some((repo_part, after_tree)) = rest
            .split_once("/-/tree/")
            .or_else(|| rest.split_once("/-/blob/"))
        {
            let is_blob = rest.contains("/-/blob/");
            repo_url = format!("{}/{}.git", host, repo_part.trim_end_matches(".git"));
            raw_tree_tail = Some(after_tree.to_string());
            parse_branch_and_sub_from_tail(
                after_tree,
                is_blob,
                &mut inferred_branch,
                &mut inferred_sub_path,
            );
        } else {
            let segs: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
            if segs.len() >= 2 {
                let owner = segs[0];
                let repo = segs[1].trim_end_matches(".git");
                repo_url = format!("{}/{}/{}.git", host, owner, repo);
                if segs.len() >= 4 && (segs[2] == "tree" || segs[2] == "blob") {
                    let is_blob = segs[2] == "blob";
                    let tail = segs[3..].join("/");
                    raw_tree_tail = Some(tail.clone());
                    parse_branch_and_sub_from_tail(
                        &tail,
                        is_blob,
                        &mut inferred_branch,
                        &mut inferred_sub_path,
                    );
                }
            }
        }
    } else if !without_query.contains("://")
        && !without_query.starts_with("git@")
        && !without_query.starts_with('/')
        && !without_query.starts_with('~')
        && !without_query.starts_with('.')
    {
        // Shorthand `owner/repo` or `owner/repo/sub/path`
        let segs: Vec<&str> = without_query.split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() >= 2 && !segs[0].contains(':') {
            let owner = segs[0];
            let repo = segs[1].trim_end_matches(".git");
            repo_url = format!("https://github.com/{}/{}.git", owner, repo);
            if segs.len() > 2 {
                inferred_sub_path = Some(segs[2..].join("/"));
            }
        }
    }

    let final_branch = explicit_branch
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or(cli_branch)
        .or(inferred_branch);

    let final_sub_path = explicit_sub_path
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or(inferred_sub_path);

    Ok(ParsedGitPluginSpec {
        repo_url,
        branch: final_branch,
        sub_path: final_sub_path,
        raw_tree_tail,
    })
}

fn parse_branch_and_sub_from_tail(
    tail: &str,
    is_blob: bool,
    inferred_branch: &mut Option<String>,
    inferred_sub_path: &mut Option<String>,
) {
    let segs: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return;
    }
    *inferred_branch = Some(segs[0].to_string());
    if segs.len() > 1 {
        let mut sub_segs = segs[1..].to_vec();
        if is_blob && !sub_segs.is_empty() {
            sub_segs.pop();
        }
        if !sub_segs.is_empty() {
            *inferred_sub_path = Some(sub_segs.join("/"));
        }
    }
}

// ─── Import Entrypoints (Single & Multi-Plugin / Marketplace) ─────────────────

pub fn import_plugin_from_folder(
    app: &tauri::AppHandle,
    input: &ImportPluginFromFolderInput,
) -> Result<ImportPluginOutcome, String> {
    let src_dir = store::normalize_user_path(&input.folder_path)?;
    let source_label = src_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.folder_path.clone());
    let provenance = Provenance {
        kind: ProvenanceKind::FileImport,
        label: "本地目录导入".to_string(),
        source_path: Some(src_dir.to_string_lossy().into_owned()),
        ..Default::default()
    };
    resolve_or_stage_plugin_import(
        app,
        src_dir,
        None,
        source_label,
        input.project_components_to_library.unwrap_or(true),
        provenance,
    )
}

pub fn import_plugin_from_dialog(
    app: &tauri::AppHandle,
) -> Result<Option<ImportPluginOutcome>, String> {
    if let Some(folder) = app
        .dialog()
        .file()
        .set_title("选择插件目录或插件合集仓库目录（含 plugin.json / marketplace.json / skills）")
        .blocking_pick_folder()
    {
        let folder_path = folder
            .into_path()
            .map_err(|e| format!("读取插件目录路径失败: {}", e))?;
        return store::with_mutation_lock(|| {
            import_plugin_from_folder(
                app,
                &ImportPluginFromFolderInput {
                    folder_path: folder_path.to_string_lossy().into_owned(),
                    project_components_to_library: Some(true),
                },
            )
        })
        .map(Some);
    }
    Ok(None)
}

pub fn import_plugin_from_zip(
    app: &tauri::AppHandle,
    zip_path_str: &str,
) -> Result<ImportPluginOutcome, String> {
    use zip::ZipArchive;

    let zip_path = store::normalize_user_path(zip_path_str)?;
    if !zip_path.is_file() {
        return Err(format!("ZIP 文件不存在: {}", zip_path_str));
    }

    let temp_extract =
        std::env::temp_dir().join(format!("skill-switch-plugin-zip-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_extract).map_err(|e| format!("创建临时解压目录失败: {}", e))?;

    let extract_res = (|| -> Result<PathBuf, String> {
        let file = fs::File::open(&zip_path).map_err(|e| format!("打开 ZIP 失败: {}", e))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("解析 ZIP 归档失败: {}", e))?;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("读取 ZIP 条目失败: {}", e))?;
            let raw_name = entry.name().to_string();
            // ZipSlip protection
            let rel_path = sanitize_zip_rel_path(&raw_name)?;
            if rel_path.as_os_str().is_empty() {
                continue;
            }
            let out_path = temp_extract.join(&rel_path);
            if raw_name.ends_with('/') {
                fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut bytes = Vec::new();
                entry
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("解压文件失败: {}", e))?;
                fs::write(&out_path, bytes).map_err(|e| e.to_string())?;
            }
        }

        Ok(unwrap_single_child_dir(&temp_extract))
    })();

    let effective_root = match extract_res {
        Ok(r) => r,
        Err(err) => {
            let _ = fs::remove_dir_all(&temp_extract);
            return Err(err);
        }
    };

    let source_label = zip_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| zip_path_str.to_string());
    let provenance = Provenance {
        kind: ProvenanceKind::FileImport,
        label: "ZIP 归档导入".to_string(),
        source_path: Some(zip_path.to_string_lossy().into_owned()),
        ..Default::default()
    };

    resolve_or_stage_plugin_import(
        app,
        effective_root,
        Some(temp_extract),
        source_label,
        true,
        provenance,
    )
}

pub fn import_plugin_from_git(
    app: &tauri::AppHandle,
    input: &ImportPluginFromGitInput,
) -> Result<ImportPluginOutcome, String> {
    let spec = parse_git_plugin_spec(
        &input.repo_url,
        input.sub_path.as_deref(),
        input.branch.as_deref(),
    )?;

    let temp_clone =
        std::env::temp_dir().join(format!("skill-switch-plugin-git-{}", Uuid::new_v4()));

    let mut effective_sub_path = spec.sub_path.clone();

    let clone_res =
        git::clone_repository_shallow(&spec.repo_url, &temp_clone, spec.branch.as_deref());

    if let Err(primary_err) = clone_res {
        // Fallback when a GitHub `/tree/<branch-with-slash>/<subpath>` URL had a slash inside the branch name
        if input
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
            && spec.raw_tree_tail.is_some()
        {
            let _ = fs::remove_dir_all(&temp_clone);
            if git::clone_repository_shallow(&spec.repo_url, &temp_clone, None).is_ok() {
                if let Some(ref tail) = spec.raw_tree_tail {
                    effective_sub_path = resolve_subpath_from_slash_branch(&temp_clone, tail);
                }
            } else {
                let _ = fs::remove_dir_all(&temp_clone);
                return Err(primary_err);
            }
        } else {
            let _ = fs::remove_dir_all(&temp_clone);
            return Err(primary_err);
        }
    }

    let target_dir = if let Some(ref sub) = effective_sub_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        match store::resolve_project_subpath(&temp_clone, sub) {
            Ok(d) if d.is_dir() => d,
            Ok(_) => {
                let _ = fs::remove_dir_all(&temp_clone);
                return Err(format!("仓库内未找到指定插件子目录: {}", sub));
            }
            Err(err) => {
                let _ = fs::remove_dir_all(&temp_clone);
                return Err(err);
            }
        }
    } else {
        temp_clone.clone()
    };

    let provenance = Provenance {
        kind: ProvenanceKind::RepoSource,
        label: "Git 仓库导入".to_string(),
        source_url: Some(spec.repo_url.clone()),
        source_path: effective_sub_path,
        ..Default::default()
    };

    resolve_or_stage_plugin_import(
        app,
        target_dir,
        Some(temp_clone),
        spec.repo_url,
        input.project_components_to_library.unwrap_or(true),
        provenance,
    )
}

fn resolve_or_stage_plugin_import(
    app: &tauri::AppHandle,
    root_dir: PathBuf,
    cleanup_dir: Option<PathBuf>,
    source_label: String,
    project_to_library: bool,
    provenance_base: Provenance,
) -> Result<ImportPluginOutcome, String> {
    let discovered = match discover_plugins_in_source(&root_dir) {
        Ok(d) => d,
        Err(err) => {
            if let Some(ref tmp) = cleanup_dir {
                let _ = fs::remove_dir_all(tmp);
            }
            return Err(err);
        }
    };

    if discovered.candidates.len() == 1 {
        let only = discovered.candidates.into_iter().next().unwrap();
        let plugin_dir = if only.sub_path == "." {
            root_dir.clone()
        } else {
            root_dir.join(&only.sub_path)
        };
        let prov = derive_subpath_provenance(&provenance_base, &plugin_dir, &only.sub_path);
        let batch_res = import_plugins_batch_internal(
            app,
            vec![(plugin_dir, only.hint, prov)],
            project_to_library,
        );
        if let Some(ref tmp) = cleanup_dir {
            let _ = fs::remove_dir_all(tmp);
        }
        let batch = batch_res?;
        return Ok(ImportPluginOutcome::Imported {
            plugins: batch.plugins,
            warnings: batch.warnings,
            backup_sync: batch.backup_sync,
        });
    }

    // Multiple plugins discovered -> stage for instant selection without re-cloning / re-extracting!
    let candidate_dtos: Vec<DiscoveredPluginCandidate> = discovered
        .candidates
        .into_iter()
        .map(|c| DiscoveredPluginCandidate {
            sub_path: c.sub_path,
            name: c.bundle.name,
            version: c.bundle.version,
            author: c.bundle.author,
            description: c.bundle.description,
            category: c.category,
            manifest_kind: c.bundle.manifest_kind,
            components: c.bundle.components,
        })
        .collect();

    let staging_id = register_staged_import(StagedPluginImport {
        root_dir,
        cleanup_dir,
        provenance_base,
        created_at_ms: store::now_ms(),
    });

    Ok(ImportPluginOutcome::SelectPlugins {
        staging_id,
        source_label,
        marketplace_name: discovered.marketplace_name,
        candidates: candidate_dtos,
    })
}

pub fn import_staged_plugins(
    app: &tauri::AppHandle,
    input: &ImportStagedPluginsInput,
) -> Result<ImportPluginBatchResult, String> {
    if input.sub_paths.is_empty() {
        return Err("请至少选择一个要导入的插件".to_string());
    }
    let staged = take_staged_import(&input.staging_id)
        .ok_or_else(|| "导入会话已过期或不存在，请重新选择仓库/目录".to_string())?;

    let import_res = (|| -> Result<ImportPluginBatchResult, String> {
        let discovered = discover_plugins_in_source(&staged.root_dir)?;
        let mut hints_by_sub: HashMap<String, Option<MarketplaceEntryHint>> = discovered
            .candidates
            .into_iter()
            .map(|c| (c.sub_path, c.hint))
            .collect();

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        for raw_sub in &input.sub_paths {
            let trimmed = raw_sub.trim();
            if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
                continue;
            }
            let plugin_dir = if trimmed == "." {
                staged.root_dir.clone()
            } else {
                store::resolve_project_subpath(&staged.root_dir, trimmed)?
            };
            if !plugin_dir.is_dir() {
                return Err(format!("未找到插件子目录: {}", trimmed));
            }
            let hint = hints_by_sub.remove(trimmed).flatten();
            let prov = derive_subpath_provenance(&staged.provenance_base, &plugin_dir, trimmed);
            items.push((plugin_dir, hint, prov));
        }

        if items.is_empty() {
            return Err("未选择有效插件目录".to_string());
        }

        import_plugins_batch_internal(
            app,
            items,
            input.project_components_to_library.unwrap_or(true),
        )
    })();

    staged.cleanup_disk();
    import_res
}

fn derive_subpath_provenance(base: &Provenance, plugin_dir: &Path, sub_path: &str) -> Provenance {
    let mut out = base.clone();
    if sub_path == "." {
        return out;
    }
    match base.kind {
        ProvenanceKind::RepoSource => {
            let merged = match base.source_path.as_deref().filter(|s| !s.is_empty()) {
                Some(prefix) => format!("{}/{}", prefix.trim_end_matches('/'), sub_path),
                None => sub_path.to_string(),
            };
            out.source_path = Some(merged);
        }
        ProvenanceKind::FileImport => {
            if base
                .source_path
                .as_deref()
                .map(|p| p.to_ascii_lowercase().ends_with(".zip"))
                .unwrap_or(false)
            {
                if let Some(ref zp) = base.source_path {
                    out.source_path = Some(format!("{}#{}", zp, sub_path));
                }
            } else {
                out.source_path = Some(plugin_dir.to_string_lossy().into_owned());
            }
        }
        _ => {}
    }
    out
}

fn import_plugins_batch_internal(
    app: &tauri::AppHandle,
    items: Vec<(PathBuf, Option<MarketplaceEntryHint>, Provenance)>,
    project_to_library: bool,
) -> Result<ImportPluginBatchResult, String> {
    let repo_root = store::connected_repo_root(app)?;
    let mut library = store::load_repo_library(&repo_root)?;

    let mut records = Vec::new();
    let mut all_warnings = Vec::new();

    for (src_dir, hint, provenance) in items {
        let (record, warnings) = stage_single_plugin_into_library(
            app,
            &mut library,
            &src_dir,
            hint.as_ref(),
            project_to_library,
            provenance,
        )?;
        records.push(record);
        all_warnings.extend(warnings);
    }

    store::save_repo_library(&repo_root, &library)?;

    let commit_msg = if records.len() == 1 {
        format!("Import plugin: {}", records[0].name)
    } else {
        let names: Vec<&str> = records.iter().map(|r| r.name.as_str()).take(4).collect();
        format!("Import {} plugins: {}", records.len(), names.join(", "))
    };
    let backup_sync = store::sync_after_mutation(app, &commit_msg);

    let mut dtos = Vec::with_capacity(records.len());
    for rec in &records {
        dtos.push(record_to_dto(app, rec)?);
    }

    Ok(ImportPluginBatchResult {
        plugins: dtos,
        warnings: all_warnings,
        backup_sync,
    })
}

fn stage_single_plugin_into_library(
    app: &tauri::AppHandle,
    library: &mut RepoLibrary,
    src_dir: &Path,
    hint: Option<&MarketplaceEntryHint>,
    project_to_library: bool,
    provenance: Provenance,
) -> Result<(PluginRecord, Vec<String>), String> {
    let inspected = inspect_plugin_directory_opts(src_dir, hint, true)?;

    let slug = store::slugify(&inspected.name);
    let existing_idx = library.plugins.iter().position(|p| p.slug == slug);
    let plugin_id = existing_idx
        .map(|idx| library.plugins[idx].id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let created_at = existing_idx
        .map(|idx| library.plugins[idx].created_at)
        .unwrap_or_else(store::now_ms);
    let existing_values = existing_idx
        .map(|idx| library.plugins[idx].user_config_values.clone())
        .unwrap_or_default();
    let now = store::now_ms();

    let dest_plugin_dir = plugin_source_dir(app, &slug)?;
    if dest_plugin_dir.exists() && dest_plugin_dir != src_dir {
        fs::remove_dir_all(&dest_plugin_dir).map_err(|e| format!("清理旧插件源目录失败: {}", e))?;
    }
    if dest_plugin_dir != src_dir {
        copy_plugin_tree_filtered(src_dir, &dest_plugin_dir)?;
    }

    let mut user_config_values = existing_values;
    let mut sensitive_keys = HashSet::new();
    for field in &inspected.user_config_schema {
        if field.sensitive {
            sensitive_keys.insert(field.key.clone());
            user_config_values.remove(&field.key);
        } else if let Some(ref def_val) = field.default_value {
            user_config_values
                .entry(field.key.clone())
                .or_insert_with(|| def_val.clone());
        }
    }

    let mut projected_skill_ids = Vec::new();
    let mut projected_command_ids = Vec::new();
    let mut projected_mcp_server_ids = Vec::new();
    let mut warnings = inspected.warnings;

    if project_to_library {
        let skill_sources_root = store::skill_sources_dir(app)?;
        for (skill_folder_name, rel_skill_dir) in &inspected.skill_dirs {
            let staged_skill_dir = if rel_skill_dir == Path::new(".") {
                dest_plugin_dir.clone()
            } else {
                dest_plugin_dir.join(rel_skill_dir)
            };
            let Ok(raw_skill) = fs::read_to_string(staged_skill_dir.join("SKILL.md")) else {
                continue;
            };
            let proj_slug = if rel_skill_dir == Path::new(".") {
                slug.clone()
            } else {
                store::slugify(&format!("{}-{}", slug, skill_folder_name))
            };
            let target_skill_dir = skill_sources_root.join(&proj_slug);
            if target_skill_dir.exists() {
                let _ = fs::remove_dir_all(&target_skill_dir);
            }
            copy_plugin_tree_filtered(&staged_skill_dir, &target_skill_dir)?;

            // Adapt Claude `mcp__plugin_<plug>_<srv>__` references in projected copy if present
            let adapted_skill = adapt_projected_skill_namespaces(&raw_skill, &slug);
            if adapted_skill != raw_skill {
                let _ = fs::write(target_skill_dir.join("SKILL.md"), &adapted_skill);
            }

            let (parsed_name, parsed_desc) = store::parse_skill_metadata(&adapted_skill);
            let display_name = if parsed_name.is_empty() {
                format!("{}:{}", inspected.name, skill_folder_name)
            } else {
                parsed_name
            };

            let rel_source_md = if rel_skill_dir == Path::new(".") {
                "SKILL.md".to_string()
            } else {
                format!("{}/SKILL.md", path_to_slash_rel(rel_skill_dir))
            };

            let skill_id = if let Some(existing_res) = library
                .resources
                .iter_mut()
                .find(|r| r.kind == ResourceKind::Skill && r.slug == proj_slug)
            {
                existing_res.title = display_name;
                existing_res.description = parsed_desc;
                existing_res.content = adapted_skill.clone();
                existing_res.revision = store::compute_revision(&adapted_skill);
                existing_res.updated_at = now;
                existing_res.id.clone()
            } else {
                let sid = Uuid::new_v4().to_string();
                library.resources.push(Resource {
                    id: sid.clone(),
                    slug: proj_slug,
                    title: display_name,
                    description: parsed_desc,
                    kind: ResourceKind::Skill,
                    scope: ResourceScope::Global,
                    origin: ResourceOrigin::Vendor,
                    source_status: SourceStatus::Current,
                    project_id: None,
                    tags: vec![format!("plugin:{}", slug)],
                    content: adapted_skill.clone(),
                    revision: store::compute_revision(&adapted_skill),
                    source_url: inspected.repository_url.clone(),
                    source_ref: None,
                    source_path: Some(rel_source_md),
                    upstream_revision: None,
                    forked_from: None,
                    created_at: now,
                    updated_at: now,
                    provenance: Provenance {
                        kind: ProvenanceKind::Plugin,
                        label: format!("插件: {}", inspected.name),
                        source_id: Some(plugin_id.clone()),
                        source_name: Some(inspected.name.clone()),
                        ..Default::default()
                    },
                });
                sid
            };
            projected_skill_ids.push(skill_id);
        }

        let cmd_sources_root = store::slash_command_sources_dir(app)?;
        for (cmd_name, rel_cmd_file) in &inspected.command_files {
            let staged_md = dest_plugin_dir.join(rel_cmd_file);
            let Ok(raw_cmd) = fs::read_to_string(&staged_md) else {
                continue;
            };
            let proj_cmd_slug = store::slugify(&format!("{}-{}", slug, cmd_name));
            let cmd_dir = cmd_sources_root.join(&proj_cmd_slug);
            fs::create_dir_all(&cmd_dir).map_err(|e| e.to_string())?;
            fs::write(cmd_dir.join("COMMAND.md"), &raw_cmd).map_err(|e| e.to_string())?;

            let (parsed_name, parsed_desc) = store::parse_skill_metadata(&raw_cmd);
            let title = if parsed_name.is_empty() {
                cmd_name.clone()
            } else {
                parsed_name
            };
            let cid = if let Some(existing_cmd) = library
                .resources
                .iter_mut()
                .find(|r| r.kind == ResourceKind::SlashCommand && r.slug == proj_cmd_slug)
            {
                existing_cmd.title = title;
                existing_cmd.description = parsed_desc;
                existing_cmd.content = raw_cmd.clone();
                existing_cmd.revision = store::compute_revision(&raw_cmd);
                existing_cmd.updated_at = now;
                existing_cmd.id.clone()
            } else {
                let new_cid = Uuid::new_v4().to_string();
                library.resources.push(Resource {
                    id: new_cid.clone(),
                    slug: proj_cmd_slug,
                    title,
                    description: parsed_desc,
                    kind: ResourceKind::SlashCommand,
                    scope: ResourceScope::Global,
                    origin: ResourceOrigin::Vendor,
                    source_status: SourceStatus::Current,
                    project_id: None,
                    tags: vec![format!("plugin:{}", slug)],
                    content: raw_cmd.clone(),
                    revision: store::compute_revision(&raw_cmd),
                    source_url: inspected.repository_url.clone(),
                    source_ref: None,
                    source_path: None,
                    upstream_revision: None,
                    forked_from: None,
                    created_at: now,
                    updated_at: now,
                    provenance: Provenance {
                        kind: ProvenanceKind::Plugin,
                        label: format!("插件: {}", inspected.name),
                        source_id: Some(plugin_id.clone()),
                        source_name: Some(inspected.name.clone()),
                        ..Default::default()
                    },
                });
                new_cid
            };
            projected_command_ids.push(cid);
        }

        let mut sorted_mcp: Vec<(String, mcp::ParsedRawMcpEntry)> =
            inspected.inline_or_file_mcp.into_iter().collect();
        sorted_mcp.sort_by(|a, b| a.0.cmp(&b.0));

        for (srv_key, raw_srv) in sorted_mcp {
            let proj_mcp_slug = store::slugify(&format!("{}-{}", slug, srv_key));
            let existing_mcp_id = library
                .mcp_servers
                .iter()
                .find(|m| m.slug == proj_mcp_slug && m.plugin_id.as_deref() == Some(&plugin_id))
                .map(|m| m.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());

            let expanded_cmd = raw_srv.command.as_deref().map(|c| {
                expand_plugin_macros(c, &dest_plugin_dir, &user_config_values, &sensitive_keys)
            });
            let expanded_args: Vec<String> = raw_srv
                .args
                .iter()
                .map(|a| {
                    expand_plugin_macros(a, &dest_plugin_dir, &user_config_values, &sensitive_keys)
                })
                .collect();
            let expanded_url = raw_srv.url.as_deref().map(|u| {
                expand_plugin_macros(u, &dest_plugin_dir, &user_config_values, &sensitive_keys)
            });

            let mut env_specs = Vec::new();
            for (ek, ev) in &raw_srv.env {
                let expanded_ev = expand_plugin_macros(
                    ev,
                    &dest_plugin_dir,
                    &user_config_values,
                    &sensitive_keys,
                );
                let is_secret = mcp::looks_like_secret_env(ek, &expanded_ev)
                    || sensitive_keys
                        .iter()
                        .any(|sk| ev.contains(&format!("user_config.{}", sk)));
                env_specs.push(McpEnvVarSpec {
                    name: ek.clone(),
                    description: None,
                    required: is_secret,
                    is_secret,
                    default_value: if is_secret { None } else { Some(expanded_ev) },
                });
            }
            for sk in &sensitive_keys {
                if !env_specs.iter().any(|s| &s.name == sk) {
                    env_specs.push(McpEnvVarSpec {
                        name: sk.clone(),
                        description: Some("插件敏感配置项".to_string()),
                        required: true,
                        is_secret: true,
                        default_value: None,
                    });
                }
            }

            let mut header_specs = Vec::new();
            for (hk, hv) in &raw_srv.headers {
                let expanded_hv = expand_plugin_macros(
                    hv,
                    &dest_plugin_dir,
                    &user_config_values,
                    &sensitive_keys,
                );
                let is_secret = mcp::looks_like_secret_env(hk, &expanded_hv);
                header_specs.push(McpHeaderSpec {
                    name: hk.clone(),
                    description: None,
                    is_secret,
                    value_template: if !is_secret || expanded_hv.contains("${") {
                        Some(expanded_hv)
                    } else {
                        None
                    },
                });
            }

            let mcp_rec = McpServerRecord {
                id: existing_mcp_id.clone(),
                slug: proj_mcp_slug,
                name: format!("{} ({})", srv_key, inspected.name),
                description: Some(format!("由插件「{}」捆绑提供的 MCP 服务", inspected.name)),
                transport: raw_srv.transport,
                command: expanded_cmd,
                args: expanded_args,
                url: expanded_url,
                env_specs: mcp::sanitize_env_specs(&env_specs),
                header_specs: mcp::sanitize_header_specs(&header_specs),
                tags: vec![format!("plugin:{}", slug)],
                project_ids: Vec::new(),
                plugin_id: Some(plugin_id.clone()),
                created_at: now,
                updated_at: now,
                provenance: Provenance {
                    kind: ProvenanceKind::Plugin,
                    label: format!("插件: {}", inspected.name),
                    source_id: Some(plugin_id.clone()),
                    source_name: Some(inspected.name.clone()),
                    ..Default::default()
                },
            };
            mcp::upsert_library_mcp_record(library, mcp_rec);
            projected_mcp_server_ids.push(existing_mcp_id);
        }
    }

    if !inspected.components.hooks.is_empty() || !inspected.components.agents.is_empty() {
        warnings.push(format!(
            "插件「{}」包含宿主扩展组件（Hooks: {}，Agents: {}）：在同构原生端走原生挂载生效，在异构端（如 Windsurf / T3）仅投射其 Skills 与 MCP",
            inspected.name,
            inspected.components.hooks.len(),
            inspected.components.agents.len()
        ));
    }

    let plugin_record = PluginRecord {
        id: plugin_id,
        slug,
        name: inspected.name,
        version: inspected.version,
        author: inspected.author,
        description: inspected.description,
        homepage: inspected.homepage,
        repository_url: inspected.repository_url,
        manifest_kind: inspected.manifest_kind,
        user_config_schema: inspected.user_config_schema,
        user_config_values,
        components: inspected.components,
        projected_skill_ids,
        projected_command_ids,
        projected_mcp_server_ids,
        tags: vec!["plugin".to_string()],
        created_at,
        updated_at: now,
        provenance,
    };

    if let Some(idx) = existing_idx {
        library.plugins[idx] = plugin_record.clone();
    } else {
        library.plugins.push(plugin_record.clone());
    }

    Ok((plugin_record, warnings))
}

pub fn update_plugin_config(
    app: &tauri::AppHandle,
    input: &UpdatePluginConfigInput,
) -> Result<PluginDto, String> {
    let repo_root = store::connected_repo_root(app)?;
    let mut library = store::load_repo_library(&repo_root)?;
    let idx = library
        .plugins
        .iter()
        .position(|p| p.id == input.plugin_id)
        .ok_or_else(|| format!("未找到插件: {}", input.plugin_id))?;

    let sensitive_keys: HashSet<String> = library.plugins[idx]
        .user_config_schema
        .iter()
        .filter(|f| f.sensitive)
        .map(|f| f.key.clone())
        .collect();

    if let Some(ref vals) = input.user_config_values {
        for (k, v) in vals {
            // Never allow sensitive values into `user_config_values` in library.json!
            if sensitive_keys.contains(k) {
                continue;
            }
            if v.is_empty() {
                library.plugins[idx].user_config_values.remove(k);
            } else {
                library.plugins[idx]
                    .user_config_values
                    .insert(k.clone(), v.clone());
            }
        }
    }

    if let Some(ref tags) = input.tags {
        library.plugins[idx].tags = tags.clone();
    }
    library.plugins[idx].updated_at = store::now_ms();

    if let Some(ref sec_updates) = input.secret_values {
        secrets::update_namespace_secrets(
            app,
            &secrets::plugin_namespace(&input.plugin_id),
            sec_updates,
        )?;
        // Also propagate secrets to projected child MCP namespaces for seamless injection
        for mcp_id in &library.plugins[idx].projected_mcp_server_ids {
            secrets::update_namespace_secrets(app, &secrets::mcp_namespace(mcp_id), sec_updates)?;
        }
    }

    let updated = library.plugins[idx].clone();
    store::save_repo_library(&repo_root, &library)?;
    let _ = store::sync_after_mutation(app, &format!("Update plugin config: {}", updated.name));
    crate::ssh_sync::resync_plugin_to_enabled_ssh_apps(app, &updated, &library);
    record_to_dto(app, &updated)
}

pub fn install_plugin_global(
    app: &tauri::AppHandle,
    input: &InstallPluginGlobalInput,
) -> Result<InstallPluginResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let plugin = library
        .plugins
        .iter()
        .find(|p| p.id == input.plugin_id)
        .ok_or_else(|| format!("未找到插件: {}", input.plugin_id))?;

    let prefer_native = input.prefer_native.unwrap_or(true);
    let src_dir = plugin_source_dir(app, &plugin.slug)?;

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();
    let mut app_modes = HashMap::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::install_plugin_ssh(app, custom, plugin, &library, prefer_native)
            {
                Ok((mode, ssh_warnings)) => {
                    warnings.extend(ssh_warnings);
                    app_modes.insert(app_id.clone(), mode);
                    installed_apps.push(app_id.clone());
                }
                Err(err) => {
                    if input.apps.len() == 1 {
                        return Err(err);
                    }
                    warnings.push(format!("{}: {}", custom.label, err));
                    failed_apps.push(app_id.clone());
                }
            }
            continue;
        }

        let mut app_ok = true;

        // 1. Project bundled Skills, Commands, and MCP Servers so cross-app tools work immediately
        for skill_id in &plugin.projected_skill_ids {
            if let Err(err) = store::install_skill_global(
                app,
                &InstallSkillGlobalInput {
                    skill_id: skill_id.clone(),
                    apps: vec![app_id.clone()],
                },
            ) {
                warnings.push(format!("{}: 投射技能失败 ({})", app_id, err));
                app_ok = false;
            }
        }

        for cmd_id in &plugin.projected_command_ids {
            let _ = store::install_slash_command_global(
                app,
                &InstallSlashCommandGlobalInput {
                    command_id: cmd_id.clone(),
                    apps: vec![app_id.clone()],
                },
            );
        }

        if app_id != "t3" {
            for mcp_id in &plugin.projected_mcp_server_ids {
                if let Ok(mcp_res) = mcp::install_mcp_global(
                    app,
                    &InstallMcpGlobalInput {
                        server_id: mcp_id.clone(),
                        apps: vec![app_id.clone()],
                        use_env_placeholders: false,
                    },
                ) {
                    warnings.extend(mcp_res.warnings);
                }
            }
        }

        // 2. Native staging where supported & requested (enables Hooks / Agents / native registry)
        let mode = if prefer_native && src_dir.exists() {
            match stage_native_plugin_global(&home, app_id, &plugin.slug, &src_dir) {
                Ok(true) => PluginInstallMode::Native,
                Ok(false) => {
                    if !plugin.components.hooks.is_empty() {
                        warnings.push(format!(
                            "{} 采用跨端解包投射模式（已启用 Skills 与 MCP；原生 Hooks 仅在支持插件清单的宿主生效）",
                            app_id
                        ));
                    }
                    PluginInstallMode::Projected
                }
                Err(err) => {
                    warnings.push(format!("{}: 原生目录挂载降级为组件投射 ({})", app_id, err));
                    PluginInstallMode::Projected
                }
            }
        } else {
            PluginInstallMode::Projected
        };

        if app_ok {
            app_modes.insert(app_id.clone(), mode);
            installed_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallPluginResult {
        installed_apps,
        failed_apps,
        app_modes,
        warnings,
    })
}

pub fn uninstall_plugin_global(
    app: &tauri::AppHandle,
    input: &InstallPluginGlobalInput,
) -> Result<InstallPluginResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let plugin = library
        .plugins
        .iter()
        .find(|p| p.id == input.plugin_id)
        .ok_or_else(|| format!("未找到插件: {}", input.plugin_id))?;

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::uninstall_plugin_ssh(app, custom, plugin) {
                Ok(ssh_warnings) => {
                    warnings.extend(ssh_warnings);
                    installed_apps.push(app_id.clone());
                }
                Err(err) => {
                    if input.apps.len() == 1 {
                        return Err(err);
                    }
                    warnings.push(format!("{}: {}", custom.label, err));
                    failed_apps.push(app_id.clone());
                }
            }
            continue;
        }

        for skill_id in &plugin.projected_skill_ids {
            let _ = store::uninstall_skill_global(
                app,
                &InstallSkillGlobalInput {
                    skill_id: skill_id.clone(),
                    apps: vec![app_id.clone()],
                },
            );
        }
        for cmd_id in &plugin.projected_command_ids {
            let _ = store::uninstall_slash_command_global(
                app,
                &InstallSlashCommandGlobalInput {
                    command_id: cmd_id.clone(),
                    apps: vec![app_id.clone()],
                },
            );
        }
        if app_id != "t3" {
            for mcp_id in &plugin.projected_mcp_server_ids {
                if let Ok(res) = mcp::uninstall_mcp_global(
                    app,
                    &InstallMcpGlobalInput {
                        server_id: mcp_id.clone(),
                        apps: vec![app_id.clone()],
                        use_env_placeholders: false,
                    },
                ) {
                    warnings.extend(res.warnings);
                }
            }
        }
        let _ = unstage_native_plugin_global(&home, app_id, &plugin.slug);
        installed_apps.push(app_id.clone());
    }

    Ok(InstallPluginResult {
        installed_apps,
        failed_apps,
        app_modes: HashMap::new(),
        warnings,
    })
}

pub fn install_plugin_to_project(
    app: &tauri::AppHandle,
    input: &InstallPluginToProjectInput,
) -> Result<InstallPluginResult, String> {
    let project_root = store::normalize_user_path(&input.project_path)?;
    if !project_root.exists() {
        return Err(format!("项目路径不存在: {}", input.project_path));
    }

    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let plugin = library
        .plugins
        .iter()
        .find(|p| p.id == input.plugin_id)
        .ok_or_else(|| format!("未找到插件: {}", input.plugin_id))?;

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();
    let mut app_modes = HashMap::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if crate::ssh_sync::is_ssh_app(app_id, &custom_apps) {
            warnings.push("SSH 远程目标端不支持本地项目级插件装配".to_string());
            failed_apps.push(app_id.clone());
            continue;
        }
        let mut ok = true;
        for skill_id in &plugin.projected_skill_ids {
            if let Err(err) = store::install_skill_to_project(
                app,
                &InstallSkillToProjectInput {
                    skill_id: skill_id.clone(),
                    project_path: input.project_path.clone(),
                    apps: vec![app_id.clone()],
                },
            ) {
                warnings.push(format!("{}: {}", app_id, err));
                ok = false;
            }
        }
        for cmd_id in &plugin.projected_command_ids {
            let _ = store::install_slash_command_to_project(
                app,
                &InstallSlashCommandToProjectInput {
                    command_id: cmd_id.clone(),
                    project_path: input.project_path.clone(),
                    apps: vec![app_id.clone()],
                },
            );
        }
        if app_id != "t3" && app_id != "windsurf" {
            for mcp_id in &plugin.projected_mcp_server_ids {
                if let Ok(res) = mcp::install_mcp_to_project(
                    app,
                    &InstallMcpToProjectInput {
                        server_id: mcp_id.clone(),
                        project_path: input.project_path.clone(),
                        apps: vec![app_id.clone()],
                        claude_mode: input.claude_mcp_mode,
                    },
                ) {
                    warnings.extend(res.warnings);
                }
            }
        }
        if ok {
            app_modes.insert(app_id.clone(), PluginInstallMode::Projected);
            installed_apps.push(app_id.clone());
        } else {
            failed_apps.push(app_id.clone());
        }
    }

    Ok(InstallPluginResult {
        installed_apps,
        failed_apps,
        app_modes,
        warnings,
    })
}

pub fn uninstall_plugin_from_project(
    app: &tauri::AppHandle,
    input: &InstallPluginToProjectInput,
) -> Result<InstallPluginResult, String> {
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let plugin = library
        .plugins
        .iter()
        .find(|p| p.id == input.plugin_id)
        .ok_or_else(|| format!("未找到插件: {}", input.plugin_id))?;

    let mut installed_apps = Vec::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if crate::ssh_sync::is_ssh_app(app_id, &custom_apps) {
            installed_apps.push(app_id.clone());
            continue;
        }
        for skill_id in &plugin.projected_skill_ids {
            let _ = store::uninstall_skill_from_project(
                app,
                &InstallSkillToProjectInput {
                    skill_id: skill_id.clone(),
                    project_path: input.project_path.clone(),
                    apps: vec![app_id.clone()],
                },
            );
        }
        for cmd_id in &plugin.projected_command_ids {
            let _ = store::uninstall_slash_command_from_project(
                app,
                &InstallSlashCommandToProjectInput {
                    command_id: cmd_id.clone(),
                    project_path: input.project_path.clone(),
                    apps: vec![app_id.clone()],
                },
            );
        }
        if app_id != "t3" && app_id != "windsurf" {
            for mcp_id in &plugin.projected_mcp_server_ids {
                if let Ok(res) = mcp::uninstall_mcp_from_project(
                    app,
                    &InstallMcpToProjectInput {
                        server_id: mcp_id.clone(),
                        project_path: input.project_path.clone(),
                        apps: vec![app_id.clone()],
                        claude_mode: input.claude_mcp_mode,
                    },
                ) {
                    warnings.extend(res.warnings);
                }
            }
        }
        installed_apps.push(app_id.clone());
    }

    Ok(InstallPluginResult {
        installed_apps,
        failed_apps: Vec::new(),
        app_modes: HashMap::new(),
        warnings,
    })
}

pub fn delete_plugin(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let custom_apps = store::load_custom_apps(app);
    let initial_ledgers = store::load_local_state(app)
        .map(|s| s.ssh_ledgers)
        .unwrap_or_default();
    let pre_repo_root = store::connected_repo_root(app)?;
    let pre_library = store::load_repo_library(&pre_repo_root)?;
    let target_plugin = pre_library
        .plugins
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("未找到插件: {}", id))?;

    let apps_to_uninstall: Vec<String> = store::all_configured_app_ids(&custom_apps)
        .into_iter()
        .filter(|app_id| {
            if crate::ssh_sync::is_ssh_app(app_id, &custom_apps) {
                initial_ledgers
                    .get(app_id)
                    .map(|l| is_plugin_enabled_in_ssh_ledger(&pre_library, target_plugin, l))
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .collect();

    // First atomically uninstall across local apps & enabled SSH targets
    if !apps_to_uninstall.is_empty() {
        let _ = uninstall_plugin_global(
            app,
            &InstallPluginGlobalInput {
                plugin_id: id.to_string(),
                apps: apps_to_uninstall,
                prefer_native: Some(true),
            },
        );
    }

    let repo_root = store::connected_repo_root(app)?;
    let mut library = store::load_repo_library(&repo_root)?;
    let pos = library
        .plugins
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("未找到插件: {}", id))?;
    let removed = library.plugins.remove(pos);

    // Remove projected child Skills, SlashCommands, and MCP Servers + their source folders/secrets
    let skill_ids: HashSet<String> = removed.projected_skill_ids.iter().cloned().collect();
    let cmd_ids: HashSet<String> = removed.projected_command_ids.iter().cloned().collect();
    let mcp_ids: HashSet<String> = removed.projected_mcp_server_ids.iter().cloned().collect();

    let mut removed_skill_slugs = HashSet::new();
    let mut removed_cmd_slugs = HashSet::new();
    let removed_mcp_slugs: HashSet<String> = library
        .mcp_servers
        .iter()
        .filter(|m| mcp_ids.contains(&m.id))
        .map(|m| m.slug.clone())
        .collect();

    let skill_root = store::skill_sources_dir(app)?;
    let cmd_root = store::slash_command_sources_dir(app)?;

    for res in &library.resources {
        if skill_ids.contains(&res.id) {
            removed_skill_slugs.insert(res.slug.clone());
            let p = skill_root.join(&res.slug);
            if p.exists() {
                let _ = fs::remove_dir_all(p);
            }
        } else if cmd_ids.contains(&res.id) {
            removed_cmd_slugs.insert(res.slug.clone());
            let p = cmd_root.join(&res.slug);
            if p.exists() {
                let _ = fs::remove_dir_all(p);
            }
        }
    }

    if let Ok(mut state) = store::load_local_state(app) {
        let mut ledger_changed = false;
        for ledger in state.ssh_ledgers.values_mut() {
            let b_p = ledger.plugins.len();
            let b_s = ledger.skills.len();
            let b_c = ledger.commands.len();
            let b_m = ledger.mcp_servers.len();
            ledger.plugins.retain(|s| s != &removed.slug);
            ledger.skills.retain(|s| !removed_skill_slugs.contains(s));
            ledger.commands.retain(|c| !removed_cmd_slugs.contains(c));
            ledger
                .mcp_servers
                .retain(|m| !removed_mcp_slugs.contains(m));
            if ledger.plugins.len() != b_p
                || ledger.skills.len() != b_s
                || ledger.commands.len() != b_c
                || ledger.mcp_servers.len() != b_m
            {
                ledger_changed = true;
            }
        }
        if ledger_changed {
            let _ = store::save_local_state(app, &state);
        }
    }

    library
        .resources
        .retain(|r| !skill_ids.contains(&r.id) && !cmd_ids.contains(&r.id));
    library.mcp_servers.retain(|m| !mcp_ids.contains(&m.id));

    for mid in &mcp_ids {
        let _ = secrets::delete_namespace_secrets(app, &secrets::mcp_namespace(mid));
    }
    let _ = secrets::delete_namespace_secrets(app, &secrets::plugin_namespace(&removed.id));

    let pdir = plugin_source_dir(app, &removed.slug)?;
    if pdir.exists() {
        let _ = fs::remove_dir_all(pdir);
    }

    store::save_repo_library(&repo_root, &library)?;
    let _ = store::sync_after_mutation(app, &format!("Delete plugin: {}", removed.name));
    Ok(())
}

fn is_plugin_enabled_in_ssh_ledger(
    library: &RepoLibrary,
    plugin: &PluginRecord,
    ledger: &crate::domain::SshAppSyncLedger,
) -> bool {
    if ledger.plugins.contains(&plugin.slug) {
        return true;
    }
    if let Some(first_skill_id) = plugin.projected_skill_ids.first() {
        if let Some(res) = library.resources.iter().find(|r| &r.id == first_skill_id) {
            if ledger.skills.contains(&res.slug) {
                return true;
            }
        }
    }
    false
}

pub fn check_plugin_status(
    app: &tauri::AppHandle,
    input: &CheckPluginStatusInput,
) -> Result<CheckPluginStatusResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let plugin = library
        .plugins
        .iter()
        .find(|p| p.id == input.plugin_id)
        .ok_or_else(|| format!("未找到插件: {}", input.plugin_id))?;

    let app_ids = store::all_configured_app_ids(&custom_apps);
    let ssh_ledgers = if input.scope != "project" {
        store::load_local_state(app)
            .map(|s| s.ssh_ledgers)
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let mut statuses = Vec::new();

    for app_id in app_ids {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(&app_id, &custom_apps) {
            if input.scope == "project" {
                continue;
            }
            let (installed, mode) = if let Some(ledger) = ssh_ledgers.get(&app_id) {
                let in_plugins = ledger.plugins.contains(&plugin.slug);
                let in_projected = is_plugin_enabled_in_ssh_ledger(&library, plugin, ledger);
                if in_plugins {
                    let has_native_dir = custom
                        .ssh
                        .as_ref()
                        .and_then(|s| {
                            s.global_plugin_dir
                                .as_deref()
                                .filter(|d| !d.trim().is_empty())
                                .or_else(|| {
                                    s.remote_app_preset.as_deref().filter(|p| {
                                        matches!(
                                            *p,
                                            "claude"
                                                | "codex"
                                                | "cursor"
                                                | "antigravity"
                                                | "gemini"
                                        )
                                    })
                                })
                        })
                        .is_some();
                    let m = if has_native_dir {
                        PluginInstallMode::Native
                    } else {
                        PluginInstallMode::Projected
                    };
                    (true, Some(m))
                } else if in_projected {
                    (true, Some(PluginInstallMode::Projected))
                } else {
                    (false, None)
                }
            } else {
                (false, None)
            };
            let host = custom
                .ssh
                .as_ref()
                .map(|s| s.host.as_str())
                .unwrap_or("ssh");
            let details = match mode {
                Some(PluginInstallMode::Native) => {
                    Some(format!("SSH 远端原生插件目录 + 组件投射 ({})", host))
                }
                Some(PluginInstallMode::Projected) => {
                    Some(format!("SSH 远端跨端组件投射 ({})", host))
                }
                None => None,
            };
            statuses.push(PluginAppInstallStatus {
                app_id,
                installed,
                mode,
                details,
            });
            continue;
        }

        if input.scope == "project" {
            let project_raw = input
                .project_path
                .as_deref()
                .ok_or_else(|| "project scope requires projectPath".to_string())?;
            let installed = is_plugin_projected_in_project(
                app,
                &library,
                plugin,
                &app_id,
                project_raw,
                &custom_apps,
            );
            statuses.push(PluginAppInstallStatus {
                app_id,
                installed,
                mode: installed.then_some(PluginInstallMode::Projected),
                details: Some(format!(
                    "{} Skills · {} MCPs",
                    plugin.components.skills.len(),
                    plugin.components.mcp_servers.len()
                )),
            });
        } else {
            let native = is_native_plugin_staged(&home, &app_id, &plugin.slug);
            let projected =
                is_plugin_projected_global(app, &library, plugin, &app_id, &home, &custom_apps);
            let installed = native || projected;
            let mode = if native {
                Some(PluginInstallMode::Native)
            } else if projected {
                Some(PluginInstallMode::Projected)
            } else {
                None
            };
            let details = match mode {
                Some(PluginInstallMode::Native) => Some("原生插件目录 + 组件投射".to_string()),
                Some(PluginInstallMode::Projected) => {
                    Some("跨端组件投射 (Skills + MCP)".to_string())
                }
                None => None,
            };
            statuses.push(PluginAppInstallStatus {
                app_id,
                installed,
                mode,
                details,
            });
        }
    }

    Ok(CheckPluginStatusResult {
        plugin_id: plugin.id.clone(),
        statuses,
    })
}

pub fn list_all_plugins_global_enabled_apps(
    app: &tauri::AppHandle,
) -> Result<HashMap<String, Vec<String>>, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let app_ids = store::all_configured_app_ids(&custom_apps);
    let ssh_ledgers = store::load_local_state(app)
        .map(|s| s.ssh_ledgers)
        .unwrap_or_default();

    let mut map = HashMap::new();
    for plugin in &library.plugins {
        let mut enabled = Vec::new();
        for app_id in &app_ids {
            if crate::ssh_sync::find_ssh_app(app_id, &custom_apps).is_some() {
                if let Some(ledger) = ssh_ledgers.get(app_id) {
                    if is_plugin_enabled_in_ssh_ledger(&library, plugin, ledger) {
                        enabled.push(app_id.clone());
                    }
                }
                continue;
            }
            if is_native_plugin_staged(&home, app_id, &plugin.slug)
                || is_plugin_projected_global(app, &library, plugin, app_id, &home, &custom_apps)
            {
                enabled.push(app_id.clone());
            }
        }
        map.insert(plugin.id.clone(), enabled);
    }
    Ok(map)
}

// ─── Internal Helpers ─────────────────────────────────────────────────────────

fn extract_author_field(val: &JsonValue) -> Option<String> {
    val.get("author")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.trim().to_string())
            } else {
                v.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.trim().to_string())
            }
        })
        .or_else(|| {
            val.pointer("/interface/developerName")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
}

fn extract_repo_field(val: &JsonValue) -> Option<String> {
    val.get("repository")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.trim().to_string())
            } else {
                v.get("url")
                    .and_then(|u| u.as_str())
                    .map(|s| s.trim().to_string())
            }
        })
        .filter(|s| !s.is_empty())
}

fn collect_string_or_array(val: &JsonValue) -> Vec<String> {
    if let Some(s) = val.as_str() {
        let t = s.trim();
        if !t.is_empty() {
            return vec![t.to_string()];
        }
    } else if let Some(arr) = val.as_array() {
        return arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

fn sanitize_plugin_rel_subpath(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "." || trimmed == "./" || trimmed == ".\\" {
        return Some(PathBuf::from("."));
    }
    let cleaned = trimmed
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .trim_start_matches('/')
        .trim_start_matches('\\')
        .trim_end_matches('/')
        .trim_end_matches('\\');
    if cleaned.is_empty() || cleaned == "." {
        return Some(PathBuf::from("."));
    }
    let mut out = PathBuf::new();
    for comp in Path::new(cleaned).components() {
        match comp {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn join_rel_clean(base: &Path, child: &str) -> PathBuf {
    if base == Path::new(".") {
        PathBuf::from(child)
    } else {
        base.join(child)
    }
}

fn path_to_slash_rel(p: &Path) -> String {
    if p == Path::new(".") || p.as_os_str().is_empty() {
        return ".".to_string();
    }
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn extract_local_marketplace_source_path(
    entry_obj: &JsonMap<String, JsonValue>,
    remote_only_count: &mut usize,
) -> Option<String> {
    let Some(src_val) = entry_obj.get("source") else {
        return Some(".".to_string());
    };
    if let Some(s) = src_val.as_str() {
        let trimmed = s.trim();
        if trimmed.contains("://") || trimmed.starts_with("git@") {
            *remote_only_count += 1;
            return None;
        }
        return Some(trimmed.to_string());
    }
    if let Some(obj) = src_val.as_object() {
        let kind = obj.get("source").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(kind, "url" | "git" | "github" | "npm" | "pip") {
            *remote_only_count += 1;
            return None;
        }
        if kind == "git-subdir" && obj.get("url").is_some() {
            *remote_only_count += 1;
            return None;
        }
        if let Some(p) = obj.get("path").and_then(|v| v.as_str()) {
            return Some(p.trim().to_string());
        }
    }
    None
}

fn resolve_marketplace_entry_rel_dir(
    root_dir: &Path,
    plugin_root_prefix: Option<&Path>,
    raw_source: &str,
) -> Option<PathBuf> {
    let clean = sanitize_plugin_rel_subpath(raw_source)?;
    if clean == Path::new(".") {
        return Some(clean);
    }
    if let Some(prefix) = plugin_root_prefix {
        if prefix != Path::new(".") && !raw_source.starts_with("./") && !raw_source.starts_with('/')
        {
            let prefixed = prefix.join(&clean);
            if root_dir.join(&prefixed).is_dir() {
                return Some(prefixed);
            }
        }
    }
    if root_dir.join(&clean).is_dir() {
        return Some(clean);
    }
    if let Some(prefix) = plugin_root_prefix {
        if prefix != Path::new(".") {
            let prefixed = prefix.join(&clean);
            if root_dir.join(&prefixed).is_dir() {
                return Some(prefixed);
            }
        }
    }
    None
}

fn sorted_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|e| {
            let ft = e.file_type().ok()?;
            if !ft.is_dir() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | "__pycache__"
                        | "venv"
                        | "vendor"
                )
            {
                return None;
            }
            Some(e.path())
        })
        .collect();
    dirs.sort();
    dirs
}

fn resolve_subpath_from_slash_branch(clone_root: &Path, raw_tree_tail: &str) -> Option<String> {
    let segs: Vec<&str> = raw_tree_tail.split('/').filter(|s| !s.is_empty()).collect();
    for split_idx in 1..segs.len() {
        let candidate_sub = segs[split_idx..].join("/");
        if let Ok(p) = store::resolve_project_subpath(clone_root, &candidate_sub) {
            if p.is_dir() {
                return Some(candidate_sub);
            }
        }
    }
    None
}

fn native_plugin_dir_for_app(home: &Path, app_id: &str, slug: &str) -> Option<PathBuf> {
    match app_id {
        "claude" => Some(
            home.join(".claude")
                .join("plugins")
                .join("local")
                .join(slug),
        ),
        "codex" => Some(home.join(".codex").join("plugins").join(slug)),
        "cursor" => Some(
            home.join(".cursor")
                .join("plugins")
                .join("local")
                .join(slug),
        ),
        "antigravity" => Some(
            home.join(".gemini")
                .join("config")
                .join("plugins")
                .join(slug),
        ),
        "gemini" => Some(home.join(".gemini").join("extensions").join(slug)),
        _ => None,
    }
}

fn stage_native_plugin_global(
    home: &Path,
    app_id: &str,
    slug: &str,
    src_dir: &Path,
) -> Result<bool, String> {
    let Some(target_dir) = native_plugin_dir_for_app(home, app_id, slug) else {
        return Ok(false);
    };
    store::create_skill_symlink(&target_dir, src_dir)?;

    // Also update non-destructive host enablement toggles where applicable
    match app_id {
        "antigravity" => {
            let cfg_path = home.join(".gemini").join("config").join("config.json");
            let _ = set_antigravity_plugin_enabled(&cfg_path, slug, true);
        }
        "codex" => {
            let cfg_path = home.join(".codex").join("config.toml");
            let _ = set_codex_plugin_enabled(&cfg_path, slug, true);
        }
        _ => {}
    }

    Ok(true)
}

fn unstage_native_plugin_global(home: &Path, app_id: &str, slug: &str) -> Result<(), String> {
    if let Some(target_dir) = native_plugin_dir_for_app(home, app_id, slug) {
        if target_dir.exists() || store::is_symlink(&target_dir) {
            store::remove_symlink(&target_dir)?;
        }
    }
    match app_id {
        "antigravity" => {
            let cfg_path = home.join(".gemini").join("config").join("config.json");
            let _ = set_antigravity_plugin_enabled(&cfg_path, slug, false);
        }
        "codex" => {
            let cfg_path = home.join(".codex").join("config.toml");
            let _ = set_codex_plugin_enabled(&cfg_path, slug, false);
        }
        _ => {}
    }
    Ok(())
}

fn is_native_plugin_staged(home: &Path, app_id: &str, slug: &str) -> bool {
    native_plugin_dir_for_app(home, app_id, slug)
        .map(|p| p.exists())
        .unwrap_or(false)
}

fn set_antigravity_plugin_enabled(
    cfg_path: &Path,
    slug: &str,
    enabled: bool,
) -> Result<(), String> {
    if !cfg_path.exists() && !enabled {
        return Ok(());
    }
    let mut root: JsonValue = if cfg_path.exists() {
        let raw = fs::read_to_string(cfg_path).map_err(|e| e.to_string())?;
        if raw.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&raw).map_err(|e| e.to_string())?
        }
    } else {
        json!({})
    };
    let Some(obj) = root.as_object_mut() else {
        return Ok(());
    };
    let plugins_obj = obj
        .entry("plugins".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(p_map) = plugins_obj {
        if enabled {
            p_map.insert(slug.to_string(), json!({ "enabled": true }));
        } else {
            p_map.remove(slug);
        }
    }
    let formatted = format!(
        "{}\n",
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?
    );
    mcp::backup_and_atomic_write(cfg_path, &formatted)
}

fn set_codex_plugin_enabled(cfg_path: &Path, slug: &str, enabled: bool) -> Result<(), String> {
    if !cfg_path.exists() && !enabled {
        return Ok(());
    }
    let mut doc: DocumentMut = if cfg_path.exists() {
        fs::read_to_string(cfg_path)
            .map_err(|e| e.to_string())?
            .parse::<DocumentMut>()
            .map_err(|e| e.to_string())?
    } else {
        DocumentMut::new()
    };

    if enabled {
        if !doc.contains_key("plugins") {
            let mut t = Table::new();
            t.set_implicit(true);
            doc.insert("plugins", Item::Table(t));
        }
        if let Some(pt) = doc["plugins"].as_table_mut() {
            let mut entry = Table::new();
            entry.insert("enabled", toml_edit::value(true));
            pt.insert(slug, Item::Table(entry));
        }
    } else if let Some(pt) = doc.get_mut("plugins").and_then(|i| i.as_table_mut()) {
        pt.remove(slug);
    }

    mcp::backup_and_atomic_write(cfg_path, &doc.to_string())
}

fn is_plugin_projected_global(
    _app: &tauri::AppHandle,
    library: &RepoLibrary,
    plugin: &PluginRecord,
    app_id: &str,
    home: &Path,
    custom_apps: &[CustomAppConfig],
) -> bool {
    if let Some(first_skill_id) = plugin.projected_skill_ids.first() {
        if let Some(res) = library.resources.iter().find(|r| &r.id == first_skill_id) {
            if let Ok(dirs) = store::global_skill_dirs_for_app(home, app_id, custom_apps) {
                return dirs.iter().any(|d| d.join(&res.slug).exists());
            }
        }
    }
    if let Some(first_mcp_id) = plugin.projected_mcp_server_ids.first() {
        if let Some(rec) = library.mcp_servers.iter().find(|m| &m.id == first_mcp_id) {
            if let Ok(Some(target)) = mcp::resolve_global_mcp_target(home, app_id, custom_apps) {
                return mcp::target_has_mcp_entry(&target, &rec.slug);
            }
        }
    }
    false
}

fn is_plugin_projected_in_project(
    _app: &tauri::AppHandle,
    library: &RepoLibrary,
    plugin: &PluginRecord,
    app_id: &str,
    project_raw: &str,
    custom_apps: &[CustomAppConfig],
) -> bool {
    let Ok(project_root) = store::normalize_user_path(project_raw) else {
        return false;
    };
    if let Some(first_skill_id) = plugin.projected_skill_ids.first() {
        if let Some(res) = library.resources.iter().find(|r| &r.id == first_skill_id) {
            if let Ok(dirs) = store::project_skill_dirs_for_app(&project_root, app_id, custom_apps)
            {
                return dirs.iter().any(|d| d.join(&res.slug).exists());
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        if let Some(first_mcp_id) = plugin.projected_mcp_server_ids.first() {
            if let Some(rec) = library.mcp_servers.iter().find(|m| &m.id == first_mcp_id) {
                for mode in [ClaudeProjectMcpMode::Local, ClaudeProjectMcpMode::Project] {
                    if let Ok(Some(target)) = mcp::resolve_project_mcp_target(
                        &home,
                        &project_root,
                        app_id,
                        mode,
                        custom_apps,
                    ) {
                        if mcp::target_has_mcp_entry(&target, &rec.slug) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn adapt_projected_skill_namespaces(skill_content: &str, plugin_slug: &str) -> String {
    // Rewrites `mcp__plugin_<any>_<srv>__` -> `mcp__<plugin_slug>-<srv>__` when projected standalone
    let marker = "mcp__plugin_";
    if !skill_content.contains(marker) {
        return skill_content.to_string();
    }
    let mut result = String::with_capacity(skill_content.len());
    let mut rest = skill_content;
    while let Some(pos) = rest.find(marker) {
        result.push_str(&rest[..pos]);
        let after_marker = &rest[pos + marker.len()..];
        if let Some(end_ns) = after_marker.find("__") {
            let ns_body = &after_marker[..end_ns];
            if let Some((_, srv_part)) = ns_body.split_once('_') {
                result.push_str(&format!("mcp__{}-{srv_part}__", plugin_slug));
                rest = &after_marker[end_ns + 2..];
                continue;
            }
        }
        result.push_str(marker);
        rest = after_marker;
    }
    result.push_str(rest);
    result
}

fn copy_plugin_tree_filtered(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建目录 {:?} 失败: {}", dst, e))?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "node_modules" || name == "__pycache__" || name == ".DS_Store"
        {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let target = dst.join(&name);
        if file_type.is_dir() {
            copy_plugin_tree_filtered(&path, &target)?;
        } else if file_type.is_file() {
            fs::copy(&path, &target).map_err(|e| format!("复制文件 {:?} 失败: {}", path, e))?;
        }
    }
    Ok(())
}

fn sanitize_zip_rel_path(raw: &str) -> Result<PathBuf, String> {
    let mut clean = PathBuf::new();
    for comp in Path::new(raw).components() {
        match comp {
            Component::Normal(part) => {
                let s = part.to_string_lossy();
                if s == "__MACOSX" || s == ".DS_Store" {
                    return Ok(PathBuf::new());
                }
                clean.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("不安全的 ZIP 路径: {}", raw));
            }
        }
    }
    Ok(clean)
}

fn unwrap_single_child_dir(root: &Path) -> PathBuf {
    let mut current = root.to_path_buf();
    for _ in 0..4 {
        let Ok(entries) = fs::read_dir(&current) else {
            break;
        };
        let meaningful: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .map(|n| {
                        let s = n.to_string_lossy();
                        s != "__MACOSX" && s != ".DS_Store"
                    })
                    .unwrap_or(false)
            })
            .collect();

        if meaningful.len() != 1 || !meaningful[0].is_dir() {
            break;
        }
        let child_name = meaningful[0]
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Never unwrap into internal plugin component directories!
        if matches!(
            child_name.as_str(),
            "skills"
                | "commands"
                | "hooks"
                | "agents"
                | "rules"
                | "plugins"
                | "extensions"
                | "external_plugins"
                | "packages"
                | ".claude-plugin"
                | ".codex-plugin"
                | ".cursor-plugin"
                | ".antigravity-plugin"
                | ".gemini-plugin"
                | ".agents"
        ) {
            break;
        }
        current = meaningful.into_iter().next().unwrap();
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_plugin_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "skill-switch-plugin-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn inspects_claude_plugin_and_expands_root_and_user_config_macros() {
        let root = temp_plugin_dir("claude-bundle");
        fs::create_dir_all(root.join(".claude-plugin")).unwrap();
        fs::create_dir_all(root.join("skills").join("review")).unwrap();
        fs::create_dir_all(root.join("hooks")).unwrap();

        fs::write(
            root.join(".claude-plugin").join("plugin.json"),
            r#"{
  "name": "devops-suite",
  "version": "1.2.0",
  "description": "DevOps workflow & Sentry MCP",
  "userConfig": {
    "ORG_SLUG": { "title": "Organization", "default": "acme" },
    "SENTRY_TOKEN": { "title": "Sentry Token", "sensitive": true, "required": true }
  }
}"#,
        )
        .unwrap();

        fs::write(
            root.join(".mcp.json"),
            r#"{
  "mcpServers": {
    "sentry": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/bin/sentry-mcp.js", "--org", "${user_config.ORG_SLUG}"],
      "env": {
        "SENTRY_AUTH_TOKEN": "${user_config.SENTRY_TOKEN}"
      }
    }
  }
}"#,
        )
        .unwrap();

        fs::write(
            root.join("skills").join("review").join("SKILL.md"),
            "---\nname: sentry-review\ndescription: Triages Sentry issues\n---\nUse mcp__plugin_devops_sentry__list_issues to inspect errors.\n",
        )
        .unwrap();
        fs::write(root.join("hooks").join("hooks.json"), "{}").unwrap();

        let inspected = inspect_plugin_directory(&root).expect("inspect should succeed");
        assert_eq!(inspected.manifest_kind, PluginManifestKind::ClaudePlugin);
        assert_eq!(inspected.name, "devops-suite");
        assert_eq!(inspected.version.as_deref(), Some("1.2.0"));
        assert_eq!(inspected.components.skills, vec!["review".to_string()]);
        assert_eq!(inspected.components.mcp_servers, vec!["sentry".to_string()]);
        assert_eq!(
            inspected.components.hooks,
            vec!["hooks/hooks.json".to_string()]
        );
        assert!(!inspected.warnings.is_empty());

        let mut cfg_vals = HashMap::new();
        cfg_vals.insert("ORG_SLUG".to_string(), "acme".to_string());
        let mut sensitive = HashSet::new();
        sensitive.insert("SENTRY_TOKEN".to_string());

        let expanded_arg = expand_plugin_macros(
            "${CLAUDE_PLUGIN_ROOT}/bin/sentry-mcp.js",
            &root,
            &cfg_vals,
            &sensitive,
        );
        assert!(expanded_arg.ends_with("/bin/sentry-mcp.js"));
        assert!(!expanded_arg.contains("${CLAUDE_PLUGIN_ROOT}"));

        let expanded_org =
            expand_plugin_macros("${user_config.ORG_SLUG}", &root, &cfg_vals, &sensitive);
        assert_eq!(expanded_org, "acme");

        let expanded_secret =
            expand_plugin_macros("${user_config.SENTRY_TOKEN}", &root, &cfg_vals, &sensitive);
        assert_eq!(expanded_secret, "${SENTRY_TOKEN}");

        let adapted = adapt_projected_skill_namespaces(
            "Call mcp__plugin_devops_sentry__list_issues now.",
            "devops-suite",
        );
        assert_eq!(adapted, "Call mcp__devops-suite-sentry__list_issues now.");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovers_multi_plugin_marketplace_like_stitch_skills() {
        let root = temp_plugin_dir("stitch-skills-repo");
        fs::create_dir_all(root.join(".agents").join("plugins")).unwrap();
        fs::write(
            root.join(".agents")
                .join("plugins")
                .join("marketplace.json"),
            r#"{
  "name": "stitch-skills",
  "interface": { "displayName": "Stitch Skills" },
  "plugins": [
    {
      "name": "stitch-design",
      "source": { "source": "local", "path": "./plugins/stitch-design" },
      "category": "Productivity"
    },
    {
      "name": "stitch-build",
      "source": { "source": "local", "path": "./plugins/stitch-build" },
      "category": "Productivity"
    }
  ]
}"#,
        )
        .unwrap();

        for (plug, skill) in [
            ("stitch-design", "generate-design"),
            ("stitch-build", "react-components"),
        ] {
            let pdir = root.join("plugins").join(plug);
            fs::create_dir_all(pdir.join(".codex-plugin")).unwrap();
            fs::create_dir_all(pdir.join("skills").join(skill)).unwrap();
            fs::write(
                pdir.join("plugin.json"),
                format!(
                    r#"{{ "name": "{}", "version": "1.0.0", "description": "Plugin {}" }}"#,
                    plug, plug
                ),
            )
            .unwrap();
            fs::write(
                pdir.join(".codex-plugin").join("plugin.json"),
                format!(
                    r#"{{ "name": "{}", "version": "1.0.0", "skills": "./skills/" }}"#,
                    plug
                ),
            )
            .unwrap();
            fs::write(
                pdir.join("skills").join(skill).join("SKILL.md"),
                format!(
                    "---\nname: stitch::{}\ndescription: >-\n  Skill for {}\n---\nBody\n",
                    skill, plug
                ),
            )
            .unwrap();
        }

        let discovered = discover_plugins_in_source(&root).expect("should discover marketplace");
        assert_eq!(
            discovered.marketplace_name.as_deref(),
            Some("Stitch Skills")
        );
        assert_eq!(discovered.candidates.len(), 2);
        assert_eq!(discovered.candidates[0].sub_path, "plugins/stitch-design");
        assert_eq!(discovered.candidates[0].bundle.name, "stitch-design");
        assert_eq!(
            discovered.candidates[0].bundle.manifest_kind,
            PluginManifestKind::AgentPlugins
        );
        assert_eq!(
            discovered.candidates[0].bundle.components.skills,
            vec!["generate-design".to_string()]
        );
        assert_eq!(discovered.candidates[1].sub_path, "plugins/stitch-build");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unwrap_single_child_dir_preserves_dotfile_plugin_roots() {
        let zip_root = temp_plugin_dir("zip-extract");
        let wrapper = zip_root.join("my-plugin-main");
        fs::create_dir_all(wrapper.join(".claude-plugin")).unwrap();
        fs::create_dir_all(wrapper.join("skills").join("demo")).unwrap();
        fs::write(
            wrapper.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"demo-plug"}"#,
        )
        .unwrap();
        fs::write(
            wrapper.join("skills").join("demo").join("SKILL.md"),
            "# Demo\n",
        )
        .unwrap();

        let unwrapped = unwrap_single_child_dir(&zip_root);
        // Should unwrap `my-plugin-main`, NOT descend into `skills`!
        assert_eq!(unwrapped, wrapper);

        let _ = fs::remove_dir_all(zip_root);
    }

    #[test]
    fn parses_git_plugin_urls_shorthands_and_cli_commands() {
        let spec1 = parse_git_plugin_spec(
            "https://github.com/google-labs-code/stitch-skills/tree/main/plugins/stitch-build",
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            spec1.repo_url,
            "https://github.com/google-labs-code/stitch-skills.git"
        );
        assert_eq!(spec1.branch.as_deref(), Some("main"));
        assert_eq!(spec1.sub_path.as_deref(), Some("plugins/stitch-build"));

        let spec2 = parse_git_plugin_spec("google-labs-code/stitch-skills", None, None).unwrap();
        assert_eq!(
            spec2.repo_url,
            "https://github.com/google-labs-code/stitch-skills.git"
        );
        assert_eq!(spec2.branch, None);
        assert_eq!(spec2.sub_path, None);

        let spec3 = parse_git_plugin_spec(
            "codex plugin marketplace add google-labs-code/stitch-skills --ref main --sparse plugins/stitch-design",
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            spec3.repo_url,
            "https://github.com/google-labs-code/stitch-skills.git"
        );
        assert_eq!(spec3.branch.as_deref(), Some("main"));
    }
}
