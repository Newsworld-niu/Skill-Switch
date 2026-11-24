use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{Array as TomlArray, DocumentMut, InlineTable, Item, Table, Value as TomlValue};
use uuid::Uuid;

use crate::domain::{
    CheckMcpStatusInput, CheckMcpStatusResult, ClaudeProjectMcpMode, CreateMcpServerInput,
    CreateMcpServerResult, CustomAppConfig, ExternalMcpServerDto, ImportExternalMcpInput,
    InstallMcpGlobalInput, InstallMcpResult, InstallMcpToProjectInput, McpAppInstallStatus,
    McpEnvVarSpec, McpHeaderSpec, McpServerDto, McpServerRecord, McpTransport, Provenance,
    ProvenanceKind, RepoLibrary, UpdateMcpServerInput,
};
use crate::secrets;
use crate::store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Json,
    Toml,
}

#[derive(Debug, Clone)]
pub struct AppMcpTarget {
    pub config_path: PathBuf,
    pub format: ConfigFormat,
    pub root_key: String,
    /// When `Some(project_key)`, nests inside `projects[project_key].mcpServers` (Claude `local` scope)
    pub claude_local_project_key: Option<String>,
    /// True when writing into a shared repository file (e.g. `<project>/.mcp.json`), forcing `${ENV_VAR}` placeholders for secrets
    pub force_secret_placeholders: bool,
    pub scope_detail: String,
    pub warning_hint: Option<String>,
}

pub fn sanitize_env_specs(specs: &[McpEnvVarSpec]) -> Vec<McpEnvVarSpec> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for spec in specs {
        let name = spec.name.trim().to_string();
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        out.push(McpEnvVarSpec {
            name,
            description: spec
                .description
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            required: spec.required,
            is_secret: spec.is_secret,
            // NEVER persist default values for secret env vars into library.json / Git backup
            default_value: if spec.is_secret {
                None
            } else {
                spec.default_value
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            },
        });
    }
    out
}

pub fn sanitize_header_specs(specs: &[McpHeaderSpec]) -> Vec<McpHeaderSpec> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for spec in specs {
        let name = spec.name.trim().to_string();
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        // For secret headers, only keep `value_template` if it is an `${ENV_VAR}` template (e.g. "Bearer ${API_KEY}"), never raw tokens!
        let safe_template = spec
            .value_template
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .filter(|tpl| !spec.is_secret || tpl.contains("${"));

        out.push(McpHeaderSpec {
            name,
            description: spec
                .description
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            is_secret: spec.is_secret,
            value_template: safe_template,
        });
    }
    out
}

pub fn record_to_dto(app: &tauri::AppHandle, record: &McpServerRecord) -> McpServerDto {
    let configured_secret_keys =
        secrets::configured_secret_keys(app, &secrets::mcp_namespace(&record.id));
    McpServerDto {
        id: record.id.clone(),
        slug: record.slug.clone(),
        name: record.name.clone(),
        description: record.description.clone(),
        transport: record.transport,
        command: record.command.clone(),
        args: record.args.clone(),
        url: record.url.clone(),
        env_specs: record.env_specs.clone(),
        header_specs: record.header_specs.clone(),
        tags: record.tags.clone(),
        project_ids: record.project_ids.clone(),
        plugin_id: record.plugin_id.clone(),
        configured_secret_keys,
        created_at: record.created_at,
        updated_at: record.updated_at,
        provenance: record.provenance.clone(),
    }
}

pub fn resolve_global_mcp_target(
    home: &Path,
    app_id: &str,
    custom_apps: &[CustomAppConfig],
) -> Result<Option<AppMcpTarget>, String> {
    match app_id {
        "claude" => Ok(Some(AppMcpTarget {
            config_path: home.join(".claude.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "user (~/.claude.json)".to_string(),
            warning_hint: None,
        })),
        "codex" => Ok(Some(AppMcpTarget {
            config_path: home.join(".codex").join("config.toml"),
            format: ConfigFormat::Toml,
            root_key: "mcp_servers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "global (~/.codex/config.toml)".to_string(),
            warning_hint: None,
        })),
        "cursor" => Ok(Some(AppMcpTarget {
            config_path: home.join(".cursor").join("mcp.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "global (~/.cursor/mcp.json)".to_string(),
            warning_hint: None,
        })),
        "antigravity" => Ok(Some(AppMcpTarget {
            config_path: home.join(".gemini").join("config").join("mcp_config.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "global (~/.gemini/config/mcp_config.json)".to_string(),
            warning_hint: None,
        })),
        "gemini" => Ok(Some(AppMcpTarget {
            config_path: home.join(".gemini").join("settings.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "global (~/.gemini/settings.json)".to_string(),
            warning_hint: None,
        })),
        "windsurf" => Ok(Some(AppMcpTarget {
            config_path: home
                .join(".codeium")
                .join("windsurf")
                .join("mcp_config.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "global (~/.codeium/windsurf/mcp_config.json)".to_string(),
            warning_hint: None,
        })),
        "t3" => Ok(None),
        _ => {
            let custom = custom_apps
                .iter()
                .find(|c| c.id == app_id)
                .ok_or_else(|| format!("未知的应用: {}", app_id))?;
            let Some(raw_path) = custom
                .global_mcp_config_path
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            else {
                return Ok(None);
            };
            let format = if custom
                .mcp_format
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case("toml"))
                .unwrap_or(false)
                || raw_path.trim().ends_with(".toml")
            {
                ConfigFormat::Toml
            } else {
                ConfigFormat::Json
            };
            let root_key = custom
                .mcp_root_key
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(if format == ConfigFormat::Toml {
                    "mcp_servers"
                } else {
                    "mcpServers"
                })
                .to_string();

            Ok(Some(AppMcpTarget {
                config_path: store::expand_home_dir(home, raw_path),
                format,
                root_key,
                claude_local_project_key: None,
                force_secret_placeholders: false,
                scope_detail: format!("global ({})", raw_path),
                warning_hint: None,
            }))
        }
    }
}

pub fn resolve_project_mcp_target(
    home: &Path,
    project_root: &Path,
    app_id: &str,
    claude_mode: ClaudeProjectMcpMode,
    custom_apps: &[CustomAppConfig],
) -> Result<Option<AppMcpTarget>, String> {
    let project_key = project_root.to_string_lossy().into_owned();
    match app_id {
        "claude" => match claude_mode {
            ClaudeProjectMcpMode::Local => Ok(Some(AppMcpTarget {
                config_path: home.join(".claude.json"),
                format: ConfigFormat::Json,
                root_key: "mcpServers".to_string(),
                claude_local_project_key: Some(project_key),
                force_secret_placeholders: false,
                scope_detail: "local (~/.claude.json#projects)".to_string(),
                warning_hint: None,
            })),
            ClaudeProjectMcpMode::Project => Ok(Some(AppMcpTarget {
                config_path: project_root.join(".mcp.json"),
                format: ConfigFormat::Json,
                root_key: "mcpServers".to_string(),
                claude_local_project_key: None,
                force_secret_placeholders: true,
                scope_detail: "project (.mcp.json)".to_string(),
                warning_hint: None,
            })),
        },
        "codex" => Ok(Some(AppMcpTarget {
            config_path: project_root.join(".codex").join("config.toml"),
            format: ConfigFormat::Toml,
            root_key: "mcp_servers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: true,
            scope_detail: "project (.codex/config.toml)".to_string(),
            warning_hint: Some(
                "Codex 项目级 .codex/config.toml 仅在已标记为 Trusted 的项目中生效".to_string(),
            ),
        })),
        "cursor" => Ok(Some(AppMcpTarget {
            config_path: project_root.join(".cursor").join("mcp.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: true,
            scope_detail: "project (.cursor/mcp.json)".to_string(),
            warning_hint: None,
        })),
        "antigravity" => Ok(Some(AppMcpTarget {
            config_path: project_root.join(".agents").join("mcp_config.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: true,
            scope_detail: "project (.agents/mcp_config.json)".to_string(),
            warning_hint: None,
        })),
        "gemini" => Ok(Some(AppMcpTarget {
            config_path: project_root.join(".gemini").join("settings.json"),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: true,
            scope_detail: "project (.gemini/settings.json)".to_string(),
            warning_hint: None,
        })),
        "windsurf" | "t3" => Ok(None),
        _ => {
            let custom = custom_apps
                .iter()
                .find(|c| c.id == app_id)
                .ok_or_else(|| format!("未知的应用: {}", app_id))?;
            let Some(raw_rel) = custom
                .project_mcp_config_path
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            else {
                return Ok(None);
            };
            let format = if custom
                .mcp_format
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case("toml"))
                .unwrap_or(false)
                || raw_rel.trim().ends_with(".toml")
            {
                ConfigFormat::Toml
            } else {
                ConfigFormat::Json
            };
            let root_key = custom
                .mcp_root_key
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(if format == ConfigFormat::Toml {
                    "mcp_servers"
                } else {
                    "mcpServers"
                })
                .to_string();

            Ok(Some(AppMcpTarget {
                config_path: store::resolve_project_subpath(project_root, raw_rel)?,
                format,
                root_key,
                claude_local_project_key: None,
                force_secret_placeholders: true,
                scope_detail: format!("project ({})", raw_rel),
                warning_hint: None,
            }))
        }
    }
}

pub fn unsupported_mcp_reason(app_id: &str, scope: &str) -> String {
    match (app_id, scope) {
        ("t3", _) => {
            "T3 Code 通过底层 Claude / Codex / Antigravity 会话驱动 MCP，请直接在对应底层 CLI 中启用"
                .to_string()
        }
        ("windsurf", "project") => {
            "Windsurf 仅支持全局 ~/.codeium/windsurf/mcp_config.json，无项目级标准 MCP 配置文件"
                .to_string()
        }
        (_, "project") => format!("应用 {} 未配置项目级 MCP 配置文件路径", app_id),
        _ => format!("应用 {} 未配置全局 MCP 配置文件路径", app_id),
    }
}

type KeyValuePairs = Vec<(String, String)>;

/// Build resolved env map and header map for a target app, enforcing secret placeholder safety!
fn resolve_env_and_headers_for_target(
    record: &McpServerRecord,
    local_secrets: &HashMap<String, String>,
    app_id: &str,
    use_placeholders: bool,
    warnings: &mut Vec<String>,
) -> (KeyValuePairs, KeyValuePairs) {
    let mut env_pairs = Vec::new();
    let mut placeholder_used = false;

    for spec in &record.env_specs {
        let val = if spec.is_secret {
            if use_placeholders {
                placeholder_used = true;
                if app_id == "cursor" {
                    format!("${{env:{}}}", spec.name)
                } else {
                    format!("${{{}}}", spec.name)
                }
            } else if let Some(secret) = local_secrets.get(&spec.name).filter(|v| !v.is_empty()) {
                secret.clone()
            } else {
                warnings.push(format!(
                    "MCP「{}」尚未在本机配置敏感环境变量 {}，已使用占位符写入",
                    record.name, spec.name
                ));
                if app_id == "cursor" {
                    format!("${{env:{}}}", spec.name)
                } else {
                    format!("${{{}}}", spec.name)
                }
            }
        } else {
            spec.default_value.clone().unwrap_or_default()
        };
        env_pairs.push((spec.name.clone(), val));
    }

    let mut header_pairs = Vec::new();
    for hspec in &record.header_specs {
        let val = if hspec.is_secret {
            if use_placeholders {
                placeholder_used = true;
                hspec
                    .value_template
                    .clone()
                    .unwrap_or_else(|| format!("${{{}}}", hspec.name))
            } else if let Some(secret) = local_secrets.get(&hspec.name).filter(|v| !v.is_empty()) {
                secret.clone()
            } else if let Some(tpl) = &hspec.value_template {
                let mut expanded = tpl.clone();
                for (k, v) in local_secrets {
                    expanded = expanded.replace(&format!("${{{}}}", k), v);
                }
                expanded
            } else {
                format!("${{{}}}", hspec.name)
            }
        } else {
            hspec.value_template.clone().unwrap_or_default()
        };
        header_pairs.push((hspec.name.clone(), val));
    }

    if placeholder_used {
        warnings.push(format!(
            "为防止项目代码仓库泄露密钥，MCP「{}」的敏感变量已自动以 ${{ENV_VAR}} 占位符写入项目配置",
            record.name
        ));
    }

    (env_pairs, header_pairs)
}

pub fn build_json_entry_for_app(
    record: &McpServerRecord,
    local_secrets: &HashMap<String, String>,
    app_id: &str,
    force_placeholders: bool,
    warnings: &mut Vec<String>,
) -> Result<JsonValue, String> {
    if app_id == "antigravity" && record.transport == McpTransport::Sse {
        warnings.push(format!(
            "Antigravity 仅原生支持 stdio 与 Streamable HTTP（不支持旧版 SSE），MCP「{}」可能需要通过 mcp-remote 桥接",
            record.name
        ));
    }

    let (env_pairs, header_pairs) = resolve_env_and_headers_for_target(
        record,
        local_secrets,
        app_id,
        force_placeholders,
        warnings,
    );

    let mut obj = JsonMap::new();
    match record.transport {
        McpTransport::Stdio => {
            let cmd = record
                .command
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("stdio 类型的 MCP「{}」缺少 command", record.name))?;
            if app_id == "claude" {
                obj.insert("type".to_string(), json!("stdio"));
            }
            obj.insert("command".to_string(), json!(cmd));
            obj.insert("args".to_string(), json!(record.args));
            if !env_pairs.is_empty() {
                let mut env_map = JsonMap::new();
                for (k, v) in env_pairs {
                    env_map.insert(k, json!(v));
                }
                obj.insert("env".to_string(), JsonValue::Object(env_map));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            let url = record
                .url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("远程类型的 MCP「{}」缺少 url", record.name))?;

            let transport_str = if record.transport == McpTransport::Sse {
                "sse"
            } else {
                "http"
            };

            match app_id {
                "windsurf" => {
                    obj.insert("serverUrl".to_string(), json!(url));
                }
                "antigravity" => {
                    obj.insert("serverUrl".to_string(), json!(url));
                }
                "gemini" => {
                    if record.transport == McpTransport::Http {
                        obj.insert("httpUrl".to_string(), json!(url));
                    } else {
                        obj.insert("url".to_string(), json!(url));
                    }
                }
                "claude" => {
                    obj.insert("type".to_string(), json!(transport_str));
                    obj.insert("url".to_string(), json!(url));
                }
                _ => {
                    obj.insert("url".to_string(), json!(url));
                }
            }

            if !header_pairs.is_empty() {
                let mut hdr_map = JsonMap::new();
                for (k, v) in header_pairs {
                    hdr_map.insert(k, json!(v));
                }
                obj.insert("headers".to_string(), JsonValue::Object(hdr_map));
            }
        }
    }

    Ok(JsonValue::Object(obj))
}

pub(crate) fn backup_and_atomic_write(
    config_path: &Path,
    new_contents: &str,
) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败 {:?}: {}", parent, e))?;
    }

    if config_path.exists() {
        let file_name = config_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config".to_string());
        let bak_path = config_path.with_file_name(format!("{}.skill-switch.bak", file_name));
        let _ = fs::copy(config_path, &bak_path);
    }

    let file_name = config_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".to_string());
    let tmp_path = config_path.with_file_name(format!(".{}.skill-switch.tmp", file_name));
    fs::write(&tmp_path, new_contents).map_err(|e| format!("写入临时配置失败: {}", e))?;
    fs::rename(&tmp_path, config_path).map_err(|e| format!("替换配置文件失败: {}", e))
}

pub fn upsert_json_mcp_entry(
    target: &AppMcpTarget,
    slug: &str,
    entry: JsonValue,
) -> Result<(), String> {
    let mut root: JsonValue = if target.config_path.exists() {
        let raw = fs::read_to_string(&target.config_path)
            .map_err(|e| format!("读取配置文件 {:?} 失败: {}", target.config_path, e))?;
        if raw.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&raw).map_err(|e| {
                format!(
                    "配置文件 {:?} 包含无效的 JSON 语法，已中止写入以保护原有数据: {}",
                    target.config_path, e
                )
            })?
        }
    } else {
        json!({})
    };

    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| format!("配置文件 {:?} 根节点不是 JSON 对象", target.config_path))?;

    let servers_map = if let Some(ref project_key) = target.claude_local_project_key {
        let projects_val = root_obj
            .entry("projects".to_string())
            .or_insert_with(|| json!({}));
        let projects_obj = projects_val
            .as_object_mut()
            .ok_or_else(|| "~/.claude.json 中的 projects 字段不是对象".to_string())?;
        let proj_val = projects_obj
            .entry(project_key.clone())
            .or_insert_with(|| json!({}));
        let proj_obj = proj_val
            .as_object_mut()
            .ok_or_else(|| "~/.claude.json 中的项目节点不是对象".to_string())?;
        let mcp_val = proj_obj
            .entry(target.root_key.clone())
            .or_insert_with(|| json!({}));
        mcp_val
            .as_object_mut()
            .ok_or_else(|| format!("{} 字段不是 JSON 对象", target.root_key))?
    } else {
        let mcp_val = root_obj
            .entry(target.root_key.clone())
            .or_insert_with(|| json!({}));
        mcp_val
            .as_object_mut()
            .ok_or_else(|| format!("{} 字段不是 JSON 对象", target.root_key))?
    };

    servers_map.insert(slug.to_string(), entry);

    let formatted = format!(
        "{}\n",
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?
    );
    backup_and_atomic_write(&target.config_path, &formatted)
}

pub fn remove_json_mcp_entry(target: &AppMcpTarget, slug: &str) -> Result<bool, String> {
    if !target.config_path.exists() {
        return Ok(false);
    }
    let raw = fs::read_to_string(&target.config_path)
        .map_err(|e| format!("读取配置文件 {:?} 失败: {}", target.config_path, e))?;
    if raw.trim().is_empty() {
        return Ok(false);
    }
    let mut root: JsonValue = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "配置文件 {:?} 包含无效的 JSON 语法，已中止移除以保护原有数据: {}",
            target.config_path, e
        )
    })?;

    let Some(root_obj) = root.as_object_mut() else {
        return Ok(false);
    };

    let removed = if let Some(ref project_key) = target.claude_local_project_key {
        root_obj
            .get_mut("projects")
            .and_then(|p| p.as_object_mut())
            .and_then(|p| p.get_mut(project_key))
            .and_then(|proj| proj.as_object_mut())
            .and_then(|proj| proj.get_mut(&target.root_key))
            .and_then(|m| m.as_object_mut())
            .and_then(|m| m.remove(slug))
            .is_some()
    } else {
        root_obj
            .get_mut(&target.root_key)
            .and_then(|m| m.as_object_mut())
            .and_then(|m| m.remove(slug))
            .is_some()
    };

    if removed {
        let formatted = format!(
            "{}\n",
            serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?
        );
        backup_and_atomic_write(&target.config_path, &formatted)?;
    }

    Ok(removed)
}

pub fn upsert_toml_mcp_entry(
    target: &AppMcpTarget,
    slug: &str,
    record: &McpServerRecord,
    local_secrets: &HashMap<String, String>,
    force_placeholders: bool,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let mut doc: DocumentMut = if target.config_path.exists() {
        let raw = fs::read_to_string(&target.config_path)
            .map_err(|e| format!("读取 TOML 配置 {:?} 失败: {}", target.config_path, e))?;
        raw.parse::<DocumentMut>().map_err(|e| {
            format!(
                "配置文件 {:?} 包含无效的 TOML 语法，已中止写入以保护原有数据与注释: {}",
                target.config_path, e
            )
        })?
    } else {
        DocumentMut::new()
    };

    if !doc.contains_key(&target.root_key) {
        let mut parent = Table::new();
        parent.set_implicit(true);
        doc.insert(&target.root_key, Item::Table(parent));
    }

    let root_table = doc[&target.root_key]
        .as_table_mut()
        .ok_or_else(|| format!("TOML 中的 [{}] 不是表结构", target.root_key))?;

    let mut srv_table = Table::new();
    match record.transport {
        McpTransport::Stdio => {
            let cmd = record
                .command
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("stdio 类型的 MCP「{}」缺少 command", record.name))?;
            srv_table.insert("command", toml_edit::value(cmd));

            if !record.args.is_empty() {
                let mut arr = TomlArray::new();
                for arg in &record.args {
                    arr.push(arg.as_str());
                }
                srv_table.insert("args", Item::Value(TomlValue::Array(arr)));
            }

            if !record.env_specs.is_empty() {
                if force_placeholders {
                    // For shared project .codex/config.toml, list secret keys in `env_vars` so Codex forwards shell env vars without storing secrets in Git!
                    let mut env_vars_arr = TomlArray::new();
                    let mut inline_env = InlineTable::new();
                    for spec in &record.env_specs {
                        if spec.is_secret {
                            env_vars_arr.push(spec.name.as_str());
                        } else if let Some(def) = &spec.default_value {
                            inline_env.insert(&spec.name, TomlValue::from(def.as_str()));
                        }
                    }
                    if !env_vars_arr.is_empty() {
                        srv_table.insert("env_vars", Item::Value(TomlValue::Array(env_vars_arr)));
                        warnings.push(
                            "Codex 项目级配置已通过 env_vars 转发敏感环境变量（不在 .codex/config.toml 中写入明文密钥）"
                                .to_string(),
                        );
                    }
                    if !inline_env.is_empty() {
                        srv_table.insert("env", Item::Value(TomlValue::InlineTable(inline_env)));
                    }
                } else {
                    let mut env_table = Table::new();
                    let mut env_vars_arr = TomlArray::new();
                    for spec in &record.env_specs {
                        if spec.is_secret {
                            if let Some(secret) =
                                local_secrets.get(&spec.name).filter(|v| !v.is_empty())
                            {
                                env_table.insert(&spec.name, toml_edit::value(secret.as_str()));
                            } else {
                                env_vars_arr.push(spec.name.as_str());
                            }
                        } else if let Some(def) = &spec.default_value {
                            env_table.insert(&spec.name, toml_edit::value(def.as_str()));
                        }
                    }
                    if !env_vars_arr.is_empty() {
                        srv_table.insert("env_vars", Item::Value(TomlValue::Array(env_vars_arr)));
                    }
                    if !env_table.is_empty() {
                        srv_table.insert("env", Item::Table(env_table));
                    }
                }
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            let url = record
                .url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("远程类型的 MCP「{}」缺少 url", record.name))?;
            srv_table.insert("url", toml_edit::value(url));

            // Map Authorization Bearer env var or headers cleanly for Codex TOML
            let mut http_headers = InlineTable::new();
            for hspec in &record.header_specs {
                if hspec.is_secret {
                    if hspec.name.eq_ignore_ascii_case("authorization") {
                        // Extract ${VAR} name if present, else use env key name
                        let env_var_name = hspec
                            .value_template
                            .as_deref()
                            .and_then(extract_env_var_placeholder)
                            .unwrap_or_else(|| "MCP_BEARER_TOKEN".to_string());
                        srv_table.insert("bearer_token_env_var", toml_edit::value(env_var_name));
                    } else if !force_placeholders {
                        if let Some(secret) =
                            local_secrets.get(&hspec.name).filter(|v| !v.is_empty())
                        {
                            http_headers.insert(&hspec.name, TomlValue::from(secret.as_str()));
                        }
                    }
                } else if let Some(tpl) = &hspec.value_template {
                    http_headers.insert(&hspec.name, TomlValue::from(tpl.as_str()));
                }
            }
            if !http_headers.is_empty() {
                srv_table.insert(
                    "http_headers",
                    Item::Value(TomlValue::InlineTable(http_headers)),
                );
            }
        }
    }

    root_table.insert(slug, Item::Table(srv_table));
    backup_and_atomic_write(&target.config_path, &doc.to_string())
}

pub fn remove_toml_mcp_entry(target: &AppMcpTarget, slug: &str) -> Result<bool, String> {
    if !target.config_path.exists() {
        return Ok(false);
    }
    let raw = fs::read_to_string(&target.config_path)
        .map_err(|e| format!("读取 TOML 配置 {:?} 失败: {}", target.config_path, e))?;
    let mut doc: DocumentMut = raw.parse::<DocumentMut>().map_err(|e| {
        format!(
            "配置文件 {:?} 包含无效的 TOML 语法，已中止移除以保护原有数据与注释: {}",
            target.config_path, e
        )
    })?;

    let removed = doc
        .get_mut(&target.root_key)
        .and_then(|item| item.as_table_mut())
        .and_then(|table| table.remove(slug))
        .is_some();

    if removed {
        backup_and_atomic_write(&target.config_path, &doc.to_string())?;
    }
    Ok(removed)
}

pub fn target_has_mcp_entry(target: &AppMcpTarget, slug: &str) -> bool {
    if !target.config_path.exists() {
        return false;
    }
    let Ok(raw) = fs::read_to_string(&target.config_path) else {
        return false;
    };
    match target.format {
        ConfigFormat::Json => {
            let Ok(root) = serde_json::from_str::<JsonValue>(&raw) else {
                return false;
            };
            if let Some(ref project_key) = target.claude_local_project_key {
                root.get("projects")
                    .and_then(|p| p.get(project_key))
                    .and_then(|proj| proj.get(&target.root_key))
                    .and_then(|m| m.get(slug))
                    .is_some()
            } else {
                root.get(&target.root_key)
                    .and_then(|m| m.get(slug))
                    .is_some()
            }
        }
        ConfigFormat::Toml => {
            let Ok(doc) = raw.parse::<DocumentMut>() else {
                return false;
            };
            doc.get(&target.root_key)
                .and_then(|t| t.as_table())
                .map(|t| t.contains_key(slug))
                .unwrap_or(false)
        }
    }
}

fn extract_env_var_placeholder(input: &str) -> Option<String> {
    let start = input.find("${")?;
    let after = &input[start + 2..];
    let end = after.find('}')?;
    let inner = after[..end].trim();
    let stripped = inner.strip_prefix("env:").unwrap_or(inner).trim();
    (!stripped.is_empty()).then(|| stripped.to_string())
}

pub fn looks_like_secret_env(key: &str, value: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    let key_suspicious = [
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "AUTH",
        "PAT",
        "DSN",
        "COOKIE",
        "BEARER",
    ]
    .iter()
    .any(|kw| upper.contains(kw));

    let trimmed = value.trim();
    let is_placeholder = trimmed.starts_with("${") && trimmed.ends_with('}');
    if is_placeholder {
        return key_suspicious;
    }

    let val_suspicious = trimmed.starts_with("sk-")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("github_pat_")
        || trimmed.starts_with("glpat-")
        || trimmed.starts_with("xoxb-")
        || trimmed.starts_with("xoxp-")
        || trimmed.starts_with("AIza");

    key_suspicious || val_suspicious
}

// ─── High-level CRUD & Install Operations ─────────────────────────────────────

pub fn list_mcp_servers(app: &tauri::AppHandle) -> Result<Vec<McpServerDto>, String> {
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let mut items: Vec<McpServerDto> = library
        .mcp_servers
        .iter()
        .map(|rec| record_to_dto(app, rec))
        .collect();
    items.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    Ok(items)
}

pub fn get_mcp_server(app: &tauri::AppHandle, id: &str) -> Result<Option<McpServerDto>, String> {
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    Ok(library
        .mcp_servers
        .iter()
        .find(|rec| rec.id == id)
        .map(|rec| record_to_dto(app, rec)))
}

pub fn create_mcp_server(
    app: &tauri::AppHandle,
    input: &CreateMcpServerInput,
) -> Result<CreateMcpServerResult, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("MCP Server 名称不能为空".to_string());
    }
    validate_transport_fields(
        input.transport,
        input.command.as_deref(),
        input.url.as_deref(),
        name,
    )?;

    let repo_root = store::connected_repo_root(app)?;
    let mut library = store::load_repo_library(&repo_root)?;
    let slug = input
        .slug
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(store::slugify)
        .unwrap_or_else(|| store::slugify(name));

    if library.mcp_servers.iter().any(|s| s.slug == slug) {
        return Err(format!("已存在标识为「{}」的 MCP Server", slug));
    }

    let now = store::now_ms();
    let id = Uuid::new_v4().to_string();
    let record = McpServerRecord {
        id: id.clone(),
        slug,
        name: name.to_string(),
        description: input
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        transport: input.transport,
        command: input
            .command
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        args: input
            .args
            .iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect(),
        url: input
            .url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        env_specs: sanitize_env_specs(&input.env_specs),
        header_specs: sanitize_header_specs(&input.header_specs),
        tags: input.tags.clone(),
        project_ids: input.project_ids.clone(),
        plugin_id: None,
        created_at: now,
        updated_at: now,
        provenance: Provenance {
            kind: ProvenanceKind::Manual,
            label: "本地创建".to_string(),
            ..Default::default()
        },
    };

    if !input.secret_values.is_empty() {
        secrets::update_namespace_secrets(app, &secrets::mcp_namespace(&id), &input.secret_values)?;
    }

    library.mcp_servers.push(record.clone());
    store::save_repo_library(&repo_root, &library)?;
    let backup_sync =
        store::sync_after_mutation(app, &format!("Create MCP server: {}", record.name));

    Ok(CreateMcpServerResult {
        server: record_to_dto(app, &record),
        backup_sync,
    })
}

pub fn update_mcp_server(
    app: &tauri::AppHandle,
    input: &UpdateMcpServerInput,
) -> Result<McpServerDto, String> {
    let repo_root = store::connected_repo_root(app)?;
    let mut library = store::load_repo_library(&repo_root)?;

    let idx = library
        .mcp_servers
        .iter()
        .position(|s| s.id == input.id)
        .ok_or_else(|| format!("未找到 MCP Server: {}", input.id))?;

    {
        let rec = &mut library.mcp_servers[idx];
        if let Some(ref name) = input.name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err("MCP Server 名称不能为空".to_string());
            }
            rec.name = trimmed.to_string();
        }
        if let Some(ref desc) = input.description {
            let trimmed = desc.trim();
            rec.description = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(transport) = input.transport {
            rec.transport = transport;
        }
        if let Some(ref cmd) = input.command {
            let trimmed = cmd.trim();
            rec.command = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(ref args) = input.args {
            rec.args = args
                .iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
        }
        if let Some(ref url) = input.url {
            let trimmed = url.trim();
            rec.url = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(ref env_specs) = input.env_specs {
            rec.env_specs = sanitize_env_specs(env_specs);
        }
        if let Some(ref header_specs) = input.header_specs {
            rec.header_specs = sanitize_header_specs(header_specs);
        }
        if let Some(ref tags) = input.tags {
            rec.tags = tags.clone();
        }
        if let Some(ref project_ids) = input.project_ids {
            rec.project_ids = project_ids.clone();
        }

        validate_transport_fields(
            rec.transport,
            rec.command.as_deref(),
            rec.url.as_deref(),
            &rec.name,
        )?;
        rec.updated_at = store::now_ms();
    }

    if let Some(ref secret_updates) = input.secret_values {
        secrets::update_namespace_secrets(app, &secrets::mcp_namespace(&input.id), secret_updates)?;
    }

    let updated_record = library.mcp_servers[idx].clone();
    store::save_repo_library(&repo_root, &library)?;
    let _ = store::sync_after_mutation(app, &format!("Update MCP server: {}", updated_record.name));

    // Re-sync already enabled global targets so updates apply immediately
    let custom_apps = store::load_custom_apps(app);
    if let Ok(home) = dirs::home_dir().ok_or_else(|| "无法获取主目录".to_string()) {
        for app_id in store::all_configured_app_ids(&custom_apps) {
            if crate::ssh_sync::is_ssh_app(&app_id, &custom_apps) {
                continue;
            }
            if let Ok(Some(target)) = resolve_global_mcp_target(&home, &app_id, &custom_apps) {
                if target_has_mcp_entry(&target, &updated_record.slug) {
                    let _ = install_record_to_target(
                        app,
                        &updated_record,
                        &app_id,
                        &target,
                        false,
                        &mut Vec::new(),
                    );
                }
            }
        }
    }
    crate::ssh_sync::resync_mcp_to_enabled_ssh_apps(app, &updated_record, &library);

    Ok(record_to_dto(app, &updated_record))
}

pub fn delete_mcp_server(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let repo_root = store::connected_repo_root(app)?;
    let mut library = store::load_repo_library(&repo_root)?;
    let pos = library
        .mcp_servers
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| format!("未找到 MCP Server: {}", id))?;

    let removed = library.mcp_servers.remove(pos);

    // Cleanly remove from any global configs where it was installed
    let custom_apps = store::load_custom_apps(app);
    if let Some(home) = dirs::home_dir() {
        for app_id in store::all_configured_app_ids(&custom_apps) {
            if crate::ssh_sync::is_ssh_app(&app_id, &custom_apps) {
                continue;
            }
            if let Ok(Some(target)) = resolve_global_mcp_target(&home, &app_id, &custom_apps) {
                let _ = remove_from_target(&target, &removed.slug);
            }
        }
    }

    // Best-effort SSH uninstall + clean SSH ledgers even if offline
    let initial_ledgers = store::load_local_state(app)
        .map(|s| s.ssh_ledgers)
        .unwrap_or_default();
    for (app_id, ledger) in &initial_ledgers {
        if ledger.mcp_servers.contains(&removed.slug) {
            if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
                let _ = crate::ssh_sync::uninstall_mcp_ssh(app, custom, &removed.slug);
            }
        }
    }
    if let Ok(mut state) = store::load_local_state(app) {
        let mut ledger_changed = false;
        for ledger in state.ssh_ledgers.values_mut() {
            let before = ledger.mcp_servers.len();
            ledger.mcp_servers.retain(|s| s != &removed.slug);
            if ledger.mcp_servers.len() != before {
                ledger_changed = true;
            }
        }
        if ledger_changed {
            let _ = store::save_local_state(app, &state);
        }
    }

    let _ = secrets::delete_namespace_secrets(app, &secrets::mcp_namespace(id));
    store::save_repo_library(&repo_root, &library)?;
    let _ = store::sync_after_mutation(app, &format!("Delete MCP server: {}", removed.name));
    Ok(())
}

pub fn install_mcp_global(
    app: &tauri::AppHandle,
    input: &InstallMcpGlobalInput,
) -> Result<InstallMcpResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let record = library
        .mcp_servers
        .iter()
        .find(|s| s.id == input.server_id)
        .ok_or_else(|| format!("未找到 MCP Server: {}", input.server_id))?;

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::install_mcp_ssh(
                app,
                custom,
                record,
                &library,
                input.use_env_placeholders,
            ) {
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

        match resolve_global_mcp_target(&home, app_id, &custom_apps) {
            Ok(Some(target)) => {
                if let Some(ref hint) = target.warning_hint {
                    warnings.push(hint.clone());
                }
                match install_record_to_target(
                    app,
                    record,
                    app_id,
                    &target,
                    input.use_env_placeholders,
                    &mut warnings,
                ) {
                    Ok(()) => installed_apps.push(app_id.clone()),
                    Err(err) => {
                        warnings.push(format!("{}: {}", app_id, err));
                        failed_apps.push(app_id.clone());
                    }
                }
            }
            Ok(None) => {
                warnings.push(unsupported_mcp_reason(app_id, "global"));
                failed_apps.push(app_id.clone());
            }
            Err(err) => {
                warnings.push(format!("{}: {}", app_id, err));
                failed_apps.push(app_id.clone());
            }
        }
    }

    Ok(InstallMcpResult {
        installed_apps,
        failed_apps,
        warnings,
    })
}

pub fn uninstall_mcp_global(
    app: &tauri::AppHandle,
    input: &InstallMcpGlobalInput,
) -> Result<InstallMcpResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let record = library
        .mcp_servers
        .iter()
        .find(|s| s.id == input.server_id)
        .ok_or_else(|| format!("未找到 MCP Server: {}", input.server_id))?;

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(app_id, &custom_apps) {
            match crate::ssh_sync::uninstall_mcp_ssh(app, custom, &record.slug) {
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

        match resolve_global_mcp_target(&home, app_id, &custom_apps) {
            Ok(Some(target)) => match remove_from_target(&target, &record.slug) {
                Ok(_) => installed_apps.push(app_id.clone()),
                Err(err) => {
                    warnings.push(format!("{}: {}", app_id, err));
                    failed_apps.push(app_id.clone());
                }
            },
            Ok(None) => {
                installed_apps.push(app_id.clone());
            }
            Err(err) => {
                warnings.push(format!("{}: {}", app_id, err));
                failed_apps.push(app_id.clone());
            }
        }
    }

    Ok(InstallMcpResult {
        installed_apps,
        failed_apps,
        warnings,
    })
}

pub fn install_mcp_to_project(
    app: &tauri::AppHandle,
    input: &InstallMcpToProjectInput,
) -> Result<InstallMcpResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let project_root = store::normalize_user_path(&input.project_path)?;
    if !project_root.exists() {
        return Err(format!("项目路径不存在: {}", input.project_path));
    }

    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let record = library
        .mcp_servers
        .iter()
        .find(|s| s.id == input.server_id)
        .ok_or_else(|| format!("未找到 MCP Server: {}", input.server_id))?;

    let claude_mode = input.claude_mode.unwrap_or_default();
    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if crate::ssh_sync::is_ssh_app(app_id, &custom_apps) {
            warnings.push("SSH 远程目标端仅支持全局 MCP 配置同步".to_string());
            failed_apps.push(app_id.clone());
            continue;
        }
        match resolve_project_mcp_target(&home, &project_root, app_id, claude_mode, &custom_apps) {
            Ok(Some(target)) => {
                if let Some(ref hint) = target.warning_hint {
                    warnings.push(hint.clone());
                }
                let force_placeholders = target.force_secret_placeholders;
                match install_record_to_target(
                    app,
                    record,
                    app_id,
                    &target,
                    force_placeholders,
                    &mut warnings,
                ) {
                    Ok(()) => installed_apps.push(app_id.clone()),
                    Err(err) => {
                        warnings.push(format!("{}: {}", app_id, err));
                        failed_apps.push(app_id.clone());
                    }
                }
            }
            Ok(None) => {
                warnings.push(unsupported_mcp_reason(app_id, "project"));
                failed_apps.push(app_id.clone());
            }
            Err(err) => {
                warnings.push(format!("{}: {}", app_id, err));
                failed_apps.push(app_id.clone());
            }
        }
    }

    Ok(InstallMcpResult {
        installed_apps,
        failed_apps,
        warnings,
    })
}

pub fn uninstall_mcp_from_project(
    app: &tauri::AppHandle,
    input: &InstallMcpToProjectInput,
) -> Result<InstallMcpResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let project_root = store::normalize_user_path(&input.project_path)?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let record = library
        .mcp_servers
        .iter()
        .find(|s| s.id == input.server_id)
        .ok_or_else(|| format!("未找到 MCP Server: {}", input.server_id))?;

    let mut installed_apps = Vec::new();
    let mut failed_apps = Vec::new();
    let mut warnings = Vec::new();

    for app_id in &input.apps {
        if crate::ssh_sync::is_ssh_app(app_id, &custom_apps) {
            installed_apps.push(app_id.clone());
            continue;
        }
        if app_id == "claude" {
            // Remove from BOTH `local` (~/.claude.json#projects) AND `project` (<project>/.mcp.json) so user isn't left with stale entries!
            let mut ok = true;
            for mode in [ClaudeProjectMcpMode::Local, ClaudeProjectMcpMode::Project] {
                if let Ok(Some(target)) =
                    resolve_project_mcp_target(&home, &project_root, "claude", mode, &custom_apps)
                {
                    if let Err(err) = remove_from_target(&target, &record.slug) {
                        warnings.push(format!("claude ({:?}): {}", mode, err));
                        ok = false;
                    }
                }
            }
            if ok {
                installed_apps.push(app_id.clone());
            } else {
                failed_apps.push(app_id.clone());
            }
            continue;
        }

        match resolve_project_mcp_target(
            &home,
            &project_root,
            app_id,
            ClaudeProjectMcpMode::Local,
            &custom_apps,
        ) {
            Ok(Some(target)) => match remove_from_target(&target, &record.slug) {
                Ok(_) => installed_apps.push(app_id.clone()),
                Err(err) => {
                    warnings.push(format!("{}: {}", app_id, err));
                    failed_apps.push(app_id.clone());
                }
            },
            Ok(None) => installed_apps.push(app_id.clone()),
            Err(err) => {
                warnings.push(format!("{}: {}", app_id, err));
                failed_apps.push(app_id.clone());
            }
        }
    }

    Ok(InstallMcpResult {
        installed_apps,
        failed_apps,
        warnings,
    })
}

pub fn check_mcp_status(
    app: &tauri::AppHandle,
    input: &CheckMcpStatusInput,
) -> Result<CheckMcpStatusResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let record = library
        .mcp_servers
        .iter()
        .find(|s| s.id == input.server_id)
        .ok_or_else(|| format!("未找到 MCP Server: {}", input.server_id))?;

    let app_ids = store::all_configured_app_ids(&custom_apps);
    let mut statuses = Vec::new();

    if input.scope == "project" {
        let project_raw = input
            .project_path
            .as_deref()
            .ok_or_else(|| "project scope requires projectPath".to_string())?;
        let project_root = store::normalize_user_path(project_raw)?;

        for app_id in app_ids {
            if crate::ssh_sync::find_ssh_app(&app_id, &custom_apps).is_some() {
                statuses.push(McpAppInstallStatus {
                    app_id,
                    supported: false,
                    installed: false,
                    config_path: None,
                    scope_detail: None,
                    has_drift: false,
                    unsupported_reason: Some("SSH 远程目标端仅支持全局 MCP 配置同步".to_string()),
                });
                continue;
            }
            if app_id == "claude" {
                let local_target = resolve_project_mcp_target(
                    &home,
                    &project_root,
                    "claude",
                    ClaudeProjectMcpMode::Local,
                    &custom_apps,
                )?
                .unwrap();
                let proj_target = resolve_project_mcp_target(
                    &home,
                    &project_root,
                    "claude",
                    ClaudeProjectMcpMode::Project,
                    &custom_apps,
                )?
                .unwrap();
                let in_local = target_has_mcp_entry(&local_target, &record.slug);
                let in_proj = target_has_mcp_entry(&proj_target, &record.slug);
                let detail = match (in_local, in_proj) {
                    (true, true) => Some("local + .mcp.json".to_string()),
                    (true, false) => Some("local (~/.claude.json)".to_string()),
                    (false, true) => Some("project (.mcp.json)".to_string()),
                    (false, false) => Some("未安装".to_string()),
                };
                statuses.push(McpAppInstallStatus {
                    app_id,
                    supported: true,
                    installed: in_local || in_proj,
                    config_path: Some(
                        if in_proj {
                            &proj_target.config_path
                        } else {
                            &local_target.config_path
                        }
                        .to_string_lossy()
                        .into_owned(),
                    ),
                    scope_detail: detail,
                    has_drift: false,
                    unsupported_reason: None,
                });
                continue;
            }

            match resolve_project_mcp_target(
                &home,
                &project_root,
                &app_id,
                ClaudeProjectMcpMode::Local,
                &custom_apps,
            )? {
                Some(target) => {
                    let installed = target_has_mcp_entry(&target, &record.slug);
                    statuses.push(McpAppInstallStatus {
                        app_id,
                        supported: true,
                        installed,
                        config_path: Some(target.config_path.to_string_lossy().into_owned()),
                        scope_detail: Some(target.scope_detail),
                        has_drift: false,
                        unsupported_reason: None,
                    });
                }
                None => {
                    statuses.push(McpAppInstallStatus {
                        unsupported_reason: Some(unsupported_mcp_reason(&app_id, "project")),
                        app_id,
                        supported: false,
                        installed: false,
                        config_path: None,
                        scope_detail: None,
                        has_drift: false,
                    });
                }
            }
        }
    } else {
        let ssh_ledgers = store::load_local_state(app)
            .map(|s| s.ssh_ledgers)
            .unwrap_or_default();

        for app_id in app_ids {
            if let Some(custom) = crate::ssh_sync::find_ssh_app(&app_id, &custom_apps) {
                let raw_path = custom
                    .global_mcp_config_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if let Some(path) = raw_path {
                    let host = custom
                        .ssh
                        .as_ref()
                        .map(|s| s.host.as_str())
                        .unwrap_or("ssh");
                    let installed = ssh_ledgers
                        .get(&app_id)
                        .map(|ledger| ledger.mcp_servers.contains(&record.slug))
                        .unwrap_or(false);
                    statuses.push(McpAppInstallStatus {
                        app_id,
                        supported: true,
                        installed,
                        config_path: Some(format!("{}:{}", host, path)),
                        scope_detail: Some(format!("SSH ({}:{})", host, path)),
                        has_drift: false,
                        unsupported_reason: None,
                    });
                } else {
                    statuses.push(McpAppInstallStatus {
                        unsupported_reason: Some(unsupported_mcp_reason(&app_id, "global")),
                        app_id,
                        supported: false,
                        installed: false,
                        config_path: None,
                        scope_detail: None,
                        has_drift: false,
                    });
                }
                continue;
            }

            match resolve_global_mcp_target(&home, &app_id, &custom_apps)? {
                Some(target) => {
                    let installed = target_has_mcp_entry(&target, &record.slug);
                    statuses.push(McpAppInstallStatus {
                        app_id,
                        supported: true,
                        installed,
                        config_path: Some(target.config_path.to_string_lossy().into_owned()),
                        scope_detail: Some(target.scope_detail),
                        has_drift: false,
                        unsupported_reason: None,
                    });
                }
                None => {
                    statuses.push(McpAppInstallStatus {
                        unsupported_reason: Some(unsupported_mcp_reason(&app_id, "global")),
                        app_id,
                        supported: false,
                        installed: false,
                        config_path: None,
                        scope_detail: None,
                        has_drift: false,
                    });
                }
            }
        }
    }

    Ok(CheckMcpStatusResult {
        server_id: record.id.clone(),
        statuses,
    })
}

pub fn list_all_mcp_global_enabled_apps(
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

    let mut targets_by_app = Vec::new();
    let mut ssh_supported_app_ids = Vec::new();
    for app_id in app_ids {
        if let Some(custom) = crate::ssh_sync::find_ssh_app(&app_id, &custom_apps) {
            if custom
                .global_mcp_config_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_some()
            {
                ssh_supported_app_ids.push(app_id);
            }
            continue;
        }
        if let Ok(Some(target)) = resolve_global_mcp_target(&home, &app_id, &custom_apps) {
            targets_by_app.push((app_id, target));
        }
    }

    let mut result = HashMap::new();
    for rec in &library.mcp_servers {
        let mut enabled = Vec::new();
        for (app_id, target) in &targets_by_app {
            if target_has_mcp_entry(target, &rec.slug) {
                enabled.push(app_id.clone());
            }
        }
        for ssh_app_id in &ssh_supported_app_ids {
            if ssh_ledgers
                .get(ssh_app_id)
                .map(|ledger| ledger.mcp_servers.contains(&rec.slug))
                .unwrap_or(false)
            {
                enabled.push(ssh_app_id.clone());
            }
        }
        result.insert(rec.id.clone(), enabled);
    }
    Ok(result)
}

pub fn scan_external_mcp_servers(
    app: &tauri::AppHandle,
) -> Result<Vec<ExternalMcpServerDto>, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let custom_apps = store::load_custom_apps(app);
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let managed_slugs: HashSet<String> =
        library.mcp_servers.iter().map(|s| s.slug.clone()).collect();

    let mut discovered = Vec::new();
    for app_id in store::all_configured_app_ids(&custom_apps) {
        if crate::ssh_sync::is_ssh_app(&app_id, &custom_apps) {
            continue;
        }
        let Ok(Some(target)) = resolve_global_mcp_target(&home, &app_id, &custom_apps) else {
            continue;
        };
        let Ok(raw) = fs::read_to_string(&target.config_path) else {
            continue;
        };
        let parsed_entries = parse_external_entries_from_raw(&raw, target.format, &target.root_key);
        for (key, parsed) in parsed_entries {
            let slug = store::slugify(&key);
            let is_managed = managed_slugs.contains(&slug) || managed_slugs.contains(&key);
            discovered.push(ExternalMcpServerDto {
                slug: key.clone(),
                name: key,
                app_id: app_id.clone(),
                config_path: target.config_path.to_string_lossy().into_owned(),
                transport: parsed.transport,
                command: parsed.command,
                args: parsed.args,
                url: parsed.url,
                detected_env_keys: parsed.env.keys().cloned().collect(),
                detected_secret_keys: parsed
                    .env
                    .iter()
                    .filter(|(k, v)| looks_like_secret_env(k, v))
                    .map(|(k, _)| k.clone())
                    .collect(),
                is_managed,
            });
        }
    }

    discovered.sort_by_key(|a| a.name.to_lowercase());
    Ok(discovered)
}

pub fn import_external_mcp_server(
    app: &tauri::AppHandle,
    input: &ImportExternalMcpInput,
) -> Result<CreateMcpServerResult, String> {
    let config_path = PathBuf::from(&input.config_path);
    if !config_path.exists() {
        return Err(format!("外部配置文件不存在: {}", input.config_path));
    }
    let custom_apps = store::load_custom_apps(app);
    if crate::ssh_sync::is_ssh_app(&input.app_id, &custom_apps) {
        return Err("不支持直接将 SSH 远端路径作为本地配置文件导入".to_string());
    }
    let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
    let target = resolve_global_mcp_target(&home, &input.app_id, &custom_apps)?
        .ok_or_else(|| format!("应用 {} 不支持全局 MCP 配置", input.app_id))?;

    let raw = fs::read_to_string(&config_path).map_err(|e| format!("读取配置文件失败: {}", e))?;
    let mut entries = parse_external_entries_from_raw(&raw, target.format, &target.root_key);
    let parsed = entries
        .remove(&input.slug)
        .ok_or_else(|| format!("在配置文件中未找到 MCP 节点: {}", input.slug))?;

    let repo_root = store::connected_repo_root(app)?;
    let mut library = store::load_repo_library(&repo_root)?;
    let base_slug = store::slugify(&input.slug);
    let mut final_slug = base_slug.clone();
    let mut suffix = 2;
    while library.mcp_servers.iter().any(|s| s.slug == final_slug) {
        final_slug = format!("{}-{}", base_slug, suffix);
        suffix += 1;
    }

    let import_secrets = input.import_secrets_to_local.unwrap_or(true);
    let mut env_specs = Vec::new();
    let mut secret_map = HashMap::new();

    let mut sorted_env: Vec<(String, String)> = parsed.env.into_iter().collect();
    sorted_env.sort_by(|a, b| a.0.cmp(&b.0));

    for (k, v) in sorted_env {
        let is_secret = looks_like_secret_env(&k, &v);
        let is_placeholder = v.trim().starts_with("${") && v.trim().ends_with('}');
        if is_secret {
            if import_secrets && !is_placeholder && !v.trim().is_empty() {
                secret_map.insert(k.clone(), v);
            }
            env_specs.push(McpEnvVarSpec {
                name: k,
                description: None,
                required: true,
                is_secret: true,
                default_value: None,
            });
        } else {
            env_specs.push(McpEnvVarSpec {
                name: k,
                description: None,
                required: false,
                is_secret: false,
                default_value: Some(v),
            });
        }
    }

    let mut header_specs = Vec::new();
    for (hk, hv) in parsed.headers {
        let is_secret = looks_like_secret_env(&hk, &hv);
        let is_placeholder = hv.contains("${");
        if is_secret {
            if import_secrets && !is_placeholder && !hv.trim().is_empty() {
                secret_map.insert(hk.clone(), hv.clone());
            }
            header_specs.push(McpHeaderSpec {
                name: hk,
                description: None,
                is_secret: true,
                value_template: if is_placeholder { Some(hv) } else { None },
            });
        } else {
            header_specs.push(McpHeaderSpec {
                name: hk,
                description: None,
                is_secret: false,
                value_template: Some(hv),
            });
        }
    }

    let now = store::now_ms();
    let id = Uuid::new_v4().to_string();
    let record = McpServerRecord {
        id: id.clone(),
        slug: final_slug,
        name: input.slug.clone(),
        description: Some(format!("从 {} 导入的 MCP 服务", input.app_id)),
        transport: parsed.transport,
        command: parsed.command,
        args: parsed.args,
        url: parsed.url,
        env_specs: sanitize_env_specs(&env_specs),
        header_specs: sanitize_header_specs(&header_specs),
        tags: vec!["imported".to_string()],
        project_ids: Vec::new(),
        plugin_id: None,
        created_at: now,
        updated_at: now,
        provenance: Provenance {
            kind: ProvenanceKind::ExternalApp,
            label: format!("外部导入 ({})", input.app_id),
            app_id: Some(input.app_id.clone()),
            source_path: Some(input.config_path.clone()),
            ..Default::default()
        },
    };

    if !secret_map.is_empty() {
        secrets::update_namespace_secrets(app, &secrets::mcp_namespace(&id), &secret_map)?;
    }

    library.mcp_servers.push(record.clone());
    store::save_repo_library(&repo_root, &library)?;
    let backup_sync =
        store::sync_after_mutation(app, &format!("Import external MCP server: {}", record.name));

    Ok(CreateMcpServerResult {
        server: record_to_dto(app, &record),
        backup_sync,
    })
}

pub(crate) fn install_record_to_target(
    app: &tauri::AppHandle,
    record: &McpServerRecord,
    app_id: &str,
    target: &AppMcpTarget,
    use_placeholders: bool,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let mut merged_secrets =
        secrets::load_namespace_secrets(app, &secrets::mcp_namespace(&record.id))?;
    if let Some(ref pid) = record.plugin_id {
        if let Ok(plugin_secrets) =
            secrets::load_namespace_secrets(app, &secrets::plugin_namespace(pid))
        {
            for (k, v) in plugin_secrets {
                merged_secrets.entry(k).or_insert(v);
            }
        }
    }

    let force_placeholders = use_placeholders || target.force_secret_placeholders;
    match target.format {
        ConfigFormat::Json => {
            let entry = build_json_entry_for_app(
                record,
                &merged_secrets,
                app_id,
                force_placeholders,
                warnings,
            )?;
            upsert_json_mcp_entry(target, &record.slug, entry)
        }
        ConfigFormat::Toml => upsert_toml_mcp_entry(
            target,
            &record.slug,
            record,
            &merged_secrets,
            force_placeholders,
            warnings,
        ),
    }
}

pub(crate) fn remove_from_target(target: &AppMcpTarget, slug: &str) -> Result<bool, String> {
    match target.format {
        ConfigFormat::Json => remove_json_mcp_entry(target, slug),
        ConfigFormat::Toml => remove_toml_mcp_entry(target, slug),
    }
}

pub(crate) fn upsert_library_mcp_record(library: &mut RepoLibrary, record: McpServerRecord) {
    if let Some(existing) = library.mcp_servers.iter_mut().find(|r| r.id == record.id) {
        *existing = record;
    } else {
        library.mcp_servers.push(record);
    }
}

fn validate_transport_fields(
    transport: McpTransport,
    command: Option<&str>,
    url: Option<&str>,
    name: &str,
) -> Result<(), String> {
    match transport {
        McpTransport::Stdio => {
            if command.map(str::trim).filter(|s| !s.is_empty()).is_none() {
                return Err(format!(
                    "stdio 类型的 MCP「{}」必须填写启动命令 (command)",
                    name
                ));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            let u = url
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("远程类型的 MCP「{}」必须填写服务地址 (url)", name))?;
            if !u.starts_with("http://") && !u.starts_with("https://") && !u.starts_with("${") {
                return Err(format!(
                    "MCP「{}」的 URL 需以 http:// 或 https:// 开头",
                    name
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
pub(crate) struct ParsedRawMcpEntry {
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: HashMap<String, String>,
    pub headers: HashMap<String, String>,
}

pub(crate) fn parse_external_entries_from_raw(
    raw: &str,
    format: ConfigFormat,
    root_key: &str,
) -> HashMap<String, ParsedRawMcpEntry> {
    let mut map = HashMap::new();
    match format {
        ConfigFormat::Json => {
            let Ok(root) = serde_json::from_str::<JsonValue>(raw) else {
                return map;
            };
            let servers_val = root.get(root_key).or_else(|| {
                // Support standalone `{ "my-server": { "command": ... } }` inside plugin `.mcp.json`
                if root.get("mcpServers").is_none() && root.is_object() {
                    Some(&root)
                } else {
                    None
                }
            });
            let Some(servers_obj) = servers_val.and_then(|v| v.as_object()) else {
                return map;
            };

            for (key, val) in servers_obj {
                let Some(obj) = val.as_object() else {
                    continue;
                };
                if let Some(parsed) = parse_json_mcp_object(obj) {
                    map.insert(key.clone(), parsed);
                }
            }
        }
        ConfigFormat::Toml => {
            let Ok(doc) = raw.parse::<DocumentMut>() else {
                return map;
            };
            let Some(table) = doc.get(root_key).and_then(|v| v.as_table()) else {
                return map;
            };
            for (key, item) in table.iter() {
                let Some(obj) = item.as_table() else {
                    continue;
                };
                let cmd = obj
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let url = obj
                    .get("url")
                    .or_else(|| obj.get("http_url"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                if cmd.is_none() && url.is_none() {
                    continue;
                }
                let transport = if url.is_some() {
                    McpTransport::Http
                } else {
                    McpTransport::Stdio
                };
                let args = obj
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut env = HashMap::new();
                if let Some(env_tbl) = obj.get("env").and_then(|v| v.as_table_like()) {
                    for (ek, ev) in env_tbl.iter() {
                        if let Some(s) = ev.as_str() {
                            env.insert(ek.to_string(), s.to_string());
                        }
                    }
                }
                if let Some(env_vars) = obj.get("env_vars").and_then(|v| v.as_array()) {
                    for ev in env_vars.iter() {
                        if let Some(k) = ev.as_str() {
                            env.entry(k.to_string())
                                .or_insert_with(|| format!("${{{}}}", k));
                        }
                    }
                }
                let mut headers = HashMap::new();
                if let Some(bearer_env) = obj.get("bearer_token_env_var").and_then(|v| v.as_str()) {
                    headers.insert(
                        "Authorization".to_string(),
                        format!("Bearer ${{{}}}", bearer_env),
                    );
                }
                map.insert(
                    key.to_string(),
                    ParsedRawMcpEntry {
                        transport,
                        command: cmd,
                        args,
                        url,
                        env,
                        headers,
                    },
                );
            }
        }
    }
    map
}

pub(crate) fn parse_json_mcp_object(obj: &JsonMap<String, JsonValue>) -> Option<ParsedRawMcpEntry> {
    let command = obj
        .get("command")
        .and_then(|v| v.as_str())
        .map(String::from);
    let url = obj
        .get("url")
        .or_else(|| obj.get("serverUrl"))
        .or_else(|| obj.get("httpUrl"))
        .and_then(|v| v.as_str())
        .map(String::from);

    if command.is_none() && url.is_none() {
        return None;
    }

    let explicit_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let transport = if explicit_type.eq_ignore_ascii_case("sse") {
        McpTransport::Sse
    } else if url.is_some() || explicit_type.eq_ignore_ascii_case("http") {
        McpTransport::Http
    } else {
        McpTransport::Stdio
    };

    let args = obj
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut env = HashMap::new();
    if let Some(env_obj) = obj.get("env").and_then(|v| v.as_object()) {
        for (k, v) in env_obj {
            if let Some(s) = v.as_str() {
                env.insert(k.clone(), s.to_string());
            } else {
                env.insert(k.clone(), v.to_string());
            }
        }
    }

    let mut headers = HashMap::new();
    if let Some(hdr_obj) = obj.get("headers").and_then(|v| v.as_object()) {
        for (k, v) in hdr_obj {
            if let Some(s) = v.as_str() {
                headers.insert(k.clone(), s.to_string());
            }
        }
    }

    Some(ParsedRawMcpEntry {
        transport,
        command,
        args,
        url,
        env,
        headers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "skill-switch-mcp-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_stdio_record() -> McpServerRecord {
        McpServerRecord {
            id: "mcp-1".to_string(),
            slug: "github".to_string(),
            name: "GitHub MCP".to_string(),
            description: Some("GitHub integration".to_string()),
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string(),
            ],
            url: None,
            env_specs: sanitize_env_specs(&[
                McpEnvVarSpec {
                    name: "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                    description: None,
                    required: true,
                    is_secret: true,
                    default_value: Some("SHOULD_BE_STRIPPED".to_string()),
                },
                McpEnvVarSpec {
                    name: "LOG_LEVEL".to_string(),
                    description: None,
                    required: false,
                    is_secret: false,
                    default_value: Some("info".to_string()),
                },
            ]),
            header_specs: Vec::new(),
            tags: Vec::new(),
            project_ids: Vec::new(),
            plugin_id: None,
            created_at: 1,
            updated_at: 1,
            provenance: Default::default(),
        }
    }

    #[test]
    fn sanitize_strips_secret_default_values() {
        let rec = sample_stdio_record();
        let secret_spec = rec
            .env_specs
            .iter()
            .find(|s| s.name == "GITHUB_PERSONAL_ACCESS_TOKEN")
            .unwrap();
        assert_eq!(secret_spec.default_value, None);
    }

    #[test]
    fn json_surgical_merge_preserves_siblings_and_claude_local_vs_project_safety() {
        let dir = temp_dir("json-merge");
        let claude_json = dir.join(".claude.json");
        fs::write(
            &claude_json,
            r#"{
  "oauthAccount": "keep-me",
  "mcpServers": {
    "existing": { "command": "uvx", "args": ["mcp-fetch"] }
  }
}"#,
        )
        .unwrap();

        let rec = sample_stdio_record();
        let mut local_secrets = HashMap::new();
        local_secrets.insert(
            "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
            "ghp_super_secret".to_string(),
        );

        // 1. Claude `local` project scope -> injects secret inside `~/.claude.json#projects[path].mcpServers`
        let local_target = AppMcpTarget {
            config_path: claude_json.clone(),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: Some("/Users/demo/my-repo".to_string()),
            force_secret_placeholders: false,
            scope_detail: "local".to_string(),
            warning_hint: None,
        };
        let mut warnings = Vec::new();
        let entry =
            build_json_entry_for_app(&rec, &local_secrets, "claude", false, &mut warnings).unwrap();
        upsert_json_mcp_entry(&local_target, &rec.slug, entry).unwrap();

        let parsed: JsonValue =
            serde_json::from_str(&fs::read_to_string(&claude_json).unwrap()).unwrap();
        assert_eq!(parsed["oauthAccount"], "keep-me");
        assert_eq!(parsed["mcpServers"]["existing"]["command"], "uvx");
        assert_eq!(
            parsed["projects"]["/Users/demo/my-repo"]["mcpServers"]["github"]["env"]
                ["GITHUB_PERSONAL_ACCESS_TOKEN"],
            "ghp_super_secret"
        );
        assert!(dir.join(".claude.json.skill-switch.bak").exists());

        // 2. Shared `<project>/.mcp.json` -> MUST NEVER write raw secret, must emit `${GITHUB_PERSONAL_ACCESS_TOKEN}`
        let shared_mcp_json = dir.join(".mcp.json");
        let proj_target = AppMcpTarget {
            config_path: shared_mcp_json.clone(),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: true,
            scope_detail: "project".to_string(),
            warning_hint: None,
        };
        let proj_entry =
            build_json_entry_for_app(&rec, &local_secrets, "claude", true, &mut warnings).unwrap();
        upsert_json_mcp_entry(&proj_target, &rec.slug, proj_entry).unwrap();

        let shared_raw = fs::read_to_string(&shared_mcp_json).unwrap();
        assert!(
            !shared_raw.contains("ghp_super_secret"),
            "Shared .mcp.json must NEVER contain plaintext secret!"
        );
        assert!(shared_raw.contains("${GITHUB_PERSONAL_ACCESS_TOKEN}"));

        // 3. Malformed JSON refuses overwrite
        let broken_json = dir.join("broken.json");
        fs::write(&broken_json, "{ not valid json ").unwrap();
        let broken_target = AppMcpTarget {
            config_path: broken_json.clone(),
            format: ConfigFormat::Json,
            root_key: "mcpServers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "global".to_string(),
            warning_hint: None,
        };
        assert!(upsert_json_mcp_entry(&broken_target, "github", json!({})).is_err());
        assert_eq!(
            fs::read_to_string(&broken_json).unwrap(),
            "{ not valid json "
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn toml_surgical_merge_preserves_comments_and_other_tables() {
        let dir = temp_dir("toml-merge");
        let codex_toml = dir.join("config.toml");
        fs::write(
            &codex_toml,
            r#"# Important top comment preserved!
model = "gpt-5-codex"

[features]
# Inline comment preserved!
unified_exec = true

[mcp_servers.keep_me]
command = "uvx"
args = ["mcp-server-sqlite"]
"#,
        )
        .unwrap();

        let rec = sample_stdio_record();
        let mut local_secrets = HashMap::new();
        local_secrets.insert(
            "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
            "ghp_secret_toml".to_string(),
        );
        let target = AppMcpTarget {
            config_path: codex_toml.clone(),
            format: ConfigFormat::Toml,
            root_key: "mcp_servers".to_string(),
            claude_local_project_key: None,
            force_secret_placeholders: false,
            scope_detail: "global".to_string(),
            warning_hint: None,
        };

        let mut warnings = Vec::new();
        upsert_toml_mcp_entry(
            &target,
            &rec.slug,
            &rec,
            &local_secrets,
            false,
            &mut warnings,
        )
        .unwrap();

        let after_insert = fs::read_to_string(&codex_toml).unwrap();
        assert!(after_insert.contains("# Important top comment preserved!"));
        assert!(after_insert.contains("# Inline comment preserved!"));
        assert!(after_insert.contains("model = \"gpt-5-codex\""));
        assert!(after_insert.contains("[mcp_servers.keep_me]"));
        assert!(after_insert.contains("[mcp_servers.github]"));
        assert!(target_has_mcp_entry(&target, "github"));

        remove_toml_mcp_entry(&target, "github").unwrap();
        let after_remove = fs::read_to_string(&codex_toml).unwrap();
        assert!(after_remove.contains("# Important top comment preserved!"));
        assert!(after_remove.contains("[mcp_servers.keep_me]"));
        assert!(!target_has_mcp_entry(&target, "github"));

        let _ = fs::remove_dir_all(dir);
    }
}
