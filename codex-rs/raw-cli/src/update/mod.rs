use std::env;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use reqwest::Client;
use reqwest::Response;
use reqwest::StatusCode;
use reqwest::Url;
use semver::Version;
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;
use tempfile::Builder;
use tempfile::NamedTempFile;
use tokio::process::Command;

mod archive;
mod install;

const RELEASE_API_URL: &str =
    "https://api.github.com/repos/bproject07/Codex-Source/releases/latest";
const RELEASE_DOWNLOAD_ROOT: &str = "https://github.com/bproject07/Codex-Source/releases/download";
const GITHUB_API_VERSION: &str = "2026-03-10";
const TAG_PREFIX: &str = "codex-raw-v";
const CHECKSUM_ASSET: &str = "SHA256SUMS";
const MAX_METADATA_BYTES: u64 = 2 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UpdateMode {
    CheckOnly,
    Install,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    immutable: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Debug)]
struct SelectedRelease {
    version: Version,
    archive: GitHubAsset,
    checksums: GitHubAsset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReleasePlatform {
    target: &'static str,
    archive_kind: ArchiveKind,
    executable_name: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UpdateAvailability {
    Older,
    Current,
    Newer,
}

pub(crate) async fn run(mode: UpdateMode) -> Result<()> {
    let current_version =
        Version::parse(env!("CARGO_PKG_VERSION")).context("invalid built-in codex-raw version")?;
    let platform = release_platform()?;
    let client = github_client()?;
    let release = fetch_latest_release(&client, platform).await?;

    match availability(&current_version, &release.version) {
        UpdateAvailability::Older => {
            bail!(
                "latest stable release {} is older than this build {}; refusing to downgrade",
                release.version,
                current_version
            );
        }
        UpdateAvailability::Current => {
            println!("codex-raw {current_version} is up to date.");
            return Ok(());
        }
        UpdateAvailability::Newer => {}
    }
    println!(
        "Update available: codex-raw {current_version} -> {}",
        release.version
    );
    if mode == UpdateMode::CheckOnly {
        return Ok(());
    }

    let installation = install::Installation::discover()?;
    let mut update_lock = installation.open_update_lock()?;
    let _update_guard = update_lock.try_write().map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            anyhow::anyhow!("another codex-raw update is already in progress")
        } else {
            anyhow::Error::new(error).context("failed to lock the codex-raw installation")
        }
    })?;
    installation.ensure_no_handoff()?;
    verify_binary_version(installation.target(), &current_version)
        .await
        .context("installed codex-raw changed while the update lock was acquired; rerun update")?;
    let staging_dir = installation.staging_dir();

    let mut checksum_file =
        download_to_temp(&client, staging_dir, &release.checksums, MAX_CHECKSUM_BYTES).await?;
    checksum_file.as_file_mut().rewind()?;
    let mut checksum_text = String::new();
    checksum_file
        .as_file_mut()
        .read_to_string(&mut checksum_text)
        .context("SHA256SUMS is not valid UTF-8 text")?;
    let expected_archive_digest = archive::checksum_for(&checksum_text, &release.archive.name)?;

    let mut archive_file =
        download_to_temp(&client, staging_dir, &release.archive, MAX_ARCHIVE_BYTES).await?;
    let actual_archive_digest = archive::sha256_file(archive_file.as_file_mut())?;
    ensure!(
        actual_archive_digest == expected_archive_digest,
        "archive checksum does not match the exact SHA256SUMS entry"
    );

    let mut staged = Builder::new()
        .prefix(&format!(".{}.update-", platform.executable_name))
        .tempfile_in(staging_dir)
        .with_context(|| installation.permission_hint("cannot create an update staging file"))?;
    archive::extract_binary(
        archive_file.as_file_mut(),
        staged.as_file_mut(),
        platform,
        &release.version,
    )?;
    installation.prepare_staged(staged.as_file())?;
    staged.as_file().sync_all()?;
    let (_, staged_path) = staged
        .keep()
        .context("failed to preserve the verified staged executable")?;
    let mut staged = install::OwnedStaged::new(staged_path);
    verify_binary_version(staged.path(), &release.version).await?;

    #[cfg(windows)]
    {
        let handoff = install::begin_windows_install(
            &installation,
            &staged,
            &current_version,
            &release.version,
        )
        .await?;
        drop(_update_guard);
        drop(update_lock);
        return install::confirm_windows_install(handoff, &mut staged).await;
    }

    #[cfg(not(windows))]
    install::install(
        &installation,
        &mut staged,
        &current_version,
        &release.version,
    )
    .await
}

fn availability(current: &Version, latest: &Version) -> UpdateAvailability {
    match latest.cmp_precedence(current) {
        std::cmp::Ordering::Less => UpdateAvailability::Older,
        std::cmp::Ordering::Equal => UpdateAvailability::Current,
        std::cmp::Ordering::Greater => UpdateAvailability::Newer,
    }
}

async fn fetch_latest_release(
    client: &Client,
    platform: ReleasePlatform,
) -> Result<SelectedRelease> {
    let response = client
        .get(RELEASE_API_URL)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .send()
        .await
        .context("failed to query the exact Codex Raw GitHub repository")?;
    let response = accepted_github_response(response, "latest Codex Raw release")?;
    let bytes = read_bounded_response(response, MAX_METADATA_BYTES).await?;
    let release: GitHubRelease =
        serde_json::from_slice(&bytes).context("invalid GitHub release metadata")?;
    select_release(release, platform)
}

fn select_release(release: GitHubRelease, platform: ReleasePlatform) -> Result<SelectedRelease> {
    ensure!(!release.draft, "refusing a draft release");
    ensure!(!release.prerelease, "refusing a prerelease");
    ensure!(
        release.immutable,
        "refusing a release that GitHub does not mark immutable"
    );
    let version_text = release
        .tag_name
        .strip_prefix(TAG_PREFIX)
        .context("release tag does not use the codex-raw-v prefix")?;
    let version = Version::parse(version_text).context("release tag is not valid SemVer")?;
    ensure!(
        version.pre.is_empty(),
        "stable GitHub release has a prerelease SemVer tag"
    );
    ensure!(
        release.tag_name == format!("{TAG_PREFIX}{version}"),
        "release tag is not in canonical codex-raw-v<semver> form"
    );

    let archive_name = archive_name(&version, platform);
    let archive = select_asset(&release.assets, &release.tag_name, &archive_name)?;
    let checksums = select_asset(&release.assets, &release.tag_name, CHECKSUM_ASSET)?;
    ensure!(
        archive.size > 0 && archive.size <= MAX_ARCHIVE_BYTES,
        "release archive has an invalid size"
    );
    ensure!(
        checksums.size > 0 && checksums.size <= MAX_CHECKSUM_BYTES,
        "SHA256SUMS has an invalid size"
    );
    Ok(SelectedRelease {
        version,
        archive,
        checksums,
    })
}

fn select_asset(assets: &[GitHubAsset], tag: &str, name: &str) -> Result<GitHubAsset> {
    let matching = assets
        .iter()
        .filter(|asset| asset.name == name)
        .collect::<Vec<_>>();
    ensure!(
        matching.len() == 1,
        "release must contain exactly one asset named {name}"
    );
    let asset = matching[0];
    let expected_url = Url::parse(&format!("{RELEASE_DOWNLOAD_ROOT}/{tag}/{name}"))?;
    let actual_url = Url::parse(&asset.browser_download_url)
        .with_context(|| format!("asset {name} has an invalid download URL"))?;
    ensure!(
        actual_url == expected_url,
        "asset {name} does not use the exact Codex Raw GitHub release URL"
    );
    github_asset_digest(asset)?;
    Ok(asset.clone())
}

fn github_asset_digest(asset: &GitHubAsset) -> Result<[u8; 32]> {
    let digest = asset
        .digest
        .as_deref()
        .with_context(|| format!("GitHub did not provide a digest for asset {}", asset.name))?;
    let hex = digest
        .strip_prefix("sha256:")
        .with_context(|| format!("asset {} does not have a SHA-256 digest", asset.name))?;
    archive::parse_sha256(hex)
        .with_context(|| format!("invalid GitHub digest for asset {}", asset.name))
}

fn verify_asset_digest(asset: &GitHubAsset, actual: [u8; 32]) -> Result<()> {
    ensure!(
        actual == github_asset_digest(asset)?,
        "release asset {} does not match its GitHub digest",
        asset.name
    );
    Ok(())
}

fn github_client() -> Result<Client> {
    let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 {
            attempt.error("too many GitHub redirects")
        } else if allowed_github_url(attempt.url()) {
            attempt.follow()
        } else {
            attempt.error("GitHub redirected to a non-GitHub HTTPS host")
        }
    });
    Client::builder()
        .user_agent(format!("codex-raw/{}", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .redirect(redirect_policy)
        .build()
        .context("failed to construct the update HTTP client")
}

fn allowed_github_url(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    matches!(
        url.host_str(),
        Some(
            "api.github.com"
                | "github.com"
                | "objects.githubusercontent.com"
                | "release-assets.githubusercontent.com"
                | "github-releases.githubusercontent.com"
        )
    )
}

fn accepted_github_response(response: Response, subject: &str) -> Result<Response> {
    let status = response.status();
    if matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
    ) {
        bail!("GitHub rate-limited the {subject} request (HTTP {status}); wait and try again");
    }
    response
        .error_for_status()
        .with_context(|| format!("GitHub rejected the {subject} request"))
}

async fn download_to_temp(
    client: &Client,
    parent: &Path,
    asset: &GitHubAsset,
    max_bytes: u64,
) -> Result<NamedTempFile> {
    let mut temp = Builder::new()
        .prefix(".codex-raw-download-")
        .tempfile_in(parent)
        .context("failed to create a temporary download file")?;
    let response = client
        .get(Url::parse(&asset.browser_download_url)?)
        .header("Accept", "application/octet-stream")
        .send()
        .await
        .with_context(|| format!("failed to download release asset {}", asset.name))?;
    let mut response = accepted_github_response(response, &format!("asset {}", asset.name))?;
    if let Some(length) = response.content_length() {
        ensure!(
            length <= max_bytes && length == asset.size,
            "release asset {} has an unexpected content length",
            asset.name
        );
    }

    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("failed while downloading {}", asset.name))?
    {
        written = written
            .checked_add(chunk.len() as u64)
            .context("release asset size overflow")?;
        ensure!(
            written <= max_bytes,
            "release asset {} exceeds the size limit",
            asset.name
        );
        temp.as_file_mut().write_all(&chunk)?;
        hasher.update(&chunk);
    }
    ensure!(
        written == asset.size,
        "release asset {} size differs from GitHub metadata",
        asset.name
    );
    temp.as_file_mut().flush()?;
    temp.as_file().sync_all()?;
    let actual: [u8; 32] = hasher.finalize().into();
    verify_asset_digest(asset, actual)?;
    Ok(temp)
}

async fn read_bounded_response(mut response: Response, limit: u64) -> Result<Vec<u8>> {
    if let Some(length) = response.content_length() {
        ensure!(length <= limit, "GitHub release metadata is too large");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= limit as usize,
            "GitHub release metadata is too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn verify_binary_version(path: &Path, version: &Version) -> Result<()> {
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .context("timed out while checking the staged codex-raw version")?
        .context("failed to run the staged codex-raw binary")?;
    ensure!(
        output.status.success(),
        "staged codex-raw failed its version check"
    );
    let stdout = String::from_utf8(output.stdout).context("invalid staged version output")?;
    ensure!(
        stdout.trim() == format!("codex-raw {version}"),
        "staged binary version does not match release metadata"
    );
    Ok(())
}

fn release_platform() -> Result<ReleasePlatform> {
    match (env::consts::OS, env::consts::ARCH) {
        ("windows", "x86_64") => Ok(ReleasePlatform {
            target: "x86_64-pc-windows-msvc",
            archive_kind: ArchiveKind::Zip,
            executable_name: "codex-raw.exe",
        }),
        ("linux", "x86_64") => Ok(ReleasePlatform {
            target: "x86_64-unknown-linux-gnu",
            archive_kind: ArchiveKind::TarGz,
            executable_name: "codex-raw",
        }),
        ("linux", "aarch64") => Ok(ReleasePlatform {
            target: "aarch64-unknown-linux-gnu",
            archive_kind: ArchiveKind::TarGz,
            executable_name: "codex-raw",
        }),
        ("macos", "x86_64") => Ok(ReleasePlatform {
            target: "x86_64-apple-darwin",
            archive_kind: ArchiveKind::TarGz,
            executable_name: "codex-raw",
        }),
        (os, arch) => bail!("self-update is not supported on {os}/{arch}"),
    }
}

fn archive_name(version: &Version, platform: ReleasePlatform) -> String {
    let extension = match platform.archive_kind {
        ArchiveKind::TarGz => "tar.gz",
        ArchiveKind::Zip => "zip",
    };
    format!("codex-raw-{version}-{}.{extension}", platform.target)
}

#[cfg(test)]
mod tests;
