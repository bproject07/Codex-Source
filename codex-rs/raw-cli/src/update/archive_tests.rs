use std::fs::File;
use std::io::Seek;
use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use pretty_assertions::assert_eq;
use semver::Version;
use tempfile::tempfile;
use zip::write::SimpleFileOptions;

use super::MAX_ARCHIVE_ENTRIES;
use super::MAX_BINARY_BYTES;
use super::checksum_for;
use super::extract_binary;
use crate::update::ArchiveKind;
use crate::update::ReleasePlatform;

const VERSION_TEXT: &str = "0.2.0";
const TAR_EXPECTED: &str = "codex-raw-0.2.0-x86_64-unknown-linux-gnu/codex-raw";
const ZIP_EXPECTED: &str = "codex-raw-0.2.0-x86_64-pc-windows-msvc/codex-raw.exe";
const ZIP_DUPLICATE_PLACEHOLDER: &str = "codex-raw-0.2.0-x86_64-pc-windows-msvc/codex-raw.ex_";

fn tar_platform() -> ReleasePlatform {
    ReleasePlatform {
        target: "x86_64-unknown-linux-gnu",
        archive_kind: ArchiveKind::TarGz,
        executable_name: "codex-raw",
    }
}

fn zip_platform() -> ReleasePlatform {
    ReleasePlatform {
        target: "x86_64-pc-windows-msvc",
        archive_kind: ArchiveKind::Zip,
        executable_name: "codex-raw.exe",
    }
}

#[test]
fn checksum_manifest_requires_one_exact_well_formed_entry() {
    let expected = "codex-raw-0.2.0-x86_64-unknown-linux-gnu.tar.gz";
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(checksum_for(&format!("{digest}  {expected}\n"), expected).is_ok());
    assert!(checksum_for("not-a-digest  file.tar.gz\n", expected).is_err());
    assert!(checksum_for(&format!("{digest}\n"), expected).is_err());
    assert!(checksum_for(&format!("{digest}  wrong.tar.gz\n"), expected).is_err());
    assert!(checksum_for(&format!("{digest}  nested/{expected}\n"), expected).is_err());
    assert!(
        checksum_for(
            &format!("{digest}  {expected}\n{digest}  {expected}\n"),
            expected
        )
        .is_err()
    );
}

#[test]
fn tar_extracts_only_the_exact_regular_binary() {
    let mut archive = tar_archive(&[
        TarFixture::File("unrelated.txt", b"ignore"),
        TarFixture::File(TAR_EXPECTED, b"verified binary"),
    ]);
    let mut output = tempfile().expect("output");

    extract_binary(
        &mut archive,
        &mut output,
        tar_platform(),
        &Version::parse(VERSION_TEXT).expect("version"),
    )
    .expect("extract exact binary");
    output.rewind().expect("rewind output");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut output, &mut bytes).expect("read output");
    assert_eq!(bytes, b"verified binary");
}

#[test]
fn tar_rejects_duplicate_symlink_traversal_and_entry_bomb() {
    let version = Version::parse(VERSION_TEXT).expect("version");
    let mut duplicate = tar_archive(&[
        TarFixture::File(TAR_EXPECTED, b"one"),
        TarFixture::File(TAR_EXPECTED, b"two"),
    ]);
    assert!(
        extract_binary(
            &mut duplicate,
            &mut tempfile().expect("output"),
            tar_platform(),
            &version
        )
        .is_err()
    );

    let mut symlink = tar_archive(&[TarFixture::Symlink(TAR_EXPECTED, "elsewhere")]);
    assert!(
        extract_binary(
            &mut symlink,
            &mut tempfile().expect("output"),
            tar_platform(),
            &version
        )
        .is_err()
    );

    let mut traversal = tar_archive(&[TarFixture::RawFile("../codex-raw", b"wrong")]);
    assert!(
        extract_binary(
            &mut traversal,
            &mut tempfile().expect("output"),
            tar_platform(),
            &version
        )
        .is_err()
    );

    let fixtures = (0..=MAX_ARCHIVE_ENTRIES)
        .map(|_| TarFixture::File("unrelated", b"x"))
        .collect::<Vec<_>>();
    let mut entry_bomb = tar_archive(&fixtures);
    assert!(
        extract_binary(
            &mut entry_bomb,
            &mut tempfile().expect("output"),
            tar_platform(),
            &version
        )
        .is_err()
    );
}

#[test]
fn tar_rejects_oversized_declared_binary_before_copying() {
    let mut archive = oversized_tar(TAR_EXPECTED, MAX_BINARY_BYTES + 1);
    assert!(
        extract_binary(
            &mut archive,
            &mut tempfile().expect("output"),
            tar_platform(),
            &Version::parse(VERSION_TEXT).expect("version")
        )
        .is_err()
    );
}

#[test]
fn zip_extracts_exact_file_and_rejects_duplicate_symlink_or_traversal() {
    let version = Version::parse(VERSION_TEXT).expect("version");
    let mut valid = zip_archive(&[ZipFixture::File(ZIP_EXPECTED, b"binary")]);
    let mut output = tempfile().expect("output");
    extract_binary(&mut valid, &mut output, zip_platform(), &version).expect("extract zip");

    let mut duplicate = zip_archive(&[
        ZipFixture::File(ZIP_EXPECTED, b"one"),
        ZipFixture::File(ZIP_DUPLICATE_PLACEHOLDER, b"two"),
    ]);
    forge_zip_filename(&mut duplicate, ZIP_DUPLICATE_PLACEHOLDER, ZIP_EXPECTED);
    assert!(
        extract_binary(
            &mut duplicate,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );

    let mut symlink = zip_archive(&[ZipFixture::Symlink(ZIP_EXPECTED, "elsewhere")]);
    assert!(
        extract_binary(
            &mut symlink,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );

    let mut traversal = zip_archive(&[ZipFixture::File("../codex-raw.exe", b"wrong")]);
    assert!(
        extract_binary(
            &mut traversal,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );
}

#[test]
fn zip_rejects_entry_and_declared_size_bombs() {
    let version = Version::parse(VERSION_TEXT).expect("version");
    let entry_names = (0..=MAX_ARCHIVE_ENTRIES)
        .map(|index| format!("unrelated-{index}"))
        .collect::<Vec<_>>();
    let fixtures = entry_names
        .iter()
        .map(|name| ZipFixture::File(name.as_str(), b"x"))
        .collect::<Vec<_>>();
    let mut entry_bomb = zip_archive(&fixtures);
    assert!(
        extract_binary(
            &mut entry_bomb,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );

    let mut size_bomb = zip_archive(&[ZipFixture::File(ZIP_EXPECTED, b"x")]);
    forge_zip_uncompressed_size(&mut size_bomb, (MAX_BINARY_BYTES + 1) as u32);
    assert!(
        extract_binary(
            &mut size_bomb,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );

    let mut encrypted = zip_archive(&[ZipFixture::File(ZIP_EXPECTED, b"x")]);
    forge_zip_encryption_flag(&mut encrypted);
    assert!(
        extract_binary(
            &mut encrypted,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );

    let mut zip64 = zip_archive(&[ZipFixture::File(ZIP_EXPECTED, b"x")]);
    forge_zip64_entry_count(&mut zip64);
    assert!(
        extract_binary(
            &mut zip64,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );

    let mut truncated = zip_archive(&[ZipFixture::File(ZIP_EXPECTED, b"x")]);
    let truncated_len = truncated
        .metadata()
        .expect("zip metadata")
        .len()
        .checked_sub(1)
        .expect("non-empty zip");
    truncated.set_len(truncated_len).expect("truncate zip");
    truncated.rewind().expect("rewind truncated zip");
    assert!(
        extract_binary(
            &mut truncated,
            &mut tempfile().expect("output"),
            zip_platform(),
            &version
        )
        .is_err()
    );
}

enum TarFixture<'a> {
    File(&'a str, &'a [u8]),
    RawFile(&'a str, &'a [u8]),
    Symlink(&'a str, &'a str),
}

fn tar_archive(fixtures: &[TarFixture<'_>]) -> File {
    let mut file = tempfile().expect("archive");
    {
        let encoder = GzEncoder::new(&mut file, Compression::default());
        let mut archive = tar::Builder::new(encoder);
        for fixture in fixtures {
            match fixture {
                TarFixture::File(path, contents) => {
                    let mut header = tar::Header::new_gnu();
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_mode(0o755);
                    header.set_size(contents.len() as u64);
                    header.set_cksum();
                    archive
                        .append_data(&mut header, path, *contents)
                        .expect("append tar file");
                }
                TarFixture::RawFile(path, contents) => {
                    let mut header = tar::Header::new_gnu();
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_mode(0o755);
                    header.set_size(contents.len() as u64);
                    header
                        .set_path("placeholder")
                        .expect("set placeholder path");
                    let name = &mut header.as_mut_bytes()[..100];
                    name.fill(0);
                    name[..path.len()].copy_from_slice(path.as_bytes());
                    header.set_cksum();
                    archive
                        .append(&header, *contents)
                        .expect("append raw tar file");
                }
                TarFixture::Symlink(path, target) => {
                    let mut header = tar::Header::new_gnu();
                    header.set_entry_type(tar::EntryType::Symlink);
                    header.set_mode(0o777);
                    header.set_size(0);
                    header.set_link_name(target).expect("set link target");
                    header.set_cksum();
                    archive
                        .append_data(&mut header, path, std::io::empty())
                        .expect("append tar symlink");
                }
            }
        }
        archive
            .into_inner()
            .expect("finish tar")
            .finish()
            .expect("finish gzip");
    }
    file.rewind().expect("rewind archive");
    file
}

fn oversized_tar(path: &str, size: u64) -> File {
    let mut file = tempfile().expect("archive");
    {
        let mut encoder = GzEncoder::new(&mut file, Compression::default());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_mode(0o755);
        header.set_size(size);
        header.set_path(path).expect("set tar path");
        header.set_cksum();
        encoder.write_all(header.as_bytes()).expect("write header");
        encoder
            .write_all(&[0_u8; 1024])
            .expect("write tar terminator");
        encoder.finish().expect("finish gzip");
    }
    file.rewind().expect("rewind archive");
    file
}

enum ZipFixture<'a> {
    File(&'a str, &'a [u8]),
    Symlink(&'a str, &'a str),
}

fn zip_archive(fixtures: &[ZipFixture<'_>]) -> File {
    let mut file = tempfile().expect("archive");
    {
        let mut archive = zip::ZipWriter::new(&mut file);
        for fixture in fixtures {
            match fixture {
                ZipFixture::File(path, contents) => {
                    archive
                        .start_file(*path, SimpleFileOptions::default())
                        .expect("start zip file");
                    archive.write_all(contents).expect("write zip file");
                }
                ZipFixture::Symlink(path, target) => archive
                    .add_symlink(*path, *target, SimpleFileOptions::default())
                    .expect("add zip symlink"),
            }
        }
        archive.finish().expect("finish zip");
    }
    file.rewind().expect("rewind archive");
    file
}

fn forge_zip_uncompressed_size(file: &mut File, size: u32) {
    file.rewind().expect("rewind zip");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(file, &mut bytes).expect("read zip");
    let local = bytes
        .windows(4)
        .position(|window| window == b"PK\x03\x04")
        .expect("local header");
    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .expect("central directory");
    bytes[local + 22..local + 26].copy_from_slice(&size.to_le_bytes());
    bytes[central + 24..central + 28].copy_from_slice(&size.to_le_bytes());
    file.set_len(0).expect("truncate zip");
    file.rewind().expect("rewind zip");
    file.write_all(&bytes).expect("write forged zip");
    file.rewind().expect("rewind forged zip");
}

fn forge_zip_encryption_flag(file: &mut File) {
    file.rewind().expect("rewind zip");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(file, &mut bytes).expect("read zip");
    let local = bytes
        .windows(4)
        .position(|window| window == b"PK\x03\x04")
        .expect("local header");
    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .expect("central directory");
    let encrypted =
        (u16::from_le_bytes([bytes[central + 8], bytes[central + 9]]) | 1).to_le_bytes();
    bytes[local + 6..local + 8].copy_from_slice(&encrypted);
    bytes[central + 8..central + 10].copy_from_slice(&encrypted);
    file.set_len(0).expect("truncate zip");
    file.rewind().expect("rewind zip");
    file.write_all(&bytes).expect("write forged zip");
    file.rewind().expect("rewind forged zip");
}

fn forge_zip64_entry_count(file: &mut File) {
    file.rewind().expect("rewind zip");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(file, &mut bytes).expect("read zip");
    let eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .expect("end of central directory");
    bytes[eocd + 8..eocd + 12].fill(0xff);
    file.set_len(0).expect("truncate zip");
    file.rewind().expect("rewind zip");
    file.write_all(&bytes).expect("write forged zip");
    file.rewind().expect("rewind forged zip");
}

fn forge_zip_filename(file: &mut File, original: &str, replacement: &str) {
    assert_eq!(original.len(), replacement.len());
    file.rewind().expect("rewind zip");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(file, &mut bytes).expect("read zip");

    let mut replacements = 0;
    let mut offset = 0;
    while let Some(relative) = bytes[offset..]
        .windows(original.len())
        .position(|window| window == original.as_bytes())
    {
        let start = offset + relative;
        let end = start + original.len();
        bytes[start..end].copy_from_slice(replacement.as_bytes());
        replacements += 1;
        offset = end;
    }
    assert_eq!(replacements, 2);

    file.set_len(0).expect("truncate zip");
    file.rewind().expect("rewind zip");
    file.write_all(&bytes).expect("write forged zip");
    file.rewind().expect("rewind forged zip");
}
