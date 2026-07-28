# Fork workflow policy

This public fork intentionally carries only two workflows:

- `codex-raw-ci.yml` checks formatting, Clippy, nextest tests, and a locked
  release build for the `codex-raw` crate.
- `codex-raw-release.yml` builds three platform archives and publishes a
  GitHub release only for `codex-raw-v*` tags in `bproject07/Codex-Source`.

All third-party actions are pinned to full commit SHAs. The release workflow
has no manual trigger, validates the tag against Cargo metadata, and grants
`contents: write` only to the final publishing job. It does not inherit any
OpenAI signing, npm, WinGet, R2, or self-hosted-runner release flow.
