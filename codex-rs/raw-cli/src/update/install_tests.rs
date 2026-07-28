use std::fs;

use tempfile::tempdir;

use super::InstallLayout;
use super::Installation;
#[cfg(unix)]
use super::discover_managed_for_test;

#[test]
fn update_lock_rejects_a_second_mutating_updater() {
    let directory = tempdir().expect("temporary install");
    let target = directory.path().join(if cfg!(windows) {
        "codex-raw.exe"
    } else {
        "codex-raw"
    });
    fs::write(&target, b"binary").expect("write target");
    let installation = Installation {
        target,
        layout: InstallLayout::Standalone,
    };
    let mut first = installation.open_update_lock().expect("first lock file");
    let _guard = first.try_write().expect("first exclusive lock");
    let mut second = installation.open_update_lock().expect("second lock file");

    let error = second
        .try_write()
        .expect_err("second exclusive lock must contend");
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}

#[cfg(unix)]
#[test]
fn managed_layout_requires_current_to_select_the_running_release() {
    let directory = tempdir().expect("temporary install");
    let releases = directory.path().join("releases");
    let active = releases.join("0.1.0");
    fs::create_dir_all(&active).expect("create active release");
    let target = active.join("codex-raw");
    fs::write(&target, b"old").expect("write target");
    std::os::unix::fs::symlink("releases/0.1.0", directory.path().join("current"))
        .expect("create current link");

    assert!(discover_managed_for_test(&target).expect("detect managed layout"));

    fs::remove_file(directory.path().join("current")).expect("remove current link");
    std::os::unix::fs::symlink("releases/missing", directory.path().join("current"))
        .expect("create bad current link");
    assert!(discover_managed_for_test(&target).is_err());
}
