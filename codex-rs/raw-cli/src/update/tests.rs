use pretty_assertions::assert_eq;
use semver::Version;

use super::ArchiveKind;
use super::CHECKSUM_ASSET;
use super::GitHubAsset;
use super::GitHubRelease;
use super::ReleasePlatform;
use super::UpdateAvailability;
use super::allowed_github_url;
use super::availability;
use super::select_release;
use super::verify_asset_digest;

const DIGEST: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn linux_platform() -> ReleasePlatform {
    ReleasePlatform {
        target: "x86_64-unknown-linux-gnu",
        archive_kind: ArchiveKind::TarGz,
        executable_name: "codex-raw",
    }
}

fn asset(tag: &str, name: &str, size: u64) -> GitHubAsset {
    GitHubAsset {
        name: name.to_string(),
        browser_download_url: format!(
            "https://github.com/bproject07/Codex-Source/releases/download/{tag}/{name}"
        ),
        size,
        digest: Some(DIGEST.to_string()),
    }
}

fn stable_release(version: &str) -> GitHubRelease {
    let tag = format!("codex-raw-v{version}");
    let archive_name = format!("codex-raw-{version}-x86_64-unknown-linux-gnu.tar.gz");
    GitHubRelease {
        tag_name: tag.clone(),
        draft: false,
        prerelease: false,
        immutable: true,
        assets: vec![
            asset(&tag, &archive_name, 1024),
            asset(&tag, CHECKSUM_ASSET, 128),
        ],
    }
}

#[test]
fn selects_only_an_exact_immutable_stable_release() {
    let selected =
        select_release(stable_release("0.2.0"), linux_platform()).expect("select release");

    assert_eq!(selected.version, Version::parse("0.2.0").expect("version"));
    assert_eq!(
        selected.archive.name,
        "codex-raw-0.2.0-x86_64-unknown-linux-gnu.tar.gz"
    );
    assert_eq!(selected.checksums.name, CHECKSUM_ASSET);
}

#[test]
fn semver_precedence_refuses_older_and_ignores_build_metadata() {
    let current = Version::parse("0.2.0").expect("current");
    assert_eq!(
        availability(&current, &Version::parse("0.1.9").expect("older")),
        UpdateAvailability::Older
    );
    assert_eq!(
        availability(&current, &Version::parse("0.2.0").expect("equal")),
        UpdateAvailability::Current
    );
    assert_eq!(
        availability(
            &Version::parse("0.2.0+local").expect("local build"),
            &Version::parse("0.2.0+release").expect("release build")
        ),
        UpdateAvailability::Current
    );
    assert_eq!(
        availability(&current, &Version::parse("0.2.1").expect("newer")),
        UpdateAvailability::Newer
    );
}

#[test]
fn rejects_mutable_draft_or_prerelease_identity() {
    let mut release = stable_release("0.2.0");
    release.immutable = false;
    assert!(select_release(release, linux_platform()).is_err());

    let mut release = stable_release("0.2.0");
    release.draft = true;
    assert!(select_release(release, linux_platform()).is_err());

    let mut release = stable_release("0.2.0");
    release.prerelease = true;
    assert!(select_release(release, linux_platform()).is_err());

    assert!(select_release(stable_release("0.2.0-rc.1"), linux_platform()).is_err());
}

#[test]
fn exact_platform_assets_must_be_unique_and_present() {
    let mut missing = stable_release("0.2.0");
    missing.assets.remove(0);
    assert!(select_release(missing, linux_platform()).is_err());

    let mut duplicate = stable_release("0.2.0");
    duplicate.assets.push(duplicate.assets[0].clone());
    assert!(select_release(duplicate, linux_platform()).is_err());

    let mut missing_sums = stable_release("0.2.0");
    missing_sums
        .assets
        .retain(|asset| asset.name != CHECKSUM_ASSET);
    assert!(select_release(missing_sums, linux_platform()).is_err());
}

#[test]
fn rejects_untrusted_asset_url_and_missing_or_wrong_digest() {
    let mut release = stable_release("0.2.0");
    release.assets[0].browser_download_url =
        "https://example.com/codex-raw-0.2.0.tar.gz".to_string();
    assert!(select_release(release, linux_platform()).is_err());

    let mut release = stable_release("0.2.0");
    release.assets[0].digest = None;
    assert!(select_release(release, linux_platform()).is_err());

    let mut release = stable_release("0.2.0");
    release.assets[0].digest = Some("sha512:aa".to_string());
    assert!(select_release(release, linux_platform()).is_err());

    let release = stable_release("0.2.0");
    assert!(verify_asset_digest(&release.assets[0], [0xbb; 32]).is_err());
}

#[test]
fn redirect_allowlist_rejects_non_github_hosts_and_plain_http() {
    assert!(allowed_github_url(
        &"https://release-assets.githubusercontent.com/path"
            .parse()
            .expect("GitHub URL")
    ));
    assert!(!allowed_github_url(
        &"https://github.com.example.org/path"
            .parse()
            .expect("lookalike URL")
    ));
    assert!(!allowed_github_url(
        &"https://attacker.githubusercontent.com/path"
            .parse()
            .expect("unapproved GitHubusercontent URL")
    ));
    assert!(!allowed_github_url(
        &"http://github.com/path".parse().expect("plain HTTP URL")
    ));
}
