use std::env;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use fd_lock::RwLock;
use semver::Version;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[derive(Debug)]
pub(super) struct Installation {
    target: PathBuf,
    layout: InstallLayout,
}

#[derive(Debug)]
enum InstallLayout {
    Standalone,
    #[cfg(unix)]
    Managed(ManagedLayout),
}

#[cfg(unix)]
#[derive(Debug)]
struct ManagedLayout {
    root: PathBuf,
    releases: PathBuf,
    current_link: PathBuf,
    active_release: PathBuf,
}

#[derive(Debug)]
pub(super) struct OwnedStaged {
    path: PathBuf,
    remove_on_drop: bool,
}

impl Installation {
    pub(super) fn discover() -> Result<Self> {
        let target = fs::canonicalize(env::current_exe().context("cannot locate codex-raw")?)
            .context("cannot resolve the running codex-raw binary")?;
        let expected_name = if cfg!(windows) {
            "codex-raw.exe"
        } else {
            "codex-raw"
        };
        ensure!(
            target.file_name().and_then(|name| name.to_str()) == Some(expected_name),
            "self-update requires the installed binary to retain the {expected_name} filename"
        );
        ensure!(
            target.symlink_metadata()?.file_type().is_file(),
            "running codex-raw path is not a regular file"
        );
        #[cfg(unix)]
        let layout = detect_managed_layout(&target)?
            .map(InstallLayout::Managed)
            .unwrap_or(InstallLayout::Standalone);
        #[cfg(windows)]
        let layout = InstallLayout::Standalone;
        Ok(Self { target, layout })
    }

    #[expect(
        clippy::expect_used,
        reason = "a canonical current executable path is absolute and has a parent"
    )]
    pub(super) fn staging_dir(&self) -> &Path {
        self.target
            .parent()
            .expect("validated executable always has a parent")
    }

    fn lock_path(&self) -> PathBuf {
        match &self.layout {
            InstallLayout::Standalone => self.staging_dir().join(".codex-raw.update.lock"),
            #[cfg(unix)]
            InstallLayout::Managed(layout) => layout.root.join(".codex-raw.update.lock"),
        }
    }

    pub(super) fn open_update_lock(&self) -> Result<RwLock<File>> {
        self.ensure_no_handoff()?;
        let path = self.lock_path();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| self.permission_hint("cannot open the installation update lock"))?;
        Ok(RwLock::new(file))
    }

    pub(super) fn ensure_no_handoff(&self) -> Result<()> {
        #[cfg(windows)]
        ensure!(
            !self.handoff_path().exists(),
            "a Windows update ownership handoff is already active at {}; if no updater is running, inspect and remove this stale marker manually",
            self.handoff_path().display()
        );
        Ok(())
    }

    pub(super) fn target(&self) -> &Path {
        &self.target
    }

    #[cfg(windows)]
    fn handoff_path(&self) -> PathBuf {
        self.staging_dir().join(".codex-raw.update.handoff")
    }

    pub(super) fn prepare_staged(&self, staged: &File) -> Result<()> {
        ensure!(
            staged.metadata()?.file_type().is_file(),
            "staged update is not a regular file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            let target_metadata = self.target.metadata()?;
            staged.set_permissions(target_metadata.permissions())?;
            let staged_metadata = staged.metadata()?;
            ensure!(
                staged_metadata.uid() == target_metadata.uid()
                    && staged_metadata.gid() == target_metadata.gid(),
                "staged binary owner/group differs from the installed executable; use the installation owner"
            );
            ensure!(
                target_metadata.mode() & 0o111 != 0,
                "installed codex-raw is not executable"
            );
        }
        Ok(())
    }

    pub(super) fn permission_hint(&self, message: &str) -> String {
        if cfg!(target_os = "linux") {
            format!(
                "{message}: {}. For a root-owned installation, run `sudo codex-raw update`",
                self.target.display()
            )
        } else {
            format!("{message}: {}", self.target.display())
        }
    }
}

impl OwnedStaged {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            remove_on_drop: true,
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(windows)]
    fn preserve(&mut self) {
        self.remove_on_drop = false;
    }
}

impl Drop for OwnedStaged {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
pub(super) async fn install(
    installation: &Installation,
    staged: &mut OwnedStaged,
    current_version: &Version,
    new_version: &Version,
) -> Result<()> {
    unix::install(installation, staged, current_version, new_version).await
}

#[cfg(not(any(unix, windows)))]
pub(super) async fn install(
    installation: &Installation,
    staged: &mut OwnedStaged,
    current_version: &Version,
    new_version: &Version,
) -> Result<()> {
    let _ = (installation, staged, current_version, new_version);
    Err(anyhow::anyhow!(
        "self-update is not supported on this operating system"
    ))
}

#[cfg(windows)]
pub(super) async fn begin_windows_install(
    installation: &Installation,
    staged: &OwnedStaged,
    current_version: &Version,
    new_version: &Version,
) -> Result<windows::WindowsHandoff> {
    windows::begin(installation, staged, current_version, new_version).await
}

#[cfg(windows)]
pub(super) async fn confirm_windows_install(
    handoff: windows::WindowsHandoff,
    staged: &mut OwnedStaged,
) -> Result<()> {
    handoff.confirm(staged).await
}

#[cfg(unix)]
fn detect_managed_layout(target: &Path) -> Result<Option<ManagedLayout>> {
    let active_release = target.parent().context("installed binary has no parent")?;
    let releases = active_release
        .parent()
        .context("installed binary has no release parent")?;
    if releases.file_name().and_then(|name| name.to_str()) != Some("releases") {
        return Ok(None);
    }

    let root = releases.parent().context("managed releases has no root")?;
    let current_link = root.join("current");
    ensure!(
        releases.symlink_metadata()?.file_type().is_dir(),
        "managed releases path is not a real directory"
    );
    ensure!(
        current_link.symlink_metadata()?.file_type().is_symlink(),
        "binary is under releases/ but the managed current link is missing or unsafe"
    );
    ensure!(
        fs::canonicalize(&current_link)? == fs::canonicalize(active_release)?,
        "managed current link does not select the running release"
    );
    ensure!(
        target == active_release.join("codex-raw"),
        "managed release has an unexpected executable path"
    );
    Ok(Some(ManagedLayout {
        root: root.to_path_buf(),
        releases: releases.to_path_buf(),
        current_link,
        active_release: active_release.to_path_buf(),
    }))
}

pub(super) fn unique_sidecar(target: &Path, kind: &str, version: &Version) -> Result<PathBuf> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .context("installed executable name is not valid UTF-8")?;
    let path = target.with_file_name(format!(".{name}.{kind}-{version}-{nonce}"));
    ensure!(!path.exists(), "update sidecar path already exists");
    Ok(path)
}

#[cfg(all(test, unix))]
pub(super) fn discover_managed_for_test(target: &Path) -> Result<bool> {
    detect_managed_layout(target).map(|layout| layout.is_some())
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
