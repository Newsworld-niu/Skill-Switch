use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Upper bound for any single git invocation (network ops included).
const GIT_TIMEOUT: Duration = Duration::from_secs(120);
/// Full clones of large repos can legitimately take longer.
const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(600);

/// A `git` command that never waits for interactive input: there is no terminal
/// in a GUI app, so a credential prompt would otherwise hang until killed.
fn git_command() -> Command {
    let mut command = Command::new("git");
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::null());
    command
}

/// Runs the command, killing it if it does not finish within `timeout`.
fn output_with_timeout(command: &mut Command, timeout: Duration) -> Result<Output, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动 git：{}", error))?;

    let read_pipe = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut buffer);
            }
            buffer
        })
    };
    let stdout = read_pipe(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = read_pipe(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "git 操作超时（{} 秒），请检查网络或凭证配置",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

fn run_git(path: Option<&Path>, args: &[&str]) -> Result<Output, String> {
    let mut command = git_command();
    if let Some(path) = path {
        command.arg("-C").arg(path);
    }
    command.args(args);
    output_with_timeout(&mut command, GIT_TIMEOUT)
}

pub fn git_available() -> bool {
    run_git(None, &["--version"])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn is_git_repo(path: &Path) -> bool {
    git_root(path).is_some()
}

pub fn git_root(path: &Path) -> Option<PathBuf> {
    let output = run_git(Some(path), &["rev-parse", "--show-toplevel"]).ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(PathBuf::from(stdout))
    }
}

pub fn branch(path: &Path) -> Option<String> {
    let output = run_git(Some(path), &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!stdout.is_empty()).then_some(stdout)
}

pub fn dirty(path: &Path) -> bool {
    let output = run_git(Some(path), &["status", "--porcelain"]);

    match output {
        Ok(output) if output.status.success() => {
            !String::from_utf8_lossy(&output.stdout).trim().is_empty()
        }
        _ => false,
    }
}

pub fn status_porcelain(path: &Path) -> Result<String, String> {
    let output = run_git(Some(path), &["status", "--porcelain"])?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn add_all(path: &Path) -> Result<(), String> {
    let output = run_git(Some(path), &["add", "-A"])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn commit(path: &Path, message: &str) -> Result<bool, String> {
    let status = status_porcelain(path)?;
    if status.trim().is_empty() {
        return Ok(false);
    }

    let mut command = git_command();
    command
        .arg("-C")
        .arg(path)
        .args(["commit", "-m", message])
        .env("GIT_AUTHOR_NAME", "SkillSwitch")
        .env("GIT_AUTHOR_EMAIL", "skill-switch@localhost")
        .env("GIT_COMMITTER_NAME", "SkillSwitch")
        .env("GIT_COMMITTER_EMAIL", "skill-switch@localhost");
    let output = output_with_timeout(&mut command, GIT_TIMEOUT)?;

    if output.status.success() {
        Ok(true)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn init_repository(path: &Path) -> Result<(), String> {
    let output = run_git(Some(path), &["init"])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn remote_url(path: &Path, remote: &str) -> Result<Option<String>, String> {
    let output = run_git(Some(path), &["remote", "get-url", remote])?;
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!stdout.is_empty()).then_some(stdout))
}

pub fn add_remote(path: &Path, remote: &str, url: &str) -> Result<(), String> {
    let output = run_git(Some(path), &["remote", "add", remote, url])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn set_remote_url(path: &Path, remote: &str, url: &str) -> Result<(), String> {
    let output = run_git(Some(path), &["remote", "set-url", remote, url])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn configure_identity(path: &Path) -> Result<(), String> {
    let name = run_git(Some(path), &["config", "user.name", "SkillSwitch"])?;
    if !name.status.success() {
        return Err(String::from_utf8_lossy(&name.stderr).trim().to_string());
    }

    let email = run_git(
        Some(path),
        &["config", "user.email", "skill-switch@localhost"],
    )?;
    if !email.status.success() {
        return Err(String::from_utf8_lossy(&email.stderr).trim().to_string());
    }

    Ok(())
}

pub fn checkout_branch(path: &Path, branch: &str) -> Result<(), String> {
    let output = run_git(Some(path), &["checkout", "-B", branch])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn push_branch(path: &Path, remote: &str, branch: &str) -> Result<(), String> {
    let output = run_git(Some(path), &["push", "--set-upstream", remote, branch])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn remote_branch_exists(remote_url: &str, branch: &str) -> Result<bool, String> {
    let mut command = git_command();
    command.args(["ls-remote", "--heads", remote_url, branch]);
    let output = output_with_timeout(&mut command, GIT_TIMEOUT)?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

pub fn tracked(project_path: &Path, absolute_path: &Path) -> bool {
    let Some(root) = git_root(project_path) else {
        return false;
    };

    let Ok(relative) = absolute_path.strip_prefix(&root) else {
        return false;
    };

    let mut command = git_command();
    command
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "--error-unmatch", "--"])
        .arg(relative);
    let output = output_with_timeout(&mut command, GIT_TIMEOUT);

    matches!(output, Ok(output) if output.status.success())
}

pub fn head(path: &Path) -> Option<String> {
    let output = run_git(Some(path), &["rev-parse", "HEAD"]).ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!stdout.is_empty()).then_some(stdout)
}

pub fn ahead_behind(path: &Path) -> (usize, usize) {
    let output = run_git(
        Some(path),
        &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
    );

    let Ok(output) = output else {
        return (0, 0);
    };

    if !output.status.success() {
        return (0, 0);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    let behind = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let ahead = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    (ahead, behind)
}

pub fn clone_repository(
    remote_url: &str,
    destination: &Path,
    branch: Option<&str>,
) -> Result<(), String> {
    let mut command = git_command();
    command.arg("clone");
    if let Some(branch) = branch {
        command.arg("--branch").arg(branch);
    }
    command.arg(remote_url).arg(destination);

    let output = output_with_timeout(&mut command, GIT_CLONE_TIMEOUT)?;
    if output.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

pub fn clone_repository_shallow(
    remote_url: &str,
    destination: &Path,
    branch: Option<&str>,
) -> Result<(), String> {
    let mut command = git_command();
    command.args(["clone", "--depth", "1"]);
    if let Some(branch) = branch {
        command.arg("--branch").arg(branch);
    }
    command.arg(remote_url).arg(destination);

    let output = output_with_timeout(&mut command, GIT_CLONE_TIMEOUT)?;
    if output.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

pub fn pull(path: &Path) -> Result<(), String> {
    let output = run_git(Some(path), &["pull"])?;
    if output.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

pub fn push(path: &Path) -> Result<(), String> {
    let output = run_git(Some(path), &["push"])?;
    if output.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

pub fn ls_remote_head(remote_url: &str, reference: Option<&str>) -> Result<String, String> {
    let mut command = git_command();
    command.arg("ls-remote").arg(remote_url);
    if let Some(reference) = reference {
        command.arg(reference);
    }

    let output = output_with_timeout(&mut command, GIT_TIMEOUT)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let head = stdout
        .lines()
        .find_map(|line| line.split_whitespace().next())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "remote head not found".to_string())?;
    Ok(head.to_string())
}

pub fn read_file_at_revision(
    repo_path: &Path,
    revision: &str,
    source_path: &str,
) -> Result<String, String> {
    let spec = format!("{revision}:{source_path}");
    let output = run_git(Some(repo_path), &["show", &spec])?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn merge_file(base: &Path, local: &Path, remote: &Path) -> Result<(String, bool), String> {
    let mut command = git_command();
    command
        .args(["merge-file", "--stdout"])
        .arg(local)
        .arg(base)
        .arg(remote);
    let output = output_with_timeout(&mut command, GIT_TIMEOUT)?;

    let merged = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok((merged, true));
    }

    if output.status.code() == Some(1) {
        return Ok((merged, false));
    }

    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_git_captures_output() {
        let output = run_git(None, &["--version"]).unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("git version"));
    }

    #[cfg(unix)]
    #[test]
    fn output_with_timeout_kills_hung_process() {
        let mut command = Command::new("sleep");
        command.arg("5");
        let started = Instant::now();
        let result = output_with_timeout(&mut command, Duration::from_millis(200));
        assert!(result.unwrap_err().contains("超时"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
