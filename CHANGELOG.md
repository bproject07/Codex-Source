# Changelog

All notable Codex Raw changes are documented in this file. Changes inherited
from upstream Codex remain documented in the
[upstream release history](https://github.com/openai/codex/releases).

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and uses semantic versioning for the `codex-raw` executable.

## [Unreleased]

## [0.1.0] - 2026-07-28

### Added

- A separate `codex-raw` executable with isolated local state.
- Stateless one-shot, persistent JSONL, and OpenAI-compatible local HTTP modes.
- Responses, Chat Completions, image generation, streaming, and function tools.
- Focused tests, SDK smoke checks, and cross-platform release automation.

### Security

- Loopback-only default for the HTTP server.
- Optional bearer-token protection and mandatory protection for non-loopback
  listeners.
- Explicit credential exclusions and separate Raw authentication storage.

[Unreleased]: https://github.com/bproject07/Codex-Source/compare/codex-raw-v0.1.0...HEAD
[0.1.0]: https://github.com/bproject07/Codex-Source/releases/tag/codex-raw-v0.1.0
