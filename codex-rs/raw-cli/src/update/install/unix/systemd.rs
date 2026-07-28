//! Exact-unit systemd lifecycle and local health verification.

use std::fs;
use std::fs::File;
use std::io::Read;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Output;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use tokio::process::Command;

pub(super) const SYSTEMD_UNIT: &str = "codex-raw-api.service";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CMDLINE_BYTES: u64 = 64 * 1024;

#[derive(Debug)]
pub(super) struct ManagedService {
    health_url: String,
}

#[cfg(target_os = "linux")]
pub(super) async fn matching_systemd_service(target: &Path) -> Result<Option<ManagedService>> {
    let active = match systemctl_output(&["is-active", "--quiet"]).await {
        Ok(output) => output.status.success(),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if !active {
        return Ok(None);
    }
    let pid = systemd_main_pid().await?;
    verify_process_executable(pid, target)?;
    Ok(Some(ManagedService {
        health_url: health_url_from_cmdline(pid)?,
    }))
}

#[cfg(not(target_os = "linux"))]
pub(super) async fn matching_systemd_service(_target: &Path) -> Result<Option<ManagedService>> {
    Ok(None)
}

#[cfg(target_os = "linux")]
async fn systemd_main_pid() -> Result<u32> {
    let output = systemctl_output(&["show", "--property=MainPID", "--value"]).await?;
    ensure!(output.status.success(), "systemctl could not read MainPID");
    let pid = String::from_utf8(output.stdout)?.trim().parse()?;
    ensure!(pid != 0, "active {SYSTEMD_UNIT} has no MainPID");
    Ok(pid)
}

#[cfg(target_os = "linux")]
fn verify_process_executable(pid: u32, expected: &Path) -> Result<()> {
    let actual = fs::canonicalize(format!("/proc/{pid}/exe"))
        .context("cannot resolve the service executable")?;
    ensure!(
        actual == fs::canonicalize(expected)?,
        "refusing to manage {SYSTEMD_UNIT} because MainPID runs a different executable"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn health_url_from_cmdline(pid: u32) -> Result<String> {
    let mut file = File::open(format!("/proc/{pid}/cmdline"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_CMDLINE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_CMDLINE_BYTES,
        "service command line is too large"
    );
    health_url_from_cmdline_bytes(&bytes)
}

fn health_url_from_cmdline_bytes(bytes: &[u8]) -> Result<String> {
    let args = bytes
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();
    ensure!(
        args.iter().any(|arg| *arg == b"api-server"),
        "{SYSTEMD_UNIT} is not running the api-server subcommand"
    );
    let mut listen = None;
    for (index, arg) in args.iter().enumerate() {
        if *arg == b"--listen" {
            listen = args.get(index + 1).copied();
        } else if let Some(value) = arg.strip_prefix(b"--listen=") {
            listen = Some(value);
        }
    }
    let listen = std::str::from_utf8(listen.unwrap_or(b"127.0.0.1:8080"))
        .context("service --listen is not UTF-8")?;
    let mut address: SocketAddr = listen.parse().context("invalid service --listen address")?;
    if address.ip().is_unspecified() {
        address.set_ip(match address.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        });
    }
    Ok(format!("http://{address}/healthz"))
}

#[cfg(target_os = "linux")]
async fn systemctl_output(arguments: &[&str]) -> Result<Output> {
    let mut command = Command::new("systemctl");
    command
        .arg("--no-ask-password")
        .args(arguments)
        .arg(SYSTEMD_UNIT)
        .kill_on_drop(true);
    tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .context("systemctl timed out")?
        .context("failed to execute systemctl")
}

#[cfg(target_os = "linux")]
pub(super) async fn stop_service_bounded() -> Result<()> {
    let output = systemctl_output(&["--no-block", "stop"]).await?;
    ensure!(
        output.status.success(),
        "systemctl stop {SYSTEMD_UNIT} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if service_is_fully_stopped().await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    bail!("{SYSTEMD_UNIT} did not stop within the bounded graceful-shutdown window")
}

#[cfg(target_os = "linux")]
async fn service_is_fully_stopped() -> Result<bool> {
    let output = systemctl_output(&[
        "show",
        "--property=ActiveState",
        "--property=SubState",
        "--property=MainPID",
    ])
    .await?;
    ensure!(
        output.status.success(),
        "systemctl could not inspect the service stop state"
    );
    let fields = String::from_utf8(output.stdout)?;
    parse_fully_stopped(&fields)
}

fn parse_fully_stopped(fields: &str) -> Result<bool> {
    let mut active_state = None;
    let mut sub_state = None;
    let mut main_pid = None;
    for line in fields.lines() {
        if let Some(value) = line.strip_prefix("ActiveState=") {
            active_state = Some(value);
        } else if let Some(value) = line.strip_prefix("SubState=") {
            sub_state = Some(value);
        } else if let Some(value) = line.strip_prefix("MainPID=") {
            main_pid = Some(value.parse::<u32>()?);
        }
    }
    let active_state = active_state.context("systemctl omitted ActiveState")?;
    let sub_state = sub_state.context("systemctl omitted SubState")?;
    let main_pid = main_pid.context("systemctl omitted MainPID")?;
    Ok(matches!(active_state, "inactive" | "failed")
        && matches!(sub_state, "dead" | "failed")
        && main_pid == 0)
}

#[cfg(not(target_os = "linux"))]
pub(super) async fn stop_service_bounded() -> Result<()> {
    Ok(())
}

pub(super) async fn stop_with_recovery(expected: &Path, service: &ManagedService) -> Result<()> {
    if let Err(stop_error) = stop_service_bounded().await {
        let recovery = start_and_verify_service(expected, service).await;
        bail!(
            "could not stop {SYSTEMD_UNIT} safely ({stop_error:#}); old service recovery: {}",
            result_label(&recovery)
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) async fn start_and_verify_service(
    expected: &Path,
    service: &ManagedService,
) -> Result<()> {
    let output = systemctl_output(&["start"]).await?;
    ensure!(
        output.status.success(),
        "systemctl start {SYSTEMD_UNIT} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let health_client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(2))
        .build()?;
    let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
    let mut successful_checks = 0_u8;
    while tokio::time::Instant::now() < deadline {
        let active = systemctl_output(&["is-active", "--quiet"])
            .await
            .is_ok_and(|output| output.status.success());
        let process_matches = if active {
            systemd_main_pid()
                .await
                .and_then(|pid| verify_process_executable(pid, expected))
                .is_ok()
        } else {
            false
        };
        let health_ok = if process_matches {
            health_client
                .get(&service.health_url)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
        } else {
            false
        };
        successful_checks = if health_ok { successful_checks + 1 } else { 0 };
        if successful_checks == 2 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    bail!("{SYSTEMD_UNIT} did not remain active on the expected binary with a healthy /healthz")
}

#[cfg(not(target_os = "linux"))]
pub(super) async fn start_and_verify_service(
    _expected: &Path,
    _service: &ManagedService,
) -> Result<()> {
    Ok(())
}

fn result_label(result: &Result<()>) -> String {
    match result {
        Ok(()) => "succeeded".to_string(),
        Err(error) => format!("failed ({error:#})"),
    }
}

#[cfg(test)]
#[path = "systemd_tests.rs"]
mod tests;
