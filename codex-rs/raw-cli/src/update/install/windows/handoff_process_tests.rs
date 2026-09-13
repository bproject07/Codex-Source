use std::fs;

use pretty_assertions::assert_eq;
use semver::Version;
use tempfile::tempdir;

use super::begin;
use super::stop_helper;
use super::wait_for_signal;
use crate::update::install::InstallLayout;
use crate::update::install::Installation;
use crate::update::install::OwnedStaged;

#[tokio::test]
async fn helper_transfers_lock_ownership_without_a_console() {
    let directory = tempdir().expect("temporary installation");
    let target = directory.path().join("codex-raw.exe");
    let staged_path = directory.path().join("staged.exe");
    fs::write(&target, b"installed").expect("write installed fixture");
    fs::write(&staged_path, b"staged").expect("write staged fixture");
    let installation = Installation {
        target,
        layout: InstallLayout::Standalone,
    };
    let staged = OwnedStaged::new(staged_path);
    let mut lock = installation.open_update_lock().expect("open updater lock");
    let guard = lock.try_write().expect("own updater lock");
    let current_version = Version::parse("0.1.1").expect("current version");
    let new_version = Version::parse("0.1.2").expect("new version");
    let mut handoff = begin(&installation, &staged, &current_version, &new_version)
        .await
        .expect("helper starts and queues for the updater lock");

    drop(guard);
    let ownership = wait_for_signal(
        &mut handoff.child,
        &handoff.owner_ready,
        handoff.marker.token(),
        &handoff.status,
    )
    .await;
    // The helper waits for this test process to exit before replacing anything.
    // Stop our helper before ending the test, including on a failed handshake.
    stop_helper(&mut handoff.child);
    ownership.expect("helper takes ownership after the parent releases its lock");
    drop(handoff);

    assert_eq!(
        fs::read(installation.target()).expect("installed fixture"),
        b"installed".to_vec()
    );
    assert_eq!(
        fs::read(staged.path()).expect("staged fixture"),
        b"staged".to_vec()
    );
    assert!(!installation.handoff_path().exists());
    assert!(lock.try_write().is_ok());
}
