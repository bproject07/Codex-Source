use std::env;
use std::ffi::OsStr;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use semver::Version;

use super::super::Installation;
use super::super::OwnedStaged;
use super::super::unique_sidecar;
use super::WINDOWS_APPLY_SCRIPT;
use super::queued_lock::QUEUED_LOCK_SOURCE;
use super::restart_plan::RESTART_PLAN_SOURCE;

const HANDSHAKE_WAIT: Duration = Duration::from_secs(30);
const SAFE_HELPER_ENVIRONMENT: [&str; 4] = ["SystemRoot", "TEMP", "TMP", "WINDIR"];

#[derive(Debug)]
pub(crate) struct WindowsHandoff {
    child: Child,
    marker: OwnedHandoffMarker,
    owner_ready: PathBuf,
    status: PathBuf,
    target: PathBuf,
    new_version: Version,
}

impl WindowsHandoff {
    pub(crate) async fn confirm(mut self, staged: &mut OwnedStaged) -> Result<()> {
        if let Err(error) = wait_for_signal(
            &mut self.child,
            &self.owner_ready,
            self.marker.token(),
            &self.status,
        )
        .await
        {
            stop_helper(&mut self.child);
            return Err(error)
                .context("detached Windows helper did not prove ownership of the updater lock");
        }
        let _ = fs::remove_file(&self.owner_ready);
        self.marker.preserve();
        staged.preserve();

        println!(
            "Verified codex-raw {}; the detached helper owns the update lock and will replace {} after this process exits.",
            self.new_version,
            self.target.display()
        );
        println!(
            "Restart Manager will request graceful shutdown only for validated processes running this exact executable, then restart eligible registered processes or services; no process is force-killed."
        );
        println!(
            "The helper uses bounded waits. On failure, details are written to {} and replacement is rolled back when necessary.",
            self.status.display()
        );
        Ok(())
    }
}

pub(crate) async fn begin(
    installation: &Installation,
    staged: &OwnedStaged,
    current_version: &Version,
    new_version: &Version,
) -> Result<WindowsHandoff> {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    installation.ensure_no_handoff()?;
    let token = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let marker = OwnedHandoffMarker::create(installation.handoff_path(), token)?;
    let backup = unique_sidecar(installation.target(), "backup", current_version)?;
    let waiter_ready = unique_sidecar(installation.target(), "handoff-waiter", new_version)?;
    let owner_ready = unique_sidecar(installation.target(), "handoff-owner", new_version)?;
    let status = unique_sidecar(installation.target(), "update-status", new_version)?;
    let powershell = system_powershell()?;
    let mut command = Command::new(powershell);
    configure_minimal_environment(&mut command);
    let mut child = command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            WINDOWS_APPLY_SCRIPT,
        ])
        .env("CODEX_RAW_UPDATE_TARGET", installation.target())
        .env("CODEX_RAW_UPDATE_STAGED", staged.path())
        .env("CODEX_RAW_UPDATE_BACKUP", &backup)
        .env("CODEX_RAW_UPDATE_STATUS", &status)
        .env("CODEX_RAW_UPDATE_LOCK", installation.lock_path())
        .env("CODEX_RAW_UPDATE_MARKER", marker.path())
        .env("CODEX_RAW_UPDATE_WAITER_READY", &waiter_ready)
        .env("CODEX_RAW_UPDATE_OWNER_READY", &owner_ready)
        .env("CODEX_RAW_UPDATE_TOKEN", marker.token())
        .env("CODEX_RAW_UPDATE_VERSION", new_version.to_string())
        .env("CODEX_RAW_UPDATE_QUEUED_LOCK_SOURCE", QUEUED_LOCK_SOURCE)
        .env("CODEX_RAW_UPDATE_RESTART_PLAN_SOURCE", RESTART_PLAN_SOURCE)
        .env(
            "CODEX_RAW_UPDATE_PARENT_PID",
            std::process::id().to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .context("failed to start the detached Windows update helper")?;

    if let Err(error) = wait_for_signal(&mut child, &waiter_ready, marker.token(), &status).await {
        stop_helper(&mut child);
        return Err(error)
            .context("detached Windows helper did not open the shared updater lock handle");
    }
    let _ = fs::remove_file(waiter_ready);
    Ok(WindowsHandoff {
        child,
        marker,
        owner_ready,
        status,
        target: installation.target().to_path_buf(),
        new_version: new_version.clone(),
    })
}

async fn wait_for_signal(
    child: &mut Child,
    signal: &Path,
    token: &str,
    status: &Path,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + HANDSHAKE_WAIT;
    loop {
        if signal.exists() {
            let actual = fs::read_to_string(signal)
                .context("failed to read Windows helper ownership signal")?;
            ensure!(
                actual == token,
                "Windows helper ownership signal token mismatch"
            );
            return Ok(());
        }
        if let Some(exit) = child.try_wait()? {
            bail!(
                "Windows update helper exited before handoff ({exit}): {}",
                helper_status(status)
            );
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Windows update helper handoff timed out after {} seconds",
                HANDSHAKE_WAIT.as_secs()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn stop_helper(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn helper_status(path: &Path) -> String {
    let Ok(metadata) = path.metadata() else {
        return "no helper status was written".to_string();
    };
    if metadata.len() > 64 * 1024 {
        return "helper status exceeded the safe display limit".to_string();
    }
    fs::read_to_string(path).unwrap_or_else(|_| "helper status could not be read".to_string())
}

fn system_powershell() -> Result<PathBuf> {
    let root = env::var_os("SystemRoot").context("SystemRoot is not set")?;
    let path = PathBuf::from(root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    ensure!(
        path.is_file(),
        "trusted system PowerShell was not found at {}",
        path.display()
    );
    Ok(path)
}

fn configure_minimal_environment(command: &mut Command) {
    let values = env::vars_os()
        .filter(|(name, _)| is_safe_helper_environment(name))
        .collect::<Vec<_>>();
    command.env_clear();
    command.envs(values);
}

fn is_safe_helper_environment(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    SAFE_HELPER_ENVIRONMENT
        .iter()
        .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

#[derive(Debug)]
struct OwnedHandoffMarker {
    path: PathBuf,
    token: String,
    remove_on_drop: bool,
}

impl OwnedHandoffMarker {
    fn create(path: PathBuf, token: String) -> Result<Self> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .context("another Windows update handoff is already active")?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
        Ok(Self {
            path,
            token,
            remove_on_drop: true,
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn token(&self) -> &str {
        &self.token
    }

    fn preserve(&mut self) {
        self.remove_on_drop = false;
    }
}

impl Drop for OwnedHandoffMarker {
    fn drop(&mut self) {
        let owns_marker = fs::read_to_string(&self.path).is_ok_and(|actual| actual == self.token);
        if self.remove_on_drop && owns_marker {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;

    use tempfile::tempdir;

    use super::OwnedHandoffMarker;
    use super::is_safe_helper_environment;

    #[test]
    fn helper_environment_allowlist_excludes_auth_secrets() {
        assert!(is_safe_helper_environment(OsStr::new("SystemRoot")));
        assert!(is_safe_helper_environment(OsStr::new("systemroot")));
        assert!(!is_safe_helper_environment(OsStr::new(
            "CODEX_RAW_API_TOKEN"
        )));
        assert!(!is_safe_helper_environment(OsStr::new("OPENAI_API_KEY")));
        assert!(!is_safe_helper_environment(OsStr::new("GITHUB_TOKEN")));
        assert!(!is_safe_helper_environment(OsStr::new("PSModulePath")));
        assert!(!is_safe_helper_environment(OsStr::new("PATH")));
    }

    #[test]
    fn marker_drop_never_removes_another_handoff_token() {
        let directory = tempdir().expect("temporary handoff directory");
        let path = directory.path().join("handoff");
        let marker =
            OwnedHandoffMarker::create(path.clone(), "ours".to_string()).expect("create marker");
        fs::write(&path, "another").expect("replace marker token");

        drop(marker);

        assert_eq!(fs::read_to_string(path).expect("marker remains"), "another");
    }
}
