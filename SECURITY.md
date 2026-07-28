# Security Policy

This policy applies to the `bproject07/Codex-Source` fork and its fork-specific
changes. This repository is an independent derivative of
[`openai/codex`](https://github.com/openai/codex); it is not an official OpenAI
support or security-reporting channel.

## Supported versions

Security fixes are applied to the current default branch. Older commits,
development branches, locally modified builds, and third-party binaries may not
receive fixes.

## Report a vulnerability privately

Do not open a public issue, discussion, or pull request for a suspected
vulnerability.

Use GitHub's private vulnerability reporting for this repository:

[Report a vulnerability privately](https://github.com/bproject07/Codex-Source/security/advisories/new)

Include, when possible:

- the affected commit, build, and operating system;
- the affected component and configuration;
- reproduction steps or a minimal proof of concept;
- the expected impact and any known mitigations; and
- whether the issue has been disclosed elsewhere.

Do not include access tokens, account credentials, private prompts, or other
people's data. The maintainers will coordinate remediation and disclosure
through the private advisory.

If the issue affects an OpenAI service, account, or unmodified upstream Codex
rather than this fork, use
[OpenAI's security policy](https://github.com/openai/codex/security/policy)
instead.

## Fork-specific security boundaries

Codex Raw's HTTP API defaults to token-free loopback operation for trusted local
clients. It supports a shared bearer token and refuses non-loopback listeners
without one. The token protects `/readyz` and `/v1`; `/healthz` intentionally
remains open. This is not per-user authorization, and the built-in server uses
plain HTTP. For network use, place it behind a hardened reverse proxy that
enforces TLS, connection/header timeouts, and connection limits; restrict the
Raw port with a firewall and prevent direct public access. See the
[Codex Raw security essentials](README.md#security-essentials) for the complete
operating guidance.

Users remain responsible for reviewing commands, protecting local state, and
using sandbox, approval, and network controls appropriate to their environment.
