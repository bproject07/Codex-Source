# Install Codex Raw

This guide covers both supported installation paths:

- **Prebuilt release archive** — recommended for most users; Rust is not
  required.
- **Build from source** — intended for contributors, auditors, and platforms
  without a published binary.

Codex Raw installs as a separate `codex-raw` executable. It must not replace
the standard `codex` executable.

## If no release is listed

Published binaries appear on the
[GitHub Releases page](https://github.com/bproject07/Codex-Source/releases).
If that page does not contain a release yet, build from source. GitHub's
automatically generated “Source code” ZIP and TAR files contain source code,
not a runnable `codex-raw` binary.

## Prebuilt releases

Every release archive contains one platform binary, the version-matched public
guides and policies, the Raw implementation notes, `LICENSE`, and `NOTICE`
inside a directory named:

```text
codex-raw-<version>-<target>/
```

Choose the archive that exactly matches the operating system and CPU:

| System        | CPU              | Release archive                                        |
| ------------- | ---------------- | ------------------------------------------------------ |
| Windows       | Intel/AMD 64-bit | `codex-raw-<version>-x86_64-pc-windows-msvc.zip`       |
| Linux (glibc) | Intel/AMD 64-bit | `codex-raw-<version>-x86_64-unknown-linux-gnu.tar.gz`  |
| Linux (glibc) | ARM64/AArch64    | `codex-raw-<version>-aarch64-unknown-linux-gnu.tar.gz` |
| macOS         | Intel 64-bit     | `codex-raw-<version>-x86_64-apple-darwin.tar.gz`       |

There is currently no prebuilt Windows ARM64 or native macOS Apple Silicon
archive. The Linux archives target glibc and are not Alpine/musl binaries.
Other targets may be buildable from source, but they are not part of the
published binary or self-update matrix.

> [!IMPORTANT]
> Running an ARM64 binary on an Intel/AMD computer, or the reverse, produces an
> operating-system error such as `Exec format error`. That error means the
> wrong archive was selected; it does not mean the release binary is damaged.

### Verify the download

Download `SHA256SUMS` from the same release as the archive. Before publication,
the release workflow runs each binary on its native build runner and verifies
the complete archive set and its SHA-256 digests. It does not currently
code-sign or notarize the artifacts. Verify the checksum and review the source
before overriding any operating-system trust warning.

On Windows PowerShell:

```powershell
$archive = "codex-raw-0.1.0-x86_64-pc-windows-msvc.zip" # Use the release version.
$line = Get-Content .\SHA256SUMS |
  Where-Object { $_ -match "  $([regex]::Escape($archive))$" }

if (@($line).Count -ne 1) {
  throw "The archive has no unique SHA256SUMS entry."
}

$expected = ($line -split "\s+")[0].ToLowerInvariant()
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()

if ($actual -ne $expected) {
  throw "SHA-256 verification failed."
}
```

On Linux:

```shell
archive="codex-raw-0.1.0-x86_64-unknown-linux-gnu.tar.gz" # Use your release/CPU.
expected="$(awk -v name="$archive" '$2 == name { print $1 }' SHA256SUMS)"
actual="$(sha256sum "$archive" | awk '{ print $1 }')"
test -n "$expected" && test "$actual" = "$expected"
```

On macOS:

```shell
archive="codex-raw-0.1.0-x86_64-apple-darwin.tar.gz" # Use the release version.
expected="$(awk -v name="$archive" '$2 == name { print $1 }' SHA256SUMS)"
actual="$(shasum -a 256 "$archive" | awk '{ print $1 }')"
test -n "$expected" && test "$actual" = "$expected"
```

No output from the final `test` command means the checksum matched. A non-zero
exit status means the file must not be installed.

### Windows x64

After verifying and extracting the ZIP:

```powershell
$version = "0.1.0" # Replace with the downloaded release version.
$bundle = "codex-raw-$version-x86_64-pc-windows-msvc"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\codex-raw"

Expand-Archive -LiteralPath "$bundle.zip" -DestinationPath . -Force
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item -LiteralPath ".\$bundle\codex-raw.exe" `
  -Destination (Join-Path $installDir "codex-raw.exe") -Force

$rawExe = Join-Path $installDir "codex-raw.exe"
& $rawExe --version

$userPathEntries = @(
  [Environment]::GetEnvironmentVariable("Path", "User") -split ";" |
    Where-Object { $_ }
)
if ($userPathEntries -notcontains $installDir) {
  [Environment]::SetEnvironmentVariable(
    "Path",
    (($userPathEntries + $installDir) -join ";"),
    "User"
  )
}

$env:Path = "$installDir;$env:Path"
codex-raw --version
```

The final two commands activate `codex-raw` in the current shell. The user
`PATH` change applies automatically to new terminals.

### Linux x64 or ARM64

After verifying the matching TAR archive:

```shell
version="0.1.0" # Replace with the downloaded release version.

case "$(uname -m)" in
  x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
  aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
  *) echo "No prebuilt Linux archive for $(uname -m)" >&2; exit 1 ;;
esac

bundle="codex-raw-${version}-${target}"
tar -xzf "${bundle}.tar.gz"
install -d "$HOME/.local/bin"
install -m 0755 "${bundle}/codex-raw" "$HOME/.local/bin/codex-raw"
export PATH="$HOME/.local/bin:$PATH"
codex-raw --version
```

Add `export PATH="$HOME/.local/bin:$PATH"` to the shell profile to keep it in
future terminals. A system-wide or service installation may instead use a
root-owned managed layout; updating such an installation requires `sudo`.

### macOS Intel

After verifying the TAR archive:

```shell
version="0.1.0" # Replace with the downloaded release version.
bundle="codex-raw-${version}-x86_64-apple-darwin"

tar -xzf "${bundle}.tar.gz"
install -d "$HOME/.local/bin"
install -m 0755 "${bundle}/codex-raw" "$HOME/.local/bin/codex-raw"
export PATH="$HOME/.local/bin:$PATH"
codex-raw --version
```

Add `export PATH="$HOME/.local/bin:$PATH"` to the shell profile to keep it in
future terminals.

The current macOS archive is Intel-only and is not notarized. No native Apple
Silicon release or self-update target is currently published.

## Build from source

Clone the source repository when you want to inspect the implementation,
contribute changes, or build without a matching release archive:

```shell
git clone https://github.com/bproject07/Codex-Source.git
cd Codex-Source/codex-rs
```

If the
[main-branch source ZIP](https://github.com/bproject07/Codex-Source/archive/refs/heads/main.zip)
was downloaded instead, extract it and enter `Codex-Source-main/codex-rs`.

Release CI uses Rust `1.95.0`. Install Rust through
[rustup](https://rustup.rs/) and select the same toolchain:

```shell
rustup toolchain install 1.95.0
cargo +1.95.0 build --locked --release --package codex-raw
```

Platform-native C/C++ build tools and system libraries may also be required.
On Windows, use the MSVC Rust target with the Visual Studio C++ Build Tools.
The prebuilt release is the simpler path when a supported archive exists.

The executable is created at:

```text
Windows: codex-rs\target\release\codex-raw.exe
Unix:   codex-rs/target/release/codex-raw
```

Install only that executable.

On Windows PowerShell, while still in `Codex-Source\codex-rs`:

```powershell
$installDir = Join-Path $env:LOCALAPPDATA "Programs\codex-raw"
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item -LiteralPath ".\target\release\codex-raw.exe" `
  -Destination (Join-Path $installDir "codex-raw.exe") -Force
$env:Path = "$installDir;$env:Path"
codex-raw --version
```

Use the persistent user `PATH` block in [Windows x64](#windows-x64) if this is
the first installation.

On Linux or macOS, while still in `Codex-Source/codex-rs`:

```shell
install -d "$HOME/.local/bin"
install -m 0755 target/release/codex-raw "$HOME/.local/bin/codex-raw"
export PATH="$HOME/.local/bin:$PATH"
codex-raw --version
```

Add the export to the shell profile for future terminals. Build output under
`target/` is generated and must not be committed.

Contributors should follow [CONTRIBUTING.md](CONTRIBUTING.md) and the
repository's `AGENTS.md` before making changes.

## First-run verification

Check that Raw and standard Codex resolve to different files:

```shell
codex-raw --version
codex-raw --help
```

On Linux or macOS:

```shell
command -v codex
command -v codex-raw
```

On Windows PowerShell:

```powershell
(Get-Command codex -ErrorAction SilentlyContinue).Source
(Get-Command codex-raw -ErrorAction SilentlyContinue).Source
```

Their state is separate as well:

```text
Standard Codex: ~/.codex
Codex Raw:      ~/.codex-raw
```

Continue with the [README quick start](README.md#quick-start).

## Updating

Builds that expose the `update` command include self-update. It is
**user-triggered**, not a background polling service.

Check without changing files or stopping a service:

```shell
codex-raw update --check
```

Install a newer verified stable release:

```shell
codex-raw update
```

For a root-owned Linux installation:

```shell
sudo codex-raw update
```

The updater:

1. Detects the current operating system and CPU.
2. Selects the exact archive for that platform.
3. Accepts only a newer stable immutable GitHub release.
4. Verifies GitHub's asset digest and the exact `SHA256SUMS` entry.
5. Extracts and version-checks a staged executable.
6. Replaces the installed version and preserves rollback data.

It reports an equal SemVer precedence as already up to date without
reinstalling it. Older releases, drafts, prereleases, mutable releases,
ambiguous assets, invalid archives, and checksum mismatches are rejected.

On Windows, a detached helper waits for the exact installed executable and
uses Windows Restart Manager to request graceful shutdown from processes whose
identity and full image path match it. It attempts to restart eligible
registered application processes, but successful relaunch is not guaranteed.
It does not kill target Raw processes by name or force-kill them.

On Linux, service management applies only to the active fixed unit
`codex-raw-api.service` after both its `MainPID` executable and `api-server`
command line are verified. A mismatch aborts the update. If startup or health
verification fails, Raw attempts best-effort rollback only after safely
stopping the failed updated service; if that safe stop fails, rollback is not
attempted. Other running Unix processes must be restarted manually.

## Troubleshooting

| Symptom                                       | Likely cause                                        | Resolution                                                    |
| --------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------------- |
| `Exec format error`                           | Binary CPU architecture does not match the computer | Download the x64 or ARM64 archive that matches `uname -m`     |
| `Permission denied` on Linux/macOS            | Executable bit is missing                           | Run `chmod 0755 /path/to/codex-raw`                           |
| Missing `GLIBC_*` symbol or failure on Alpine | The published Linux binary targets glibc, not musl  | Use a compatible glibc distribution or build from source      |
| `self-update is not supported on …`           | No updater asset exists for this OS/CPU             | Build from source; self-update is unavailable for that target |
| Update cannot write the installation          | The executable is root-owned                        | Run `sudo codex-raw update` after reviewing the release       |
| No release is found                           | No public immutable release is currently available  | Build from source and retry after a release is available      |

For usage questions, see [SUPPORT.md](SUPPORT.md). Report suspected
vulnerabilities privately as described in [SECURITY.md](SECURITY.md).
