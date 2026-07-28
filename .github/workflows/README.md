# Fork workflow policy

This public fork intentionally carries only two workflows:

- `codex-raw-ci.yml` checks formatting, Clippy, nextest tests, and a locked
  release build for the `codex-raw` crate.
- `codex-raw-release.yml` builds four platform archives (Linux x64,
  GitHub-hosted native Linux ARM64, Windows x64, and macOS Intel) and
  publishes a GitHub release only for `codex-raw-v*` tags in
  `bproject07/Codex-Source`.

All third-party actions are pinned to full commit SHAs. The release workflow
has no manual trigger, validates the tag against Cargo metadata, and grants
`contents: write` only to the final publishing job. It does not inherit any
OpenAI signing, npm, WinGet, R2, or self-hosted-runner release flow.

Before the first release, a repository administrator must enable
**Settings → Releases → Enable release immutability** as described in
[GitHub's release protection instructions][immutable-releases]. This setting
applies only to future releases. After enabling it, set the repository
variable `CODEX_RAW_IMMUTABLE_RELEASES` to the exact value `enabled`. The
publish job treats that variable as a fail-closed gate and stops before
creating a draft when it is absent or different.

Protect `codex-raw-v*` with a repository tag ruleset that prevents tag updates
and deletion by normal release actors. The workflow resolves lightweight and
annotated tags immediately before draft creation, immediately before
publication, and again after GitHub reports the release immutable. Those checks
fail if the tag no longer points to the commit that triggered the workflow, but
the ruleset also closes the unavoidable API check-to-publish race window.

The workflow then follows GitHub's immutable-release guidance: it creates a
draft, packages the platform binary with the version-matched public Markdown
guides, benchmark report, and policies, uploads exactly the four archives plus
`SHA256SUMS`, verifies every draft asset's name, state, size, and SHA-256
digest, and only then publishes the draft. Draft verification uses the
release's numeric ID because GitHub's REST tag endpoint does not expose draft
releases. The workflow also checks GitHub's real `immutable` field after
publication.

If the gate was set incorrectly and GitHub reports a mutable release, the
workflow immediately deletes only the release, retains the tag, and fails.
Enable immutability and use **Re-run all jobs** for that workflow run. A
failure after draft creation but before publication leaves the draft for
inspection; delete that draft, but not its tag, before re-running.

[immutable-releases]: https://docs.github.com/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes
