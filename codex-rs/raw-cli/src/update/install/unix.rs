//! Atomic Unix installation, managed-release switching, and systemd verification.

use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use semver::Version;

use super::super::verify_binary_version;
use super::InstallLayout;
use super::Installation;
use super::ManagedLayout;
use super::OwnedStaged;
use super::unique_sidecar;

mod systemd;
use systemd::ManagedService;
use systemd::SYSTEMD_UNIT;
use systemd::matching_systemd_service;
use systemd::start_and_verify_service;
use systemd::stop_service_bounded;
use systemd::stop_with_recovery;

#[derive(Debug)]
enum AppliedUpdate {
    Standalone {
        target: PathBuf,
        backup: PathBuf,
    },
    Managed {
        current_link: PathBuf,
        previous_link: PathBuf,
        new_release: PathBuf,
        new_target: PathBuf,
        root: PathBuf,
    },
}

pub(super) async fn install(
    installation: &Installation,
    staged: &mut OwnedStaged,
    current_version: &Version,
    new_version: &Version,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    let service = matching_systemd_service(&installation.target).await?;
    #[cfg(not(target_os = "linux"))]
    let service: Option<ManagedService> = None;

    if let Some(service) = &service {
        stop_with_recovery(&installation.target, service).await?;
    }
    let applied = match apply_update(installation, staged.path(), current_version, new_version) {
        Ok(applied) => applied,
        Err(update_error) => {
            if let Some(service) = &service
                && let Err(restart_error) =
                    recover_old_service(installation, current_version, service).await
            {
                bail!(
                    "update failed ({update_error:#}); old service recovery also failed ({restart_error:#})"
                );
            }
            return Err(update_error);
        }
    };

    if let Some(service) = &service
        && let Err(start_error) = start_and_verify_service(applied.new_target(), service).await
    {
        if let Err(stop_error) = stop_service_bounded().await {
            bail!(
                "updated service failed verification ({start_error:#}) and could not be stopped safely ({stop_error:#}); the previous release remains available but automatic rollback was not attempted"
            );
        }
        let rollback_result = applied.rollback();
        let restart_result = recover_old_service(installation, current_version, service).await;
        bail!(
            "updated service failed verification ({start_error:#}); rollback: {}; old service verification: {}",
            result_label(&rollback_result),
            result_label(&restart_result)
        );
    }

    println!("Installed codex-raw {new_version}.");
    println!("{}", applied.backup_description());
    if service.is_some() {
        println!("Gracefully restarted and verified {SYSTEMD_UNIT}.");
    } else {
        println!("Restart any already-running codex-raw process to use the new version.");
    }
    Ok(())
}

async fn recover_old_service(
    installation: &Installation,
    current_version: &Version,
    service: &ManagedService,
) -> Result<()> {
    verify_binary_version(&installation.target, current_version)
        .await
        .context("the previous codex-raw binary is not safely selected")?;
    if let InstallLayout::Managed(layout) = &installation.layout {
        let selected = fs::canonicalize(layout.current_link.join("codex-raw"))
            .context("cannot resolve the managed current executable during recovery")?;
        ensure!(
            selected == fs::canonicalize(&installation.target)?,
            "managed current does not select the previous codex-raw release"
        );
    }
    start_and_verify_service(&installation.target, service).await
}

fn apply_update(
    installation: &Installation,
    staged: &Path,
    current_version: &Version,
    new_version: &Version,
) -> Result<AppliedUpdate> {
    match &installation.layout {
        InstallLayout::Standalone => {
            let backup = unique_sidecar(&installation.target, "backup", current_version)?;
            fs::hard_link(&installation.target, &backup)
                .context("failed to create an exact backup hard link")?;
            if let Err(error) = fs::rename(staged, &installation.target) {
                let cleanup = fs::remove_file(&backup);
                if let Err(cleanup_error) = cleanup {
                    return Err(error).context(format!(
                        "atomic executable replacement failed; backup cleanup also failed: {cleanup_error}"
                    ));
                }
                return Err(error).context("atomic executable replacement failed");
            }
            let applied = AppliedUpdate::Standalone {
                target: installation.target.clone(),
                backup,
            };
            if let Err(sync_error) = sync_directory(
                installation
                    .target
                    .parent()
                    .context("installed binary has no parent")?,
            ) {
                let rollback_result = applied.rollback();
                bail!(
                    "replacement directory sync failed ({sync_error:#}); rollback: {}",
                    result_label(&rollback_result)
                );
            }
            Ok(applied)
        }
        InstallLayout::Managed(layout) => apply_managed(layout, staged, new_version),
    }
}

fn apply_managed(
    layout: &ManagedLayout,
    staged: &Path,
    new_version: &Version,
) -> Result<AppliedUpdate> {
    let new_release = layout.releases.join(new_version.to_string());
    ensure!(
        !new_release.exists(),
        "managed release directory already exists: {}",
        new_release.display()
    );
    let release_metadata = layout.active_release.metadata()?;
    let mut builder = fs::DirBuilder::new();
    builder.mode(release_metadata.mode() & 0o7777);
    builder
        .create(&new_release)
        .context("failed to create the managed release directory")?;
    let new_metadata = new_release.metadata()?;
    if new_metadata.uid() != release_metadata.uid() || new_metadata.gid() != release_metadata.gid()
    {
        let _ = fs::remove_dir(&new_release);
        bail!("new managed release owner/group differs from the active release");
    }

    let new_target = new_release.join("codex-raw");
    let copy_result = copy_new_file(staged, &new_target);
    if let Err(error) = copy_result {
        let _ = fs::remove_file(&new_target);
        let _ = fs::remove_dir(&new_release);
        return Err(error);
    }
    if let Err(error) = sync_directory(&new_release) {
        let _ = fs::remove_file(&new_target);
        let _ = fs::remove_dir(&new_release);
        return Err(error).context("failed to sync the managed release directory");
    }
    if let Err(error) = sync_directory(&layout.releases) {
        let _ = fs::remove_file(&new_target);
        let _ = fs::remove_dir(&new_release);
        let _ = sync_directory(&layout.releases);
        return Err(error).context("failed to sync the managed releases parent directory");
    }

    let previous_link =
        fs::read_link(&layout.current_link).context("failed to read managed current link")?;
    let temporary_link = layout.root.join(format!(
        ".current.update-{}-{}",
        new_version,
        std::process::id()
    ));
    ensure!(
        !temporary_link.exists(),
        "temporary managed current link already exists"
    );
    std::os::unix::fs::symlink(
        Path::new("releases").join(new_version.to_string()),
        &temporary_link,
    )?;
    if let Err(error) = fs::rename(&temporary_link, &layout.current_link) {
        let _ = fs::remove_file(&temporary_link);
        let _ = fs::remove_file(&new_target);
        let _ = fs::remove_dir(&new_release);
        return Err(error).context("failed to atomically switch the managed current link");
    }
    let applied = AppliedUpdate::Managed {
        current_link: layout.current_link.clone(),
        previous_link,
        new_release,
        new_target,
        root: layout.root.clone(),
    };
    if let Err(sync_error) = sync_directory(&layout.root) {
        let rollback_result = applied.rollback();
        bail!(
            "managed current-link sync failed ({sync_error:#}); rollback: {}",
            result_label(&rollback_result)
        );
    }
    let _ = fs::remove_file(staged);
    Ok(applied)
}

fn copy_new_file(source: &Path, destination: &Path) -> Result<()> {
    let mut source_file = File::open(source)?;
    let source_metadata = source_file.metadata()?;
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    std::io::copy(&mut source_file, &mut destination_file)?;
    destination_file.set_permissions(source_metadata.permissions())?;
    destination_file.sync_all()?;
    ensure!(
        destination_file.metadata()?.len() == source_metadata.len(),
        "managed release binary copy is incomplete"
    );
    Ok(())
}

impl AppliedUpdate {
    fn new_target(&self) -> &Path {
        match self {
            Self::Standalone { target, .. } => target,
            Self::Managed { new_target, .. } => new_target,
        }
    }

    fn rollback(&self) -> Result<()> {
        match self {
            Self::Standalone { target, backup } => {
                fs::rename(backup, target).context("failed to atomically restore the backup")?;
                sync_directory(target.parent().context("target has no parent")?)
            }
            Self::Managed {
                current_link,
                previous_link,
                new_release,
                new_target,
                root,
            } => {
                let temporary_link = root.join(format!(".current.rollback-{}", std::process::id()));
                ensure!(!temporary_link.exists(), "rollback link already exists");
                std::os::unix::fs::symlink(previous_link, &temporary_link)?;
                fs::rename(&temporary_link, current_link)?;
                sync_directory(root)?;
                fs::remove_file(new_target)?;
                fs::remove_dir(new_release)?;
                sync_directory(
                    new_release
                        .parent()
                        .context("managed release has no releases parent")?,
                )
            }
        }
    }

    fn backup_description(&self) -> String {
        match self {
            Self::Standalone { backup, .. } => {
                format!("Previous binary kept at {}.", backup.display())
            }
            Self::Managed { previous_link, .. } => format!(
                "Previous managed release remains available through {}.",
                previous_link.display()
            ),
        }
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn result_label(result: &Result<()>) -> String {
    match result {
        Ok(()) => "succeeded".to_string(),
        Err(error) => format!("failed ({error:#})"),
    }
}

#[cfg(test)]
#[path = "unix_tests.rs"]
mod tests;
