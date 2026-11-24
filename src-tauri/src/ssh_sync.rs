use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use toml_edit::{Array as TomlArray, DocumentMut, InlineTable, Item, Table, Value as TomlValue};
use uuid::Uuid;

use crate::domain::{
    CustomAppConfig, DiscoveredSshHost, McpServerRecord, McpTransport, PluginInstallMode,
    PluginRecord, RepoLibrary, Resource, ResourceKind, SshAppSyncLedger, SshConnectionTestResult,
    SshSyncSummaryResult, SshTargetConfig,
};
use crate::{mcp, plugins, secrets, store};

// ─── Remote POSIX Shell Snippets ──────────────────────────────────────────────
//
// All dynamic paths are passed exclusively as positional parameters (`$1`, `$2`, ...)
// via `sh -c '<script>' sh '<escaped_arg>'`. User input is NEVER interpolated into
// the script text itself.

const REMOTE_UPLOAD_DIR_SCRIPT: &str = r#"
p="$1"
h="${HOME%/}"
case "$p" in
  "~"|"~/")
    echo "refusing unsafe remote target: $p" >&2
    exit 2
    ;;
  "~/"*)
    if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
    p="${h}${p#?}"
    ;;
  /*) ;;
  *)
    if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
    p="${h}/$p"
    ;;
esac
p_clean="${p%/}"
if [ -z "$p_clean" ] || [ "$p_clean" = "/" ] || [ "$p_clean" = "$h" ] || [ "$p_clean" = "." ] || [ "$p_clean" = ".." ]; then
  echo "refusing unsafe remote target: $p" >&2
  exit 2
fi
rm -rf "$p_clean" && mkdir -p "$p_clean" && tar -xzf - -C "$p_clean"
"#;

const REMOTE_REMOVE_PATHS_SCRIPT: &str = r#"
h="${HOME%/}"
for raw in "$@"; do
  p="$raw"
  case "$p" in
    ""|"~"|"~/"|"/"|"."|".."|'$HOME'|'${HOME}')
      echo "refusing unsafe remote rm target: $raw" >&2
      exit 2
      ;;
    "~/"*)
      if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
      p="${h}${p#?}"
      ;;
    /*) ;;
    *)
      if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
      p="${h}/$p"
      ;;
  esac
  p_clean="${p%/}"
  if [ -z "$p_clean" ] || [ "$p_clean" = "/" ] || [ "$p_clean" = "$h" ] || [ "$p_clean" = "." ] || [ "$p_clean" = ".." ]; then
    echo "refusing unsafe remote rm target: $raw" >&2
    exit 2
  fi
  rm -rf "$p_clean"
done
"#;

const REMOTE_READ_FILE_SCRIPT: &str = r#"
p="$1"
h="${HOME%/}"
case "$p" in
  "~"|"~/")
    echo "remote path is home directory, not a file: $p" >&2
    exit 2
    ;;
  "~/"*)
    if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
    p="${h}${p#?}"
    ;;
  /*) ;;
  *)
    if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
    p="${h}/$p"
    ;;
esac
if [ ! -e "$p" ]; then
  printf 'MISSING\n'
  exit 0
fi
if [ -d "$p" ]; then
  echo "remote path is a directory: $p" >&2
  exit 3
fi
printf 'EXISTS\n'
cat "$p"
"#;

const REMOTE_WRITE_FILE_ATOMIC_SCRIPT: &str = r#"
p="$1"
h="${HOME%/}"
case "$p" in
  ""|"~"|"~/"|"/"|"."|"..")
    echo "refusing unsafe remote file target: $p" >&2
    exit 2
    ;;
  "~/"*)
    if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
    p="${h}${p#?}"
    ;;
  /*) ;;
  *)
    if [ -z "$HOME" ]; then echo "remote HOME is unset" >&2; exit 2; fi
    p="${h}/$p"
    ;;
esac
p_clean="${p%/}"
if [ -z "$p_clean" ] || [ "$p_clean" = "/" ] || [ "$p_clean" = "$h" ]; then
  echo "refusing unsafe remote file target: $p" >&2
  exit 2
fi
dir=$(dirname "$p_clean")
mkdir -p "$dir" || exit 1
tmp="${p_clean}.skillswitch.$$"
if cat > "$tmp" && mv -f "$tmp" "$p_clean"; then
  exit 0
else
  rm -f "$tmp"
  exit 1
fi
"#;

const REMOTE_LIST_DIR_SCRIPT: &str = r#"
p="$1"
h="${HOME%/}"
case "$p" in
  "~"|"~/")
    if [ -z "$HOME" ]; then exit 0; fi
    p="$HOME"
    ;;
  "~/"*)
    if [ -z "$HOME" ]; then exit 0; fi
    p="${h}${p#?}"
    ;;
  /*) ;;
  *)
    if [ -z "$HOME" ]; then exit 0; fi
    p="${h}/$p"
    ;;
esac
if [ ! -d "$p" ]; then
  exit 0
fi
for item in "$p"/*; do
  [ -e "$item" ] || [ -L "$item" ] || continue
  base=$(basename "$item")
  if [ -d "$item" ]; then
    printf 'D\t%s\n' "$base"
  else
    printf 'F\t%s\n' "$base"
  fi
done
"#;

// ─── 1–3. SSH App Lookup & Local Ledger Accessors ─────────────────────────────

pub fn is_ssh_app(app_id: &str, custom_apps: &[CustomAppConfig]) -> bool {
    find_ssh_app(app_id, custom_apps).is_some()
}

pub fn find_ssh_app<'a>(
    app_id: &str,
    custom_apps: &'a [CustomAppConfig],
) -> Option<&'a CustomAppConfig> {
    custom_apps.iter().find(|c| {
        c.id == app_id
            && c.ssh
                .as_ref()
                .map(|s| !s.host.trim().is_empty())
                .unwrap_or(false)
    })
}

pub fn get_app_ledger(app: &tauri::AppHandle, app_id: &str) -> SshAppSyncLedger {
    store::load_local_state(app)
        .ok()
        .and_then(|state| state.ssh_ledgers.get(app_id).cloned())
        .unwrap_or_default()
}

fn mutate_app_ledger<F>(app: &tauri::AppHandle, app_id: &str, mutate: F) -> Result<(), String>
where
    F: FnOnce(&mut SshAppSyncLedger),
{
    let mut state = store::load_local_state(app)?;
    let ledger = state.ssh_ledgers.entry(app_id.to_string()).or_default();
    mutate(ledger);
    ledger.last_synced_at = Some(store::now_ms());
    store::save_local_state(app, &state)
}

fn push_unique(items: &mut Vec<String>, slug: &str) {
    if !items.iter().any(|existing| existing == slug) {
        items.push(slug.to_string());
    }
}

fn remove_slug(items: &mut Vec<String>, slug: &str) {
    items.retain(|existing| existing != slug);
}

fn require_ssh_config(custom: &CustomAppConfig) -> Result<&SshTargetConfig, String> {
    custom
        .ssh
        .as_ref()
        .filter(|s| !s.host.trim().is_empty())
        .ok_or_else(|| format!("目标端「{}」未配置有效的 SSH 主机地址", custom.label))
}

// ─── 4. `~/.ssh/config` Discovery with Recursive `Include` Glob Resolution ────

pub fn discover_ssh_hosts() -> Result<Vec<DiscoveredSshHost>, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取本机用户主目录".to_string())?;
    let ssh_dir = home.join(".ssh");
    let config_path = ssh_dir.join("config");
    if !config_path.is_file() {
        return Ok(Vec::new());
    }
    discover_ssh_hosts_from_path(&config_path, &ssh_dir, &home)
}

pub(crate) fn discover_ssh_hosts_from_path(
    config_path: &Path,
    ssh_base_dir: &Path,
    home_dir: &Path,
) -> Result<Vec<DiscoveredSshHost>, String> {
    let mut visited_files = HashSet::new();
    let mut seen_aliases = HashSet::new();
    let mut discovered = Vec::new();
    parse_ssh_config_recursive(
        config_path,
        ssh_base_dir,
        home_dir,
        &mut visited_files,
        &mut seen_aliases,
        &mut discovered,
    );
    Ok(discovered)
}

#[derive(Default)]
struct PendingHostBlock {
    aliases: Vec<String>,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    source_file: String,
}

enum SshParseSection {
    TopLevel,
    MatchIgnored,
    Host(PendingHostBlock),
}

fn parse_ssh_config_recursive(
    file_path: &Path,
    ssh_base_dir: &Path,
    home_dir: &Path,
    visited_files: &mut HashSet<PathBuf>,
    seen_aliases: &mut HashSet<String>,
    out: &mut Vec<DiscoveredSshHost>,
) {
    let visit_key = fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
    if !visited_files.insert(visit_key) {
        return;
    }

    let Ok(raw) = fs::read_to_string(file_path) else {
        return;
    };

    let source_file_str = file_path.to_string_lossy().into_owned();
    let mut section = SshParseSection::TopLevel;

    for line in raw.lines() {
        let tokens = tokenize_ssh_config_line(line);
        if tokens.is_empty() {
            continue;
        }

        let keyword = tokens[0].to_ascii_lowercase();
        let args = &tokens[1..];
        if args.is_empty() {
            continue;
        }

        match keyword.as_str() {
            "include" => {
                flush_section(&mut section, seen_aliases, out);
                for pattern_raw in args {
                    let resolved_pattern =
                        resolve_ssh_include_pattern_path(pattern_raw, ssh_base_dir, home_dir);
                    for included_file in expand_path_glob(&resolved_pattern) {
                        parse_ssh_config_recursive(
                            &included_file,
                            ssh_base_dir,
                            home_dir,
                            visited_files,
                            seen_aliases,
                            out,
                        );
                    }
                }
            }
            "host" => {
                flush_section(&mut section, seen_aliases, out);
                section = SshParseSection::Host(PendingHostBlock {
                    aliases: args.to_vec(),
                    source_file: source_file_str.clone(),
                    ..Default::default()
                });
            }
            "match" => {
                flush_section(&mut section, seen_aliases, out);
                section = SshParseSection::MatchIgnored;
            }
            "hostname" => {
                if let SshParseSection::Host(ref mut block) = section {
                    if block.hostname.is_none() {
                        block.hostname = Some(args[0].clone());
                    }
                }
            }
            "user" => {
                if let SshParseSection::Host(ref mut block) = section {
                    if block.user.is_none() {
                        block.user = Some(args[0].clone());
                    }
                }
            }
            "port" => {
                if let SshParseSection::Host(ref mut block) = section {
                    if block.port.is_none() {
                        if let Ok(parsed_port) = args[0].parse::<u16>() {
                            block.port = Some(parsed_port);
                        }
                    }
                }
            }
            "identityfile" => {
                if let SshParseSection::Host(ref mut block) = section {
                    if block.identity_file.is_none() {
                        block.identity_file = Some(args[0].clone());
                    }
                }
            }
            _ => {}
        }
    }

    flush_section(&mut section, seen_aliases, out);
}

fn flush_section(
    section: &mut SshParseSection,
    seen_aliases: &mut HashSet<String>,
    out: &mut Vec<DiscoveredSshHost>,
) {
    let prev = std::mem::replace(section, SshParseSection::TopLevel);
    let SshParseSection::Host(block) = prev else {
        return;
    };

    for raw_alias in block.aliases {
        let alias = raw_alias.trim();
        if alias.is_empty() || alias.starts_with('!') || alias.contains('*') || alias.contains('?')
        {
            continue;
        }
        if is_git_hosting_entry(alias, block.hostname.as_deref(), block.user.as_deref()) {
            continue;
        }
        if seen_aliases.insert(alias.to_string()) {
            out.push(DiscoveredSshHost {
                alias: alias.to_string(),
                hostname: block.hostname.clone(),
                user: block.user.clone(),
                port: block.port,
                identity_file: block.identity_file.clone(),
                source_file: block.source_file.clone(),
            });
        }
    }
}

fn is_git_hosting_entry(alias: &str, hostname: Option<&str>, user: Option<&str>) -> bool {
    if user.map(|u| u.eq_ignore_ascii_case("git")).unwrap_or(false) {
        return true;
    }
    is_git_hosting_domain(alias) || hostname.map(is_git_hosting_domain).unwrap_or(false)
}

fn is_git_hosting_domain(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "github.com"
            | "ssh.github.com"
            | "gist.github.com"
            | "gitlab.com"
            | "altssh.gitlab.com"
            | "bitbucket.org"
            | "altssh.bitbucket.org"
            | "ssh.dev.azure.com"
            | "vs-ssh.visualstudio.com"
            | "codeberg.org"
            | "gitee.com"
    ) || lower.ends_with(".github.com")
        || lower.ends_with(".gitlab.com")
        || lower.ends_with(".bitbucket.org")
}

/// Tokenizes a single OpenSSH config line (`Key Value`, `Key=Value`, `Key = "Quoted Value"`),
/// stripping comments starting with `#` outside double quotes.
fn tokenize_ssh_config_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    let mut first_token_done = false;

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '#' {
            break;
        }
        if first_token_done && ch == '=' && tokens.len() == 1 {
            chars.next();
            continue;
        }

        let mut token = String::new();
        let mut in_quotes = false;

        while let Some(&c) = chars.peek() {
            if c == '"' {
                in_quotes = !in_quotes;
                chars.next();
                continue;
            }
            if !in_quotes {
                if c.is_whitespace() {
                    break;
                }
                if !first_token_done && c == '=' {
                    break;
                }
            }
            token.push(c);
            chars.next();
        }

        if !token.is_empty() {
            tokens.push(token);
            first_token_done = true;
        } else if !first_token_done && chars.peek() == Some(&'=') {
            chars.next();
        }
    }

    tokens
}

fn resolve_ssh_include_pattern_path(raw: &str, ssh_base_dir: &Path, home_dir: &Path) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed == "~" {
        return home_dir.to_path_buf();
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        return home_dir.join(rest);
    }
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        ssh_base_dir.join(candidate)
    }
}

/// Expands `*` and `?` wildcards in path segments without external crates.
/// Accumulates literal prefix segments first so ancestor directories like `/` or `/Users`
/// are never scanned with `read_dir`.
fn expand_path_glob(pattern_path: &Path) -> Vec<PathBuf> {
    let raw_lossy = pattern_path.to_string_lossy();
    if !raw_lossy.contains('*') && !raw_lossy.contains('?') {
        return if pattern_path.is_file() {
            vec![pattern_path.to_path_buf()]
        } else {
            Vec::new()
        };
    }

    let components: Vec<Component<'_>> = pattern_path.components().collect();
    let mut prefix = PathBuf::new();
    let mut idx = 0;

    while idx < components.len() {
        match components[idx] {
            Component::Normal(seg) => {
                let s = seg.to_string_lossy();
                if s.contains('*') || s.contains('?') {
                    break;
                }
                prefix.push(seg);
            }
            other => {
                prefix.push(other.as_os_str());
            }
        }
        idx += 1;
    }

    let mut current = if prefix.as_os_str().is_empty() {
        vec![PathBuf::from(".")]
    } else {
        vec![prefix]
    };

    for comp in &components[idx..] {
        let Component::Normal(seg) = comp else {
            continue;
        };
        let seg_str = seg.to_string_lossy();
        let has_wildcard = seg_str.contains('*') || seg_str.contains('?');
        let mut next_paths = Vec::new();

        if has_wildcard {
            for base in &current {
                let Ok(entries) = fs::read_dir(base) else {
                    continue;
                };
                let mut matched_names: Vec<PathBuf> = entries
                    .flatten()
                    .filter_map(|entry| {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with('.') && !seg_str.starts_with('.') {
                            return None;
                        }
                        wildcard_match(&seg_str, &name).then(|| entry.path())
                    })
                    .collect();
                matched_names.sort();
                next_paths.extend(matched_names);
            }
        } else {
            for base in &current {
                let joined = base.join(seg);
                if joined.exists() {
                    next_paths.push(joined);
                }
            }
        }

        current = next_paths;
        if current.is_empty() {
            break;
        }
    }

    current.retain(|p| p.is_file());
    current.sort();
    current
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_pi: Option<usize> = None;
    let mut star_ti = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

// ─── 5–6. SSH Transport, Escaping & Staging Mechanics ─────────────────────────

pub fn posix_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn expand_remote_tilde(remote_home: &str, raw_path: &str) -> String {
    let trimmed = raw_path.trim().replace('\\', "/");
    let home = if remote_home.trim() == "/" {
        ""
    } else {
        remote_home.trim().trim_end_matches('/')
    };

    if trimmed == "~" || trimmed == "~/" {
        return if home.is_empty() {
            "/".to_string()
        } else {
            home.to_string()
        };
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return format!("{}/{}", home, rest);
    }
    if trimmed.starts_with('/') {
        return trimmed;
    }
    format!("{}/{}", home, trimmed)
}

fn expand_local_tilde(raw: &str) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"');
    if let Some(home) = dirs::home_dir() {
        if trimmed == "~" {
            return home;
        }
        if let Some(rest) = trimmed
            .strip_prefix("~/")
            .or_else(|| trimmed.strip_prefix("~\\"))
        {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn validate_no_nul(value: &str, label: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err(format!("{} 包含非法空字节", label));
    }
    Ok(())
}

pub(crate) fn validate_slug_segment(slug: &str) -> Result<(), String> {
    let trimmed = slug.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('\0')
    {
        return Err(format!("非法的资源标识 (slug): {:?}", slug));
    }
    Ok(())
}

pub(crate) fn validate_remote_removable_path(remote_target: &str) -> Result<(), String> {
    validate_no_nul(remote_target, "远端路径")?;
    let trimmed = remote_target.trim();
    let stripped = trimmed.trim_end_matches('/').trim_end_matches('\\');
    if trimmed.is_empty()
        || stripped.is_empty()
        || matches!(stripped, "~" | "." | ".." | "$HOME" | "${HOME}" | "~/$HOME")
    {
        return Err(format!(
            "拒绝操作高危远端根路径或主目录: {:?}",
            remote_target
        ));
    }
    for seg in stripped.split('/') {
        if seg == ".." {
            return Err(format!(
                "远端路径不允许包含父级越界 `..`: {:?}",
                remote_target
            ));
        }
    }
    Ok(())
}

fn join_remote_path(base_dir: &str, leaf: &str) -> Result<String, String> {
    validate_no_nul(base_dir, "远端目录")?;
    validate_slug_segment(leaf)?;
    let base_trimmed = base_dir.trim().replace('\\', "/");
    if base_trimmed.is_empty() {
        return Err("远端目标目录未配置".to_string());
    }
    let clean_base = if base_trimmed == "/" {
        return Err("不允许直接挂载在远端根目录 `/` 下".to_string());
    } else {
        base_trimmed.trim_end_matches('/')
    };
    let joined = format!("{}/{}", clean_base, leaf);
    validate_remote_removable_path(&joined)?;
    Ok(joined)
}

fn validate_ssh_destination(ssh: &SshTargetConfig) -> Result<String, String> {
    let host = ssh.host.trim();
    if host.is_empty() {
        return Err("SSH 主机地址 (host) 不能为空".to_string());
    }
    if host.starts_with('-') || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("非法的 SSH 主机地址: {}", host));
    }

    if let Some(user) = ssh.user.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        if user.starts_with('-') || user.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(format!("非法的 SSH 用户名: {}", user));
        }
        if host.contains('@') {
            Ok(host.to_string())
        } else {
            Ok(format!("{}@{}", user, host))
        }
    } else {
        Ok(host.to_string())
    }
}

fn new_hidden_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn build_ssh_command(ssh: &SshTargetConfig, remote_cmd: &str) -> Result<Command, String> {
    let destination = validate_ssh_destination(ssh)?;
    let mut cmd = new_hidden_command("ssh");
    cmd.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6"]);

    if let Some(port) = ssh.port {
        cmd.arg("-p").arg(port.to_string());
    }

    if let Some(id_raw) = ssh
        .identity_file
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        validate_no_nul(id_raw, "SSH 私钥路径")?;
        let expanded = expand_local_tilde(id_raw);
        cmd.arg("-i").arg(expanded);
    }

    cmd.arg(destination).arg("--").arg(remote_cmd);
    Ok(cmd)
}

fn build_remote_sh_invocation(script: &str, positional_args: &[&str]) -> Result<String, String> {
    let mut parts = Vec::with_capacity(positional_args.len() + 3);
    parts.push("sh".to_string());
    parts.push("-c".to_string());
    parts.push(posix_single_quote(script));
    parts.push("sh".to_string());
    for arg in positional_args {
        validate_no_nul(arg, "远端参数")?;
        parts.push(posix_single_quote(arg));
    }
    Ok(parts.join(" "))
}

fn run_ssh_script(
    ssh: &SshTargetConfig,
    script: &str,
    positional_args: &[&str],
    stdin_payload: Option<&[u8]>,
) -> Result<std::process::Output, String> {
    let remote_cmd = build_remote_sh_invocation(script, positional_args)?;
    let mut cmd = build_ssh_command(ssh, &remote_cmd)?;

    if let Some(payload) = stdin_payload {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("无法启动本机 ssh 进程: {}", e))?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(err) = stdin.write_all(payload) {
                if err.kind() != ErrorKind::BrokenPipe {
                    return Err(format!("通过 SSH 发送数据流失败: {}", err));
                }
            }
        }
        child
            .wait_with_output()
            .map_err(|e| format!("等待 SSH 进程结束失败: {}", e))
    } else {
        cmd.stdin(Stdio::null())
            .output()
            .map_err(|e| format!("无法执行本机 ssh 命令: {}", e))
    }
}

fn format_ssh_failure(ssh: &SshTargetConfig, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("退出码 {:?}", output.status.code())
    };
    format!("SSH ({}) 执行失败: {}", ssh.host.trim(), detail)
}

pub fn test_ssh_connection(ssh: &SshTargetConfig) -> Result<SshConnectionTestResult, String> {
    let remote_probe = build_remote_sh_invocation("printf '%s\\n' \"$HOME\"; uname -srm", &[])?;
    let mut cmd = build_ssh_command(ssh, &remote_probe)?;
    cmd.stdin(Stdio::null());

    let started = Instant::now();
    let output = match cmd.output() {
        Ok(out) => out,
        Err(err) => {
            return Ok(SshConnectionTestResult {
                ok: false,
                remote_home: None,
                remote_os: None,
                latency_ms: started.elapsed().as_millis() as u64,
                message: format!("无法启动本机 ssh 客户端: {}", err),
            });
        }
    };
    let latency_ms = started.elapsed().as_millis() as u64;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut lines = stdout.lines();
        let remote_home = lines
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        let remote_os = lines
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);

        let summary_parts: Vec<&str> = [remote_os.as_deref(), remote_home.as_deref()]
            .into_iter()
            .flatten()
            .collect();

        let message = if summary_parts.is_empty() {
            format!("连接成功 ({}ms)", latency_ms)
        } else {
            format!(
                "连接成功 · {}ms · {}",
                latency_ms,
                summary_parts.join(" · ")
            )
        };

        Ok(SshConnectionTestResult {
            ok: true,
            remote_home,
            remote_os,
            latency_ms,
            message,
        })
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("SSH 连接失败 (退出码 {:?})", output.status.code())
        };
        Ok(SshConnectionTestResult {
            ok: false,
            remote_home: None,
            remote_os: None,
            latency_ms,
            message,
        })
    }
}

fn query_remote_home(ssh: &SshTargetConfig) -> Result<String, String> {
    let output = run_ssh_script(ssh, "printf '%s\\n' \"$HOME\"", &[], None)?;
    if !output.status.success() {
        return Err(format_ssh_failure(ssh, &output));
    }
    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if home.is_empty() {
        return Err(format!("SSH ({}) 远端 $HOME 为空", ssh.host.trim()));
    }
    Ok(home)
}

/// Copies `local_src` into `stage_dst` while dereferencing symlinks, skipping heavy/VCS metadata,
/// and guarding against symlink directory cycles via canonicalized ancestor tracking.
pub(crate) fn stage_dir_dereferenced(local_src: &Path, stage_dst: &Path) -> Result<(), String> {
    let src_meta = fs::metadata(local_src)
        .map_err(|e| format!("读取本地源目录 {:?} 失败: {}", local_src, e))?;
    if !src_meta.is_dir() {
        return Err(format!("本地源路径不是目录: {:?}", local_src));
    }

    let root_canon = fs::canonicalize(local_src)
        .map_err(|e| format!("解析本地源目录真实路径 {:?} 失败: {}", local_src, e))?;
    let mut ancestors = HashSet::new();
    ancestors.insert(root_canon);

    copy_dereferenced_recursive(local_src, stage_dst, &mut ancestors)
}

fn copy_dereferenced_recursive(
    src_dir: &Path,
    dst_dir: &Path,
    ancestors: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    fs::create_dir_all(dst_dir)
        .map_err(|e| format!("创建临时暂存目录 {:?} 失败: {}", dst_dir, e))?;

    let entries =
        fs::read_dir(src_dir).map_err(|e| format!("读取本地目录 {:?} 失败: {}", src_dir, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(
            name.as_str(),
            ".git" | "node_modules" | "__pycache__" | ".DS_Store"
        ) {
            continue;
        }

        let entry_path = entry.path();
        // `fs::metadata` follows symlinks; broken symlinks inside a bundle are skipped gracefully.
        let Ok(meta) = fs::metadata(&entry_path) else {
            continue;
        };

        let target_path = dst_dir.join(&name);
        if meta.is_dir() {
            let Ok(canon) = fs::canonicalize(&entry_path) else {
                continue;
            };
            if !ancestors.insert(canon.clone()) {
                // Guard against symlink directory cycle!
                continue;
            }
            let copy_res = copy_dereferenced_recursive(&entry_path, &target_path, ancestors);
            ancestors.remove(&canon);
            copy_res?;
        } else if meta.is_file() {
            fs::copy(&entry_path, &target_path)
                .map_err(|e| format!("复制暂存文件 {:?} 失败: {}", entry_path, e))?;
        }
    }

    Ok(())
}

pub fn upload_dir_dereferenced(
    ssh: &SshTargetConfig,
    local_src: &Path,
    remote_dest: &str,
) -> Result<(), String> {
    validate_remote_removable_path(remote_dest)?;

    let temp_stage = std::env::temp_dir().join(format!("skillswitch-ssh-stage-{}", Uuid::new_v4()));
    if let Err(err) = stage_dir_dereferenced(local_src, &temp_stage) {
        let _ = fs::remove_dir_all(&temp_stage);
        return Err(err);
    }

    let mut tar_cmd = new_hidden_command("tar");
    let tar_output_res = tar_cmd
        .env("COPYFILE_DISABLE", "1")
        .args(["-czf", "-", "-C"])
        .arg(&temp_stage)
        .arg(".")
        .output();

    let _ = fs::remove_dir_all(&temp_stage);

    let tar_output = tar_output_res.map_err(|e| format!("无法执行本机 tar 打包命令: {}", e))?;
    if !tar_output.status.success() {
        let stderr = String::from_utf8_lossy(&tar_output.stderr)
            .trim()
            .to_string();
        return Err(format!("本地打包目录 {:?} 失败: {}", local_src, stderr));
    }

    let output = run_ssh_script(
        ssh,
        REMOTE_UPLOAD_DIR_SCRIPT,
        &[remote_dest],
        Some(&tar_output.stdout),
    )?;
    if !output.status.success() {
        return Err(format_ssh_failure(ssh, &output));
    }
    Ok(())
}

pub fn remove_remote_dir(ssh: &SshTargetConfig, remote_target: &str) -> Result<(), String> {
    remove_remote_paths(ssh, &[remote_target])
}

fn remove_remote_paths(ssh: &SshTargetConfig, remote_targets: &[&str]) -> Result<(), String> {
    if remote_targets.is_empty() {
        return Ok(());
    }
    for target in remote_targets {
        validate_remote_removable_path(target)?;
    }
    let output = run_ssh_script(ssh, REMOTE_REMOVE_PATHS_SCRIPT, remote_targets, None)?;
    if !output.status.success() {
        return Err(format_ssh_failure(ssh, &output));
    }
    Ok(())
}

pub fn read_remote_text_file(
    ssh: &SshTargetConfig,
    remote_path: &str,
) -> Result<Option<String>, String> {
    validate_remote_removable_path(remote_path)?;
    let output = run_ssh_script(ssh, REMOTE_READ_FILE_SCRIPT, &[remote_path], None)?;
    if !output.status.success() {
        return Err(format_ssh_failure(ssh, &output));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    if raw.starts_with("MISSING\n") || raw.trim() == "MISSING" {
        return Ok(None);
    }
    if let Some(body) = raw.strip_prefix("EXISTS\n") {
        return Ok(Some(body.to_string()));
    }
    if let Some(body) = raw.strip_prefix("EXISTS\r\n") {
        return Ok(Some(body.to_string()));
    }
    Ok(Some(raw.into_owned()))
}

pub fn write_remote_text_file_atomic(
    ssh: &SshTargetConfig,
    remote_path: &str,
    content: &str,
) -> Result<(), String> {
    validate_remote_removable_path(remote_path)?;
    let output = run_ssh_script(
        ssh,
        REMOTE_WRITE_FILE_ATOMIC_SCRIPT,
        &[remote_path],
        Some(content.as_bytes()),
    )?;
    if !output.status.success() {
        return Err(format_ssh_failure(ssh, &output));
    }
    Ok(())
}

fn list_remote_dir_entries(
    ssh: &SshTargetConfig,
    remote_dir: &str,
) -> Result<Vec<(String, bool)>, String> {
    validate_no_nul(remote_dir, "远端目录")?;
    let trimmed = remote_dir.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let output = run_ssh_script(ssh, REMOTE_LIST_DIR_SCRIPT, &[trimmed], None)?;
    if !output.status.success() {
        return Err(format_ssh_failure(ssh, &output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if let Some(name) = line.strip_prefix("D\t") {
            let clean = name.trim();
            if !clean.is_empty() {
                entries.push((clean.to_string(), true));
            }
        } else if let Some(name) = line.strip_prefix("F\t") {
            let clean = name.trim();
            if !clean.is_empty() {
                entries.push((clean.to_string(), false));
            }
        }
    }
    Ok(entries)
}
// ─── 7. Resource Install / Uninstall over SSH ─────────────────────────────────

pub fn install_skill_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    slug: &str,
    local_source_dir: &Path,
) -> Result<(), String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(slug)?;
    let remote_dest = join_remote_path(&custom.global_skill_dir, slug)?;
    upload_dir_dereferenced(ssh, local_source_dir, &remote_dest)?;

    mutate_app_ledger(app, &custom.id, |ledger| {
        push_unique(&mut ledger.skills, slug);
    })
}

pub fn uninstall_skill_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    slug: &str,
) -> Result<(), String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(slug)?;
    if !custom.global_skill_dir.trim().is_empty() {
        let remote_dest = join_remote_path(&custom.global_skill_dir, slug)?;
        remove_remote_dir(ssh, &remote_dest)?;
    }

    mutate_app_ledger(app, &custom.id, |ledger| {
        remove_slug(&mut ledger.skills, slug);
    })
}

fn read_local_command_markdown(slug: &str, local_cmd_md: &Path) -> Result<String, String> {
    let file_path = if local_cmd_md.is_file() {
        local_cmd_md.to_path_buf()
    } else if local_cmd_md.is_dir() {
        let primary = local_cmd_md.join("COMMAND.md");
        if primary.is_file() {
            primary
        } else {
            local_cmd_md.join(format!("{}.md", slug))
        }
    } else {
        local_cmd_md.to_path_buf()
    };

    fs::read_to_string(&file_path)
        .map_err(|e| format!("读取本地命令文件 {:?} 失败: {}", file_path, e))
}

pub fn install_command_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    slug: &str,
    local_cmd_md: &Path,
) -> Result<(), String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(slug)?;
    let cmd_dir = custom
        .global_command_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("SSH 目标端「{}」未配置全局 Commands 目录", custom.label))?;

    let content = read_local_command_markdown(slug, local_cmd_md)?;
    let md_leaf = format!("{}.md", slug);
    let remote_file = join_remote_path(cmd_dir, &md_leaf)?;
    write_remote_text_file_atomic(ssh, &remote_file, &content)?;

    mutate_app_ledger(app, &custom.id, |ledger| {
        push_unique(&mut ledger.commands, slug);
    })
}

pub fn uninstall_command_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    slug: &str,
) -> Result<(), String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(slug)?;
    if let Some(cmd_dir) = custom
        .global_command_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let md_leaf = format!("{}.md", slug);
        let remote_md = join_remote_path(cmd_dir, &md_leaf)?;
        let remote_subdir = join_remote_path(cmd_dir, slug)?;
        remove_remote_paths(ssh, &[&remote_md, &remote_subdir])?;
    }

    mutate_app_ledger(app, &custom.id, |ledger| {
        remove_slug(&mut ledger.commands, slug);
    })
}

// ─── Remote MCP Surgical Merge & Bundled Plugin Path Rewriting ────────────────

fn resolve_remote_mcp_format_and_root(
    custom: &CustomAppConfig,
) -> Result<(String, mcp::ConfigFormat, String), String> {
    let raw_path = custom
        .global_mcp_config_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("SSH 目标端「{}」未配置全局 MCP 配置文件路径", custom.label))?;

    let format = if custom
        .mcp_format
        .as_deref()
        .map(|f| f.eq_ignore_ascii_case("toml"))
        .unwrap_or(false)
        || raw_path.ends_with(".toml")
    {
        mcp::ConfigFormat::Toml
    } else {
        mcp::ConfigFormat::Json
    };

    let root_key = custom
        .mcp_root_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(if format == mcp::ConfigFormat::Toml {
            "mcp_servers"
        } else {
            "mcpServers"
        })
        .to_string();

    Ok((raw_path.to_string(), format, root_key))
}

fn effective_remote_app_dialect<'a>(
    custom: &'a CustomAppConfig,
    ssh: &'a SshTargetConfig,
) -> &'a str {
    if let Some(preset) = ssh
        .remote_app_preset
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("custom"))
    {
        return preset;
    }
    if let Some(cfg_path) = custom.global_mcp_config_path.as_deref() {
        let lower = cfg_path.to_ascii_lowercase();
        if lower.ends_with(".claude.json") || lower.contains("/.claude/") {
            return "claude";
        }
        if lower.contains(".cursor/") {
            return "cursor";
        }
        if lower.contains(".gemini/config/") {
            return "antigravity";
        }
        if lower.contains(".gemini/") {
            return "gemini";
        }
        if lower.contains("windsurf") {
            return "windsurf";
        }
        if lower.contains(".codex/") {
            return "codex";
        }
    }
    &custom.id
}

fn resolve_remote_plugin_base_dir(
    ssh: &SshTargetConfig,
    fallback_when_unset: bool,
) -> Option<String> {
    if let Some(explicit) = ssh
        .global_plugin_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(explicit.to_string());
    }

    match ssh
        .remote_app_preset
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("claude") => Some("~/.claude/plugins/local".to_string()),
        Some("codex") => Some("~/.codex/plugins".to_string()),
        Some("cursor") => Some("~/.cursor/plugins/local".to_string()),
        Some("antigravity") => Some("~/.gemini/config/plugins".to_string()),
        Some("gemini") => Some("~/.gemini/extensions".to_string()),
        _ if fallback_when_unset => Some("~/.skill-switch/plugins".to_string()),
        _ => None,
    }
}

pub(crate) fn rewrite_mcp_record_for_remote_plugin(
    record: &McpServerRecord,
    local_plugin_dir: &Path,
    remote_plugin_dir: &str,
) -> McpServerRecord {
    let raw_local = local_plugin_dir.to_string_lossy();
    let local_native = raw_local.trim_end_matches('/').trim_end_matches('\\');
    let local_slash = local_native.replace('\\', "/");
    let clean_remote = remote_plugin_dir.trim_end_matches('/');

    let rewrite_str = |input: &str| -> String {
        let mut out = input
            .replace("${CLAUDE_PLUGIN_ROOT}", clean_remote)
            .replace("${PLUGIN_ROOT}", clean_remote);
        if !local_native.is_empty() {
            out = out.replace(local_native, clean_remote);
        }
        if !local_slash.is_empty() && local_slash != local_native {
            out = out.replace(&local_slash, clean_remote);
        }
        out
    };

    let mut cloned = record.clone();
    cloned.command = cloned.command.as_deref().map(rewrite_str);
    cloned.args = cloned.args.iter().map(|a| rewrite_str(a)).collect();
    cloned.url = cloned.url.as_deref().map(rewrite_str);
    for spec in &mut cloned.env_specs {
        if let Some(ref def) = spec.default_value {
            spec.default_value = Some(rewrite_str(def));
        }
    }
    for hspec in &mut cloned.header_specs {
        if let Some(ref tpl) = hspec.value_template {
            hspec.value_template = Some(rewrite_str(tpl));
        }
    }
    cloned
}

fn mcp_record_references_local_plugin_dir(
    record: &McpServerRecord,
    local_plugin_dir: &Path,
) -> bool {
    let raw_local = local_plugin_dir.to_string_lossy();
    let local_native = raw_local.trim_end_matches('/').trim_end_matches('\\');
    if local_native.is_empty() {
        return false;
    }
    let local_slash = local_native.replace('\\', "/");
    let mentions = |s: &str| {
        s.contains(local_native)
            || s.contains(&local_slash)
            || s.contains("${CLAUDE_PLUGIN_ROOT}")
            || s.contains("${PLUGIN_ROOT}")
    };

    record.command.as_deref().map(mentions).unwrap_or(false)
        || record.args.iter().any(|a| mentions(a))
        || record.url.as_deref().map(mentions).unwrap_or(false)
        || record
            .env_specs
            .iter()
            .any(|e| e.default_value.as_deref().map(mentions).unwrap_or(false))
        || record
            .header_specs
            .iter()
            .any(|h| h.value_template.as_deref().map(mentions).unwrap_or(false))
}

pub(crate) fn merge_json_mcp_content(
    existing_raw: Option<&str>,
    root_key: &str,
    slug: &str,
    entry: JsonValue,
) -> Result<String, String> {
    let mut root: JsonValue = match existing_raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => serde_json::from_str(raw).map_err(|e| {
            format!(
                "远端 MCP JSON 配置文件包含无效语法，已中止写入以保护原有数据: {}",
                e
            )
        })?,
        None => json!({}),
    };

    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| "远端 MCP JSON 配置文件根节点不是对象".to_string())?;
    let servers_val = root_obj
        .entry(root_key.to_string())
        .or_insert_with(|| json!({}));
    let servers_obj = servers_val
        .as_object_mut()
        .ok_or_else(|| format!("远端配置中的 `{}` 字段不是 JSON 对象", root_key))?;
    servers_obj.insert(slug.to_string(), entry);

    let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    Ok(format!("{}\n", pretty))
}

pub(crate) fn remove_json_mcp_content(
    existing_raw: &str,
    root_key: &str,
    slug: &str,
) -> Result<Option<String>, String> {
    let trimmed = existing_raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut root: JsonValue = serde_json::from_str(trimmed).map_err(|e| {
        format!(
            "远端 MCP JSON 配置文件包含无效语法，已中止移除以保护原有数据: {}",
            e
        )
    })?;

    let removed = root
        .as_object_mut()
        .and_then(|obj| obj.get_mut(root_key))
        .and_then(|m| m.as_object_mut())
        .and_then(|m| m.remove(slug))
        .is_some();

    if !removed {
        return Ok(None);
    }
    let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    Ok(Some(format!("{}\n", pretty)))
}

fn extract_env_var_placeholder_name(input: &str) -> Option<String> {
    let start = input.find("${")?;
    let after = &input[start + 2..];
    let end = after.find('}')?;
    let inner = after[..end].trim();
    let stripped = inner.strip_prefix("env:").unwrap_or(inner).trim();
    (!stripped.is_empty()).then(|| stripped.to_string())
}

pub(crate) fn merge_toml_mcp_content(
    existing_raw: Option<&str>,
    root_key: &str,
    record: &McpServerRecord,
    local_secrets: &HashMap<String, String>,
    use_placeholders: bool,
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let mut doc: DocumentMut = match existing_raw.filter(|s| !s.trim().is_empty()) {
        Some(raw) => raw.parse::<DocumentMut>().map_err(|e| {
            format!(
                "远端 TOML 配置文件包含无效语法，已中止写入以保护原有数据与注释: {}",
                e
            )
        })?,
        None => DocumentMut::new(),
    };

    if !doc.contains_key(root_key) {
        let mut parent = Table::new();
        parent.set_implicit(true);
        doc.insert(root_key, Item::Table(parent));
    }

    let root_table = doc[root_key]
        .as_table_mut()
        .ok_or_else(|| format!("远端 TOML 中的 `[{}]` 不是表结构", root_key))?;

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
                if use_placeholders {
                    let mut env_vars_arr = TomlArray::new();
                    let mut inline_env = InlineTable::new();
                    for spec in &record.env_specs {
                        if spec.is_secret {
                            env_vars_arr.push(spec.name.as_str());
                        } else if let Some(ref def) = spec.default_value {
                            inline_env.insert(&spec.name, TomlValue::from(def.as_str()));
                        }
                    }
                    if !env_vars_arr.is_empty() {
                        srv_table.insert("env_vars", Item::Value(TomlValue::Array(env_vars_arr)));
                        warnings.push(format!(
                            "MCP「{}」已通过 env_vars 转发远端环境变量（未在远端 TOML 中写入明文密钥）",
                            record.name
                        ));
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
                                warnings.push(format!(
                                    "MCP「{}」尚未在本机配置敏感变量 {}，已降级为 env_vars 转发",
                                    record.name, spec.name
                                ));
                            }
                        } else if let Some(ref def) = spec.default_value {
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

            let mut http_headers = InlineTable::new();
            for hspec in &record.header_specs {
                if hspec.is_secret {
                    if hspec.name.eq_ignore_ascii_case("authorization") {
                        let env_var_name = hspec
                            .value_template
                            .as_deref()
                            .and_then(extract_env_var_placeholder_name)
                            .unwrap_or_else(|| "MCP_BEARER_TOKEN".to_string());
                        srv_table.insert("bearer_token_env_var", toml_edit::value(env_var_name));
                    } else if !use_placeholders {
                        if let Some(secret) =
                            local_secrets.get(&hspec.name).filter(|v| !v.is_empty())
                        {
                            http_headers.insert(&hspec.name, TomlValue::from(secret.as_str()));
                        }
                    }
                } else if let Some(ref tpl) = hspec.value_template {
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

    root_table.insert(&record.slug, Item::Table(srv_table));
    Ok(doc.to_string())
}

pub(crate) fn remove_toml_mcp_content(
    existing_raw: &str,
    root_key: &str,
    slug: &str,
) -> Result<Option<String>, String> {
    if existing_raw.trim().is_empty() {
        return Ok(None);
    }
    let mut doc: DocumentMut = existing_raw.parse::<DocumentMut>().map_err(|e| {
        format!(
            "远端 TOML 配置文件包含无效语法，已中止移除以保护原有数据与注释: {}",
            e
        )
    })?;

    let removed = doc
        .get_mut(root_key)
        .and_then(|item| item.as_table_mut())
        .and_then(|table| table.remove(slug))
        .is_some();

    if !removed {
        return Ok(None);
    }
    Ok(Some(doc.to_string()))
}

pub fn install_mcp_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    record: &McpServerRecord,
    library: &RepoLibrary,
    force_placeholders: bool,
) -> Result<Vec<String>, String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(&record.slug)?;
    let (remote_cfg_path, format, root_key) = resolve_remote_mcp_format_and_root(custom)?;

    let mut resolved_remote_home: Option<String> = None;
    let effective_record = if let Some(ref pid) = record.plugin_id {
        if let Some(parent_plugin) = library.plugins.iter().find(|p| &p.id == pid) {
            let local_plugin_dir = plugins::plugin_source_dir(app, &parent_plugin.slug)?;
            if mcp_record_references_local_plugin_dir(record, &local_plugin_dir) {
                let remote_home = match get_app_ledger(app, &custom.id).remote_home {
                    Some(cached) if !cached.trim().is_empty() => cached,
                    _ => query_remote_home(ssh)?,
                };
                let remote_plugin_base = resolve_remote_plugin_base_dir(ssh, true)
                    .unwrap_or_else(|| "~/.skill-switch/plugins".to_string());
                let expanded_base = expand_remote_tilde(&remote_home, &remote_plugin_base);
                let remote_plugin_dir = format!(
                    "{}/{}",
                    expanded_base.trim_end_matches('/'),
                    parent_plugin.slug
                );
                resolved_remote_home = Some(remote_home);
                rewrite_mcp_record_for_remote_plugin(record, &local_plugin_dir, &remote_plugin_dir)
            } else {
                record.clone()
            }
        } else {
            record.clone()
        }
    } else {
        record.clone()
    };

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

    let use_placeholders = force_placeholders || !ssh.inject_secrets;
    let mut warnings = Vec::new();
    let existing_raw = read_remote_text_file(ssh, &remote_cfg_path)?;

    let next_content = match format {
        mcp::ConfigFormat::Json => {
            let dialect = effective_remote_app_dialect(custom, ssh);
            let mut build_warnings = Vec::new();
            let entry = mcp::build_json_entry_for_app(
                &effective_record,
                &merged_secrets,
                dialect,
                use_placeholders,
                &mut build_warnings,
            )?;
            for w in build_warnings {
                if !ssh.inject_secrets && w.contains("为防止项目代码仓库泄露密钥") {
                    warnings.push(format!(
                        "SSH 目标端「{}」未开启密钥注入，MCP「{}」的敏感变量已以 ${{ENV_VAR}} 占位符写入远端配置",
                        custom.label, record.name
                    ));
                } else {
                    warnings.push(w);
                }
            }
            merge_json_mcp_content(existing_raw.as_deref(), &root_key, &record.slug, entry)?
        }
        mcp::ConfigFormat::Toml => merge_toml_mcp_content(
            existing_raw.as_deref(),
            &root_key,
            &effective_record,
            &merged_secrets,
            use_placeholders,
            &mut warnings,
        )?,
    };

    write_remote_text_file_atomic(ssh, &remote_cfg_path, &next_content)?;

    mutate_app_ledger(app, &custom.id, |ledger| {
        push_unique(&mut ledger.mcp_servers, &record.slug);
        if let Some(home) = resolved_remote_home {
            ledger.remote_home = Some(home);
        }
    })?;

    Ok(warnings)
}

pub fn uninstall_mcp_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    slug: &str,
) -> Result<Vec<String>, String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(slug)?;

    if custom
        .global_mcp_config_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        let (remote_cfg_path, format, root_key) = resolve_remote_mcp_format_and_root(custom)?;
        if let Some(existing_raw) = read_remote_text_file(ssh, &remote_cfg_path)? {
            let maybe_updated = match format {
                mcp::ConfigFormat::Json => remove_json_mcp_content(&existing_raw, &root_key, slug)?,
                mcp::ConfigFormat::Toml => remove_toml_mcp_content(&existing_raw, &root_key, slug)?,
            };
            if let Some(updated_content) = maybe_updated {
                write_remote_text_file_atomic(ssh, &remote_cfg_path, &updated_content)?;
            }
        }
    }

    mutate_app_ledger(app, &custom.id, |ledger| {
        remove_slug(&mut ledger.mcp_servers, slug);
    })?;

    Ok(Vec::new())
}

// ─── Remote Plugin Staging, Enablement & Child Component Projection ───────────

fn set_remote_codex_plugin_enabled(
    ssh: &SshTargetConfig,
    remote_toml_path: &str,
    slug: &str,
    enabled: bool,
) -> Result<(), String> {
    let existing = read_remote_text_file(ssh, remote_toml_path)?;
    if existing.is_none() && !enabled {
        return Ok(());
    }

    let mut doc: DocumentMut = match existing.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(raw) => raw
            .parse::<DocumentMut>()
            .map_err(|e| format!("解析远端 Codex config.toml 失败: {}", e))?,
        None => DocumentMut::new(),
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
    } else {
        return Ok(());
    }

    write_remote_text_file_atomic(ssh, remote_toml_path, &doc.to_string())
}

fn set_remote_antigravity_plugin_enabled(
    ssh: &SshTargetConfig,
    remote_json_path: &str,
    slug: &str,
    enabled: bool,
) -> Result<(), String> {
    let existing = read_remote_text_file(ssh, remote_json_path)?;
    if existing.is_none() && !enabled {
        return Ok(());
    }

    let mut root: JsonValue = match existing.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| format!("解析远端 Antigravity config.json 失败: {}", e))?,
        None => json!({}),
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
        } else if p_map.remove(slug).is_none() {
            return Ok(());
        }
    }

    let formatted = format!(
        "{}\n",
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?
    );
    write_remote_text_file_atomic(ssh, remote_json_path, &formatted)
}

fn resolve_or_materialize_skill_dir(
    app: &tauri::AppHandle,
    resource: &Resource,
) -> Result<PathBuf, String> {
    if let Ok(dir) = store::skill_source_dir_by_id(app, &resource.id) {
        if dir.join("SKILL.md").is_file() {
            return Ok(dir);
        }
    }
    let fallback_dir = store::skill_sources_dir(app)?.join(&resource.slug);
    fs::create_dir_all(&fallback_dir)
        .map_err(|e| format!("创建本地技能源目录 {:?} 失败: {}", fallback_dir, e))?;
    let skill_md = fallback_dir.join("SKILL.md");
    if !skill_md.is_file() {
        fs::write(&skill_md, &resource.content)
            .map_err(|e| format!("写入本地技能文件 {:?} 失败: {}", skill_md, e))?;
    }
    Ok(fallback_dir)
}

fn resolve_or_materialize_command_md(
    app: &tauri::AppHandle,
    resource: &Resource,
) -> Result<PathBuf, String> {
    let cmd_dir = store::slash_command_sources_dir(app)?.join(&resource.slug);
    fs::create_dir_all(&cmd_dir)
        .map_err(|e| format!("创建本地命令源目录 {:?} 失败: {}", cmd_dir, e))?;
    let cmd_md = cmd_dir.join("COMMAND.md");
    if !cmd_md.is_file() {
        fs::write(&cmd_md, &resource.content)
            .map_err(|e| format!("写入本地命令文件 {:?} 失败: {}", cmd_md, e))?;
    }
    Ok(cmd_md)
}

pub fn install_plugin_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    plugin: &PluginRecord,
    library: &RepoLibrary,
    prefer_native: bool,
) -> Result<(PluginInstallMode, Vec<String>), String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(&plugin.slug)?;
    let mut warnings = Vec::new();

    let local_plugin_dir = plugins::plugin_source_dir(app, &plugin.slug)?;
    let native_base_dir = resolve_remote_plugin_base_dir(ssh, false);
    let preset_lower = ssh
        .remote_app_preset
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase());

    // 1) Native plugin bundle staging when supported & requested
    let mode = if prefer_native && local_plugin_dir.is_dir() {
        if let Some(ref base_dir) = native_base_dir {
            let remote_plugin_dest = join_remote_path(base_dir, &plugin.slug)?;
            upload_dir_dereferenced(ssh, &local_plugin_dir, &remote_plugin_dest)?;

            match preset_lower.as_deref() {
                Some("codex") => {
                    let codex_cfg = custom
                        .global_mcp_config_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|p| p.ends_with(".toml"))
                        .unwrap_or("~/.codex/config.toml");
                    if let Err(err) =
                        set_remote_codex_plugin_enabled(ssh, codex_cfg, &plugin.slug, true)
                    {
                        warnings.push(format!("更新远端 Codex 插件启用状态提示: {}", err));
                    }
                }
                Some("antigravity") => {
                    if let Err(err) = set_remote_antigravity_plugin_enabled(
                        ssh,
                        "~/.gemini/config/config.json",
                        &plugin.slug,
                        true,
                    ) {
                        warnings.push(format!("更新远端 Antigravity 插件启用状态提示: {}", err));
                    }
                }
                _ => {}
            }
            PluginInstallMode::Native
        } else {
            if !plugin.components.hooks.is_empty() {
                warnings.push(format!(
                    "SSH 目标端「{}」采用组件投射模式（已同步 Skills 与 MCP；原生 Hooks 需配置远端插件目录或选择支持插件的预设）",
                    custom.label
                ));
            }
            PluginInstallMode::Projected
        }
    } else {
        PluginInstallMode::Projected
    };

    // If running in Projected mode without a native plugin dir, stage bundle to fallback
    // `~/.skill-switch/plugins/<slug>` whenever bundled MCP servers reference local plugin scripts!
    if mode == PluginInstallMode::Projected && local_plugin_dir.is_dir() {
        let needs_fallback_bundle = plugin.projected_mcp_server_ids.iter().any(|mid| {
            library
                .mcp_servers
                .iter()
                .find(|m| &m.id == mid)
                .map(|m| mcp_record_references_local_plugin_dir(m, &local_plugin_dir))
                .unwrap_or(false)
        });
        if needs_fallback_bundle {
            let fallback_base =
                native_base_dir.unwrap_or_else(|| "~/.skill-switch/plugins".to_string());
            let remote_fallback = join_remote_path(&fallback_base, &plugin.slug)?;
            upload_dir_dereferenced(ssh, &local_plugin_dir, &remote_fallback)?;
        }
    }

    // 2) Project child Skills, Slash Commands, and MCP Servers
    for skill_id in &plugin.projected_skill_ids {
        if let Some(res) = library
            .resources
            .iter()
            .find(|r| &r.id == skill_id && r.kind == ResourceKind::Skill)
        {
            let skill_dir = resolve_or_materialize_skill_dir(app, res)?;
            install_skill_ssh(app, custom, &res.slug, &skill_dir)?;
        }
    }

    if custom
        .global_command_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        for cmd_id in &plugin.projected_command_ids {
            if let Some(res) = library
                .resources
                .iter()
                .find(|r| &r.id == cmd_id && r.kind == ResourceKind::SlashCommand)
            {
                let cmd_md = resolve_or_materialize_command_md(app, res)?;
                if let Err(err) = install_command_ssh(app, custom, &res.slug, &cmd_md) {
                    warnings.push(format!(
                        "{}: 投射插件命令「{}」失败 ({})",
                        custom.label, res.slug, err
                    ));
                }
            }
        }
    }

    if custom
        .global_mcp_config_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        for mcp_id in &plugin.projected_mcp_server_ids {
            if let Some(mcp_rec) = library.mcp_servers.iter().find(|m| &m.id == mcp_id) {
                match install_mcp_ssh(app, custom, mcp_rec, library, false) {
                    Ok(mcp_warns) => warnings.extend(mcp_warns),
                    Err(err) => warnings.push(format!(
                        "{}: 投射插件 MCP「{}」失败 ({})",
                        custom.label, mcp_rec.slug, err
                    )),
                }
            }
        }
    }

    // 3) Record plugin slug in ledger
    mutate_app_ledger(app, &custom.id, |ledger| {
        push_unique(&mut ledger.plugins, &plugin.slug);
    })?;

    Ok((mode, warnings))
}

pub fn uninstall_plugin_ssh(
    app: &tauri::AppHandle,
    custom: &CustomAppConfig,
    plugin: &PluginRecord,
) -> Result<Vec<String>, String> {
    let ssh = require_ssh_config(custom)?;
    validate_slug_segment(&plugin.slug)?;
    let mut warnings = Vec::new();

    let library = store::connected_repo_root(app)
        .and_then(|root| store::load_repo_library(&root))
        .unwrap_or_default();

    for skill_id in &plugin.projected_skill_ids {
        if let Some(res) = library.resources.iter().find(|r| &r.id == skill_id) {
            uninstall_skill_ssh(app, custom, &res.slug)?;
        }
    }

    if custom
        .global_command_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        for cmd_id in &plugin.projected_command_ids {
            if let Some(res) = library.resources.iter().find(|r| &r.id == cmd_id) {
                if let Err(err) = uninstall_command_ssh(app, custom, &res.slug) {
                    warnings.push(format!(
                        "{}: 移除插件命令「{}」失败 ({})",
                        custom.label, res.slug, err
                    ));
                }
            }
        }
    }

    if custom
        .global_mcp_config_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        for mcp_id in &plugin.projected_mcp_server_ids {
            if let Some(mcp_rec) = library.mcp_servers.iter().find(|m| &m.id == mcp_id) {
                match uninstall_mcp_ssh(app, custom, &mcp_rec.slug) {
                    Ok(mcp_warns) => warnings.extend(mcp_warns),
                    Err(err) => warnings.push(format!(
                        "{}: 移除插件 MCP「{}」失败 ({})",
                        custom.label, mcp_rec.slug, err
                    )),
                }
            }
        }
    }

    let mut plugin_paths_to_remove = Vec::new();
    if let Some(base_dir) = resolve_remote_plugin_base_dir(ssh, false) {
        plugin_paths_to_remove.push(join_remote_path(&base_dir, &plugin.slug)?);
    }
    let fallback_path = join_remote_path("~/.skill-switch/plugins", &plugin.slug)?;
    if !plugin_paths_to_remove.iter().any(|p| p == &fallback_path) {
        plugin_paths_to_remove.push(fallback_path);
    }
    let path_refs: Vec<&str> = plugin_paths_to_remove.iter().map(String::as_str).collect();
    remove_remote_paths(ssh, &path_refs)?;

    match ssh
        .remote_app_preset
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("codex") => {
            let codex_cfg = custom
                .global_mcp_config_path
                .as_deref()
                .map(str::trim)
                .filter(|p| p.ends_with(".toml"))
                .unwrap_or("~/.codex/config.toml");
            let _ = set_remote_codex_plugin_enabled(ssh, codex_cfg, &plugin.slug, false);
        }
        Some("antigravity") => {
            let _ = set_remote_antigravity_plugin_enabled(
                ssh,
                "~/.gemini/config/config.json",
                &plugin.slug,
                false,
            );
        }
        _ => {}
    }

    mutate_app_ledger(app, &custom.id, |ledger| {
        remove_slug(&mut ledger.plugins, &plugin.slug);
    })?;

    Ok(warnings)
}

// ─── Best-Effort Auto-Resync Helpers on Local Edits ───────────────────────────

pub fn resync_skill_to_enabled_ssh_apps(
    app: &tauri::AppHandle,
    slug: &str,
    local_source_dir: &Path,
) {
    let custom_apps = store::load_custom_apps(app);
    let Ok(state) = store::load_local_state(app) else {
        return;
    };
    for custom in &custom_apps {
        if custom
            .ssh
            .as_ref()
            .map(|s| !s.host.trim().is_empty())
            .unwrap_or(false)
            && state
                .ssh_ledgers
                .get(&custom.id)
                .map(|l| l.skills.iter().any(|s| s == slug))
                .unwrap_or(false)
        {
            let _ = install_skill_ssh(app, custom, slug, local_source_dir);
        }
    }
}

pub fn resync_command_to_enabled_ssh_apps(app: &tauri::AppHandle, slug: &str, local_cmd_md: &Path) {
    let custom_apps = store::load_custom_apps(app);
    let Ok(state) = store::load_local_state(app) else {
        return;
    };
    for custom in &custom_apps {
        if custom
            .ssh
            .as_ref()
            .map(|s| !s.host.trim().is_empty())
            .unwrap_or(false)
            && state
                .ssh_ledgers
                .get(&custom.id)
                .map(|l| l.commands.iter().any(|c| c == slug))
                .unwrap_or(false)
        {
            let _ = install_command_ssh(app, custom, slug, local_cmd_md);
        }
    }
}

pub fn resync_mcp_to_enabled_ssh_apps(
    app: &tauri::AppHandle,
    record: &McpServerRecord,
    library: &RepoLibrary,
) {
    let custom_apps = store::load_custom_apps(app);
    let Ok(state) = store::load_local_state(app) else {
        return;
    };
    for custom in &custom_apps {
        if custom
            .ssh
            .as_ref()
            .map(|s| !s.host.trim().is_empty())
            .unwrap_or(false)
            && state
                .ssh_ledgers
                .get(&custom.id)
                .map(|l| l.mcp_servers.iter().any(|m| m == &record.slug))
                .unwrap_or(false)
        {
            let _ = install_mcp_ssh(app, custom, record, library, false);
        }
    }
}

pub fn resync_plugin_to_enabled_ssh_apps(
    app: &tauri::AppHandle,
    plugin: &PluginRecord,
    library: &RepoLibrary,
) {
    let custom_apps = store::load_custom_apps(app);
    let Ok(state) = store::load_local_state(app) else {
        return;
    };
    for custom in &custom_apps {
        if custom
            .ssh
            .as_ref()
            .map(|s| !s.host.trim().is_empty())
            .unwrap_or(false)
            && state
                .ssh_ledgers
                .get(&custom.id)
                .map(|l| l.plugins.iter().any(|p| p == &plugin.slug))
                .unwrap_or(false)
        {
            let _ = install_plugin_ssh(app, custom, plugin, library, true);
        }
    }
}

// ─── Settings Maintenance Actions: Calibrate & Full Re-Sync ───────────────────

pub fn calibrate_ssh_target(
    app: &tauri::AppHandle,
    app_id: &str,
) -> Result<SshAppSyncLedger, String> {
    let custom_apps = store::load_custom_apps(app);
    let custom = find_ssh_app(app_id, &custom_apps)
        .ok_or_else(|| format!("未找到 SSH 远程目标端: {}", app_id))?;
    let ssh = require_ssh_config(custom)?;

    let remote_home = query_remote_home(ssh)?;
    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;

    let managed_skills: HashSet<&str> = library
        .resources
        .iter()
        .filter(|r| r.kind == ResourceKind::Skill)
        .map(|r| r.slug.as_str())
        .collect();
    let managed_commands: HashSet<&str> = library
        .resources
        .iter()
        .filter(|r| r.kind == ResourceKind::SlashCommand)
        .map(|r| r.slug.as_str())
        .collect();
    let managed_mcps: HashSet<&str> = library
        .mcp_servers
        .iter()
        .map(|m| m.slug.as_str())
        .collect();
    let managed_plugins: HashSet<&str> = library.plugins.iter().map(|p| p.slug.as_str()).collect();

    let mut skills: Vec<String> = list_remote_dir_entries(ssh, &custom.global_skill_dir)?
        .into_iter()
        .filter_map(|(name, is_dir)| {
            (is_dir && managed_skills.contains(name.as_str())).then_some(name)
        })
        .collect();
    skills.sort();
    skills.dedup();

    let mut commands = Vec::new();
    if let Some(cmd_dir) = custom
        .global_command_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        for (name, is_dir) in list_remote_dir_entries(ssh, cmd_dir)? {
            if is_dir {
                if managed_commands.contains(name.as_str()) {
                    commands.push(name);
                }
            } else if let Some(stem) = name.strip_suffix(".md") {
                if managed_commands.contains(stem) {
                    commands.push(stem.to_string());
                }
            }
        }
        commands.sort();
        commands.dedup();
    }

    let mut mcp_servers = Vec::new();
    if custom
        .global_mcp_config_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        let (remote_cfg_path, format, root_key) = resolve_remote_mcp_format_and_root(custom)?;
        if let Some(raw) = read_remote_text_file(ssh, &remote_cfg_path)? {
            for key in extract_mcp_keys_from_raw(&raw, format, &root_key) {
                if managed_mcps.contains(key.as_str()) {
                    mcp_servers.push(key);
                } else {
                    let slugified = store::slugify(&key);
                    if managed_mcps.contains(slugified.as_str()) {
                        mcp_servers.push(slugified);
                    }
                }
            }
            mcp_servers.sort();
            mcp_servers.dedup();
        }
    }

    let mut plugins = Vec::new();
    if let Some(plugin_base) = resolve_remote_plugin_base_dir(ssh, false) {
        for (name, is_dir) in list_remote_dir_entries(ssh, &plugin_base)? {
            if is_dir && managed_plugins.contains(name.as_str()) {
                plugins.push(name);
            }
        }
    }
    // Also detect plugins projected onto the target when native plugin dir wasn't used
    let skill_set: HashSet<&str> = skills.iter().map(String::as_str).collect();
    let mcp_set: HashSet<&str> = mcp_servers.iter().map(String::as_str).collect();
    for plug in &library.plugins {
        if plugins.iter().any(|p| p == &plug.slug) {
            continue;
        }
        let projected_present = if let Some(first_sid) = plug.projected_skill_ids.first() {
            library
                .resources
                .iter()
                .find(|r| &r.id == first_sid)
                .map(|r| skill_set.contains(r.slug.as_str()))
                .unwrap_or(false)
        } else if let Some(first_mid) = plug.projected_mcp_server_ids.first() {
            library
                .mcp_servers
                .iter()
                .find(|m| &m.id == first_mid)
                .map(|m| mcp_set.contains(m.slug.as_str()))
                .unwrap_or(false)
        } else {
            false
        };
        if projected_present {
            plugins.push(plug.slug.clone());
        }
    }
    plugins.sort();
    plugins.dedup();

    let calibrated = SshAppSyncLedger {
        skills,
        commands,
        mcp_servers,
        plugins,
        remote_home: Some(remote_home),
        last_synced_at: Some(store::now_ms()),
    };

    let mut state = store::load_local_state(app)?;
    state
        .ssh_ledgers
        .insert(app_id.to_string(), calibrated.clone());
    store::save_local_state(app, &state)?;

    Ok(calibrated)
}

fn extract_mcp_keys_from_raw(raw: &str, format: mcp::ConfigFormat, root_key: &str) -> Vec<String> {
    match format {
        mcp::ConfigFormat::Json => serde_json::from_str::<JsonValue>(raw)
            .ok()
            .and_then(|root| {
                root.get(root_key)
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.keys().cloned().collect())
            })
            .unwrap_or_default(),
        mcp::ConfigFormat::Toml => raw
            .parse::<DocumentMut>()
            .ok()
            .and_then(|doc| {
                doc.get(root_key)
                    .and_then(|v| v.as_table())
                    .map(|tbl| tbl.iter().map(|(k, _)| k.to_string()).collect())
            })
            .unwrap_or_default(),
    }
}

pub fn sync_all_enabled_to_ssh_target(
    app: &tauri::AppHandle,
    app_id: &str,
) -> Result<SshSyncSummaryResult, String> {
    let custom_apps = store::load_custom_apps(app);
    let custom = find_ssh_app(app_id, &custom_apps)
        .ok_or_else(|| format!("未找到 SSH 远程目标端: {}", app_id))?;
    let ssh = require_ssh_config(custom)?;

    // Fail fast in 1 roundtrip if remote target is unreachable, and refresh remote `$HOME`
    let remote_home = query_remote_home(ssh)?;
    mutate_app_ledger(app, app_id, |ledger| {
        ledger.remote_home = Some(remote_home);
    })?;

    let repo_root = store::connected_repo_root(app)?;
    let library = store::load_repo_library(&repo_root)?;
    let snapshot = get_app_ledger(app, app_id);

    let mut warnings = Vec::new();
    let mut synced_plugins = 0usize;
    let mut synced_skills = 0usize;
    let mut synced_commands = 0usize;
    let mut synced_mcps = 0usize;

    let mut pushed_skill_slugs = HashSet::new();
    let mut pushed_command_slugs = HashSet::new();
    let mut pushed_mcp_slugs = HashSet::new();
    let mut stale_plugins = Vec::new();
    let mut stale_skills = Vec::new();
    let mut stale_commands = Vec::new();
    let mut stale_mcps = Vec::new();

    // 1. Sync enabled plugins first (which also syncs their projected components)
    for plugin_slug in &snapshot.plugins {
        let Some(plugin) = library.plugins.iter().find(|p| &p.slug == plugin_slug) else {
            stale_plugins.push(plugin_slug.clone());
            continue;
        };

        match install_plugin_ssh(app, custom, plugin, &library, true) {
            Ok((_, plug_warns)) => {
                synced_plugins += 1;
                warnings.extend(plug_warns);
                for sid in &plugin.projected_skill_ids {
                    if let Some(res) = library.resources.iter().find(|r| &r.id == sid) {
                        if pushed_skill_slugs.insert(res.slug.clone()) {
                            synced_skills += 1;
                        }
                    }
                }
                if custom
                    .global_command_dir
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .is_some()
                {
                    for cid in &plugin.projected_command_ids {
                        if let Some(res) = library.resources.iter().find(|r| &r.id == cid) {
                            if pushed_command_slugs.insert(res.slug.clone()) {
                                synced_commands += 1;
                            }
                        }
                    }
                }
                if custom
                    .global_mcp_config_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .is_some()
                {
                    for mid in &plugin.projected_mcp_server_ids {
                        if let Some(mrec) = library.mcp_servers.iter().find(|m| &m.id == mid) {
                            if pushed_mcp_slugs.insert(mrec.slug.clone()) {
                                synced_mcps += 1;
                            }
                        }
                    }
                }
            }
            Err(err) => {
                warnings.push(format!("同步插件「{}」失败: {}", plugin_slug, err));
            }
        }
    }

    // 2. Sync remaining standalone skills
    for skill_slug in &snapshot.skills {
        if pushed_skill_slugs.contains(skill_slug) {
            continue;
        }
        let Some(res) = library
            .resources
            .iter()
            .find(|r| r.kind == ResourceKind::Skill && &r.slug == skill_slug)
        else {
            stale_skills.push(skill_slug.clone());
            continue;
        };

        match resolve_or_materialize_skill_dir(app, res)
            .and_then(|dir| install_skill_ssh(app, custom, &res.slug, &dir))
        {
            Ok(()) => {
                pushed_skill_slugs.insert(res.slug.clone());
                synced_skills += 1;
            }
            Err(err) => warnings.push(format!("同步技能「{}」失败: {}", skill_slug, err)),
        }
    }

    // 3. Sync remaining standalone slash commands
    for cmd_slug in &snapshot.commands {
        if pushed_command_slugs.contains(cmd_slug) {
            continue;
        }
        let Some(res) = library
            .resources
            .iter()
            .find(|r| r.kind == ResourceKind::SlashCommand && &r.slug == cmd_slug)
        else {
            stale_commands.push(cmd_slug.clone());
            continue;
        };

        match resolve_or_materialize_command_md(app, res)
            .and_then(|md| install_command_ssh(app, custom, &res.slug, &md))
        {
            Ok(()) => {
                pushed_command_slugs.insert(res.slug.clone());
                synced_commands += 1;
            }
            Err(err) => warnings.push(format!("同步命令「{}」失败: {}", cmd_slug, err)),
        }
    }

    // 4. Sync remaining standalone MCP servers
    for mcp_slug in &snapshot.mcp_servers {
        if pushed_mcp_slugs.contains(mcp_slug) {
            continue;
        }
        let Some(mrec) = library.mcp_servers.iter().find(|m| &m.slug == mcp_slug) else {
            stale_mcps.push(mcp_slug.clone());
            continue;
        };

        match install_mcp_ssh(app, custom, mrec, &library, false) {
            Ok(mcp_warns) => {
                pushed_mcp_slugs.insert(mrec.slug.clone());
                synced_mcps += 1;
                warnings.extend(mcp_warns);
            }
            Err(err) => warnings.push(format!("同步 MCP「{}」失败: {}", mcp_slug, err)),
        }
    }

    // Prune any orphaned slugs that no longer exist in RepoLibrary
    if !stale_plugins.is_empty()
        || !stale_skills.is_empty()
        || !stale_commands.is_empty()
        || !stale_mcps.is_empty()
    {
        mutate_app_ledger(app, app_id, |ledger| {
            for s in &stale_plugins {
                remove_slug(&mut ledger.plugins, s);
            }
            for s in &stale_skills {
                remove_slug(&mut ledger.skills, s);
            }
            for s in &stale_commands {
                remove_slug(&mut ledger.commands, s);
            }
            for s in &stale_mcps {
                remove_slug(&mut ledger.mcp_servers, s);
            }
        })?;
    }

    Ok(SshSyncSummaryResult {
        app_id: app_id.to_string(),
        synced_skills,
        synced_commands,
        synced_mcps,
        synced_plugins,
        warnings,
    })
}

// ─── Unit Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{McpEnvVarSpec, McpHeaderSpec};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "skill-switch-ssh-test-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    #[test]
    fn parses_ssh_config_with_quoted_include_globs_comments_and_filters() {
        let home = make_temp_dir("ssh-config");
        let ssh_dir = home.join(".ssh");
        let config_d = ssh_dir.join("config.d");
        let lima_dir = home
            .join(".claude-cage")
            .join("state with spaces")
            .join("lima")
            .join("claude-cage-downloads-34636a03");

        fs::create_dir_all(&config_d).unwrap();
        fs::create_dir_all(&lima_dir).unwrap();

        // Quoted absolute Include path (matching Lima / claude-cage convention)
        let lima_cfg = lima_dir.join("ssh.config");
        fs::write(
            &lima_cfg,
            r#"# Generated by Lima
Host lima-claude-cage-downloads-34636a03
  IdentityFile "/Users/demo/.lima/_config/user"
  User cage
  Hostname 127.0.0.1
  Port 60022
"#,
        )
        .unwrap();

        // Wildcard relative Include `config.d/*.conf` + hidden `.DS_Store` junk file
        fs::write(config_d.join(".DS_Store"), b"\x00\x01binary-junk").unwrap();
        fs::write(
            config_d.join("work.conf"),
            r#"
Host prod-vps staging-vps *.internal.corp !blocked-vps
  HostName=10.20.30.40
  User = ubuntu # inline comment
  Port 2222
  IdentityFile ~/.ssh/id_ed25519
"#,
        )
        .unwrap();

        let main_cfg = ssh_dir.join("config");
        fs::write(
            &main_cfg,
            format!(
                r#"# Main SSH Config
Include "{}"
Include config.d/*.conf

Host *
  ServerAliveInterval 60

Host ?*
  AddKeysToAgent yes

Host github.com
  HostName ssh.github.com
  User git
  Port 443

Host github-personal
  HostName github.com
  User git
  IdentityFile ~/.ssh/id_github

Host gitlab.com
  User git

# Duplicate alias later in file should keep first-match definition
Host prod-vps
  HostName 99.99.99.99
  Port 9999
"#,
                lima_cfg.display()
            ),
        )
        .unwrap();

        let hosts = discover_ssh_hosts_from_path(&main_cfg, &ssh_dir, &home)
            .expect("should parse ssh config");

        let aliases: Vec<&str> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(
            aliases,
            vec![
                "lima-claude-cage-downloads-34636a03",
                "prod-vps",
                "staging-vps",
            ]
        );

        let lima = &hosts[0];
        assert_eq!(lima.hostname.as_deref(), Some("127.0.0.1"));
        assert_eq!(lima.user.as_deref(), Some("cage"));
        assert_eq!(lima.port, Some(60022));
        assert_eq!(
            lima.identity_file.as_deref(),
            Some("/Users/demo/.lima/_config/user")
        );

        let prod = &hosts[1];
        assert_eq!(prod.hostname.as_deref(), Some("10.20.30.40"));
        assert_eq!(prod.user.as_deref(), Some("ubuntu"));
        assert_eq!(prod.port, Some(2222));
        assert_eq!(prod.identity_file.as_deref(), Some("~/.ssh/id_ed25519"));

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn posix_single_quote_and_remote_tilde_expansion_are_safe() {
        let tricky = "path with spaces & 'single' \"double\" $(whoami) `id`; rm -rf /";
        let quoted = posix_single_quote(tricky);
        assert_eq!(
            quoted,
            "'path with spaces & '\\''single'\\'' \"double\" $(whoami) `id`; rm -rf /'"
        );

        assert_eq!(expand_remote_tilde("/home/cage", "~"), "/home/cage");
        assert_eq!(expand_remote_tilde("/home/cage/", "~/"), "/home/cage");
        assert_eq!(
            expand_remote_tilde("/home/cage/", "~/.claude/skills/review"),
            "/home/cage/.claude/skills/review"
        );
        assert_eq!(
            expand_remote_tilde("/home/cage", "/opt/mcp/config.json"),
            "/opt/mcp/config.json"
        );
        assert_eq!(
            expand_remote_tilde("/home/cage", ".claude/skills/review"),
            "/home/cage/.claude/skills/review"
        );

        // Guard rails against catastrophic `rm -rf` or SSH option injection
        assert!(validate_remote_removable_path("").is_err());
        assert!(validate_remote_removable_path("/").is_err());
        assert!(validate_remote_removable_path("~").is_err());
        assert!(validate_remote_removable_path("~/").is_err());
        assert!(validate_remote_removable_path("$HOME").is_err());
        assert!(validate_remote_removable_path("${HOME}").is_err());
        assert!(validate_remote_removable_path("~/.claude/../..").is_err());
        assert!(validate_remote_removable_path("~/.claude/skills/my-skill").is_ok());

        assert!(validate_slug_segment("").is_err());
        assert!(validate_slug_segment("..").is_err());
        assert!(validate_slug_segment("a/b").is_err());
        assert!(validate_slug_segment("valid-skill_1").is_ok());

        let evil_ssh = SshTargetConfig {
            host: "-oProxyCommand=evil".to_string(),
            ..Default::default()
        };
        assert!(validate_ssh_destination(&evil_ssh).is_err());
    }

    #[test]
    fn stages_local_dir_dereferencing_symlinks_skipping_junk_and_cycles() {
        let root = make_temp_dir("deref-stage");
        let external_scripts = root.join("external-scripts");
        let src_skill = root.join("my-skill");
        let staged_out = root.join("staged-out");

        fs::create_dir_all(&external_scripts).unwrap();
        fs::write(external_scripts.join("helper.sh"), "#!/bin/sh\necho ok\n").unwrap();

        fs::create_dir_all(src_skill.join(".git")).unwrap();
        fs::write(src_skill.join(".git").join("HEAD"), "ref: refs/heads/main").unwrap();
        fs::create_dir_all(src_skill.join("node_modules").join("pkg")).unwrap();
        fs::write(
            src_skill.join("node_modules").join("pkg").join("index.js"),
            "",
        )
        .unwrap();
        fs::create_dir_all(src_skill.join("__pycache__")).unwrap();
        fs::write(src_skill.join(".DS_Store"), "junk").unwrap();
        fs::write(src_skill.join("SKILL.md"), "# My Skill\n").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&external_scripts, src_skill.join("scripts")).unwrap();
            // Symlink directory cycle pointing back to ancestor `src_skill`
            std::os::unix::fs::symlink(&src_skill, external_scripts.join("cycle_back")).unwrap();
            // Broken symlink should be ignored gracefully
            std::os::unix::fs::symlink(
                root.join("does-not-exist"),
                src_skill.join("dangling_link"),
            )
            .unwrap();
        }

        stage_dir_dereferenced(&src_skill, &staged_out).expect("staging should succeed");

        assert!(staged_out.join("SKILL.md").is_file());
        assert!(!staged_out.join(".git").exists());
        assert!(!staged_out.join("node_modules").exists());
        assert!(!staged_out.join("__pycache__").exists());
        assert!(!staged_out.join(".DS_Store").exists());

        #[cfg(unix)]
        {
            let helper_path = staged_out.join("scripts").join("helper.sh");
            assert!(helper_path.is_file());
            let symlink_meta = fs::symlink_metadata(staged_out.join("scripts")).unwrap();
            assert!(
                !symlink_meta.file_type().is_symlink(),
                "staged directory must be materialized, not a symlink"
            );
            assert!(
                !staged_out.join("scripts").join("cycle_back").exists(),
                "symlink cycle must be skipped cleanly"
            );
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rewrites_local_plugin_root_to_remote_plugin_root_and_enforces_secret_policy() {
        let local_plugin_dir = PathBuf::from(
            "/Users/news/Library/Application Support/SkillSwitch/plugin-sources/devops-suite",
        );
        let remote_plugin_dir = "/home/cage/.claude/plugins/local/devops-suite";

        let rec = McpServerRecord {
            id: "mcp-plug-1".to_string(),
            slug: "devops-suite-sentry".to_string(),
            name: "sentry (devops-suite)".to_string(),
            description: None,
            transport: McpTransport::Stdio,
            command: Some(format!("{}/bin/run-mcp", local_plugin_dir.display())),
            args: vec![
                format!("{}/dist/server.js", local_plugin_dir.display()),
                "${CLAUDE_PLUGIN_ROOT}/config.yaml".to_string(),
            ],
            url: None,
            env_specs: vec![
                McpEnvVarSpec {
                    name: "PLUGIN_ASSETS".to_string(),
                    description: None,
                    required: false,
                    is_secret: false,
                    default_value: Some(format!("{}/assets", local_plugin_dir.display())),
                },
                McpEnvVarSpec {
                    name: "SENTRY_TOKEN".to_string(),
                    description: None,
                    required: true,
                    is_secret: true,
                    default_value: None,
                },
            ],
            header_specs: vec![McpHeaderSpec {
                name: "X-Config".to_string(),
                description: None,
                is_secret: false,
                value_template: Some("${PLUGIN_ROOT}/hdr".to_string()),
            }],
            tags: Vec::new(),
            project_ids: Vec::new(),
            plugin_id: Some("plug-1".to_string()),
            created_at: 1,
            updated_at: 1,
            provenance: Default::default(),
        };

        let rewritten =
            rewrite_mcp_record_for_remote_plugin(&rec, &local_plugin_dir, remote_plugin_dir);
        assert_eq!(
            rewritten.command.as_deref(),
            Some("/home/cage/.claude/plugins/local/devops-suite/bin/run-mcp")
        );
        assert_eq!(
            rewritten.args,
            vec![
                "/home/cage/.claude/plugins/local/devops-suite/dist/server.js".to_string(),
                "/home/cage/.claude/plugins/local/devops-suite/config.yaml".to_string(),
            ]
        );
        assert_eq!(
            rewritten.env_specs[0].default_value.as_deref(),
            Some("/home/cage/.claude/plugins/local/devops-suite/assets")
        );
        assert_eq!(
            rewritten.header_specs[0].value_template.as_deref(),
            Some("/home/cage/.claude/plugins/local/devops-suite/hdr")
        );

        let mut secrets_map = HashMap::new();
        secrets_map.insert("SENTRY_TOKEN".to_string(), "sntrys_live_secret".to_string());

        // 1. `inject_secrets == false` -> MUST emit `${SENTRY_TOKEN}` placeholder, NEVER plaintext!
        let mut warns = Vec::new();
        let entry_placeholder =
            mcp::build_json_entry_for_app(&rewritten, &secrets_map, "claude", true, &mut warns)
                .unwrap();
        let merged_json = merge_json_mcp_content(
            Some(r#"{"oauthAccount":"preserve-me","mcpServers":{"existing":{"command":"uvx"}}}"#),
            "mcpServers",
            &rewritten.slug,
            entry_placeholder,
        )
        .unwrap();
        assert!(!merged_json.contains("sntrys_live_secret"));
        assert!(merged_json.contains("${SENTRY_TOKEN}"));
        assert!(merged_json.contains("preserve-me"));

        // 2. `inject_secrets == true` -> injects real secret into trusted VM config & preserves TOML comments
        let merged_toml = merge_toml_mcp_content(
            Some("# Preserve top comment\nmodel = \"o3\"\n"),
            "mcp_servers",
            &rewritten,
            &secrets_map,
            false,
            &mut warns,
        )
        .unwrap();
        assert!(merged_toml.contains("# Preserve top comment"));
        assert!(merged_toml.contains("sntrys_live_secret"));

        let after_remove = remove_toml_mcp_content(&merged_toml, "mcp_servers", &rewritten.slug)
            .unwrap()
            .expect("entry should be removed");
        assert!(after_remove.contains("# Preserve top comment"));
        assert!(!after_remove.contains("sntrys_live_secret"));
    }
}
