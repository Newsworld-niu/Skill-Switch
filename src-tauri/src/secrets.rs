use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const LOCAL_SECRETS_FILE: &str = "local-secrets.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretVault {
    version: String,
    #[serde(default)]
    namespaces: HashMap<String, HashMap<String, String>>,
}

/// Returns `{app_data_dir}/local-secrets.json`, which is strictly machine-local
/// and outside `backup-source/` so `git add -A` never commits secrets.
pub fn local_secrets_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(app_data.join(LOCAL_SECRETS_FILE))
}

pub fn mcp_namespace(server_id: &str) -> String {
    format!("mcp:{}", server_id)
}

pub fn plugin_namespace(plugin_id: &str) -> String {
    format!("plugin:{}", plugin_id)
}

pub fn load_namespace_secrets_at(
    secrets_path: &Path,
    namespace: &str,
) -> Result<HashMap<String, String>, String> {
    let vault = load_vault(secrets_path)?;
    Ok(vault.namespaces.get(namespace).cloned().unwrap_or_default())
}

pub fn configured_secret_keys_at(
    secrets_path: &Path,
    namespace: &str,
) -> Result<Vec<String>, String> {
    let map = load_namespace_secrets_at(secrets_path, namespace)?;
    let mut keys: Vec<String> = map
        .into_iter()
        .filter(|(_, val)| !val.trim().is_empty())
        .map(|(k, _)| k)
        .collect();
    keys.sort();
    Ok(keys)
}

pub fn update_namespace_secrets_at(
    secrets_path: &Path,
    namespace: &str,
    updates: &HashMap<String, String>,
) -> Result<(), String> {
    if updates.is_empty() {
        return Ok(());
    }

    let mut vault = load_vault(secrets_path)?;
    let entry = vault.namespaces.entry(namespace.to_string()).or_default();

    for (raw_key, value) in updates {
        let key = raw_key.trim();
        if key.is_empty() {
            continue;
        }
        if value.is_empty() {
            entry.remove(key);
        } else {
            entry.insert(key.to_string(), value.clone());
        }
    }

    if entry.is_empty() {
        vault.namespaces.remove(namespace);
    }

    save_vault_atomic(secrets_path, &vault)
}

pub fn delete_namespace_secrets_at(secrets_path: &Path, namespace: &str) -> Result<(), String> {
    if !secrets_path.exists() {
        return Ok(());
    }
    let mut vault = load_vault(secrets_path)?;
    if vault.namespaces.remove(namespace).is_some() {
        save_vault_atomic(secrets_path, &vault)?;
    }
    Ok(())
}

pub fn load_namespace_secrets(
    app: &tauri::AppHandle,
    namespace: &str,
) -> Result<HashMap<String, String>, String> {
    let path = local_secrets_path(app)?;
    load_namespace_secrets_at(&path, namespace)
}

pub fn configured_secret_keys(app: &tauri::AppHandle, namespace: &str) -> Vec<String> {
    local_secrets_path(app)
        .and_then(|path| configured_secret_keys_at(&path, namespace))
        .unwrap_or_default()
}

pub fn update_namespace_secrets(
    app: &tauri::AppHandle,
    namespace: &str,
    updates: &HashMap<String, String>,
) -> Result<(), String> {
    let path = local_secrets_path(app)?;
    update_namespace_secrets_at(&path, namespace, updates)
}

pub fn delete_namespace_secrets(app: &tauri::AppHandle, namespace: &str) -> Result<(), String> {
    let path = local_secrets_path(app)?;
    delete_namespace_secrets_at(&path, namespace)
}

fn load_vault(secrets_path: &Path) -> Result<SecretVault, String> {
    if !secrets_path.exists() {
        return Ok(SecretVault {
            version: "1".to_string(),
            namespaces: HashMap::new(),
        });
    }
    let raw =
        fs::read_to_string(secrets_path).map_err(|e| format!("读取本机密钥文件失败: {}", e))?;
    if raw.trim().is_empty() {
        return Ok(SecretVault {
            version: "1".to_string(),
            namespaces: HashMap::new(),
        });
    }
    serde_json::from_str(&raw).map_err(|e| format!("解析本机密钥文件失败: {}", e))
}

fn save_vault_atomic(secrets_path: &Path, vault: &SecretVault) -> Result<(), String> {
    if let Some(parent) = secrets_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建密钥存储目录失败: {}", e))?;
    }

    let tmp_path = secrets_path.with_extension("json.tmp");
    let mut payload = vault.clone();
    if payload.version.is_empty() {
        payload.version = "1".to_string();
    }
    let serialized =
        serde_json::to_string_pretty(&payload).map_err(|e| format!("序列化本机密钥失败: {}", e))?;

    fs::write(&tmp_path, serialized).map_err(|e| format!("写入临时密钥文件失败: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&tmp_path, perms)
            .map_err(|e| format!("设置密钥文件 0600 权限失败: {}", e))?;
    }

    fs::rename(&tmp_path, secrets_path).map_err(|e| format!("原子替换本机密钥文件失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_secrets_file(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid time")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "skill-switch-secrets-{}-{}-{}",
                label,
                std::process::id(),
                nanos
            ))
            .join("local-secrets.json")
    }

    #[test]
    fn stores_updates_and_deletes_secrets_with_restricted_permissions() {
        let path = temp_secrets_file("crud");
        let ns = mcp_namespace("srv-1");

        let mut initial = HashMap::new();
        initial.insert("GITHUB_TOKEN".to_string(), "ghp_secret_123".to_string());
        initial.insert("API_KEY".to_string(), "sk-test-456".to_string());
        update_namespace_secrets_at(&path, &ns, &initial).expect("write should succeed");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "local-secrets.json must be mode 0600");
        }

        let keys = configured_secret_keys_at(&path, &ns).expect("keys");
        assert_eq!(
            keys,
            vec!["API_KEY".to_string(), "GITHUB_TOKEN".to_string()]
        );

        // Empty value deletes single key while preserving others
        let mut patch = HashMap::new();
        patch.insert("API_KEY".to_string(), String::new());
        update_namespace_secrets_at(&path, &ns, &patch).expect("patch should succeed");

        let loaded = load_namespace_secrets_at(&path, &ns).expect("load");
        assert_eq!(
            loaded.get("GITHUB_TOKEN").map(String::as_str),
            Some("ghp_secret_123")
        );
        assert!(!loaded.contains_key("API_KEY"));

        delete_namespace_secrets_at(&path, &ns).expect("delete ns");
        assert!(configured_secret_keys_at(&path, &ns)
            .expect("empty")
            .is_empty());

        if let Some(dir) = path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}
