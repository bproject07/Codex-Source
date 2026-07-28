# Changelog

All notable Codex Raw changes are documented in this file. Changes inherited
from upstream Codex remain documented in the
[upstream release history](https://github.com/openai/codex/releases).

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and uses semantic versioning for the `codex-raw` executable.

## [Unreleased]

### Added

- A release-bound benchmark report with sanitized per-sample data and exact
  release, commit, archive, binary, harness, environment, and methodology
  provenance.

### Changed

- The README now explains the removed Codex agent layer and the retained
  ChatGPT/Codex account controls near the top of the page.
- Future release archives include `BENCHMARKS.md`.
- Draft release verification now resolves the draft's numeric GitHub release
  ID before using the REST API, whose tag endpoint does not expose drafts.

## [0.1.0] - 2026-07-29

### Added

- A separate `codex-raw` executable with isolated local state.
- Stateless one-shot, persistent JSONL, and OpenAI-compatible local HTTP modes.
- Responses, Chat Completions, image generation, streaming, and function tools.
- `codex-raw update --check` and a user-triggered self-update implementation.
- Prebuilt release automation for Windows x64, Linux x64, Linux ARM64, and
  macOS Intel.
- Focused tests, OpenAI Python SDK smoke checks, and cross-platform validation.

### Security

- Loopback-only default for the HTTP server.
- Optional bearer-token protection and mandatory protection for non-loopback
  listeners.
- Explicit credential exclusions and separate Raw authentication storage.
- Immutable-release enforcement, exact asset selection, dual SHA-256
  verification, bounded archive extraction, and staged binary version checks.
- Exact-process restart handling on Windows and guarded systemd
  stop/restart/health-check with best-effort rollback handling on Linux.

[Unreleased]: https://github.com/bproject07/Codex-Source/compare/codex-raw-v0.1.0...HEAD
[0.1.0]: https://github.com/bproject07/Codex-Source/releases/tag/codex-raw-v0.1.0
