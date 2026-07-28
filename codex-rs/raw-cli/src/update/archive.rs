//! Bounded checksum parsing and exact-entry release archive extraction.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use flate2::read::GzDecoder;
use semver::Version;
use sha2::Digest;
use sha2::Sha256;

use super::ArchiveKind;
use super::ReleasePlatform;

const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DECLARED_UNCOMPRESSED_BYTES: u64 = 768 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 64;
const MAX_ZIP_CENTRAL_DIRECTORY_BYTES: u64 = 16 * 1024 * 1024;
const ZIP_EOCD_FIXED_BYTES: usize = 22;
const ZIP_MAX_EOCD_SEARCH_BYTES: u64 = ZIP_EOCD_FIXED_BYTES as u64 + u16::MAX as u64;
const ZIP_CENTRAL_HEADER_BYTES: usize = 46;
const ZIP_LOCAL_HEADER_BYTES: usize = 30;
const ZIP_ENCRYPTION_FLAGS: u16 = 1 | (1 << 6) | (1 << 13);
const ZIP64_VERSION_NEEDED: u16 = 45;
const ZIP_EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const ZIP64_EOCD_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";
const ZIP_CENTRAL_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const ZIP_LOCAL_SIGNATURE: &[u8; 4] = b"PK\x03\x04";

pub(super) fn checksum_for(manifest: &str, expected_name: &str) -> Result<[u8; 32]> {
    let mut found = None;
    for line in manifest.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        ensure!(fields.len() == 2, "malformed SHA256SUMS line");
        let digest = parse_sha256(fields[0])?;
        let name = fields[1].strip_prefix('*').unwrap_or(fields[1]);
        ensure!(
            !name.contains('/') && !name.contains('\\'),
            "SHA256SUMS contains a path instead of an asset name"
        );
        if name == expected_name {
            ensure!(found.is_none(), "duplicate SHA256SUMS entry for {name}");
            found = Some(digest);
        }
    }
    found.with_context(|| format!("SHA256SUMS has no exact entry for {expected_name}"))
}

pub(super) fn parse_sha256(value: &str) -> Result<[u8; 32]> {
    ensure!(
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "expected a 64-character SHA-256 digest"
    );
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(output)
}

pub(super) fn sha256_file(file: &mut File) -> Result<[u8; 32]> {
    file.rewind()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(file, &mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    file.rewind()?;
    Ok(hasher.finalize().into())
}

pub(super) fn extract_binary(
    archive_file: &mut File,
    output: &mut File,
    platform: ReleasePlatform,
    version: &Version,
) -> Result<()> {
    archive_file.rewind()?;
    output.set_len(0)?;
    output.rewind()?;
    let expected = format!(
        "codex-raw-{version}-{}/{}",
        platform.target, platform.executable_name
    );
    match platform.archive_kind {
        ArchiveKind::TarGz => extract_tar_gz(archive_file, output, &expected)?,
        ArchiveKind::Zip => extract_zip(archive_file, output, &expected)?,
    }
    output.flush()?;
    Ok(())
}

fn extract_tar_gz(archive_file: &mut File, output: &mut File, expected: &str) -> Result<()> {
    let decoder = GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let mut found = false;
    let mut entry_count = 0_usize;
    let mut declared_bytes = 0_u64;
    for entry in archive
        .entries()
        .context("invalid tar.gz release archive")?
    {
        entry_count += 1;
        ensure!(
            entry_count <= MAX_ARCHIVE_ENTRIES,
            "release archive contains too many entries"
        );
        let mut entry = entry.context("invalid tar.gz entry")?;
        declared_bytes = declared_bytes
            .checked_add(entry.size())
            .context("release archive size overflow")?;
        ensure!(
            declared_bytes <= MAX_DECLARED_UNCOMPRESSED_BYTES,
            "release archive declares too much uncompressed data"
        );
        if entry.path()?.as_ref() != Path::new(expected) {
            continue;
        }
        ensure!(!found, "release archive contains duplicate binaries");
        ensure!(
            entry.header().entry_type().is_file(),
            "release binary entry is not a regular file"
        );
        ensure!(
            entry.size() > 0 && entry.size() <= MAX_BINARY_BYTES,
            "release binary has an invalid size"
        );
        let copied = std::io::copy(&mut entry, output)?;
        ensure!(
            copied == entry.size(),
            "release binary was not extracted completely"
        );
        found = true;
    }
    ensure!(found, "release archive does not contain {expected}");
    Ok(())
}

fn extract_zip(archive_file: &mut File, output: &mut File, expected: &str) -> Result<()> {
    preflight_zip(archive_file)?;
    archive_file.rewind()?;
    let mut archive = zip::ZipArchive::new(archive_file).context("invalid zip release archive")?;
    ensure!(
        archive.len() <= MAX_ARCHIVE_ENTRIES,
        "release archive contains too many entries"
    );
    let mut matching_index = None;
    let mut declared_bytes = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        declared_bytes = declared_bytes
            .checked_add(entry.size())
            .context("release archive size overflow")?;
        ensure!(
            declared_bytes <= MAX_DECLARED_UNCOMPRESSED_BYTES,
            "release archive declares too much uncompressed data"
        );
        if entry.name() == expected {
            ensure!(
                matching_index.is_none(),
                "release archive contains duplicate binaries"
            );
            matching_index = Some(index);
        }
    }
    let index = matching_index.with_context(|| {
        format!("release archive does not contain the exact binary path {expected}")
    })?;
    let mut entry = archive.by_index(index)?;
    ensure!(
        entry.is_file() && !entry.is_symlink(),
        "release binary entry is not a regular file"
    );
    ensure!(
        entry.size() > 0 && entry.size() <= MAX_BINARY_BYTES,
        "release binary has an invalid size"
    );
    let copied = std::io::copy(&mut entry, output)?;
    ensure!(
        copied == entry.size(),
        "release binary was not extracted completely"
    );
    Ok(())
}

struct ZipCentralEntry<'a> {
    name: &'a [u8],
    version_needed: u16,
    flags: u16,
    compression_method: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    local_header_offset: u32,
}

fn preflight_zip(archive_file: &mut File) -> Result<()> {
    let archive_len = archive_file.metadata()?.len();
    ensure!(
        archive_len >= ZIP_EOCD_FIXED_BYTES as u64,
        "zip release archive is too short"
    );

    let tail_len = archive_len.min(ZIP_MAX_EOCD_SEARCH_BYTES);
    let tail_start = archive_len - tail_len;
    archive_file.seek(SeekFrom::Start(tail_start))?;
    let mut tail = vec![0_u8; usize::try_from(tail_len)?];
    archive_file.read_exact(&mut tail)?;

    let eocd_relative = (0..=tail.len() - ZIP_EOCD_FIXED_BYTES)
        .rev()
        .find(|offset| {
            tail[*offset..*offset + ZIP_EOCD_SIGNATURE.len()] == *ZIP_EOCD_SIGNATURE
                && *offset
                    + ZIP_EOCD_FIXED_BYTES
                    + usize::from(zip_u16(&tail[*offset + 20..*offset + 22]))
                    == tail.len()
        })
        .context("zip release archive has no unambiguous end record")?;
    let eocd_offset = tail_start + u64::try_from(eocd_relative)?;
    let eocd = &tail[eocd_relative..eocd_relative + ZIP_EOCD_FIXED_BYTES];

    ensure!(
        zip_u16(&eocd[4..6]) == 0 && zip_u16(&eocd[6..8]) == 0,
        "multi-disk zip release archives are not supported"
    );
    let entries_on_disk = zip_u16(&eocd[8..10]);
    let entry_count = zip_u16(&eocd[10..12]);
    ensure!(
        entries_on_disk == entry_count,
        "multi-disk zip release archives are not supported"
    );
    ensure!(
        entry_count != u16::MAX,
        "ZIP64 release archives are not supported"
    );
    let entry_count = usize::from(entry_count);
    ensure!(
        entry_count <= MAX_ARCHIVE_ENTRIES,
        "release archive contains too many entries"
    );

    let central_size = zip_u32(&eocd[12..16]);
    let central_offset = zip_u32(&eocd[16..20]);
    ensure!(
        central_size != u32::MAX && central_offset != u32::MAX,
        "ZIP64 release archives are not supported"
    );
    let central_size = u64::from(central_size);
    let central_offset = u64::from(central_offset);
    ensure!(
        central_size <= MAX_ZIP_CENTRAL_DIRECTORY_BYTES,
        "zip central directory is too large"
    );
    ensure!(
        central_offset.checked_add(central_size) == Some(eocd_offset),
        "zip central directory has an invalid extent"
    );

    if eocd_offset >= 20 {
        archive_file.seek(SeekFrom::Start(eocd_offset - 20))?;
        let mut locator_signature = [0_u8; 4];
        archive_file.read_exact(&mut locator_signature)?;
        ensure!(
            locator_signature != *ZIP64_EOCD_LOCATOR_SIGNATURE,
            "ZIP64 release archives are not supported"
        );
    }

    archive_file.seek(SeekFrom::Start(central_offset))?;
    let mut central = vec![0_u8; usize::try_from(central_size)?];
    archive_file.read_exact(&mut central)?;

    let mut names = HashSet::with_capacity(entry_count);
    let mut cursor = 0_usize;
    for _ in 0..entry_count {
        let fixed_end = cursor
            .checked_add(ZIP_CENTRAL_HEADER_BYTES)
            .context("zip central directory offset overflow")?;
        ensure!(
            fixed_end <= central.len()
                && central[cursor..cursor + ZIP_CENTRAL_SIGNATURE.len()] == *ZIP_CENTRAL_SIGNATURE,
            "malformed zip central directory entry"
        );
        let header = &central[cursor..fixed_end];
        let version_needed = zip_u16(&header[6..8]);
        let flags = zip_u16(&header[8..10]);
        let compression_method = zip_u16(&header[10..12]);
        let crc32 = zip_u32(&header[16..20]);
        let compressed_size = zip_u32(&header[20..24]);
        let uncompressed_size = zip_u32(&header[24..28]);
        let name_len = usize::from(zip_u16(&header[28..30]));
        let extra_len = usize::from(zip_u16(&header[30..32]));
        let comment_len = usize::from(zip_u16(&header[32..34]));
        let disk_start = zip_u16(&header[34..36]);
        let local_header_offset = zip_u32(&header[42..46]);

        ensure!(
            version_needed < ZIP64_VERSION_NEEDED
                && compressed_size != u32::MAX
                && uncompressed_size != u32::MAX
                && local_header_offset != u32::MAX,
            "ZIP64 release archives are not supported"
        );
        ensure!(
            flags & ZIP_ENCRYPTION_FLAGS == 0,
            "encrypted zip release archives are not supported"
        );
        ensure!(disk_start == 0, "multi-disk zip entry is not supported");

        let variable_len = name_len
            .checked_add(extra_len)
            .and_then(|size| size.checked_add(comment_len))
            .context("zip central directory entry length overflow")?;
        let entry_end = fixed_end
            .checked_add(variable_len)
            .context("zip central directory entry offset overflow")?;
        ensure!(
            entry_end <= central.len(),
            "truncated zip central directory entry"
        );
        let name = &central[fixed_end..fixed_end + name_len];
        ensure!(
            !name.is_empty() && name.iter().all(u8::is_ascii) && !name.contains(&0),
            "zip entry name must be non-empty ASCII"
        );
        ensure!(
            names.insert(name.to_vec()),
            "zip release archive contains duplicate entry names"
        );
        let extra = &central[fixed_end + name_len..fixed_end + name_len + extra_len];
        ensure_unambiguous_zip_extra(extra)?;

        let entry = ZipCentralEntry {
            name,
            version_needed,
            flags,
            compression_method,
            crc32,
            compressed_size,
            uncompressed_size,
            local_header_offset,
        };
        validate_zip_local_header(archive_file, &entry, central_offset)?;
        cursor = entry_end;
    }
    ensure!(
        cursor == central.len(),
        "zip central directory contains unexpected records"
    );
    Ok(())
}

fn validate_zip_local_header(
    archive_file: &mut File,
    entry: &ZipCentralEntry<'_>,
    central_offset: u64,
) -> Result<()> {
    let local_header_offset = u64::from(entry.local_header_offset);
    ensure!(
        local_header_offset
            .checked_add(ZIP_LOCAL_HEADER_BYTES as u64)
            .is_some_and(|end| end <= central_offset),
        "zip local header has an invalid offset"
    );
    archive_file.seek(SeekFrom::Start(local_header_offset))?;
    let mut header = [0_u8; ZIP_LOCAL_HEADER_BYTES];
    archive_file.read_exact(&mut header)?;
    ensure!(
        header[..ZIP_LOCAL_SIGNATURE.len()] == *ZIP_LOCAL_SIGNATURE,
        "malformed zip local header"
    );

    let version_needed = zip_u16(&header[4..6]);
    let flags = zip_u16(&header[6..8]);
    let compression_method = zip_u16(&header[8..10]);
    let crc32 = zip_u32(&header[14..18]);
    let compressed_size = zip_u32(&header[18..22]);
    let uncompressed_size = zip_u32(&header[22..26]);
    let name_len = usize::from(zip_u16(&header[26..28]));
    let extra_len = usize::from(zip_u16(&header[28..30]));
    ensure!(
        version_needed == entry.version_needed
            && flags == entry.flags
            && compression_method == entry.compression_method,
        "zip local and central headers disagree"
    );
    ensure!(
        version_needed < ZIP64_VERSION_NEEDED
            && compressed_size != u32::MAX
            && uncompressed_size != u32::MAX,
        "ZIP64 release archives are not supported"
    );
    ensure!(
        flags & ZIP_ENCRYPTION_FLAGS == 0,
        "encrypted zip release archives are not supported"
    );
    if flags & (1 << 3) == 0 {
        ensure!(
            crc32 == entry.crc32
                && compressed_size == entry.compressed_size
                && uncompressed_size == entry.uncompressed_size,
            "zip local and central integrity fields disagree"
        );
    }

    let variable_len = name_len
        .checked_add(extra_len)
        .context("zip local header length overflow")?;
    let data_offset = local_header_offset
        .checked_add(ZIP_LOCAL_HEADER_BYTES as u64)
        .and_then(|offset| offset.checked_add(u64::try_from(variable_len).ok()?))
        .context("zip local data offset overflow")?;
    ensure!(
        data_offset <= central_offset,
        "zip local header overlaps the central directory"
    );
    let mut variable = vec![0_u8; variable_len];
    archive_file.read_exact(&mut variable)?;
    ensure!(
        variable[..name_len] == *entry.name,
        "zip local and central entry names disagree"
    );
    ensure_unambiguous_zip_extra(&variable[name_len..])?;
    ensure!(
        data_offset
            .checked_add(u64::from(entry.compressed_size))
            .is_some_and(|end| end <= central_offset),
        "zip entry data overlaps the central directory"
    );
    Ok(())
}

fn ensure_unambiguous_zip_extra(mut extra: &[u8]) -> Result<()> {
    while !extra.is_empty() {
        ensure!(extra.len() >= 4, "malformed zip extra field");
        let kind = zip_u16(&extra[..2]);
        let value_len = usize::from(zip_u16(&extra[2..4]));
        let field_len = 4_usize
            .checked_add(value_len)
            .context("zip extra field length overflow")?;
        ensure!(field_len <= extra.len(), "truncated zip extra field");
        ensure!(
            kind != 0x0001 && kind != 0x7075 && kind != 0x9901,
            "ambiguous ZIP64, Unicode-path, or encrypted zip extra field"
        );
        extra = &extra[field_len..];
    }
    Ok(())
}

fn zip_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn zip_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
