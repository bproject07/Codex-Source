use std::fs;
use std::os::unix::fs::PermissionsExt;

use pretty_assertions::assert_eq;
use semver::Version;
use tempfile::tempdir;

use super::apply_managed;
use crate::update::install::ManagedLayout;

#[test]
fn managed_release_switch_is_atomic_and_rolls_back_to_old_link() {
    let directory = tempdir().expect("temporary managed install");
    let releases = directory.path().join("releases");
    let active = releases.join("0.1.0");
    fs::create_dir_all(&active).expect("create active release");
    let old_target = active.join("codex-raw");
    fs::write(&old_target, b"old").expect("write old binary");
    fs::set_permissions(&old_target, fs::Permissions::from_mode(0o755))
        .expect("make old executable");
    let current = directory.path().join("current");
    std::os::unix::fs::symlink("releases/0.1.0", &current).expect("create current link");
    let staged = active.join(".codex-raw.update-test");
    fs::write(&staged, b"new").expect("write staged binary");
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
        .expect("make staged executable");
    let layout = ManagedLayout {
        root: directory.path().to_path_buf(),
        releases: releases.clone(),
        current_link: current.clone(),
        active_release: active.clone(),
    };

    let applied = apply_managed(
        &layout,
        &staged,
        &Version::parse("0.2.0").expect("new version"),
    )
    .expect("apply managed update");
    assert_eq!(
        fs::canonicalize(&current).expect("resolve new current"),
        fs::canonicalize(releases.join("0.2.0")).expect("resolve new release")
    );
    assert_eq!(
        fs::read(releases.join("0.2.0/codex-raw")).expect("read new binary"),
        b"new"
    );
    assert!(active.exists());

    applied.rollback().expect("rollback managed update");
    assert_eq!(
        fs::canonicalize(&current).expect("resolve restored current"),
        fs::canonicalize(&active).expect("resolve old release")
    );
    assert!(!releases.join("0.2.0").exists());
    assert_eq!(fs::read(old_target).expect("read old binary"), b"old");
}

#[test]
fn existing_managed_version_never_changes_current_link() {
    let directory = tempdir().expect("temporary managed install");
    let releases = directory.path().join("releases");
    let active = releases.join("0.1.0");
    fs::create_dir_all(&active).expect("create active release");
    fs::create_dir_all(releases.join("0.2.0")).expect("create colliding release");
    let current = directory.path().join("current");
    std::os::unix::fs::symlink("releases/0.1.0", &current).expect("create current link");
    let staged = active.join(".codex-raw.update-test");
    fs::write(&staged, b"new").expect("write staged binary");
    let layout = ManagedLayout {
        root: directory.path().to_path_buf(),
        releases,
        current_link: current.clone(),
        active_release: active,
    };

    assert!(
        apply_managed(
            &layout,
            &staged,
            &Version::parse("0.2.0").expect("new version")
        )
        .is_err()
    );
    assert_eq!(
        fs::read_link(current).expect("read current"),
        std::path::PathBuf::from("releases/0.1.0")
    );
}
