# Contributing

Thank you for improving Codex-Source. This repository is an independent fork of
[`openai/codex`](https://github.com/openai/codex), with Codex Raw and other
fork-maintained changes. OpenAI does not review or endorse contributions made
to this fork.

## Before you start

Search existing issues and pull requests before opening a new one. A focused
bug fix can go directly to a pull request. For a new feature, public API change,
large refactor, or behavior that may complicate upstream synchronization, open
an issue first so scope and design can be discussed.

Security vulnerabilities must not be discussed publicly. Follow the
[security policy](../SECURITY.md) to report them privately.

All participation is subject to the
[Code of Conduct](../CODE_OF_CONDUCT.md).

## Choose the right project

Use this repository for bugs and changes caused by this fork, including Codex
Raw. If a problem reproduces in an unmodified current release of upstream
Codex, search or report it in
[`openai/codex`](https://github.com/openai/codex/issues). If you are uncertain,
say which build and commit you tested so the maintainers can route it.

## Development workflow

1. Fork the repository and create a focused topic branch from the current
   default branch.
2. Read the repository's `AGENTS.md` and the instructions closest to the files
   you change.
3. Keep unrelated fixes in separate pull requests.
4. Add or update meaningful tests for behavior changes.
5. Update user-facing documentation when commands, configuration, protocols, or
   security behavior change.
6. Run the formatting, lint, and test commands required by `AGENTS.md` for the
   affected component.
7. Complete the pull request template with the rationale, verification, and
   user-visible impact.

Do not commit generated build output, local credentials, authentication state,
private prompts, or logs containing sensitive data.

## Keeping upstream and fork changes reviewable

Codex-Source retains substantial upstream code. Keep mechanical upstream syncs
separate from fork features whenever practical. In a pull request, identify:

- whether the change affects inherited Codex behavior, fork-specific behavior,
  or both;
- any upstream commit or issue the change is based on; and
- any expected conflict or maintenance cost for future upstream synchronization.

Preserve applicable copyright, license, and attribution notices. Do not imply
that OpenAI authored, reviewed, or supports fork-specific changes.

## Pull request expectations

A reviewable pull request should:

- explain the problem and why the proposed approach is appropriate;
- link related issues or upstream context;
- stay narrowly scoped and avoid drive-by formatting;
- include tests or explain why tests are not applicable;
- document compatibility or migration impact; and
- pass the checks relevant to the changed component.

Maintainers may ask for changes, split a pull request, or decline work that is
out of scope or too costly to maintain. Opening an issue before substantial
work reduces that risk but does not guarantee acceptance.

## Contribution licensing

This project uses Apache-2.0 for both outbound and inbound licensing. By
intentionally submitting a contribution, you agree to license it under the
[Apache License 2.0](../LICENSE) and represent that you have the right to do so.
No separate OpenAI CLA or CLA bot acceptance is required for this fork. See the
[contribution licensing policy](CLA.md) for details.
