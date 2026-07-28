# Upstream provenance

Codex Raw is an independent community extension built inside the
[OpenAI Codex](https://github.com/openai/codex) source tree. It is not
affiliated with, endorsed by, or supported by OpenAI.

The first public-ready source snapshot is based on upstream commit
[`4f6eaf7af9d975b822d1d658ba1893d6d5a87646`](https://github.com/openai/codex/commit/4f6eaf7af9d975b822d1d658ba1893d6d5a87646)
from 2026-07-28. The project retains the upstream Apache-2.0 license and
notices. Raw-specific modifications are concentrated in `codex-rs/raw-cli`,
with the small workspace, launcher, documentation, and automation changes
needed to build and maintain that executable.

## Update policy

- Treat `openai/codex` as a read-only upstream.
- Integrate upstream changes on a temporary `upgrade/codex-raw-YYYYMMDD`
  branch.
- Review shared manifests and lockfiles instead of resolving them wholesale.
- Run the focused Raw checks on every supported platform before promotion.
- Publish releases only from a reviewed commit on this repository's `main`
  branch and tag them as `codex-raw-vMAJOR.MINOR.PATCH`.

Upstream Codex changes and releases are documented in the
[official repository](https://github.com/openai/codex). Codex Raw changes are
documented in this repository's `CHANGELOG.md`.
