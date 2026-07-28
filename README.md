# Codex Raw

> [!WARNING]
> Codex Raw is an independent, unofficial community fork. It is not affiliated
> with, endorsed by, or supported by OpenAI. “OpenAI” and “Codex” are marks of
> their respective owner. This project is not an OpenAI API service or a source
> of API credentials.

Codex Raw is a minimal, stateless model client built as an isolated crate inside
the Codex source tree. It uses the existing ChatGPT sign-in and model transport
from Codex, but it does not start the Codex agent pipeline or automatically add
agent instructions, tools, repository context, or conversation history.

The result is a separate executable named `codex-raw`. It can be used as a
one-shot command, a persistent JSONL process, or an OpenAI-compatible HTTP API
that defaults to loopback.

> [!IMPORTANT]
> Codex Raw is an experimental fork extension, not the standard Codex coding
> agent. It is designed for callers that want explicit control over every item
> sent to the model. Account entitlements, rate limits, safety controls, and
> server-side request framing still apply.

## Why Codex Raw exists

The standard Codex CLI is a complete coding agent. A turn may include model
instructions, tool definitions, sandbox and approval rules, repository
instructions, workspace metadata, skills, plugins, MCP context, and prior
conversation state.

Codex Raw intentionally takes a smaller path. This command:

```shell
codex-raw hello
```

creates one stateless Responses request containing one user message with the
text `hello`. The one-shot mode sets empty instructions, supplies no tools,
disables parallel tool calls, and does not store or resume a conversation.

Conceptually, the request is:

```json
{
  "model": "<selected-model>",
  "instructions": "",
  "input": [
    {
      "type": "message",
      "role": "user",
      "content": [{"type": "input_text", "text": "hello"}]
    }
  ],
  "tools": null,
  "tool_choice": "none",
  "parallel_tool_calls": false,
  "store": false,
  "stream": true
}
```

This is a conceptual representation. Models that use the Responses Lite
transport require additional protocol framing, such as a reasoning context
field and an internal transport header. That framing is not a Codex agent
prompt. Backend token accounting can therefore report more tokens than the
literal user text alone; in the recorded `hello` tests, Raw consistently
reported 7 input tokens.

## Features

- Separate `codex-raw` executable; the standard `codex` executable is not
  replaced or modified.
- Separate state directory at `~/.codex-raw` by default.
- ChatGPT browser sign-in with proactive token refresh.
- Minimal one-shot streaming requests.
- Persistent JSONL `app-server` mode for low-overhead child-process clients.
- `api-server` mode with a safe loopback default and optional bearer-token
  protection; non-loopback listeners require a token.
- Responses and Chat Completions JSON responses.
- Subscription-backed image generation through `gpt-image-2`.
- Typed Responses SSE and Chat Completions SSE streaming.
- Function tool schemas, model function calls, and function-call outputs.
- Model discovery through `/v1/models`.
- Strict validation for the intentionally small compatibility surface.
- A reusable latency benchmark and an OpenAI Python SDK smoke test.

Codex Raw deliberately does not provide:

- Codex base instructions or an agent loop;
- shell, filesystem, or other built-in tools;
- repository or workspace discovery;
- `AGENTS.md`, skills, plugins, apps, or MCP loading;
- sandbox or approval workflows;
- hidden conversation persistence or thread resume;
- automatic execution of caller-defined functions;
- image or audio input;
- a multi-user authorization system or a safely exposed public service.

## Repository layout

```text
codex-rs/
  Cargo.toml                         workspace registration
  Cargo.lock                         locked codex-raw dependencies
  raw-cli/
    Cargo.toml                       crate definition
    BUILD.bazel                      Bazel target
    README.md                        implementation notes
    scripts/
      benchmark_persistent.ps1       paired app-server/API benchmark
      openai_sdk_smoke.py            Responses and Chat SDK smoke test
    src/
      lib.rs                         CLI, login, and Raw home isolation
      runtime.rs                     shared auth, model, and HTTP runtime
      app_server.rs                  persistent JSONL server
      app_server_wire.rs             app-server wire helpers
      api_server.rs                  local HTTP server and routes
      api_server_wire.rs             Responses request parsing
      api_server_chat_wire.rs        Chat Completions translation
      api_server_image_wire.rs       Image generation validation and mapping
      api_server_response.rs         non-streaming response mapping
      api_server_stream.rs           Responses SSE encoding
      api_server_chat_stream.rs      Chat Completions SSE encoding
      api_server_http_stream.rs      upstream stream adaptation
      api_server_wire_validation.rs  tool-call correlation validation
      *_tests.rs                     unit and protocol tests
```

The product implementation is concentrated in `codex-rs/raw-cli`. Supporting
public documentation and Windows debug launchers live at the repository root
and under `.vscode`; the standard Codex agent source remains unmodified. The
Bazel target lives inside `codex-rs/raw-cli/BUILD.bazel`.

There is one Codex Raw source tree: `codex-rs/raw-cli`. Build output under
`target/` and the currently deployed executable under `dist/` are artifacts,
not source copies. Superseded deploy builds should be removed after the current
build passes health checks.

## Project documentation

- [Security policy](SECURITY.md)
- [Contributing guide](CONTRIBUTING.md)
- [Code of conduct](CODE_OF_CONDUCT.md)
- [Support boundaries](SUPPORT.md)
- [Changelog](CHANGELOG.md)
- [Upstream provenance](UPSTREAM.md)

## Documentation maintenance

Any change to Codex Raw commands, endpoints, request fields, defaults, model
controls, authentication, or security behavior must update all applicable
documentation before the change series is published:

- the rendered Clap help in `codex-rs/raw-cli/src/lib.rs`;
- this public README;
- `codex-rs/raw-cli/README.md` implementation notes.

Before committing, render `codex-raw --help` and
`codex-raw api-server --help`, then run the crate-specific formatting, tests,
and lint checks.

## Build from source

Install Git, [Rust through rustup](https://rustup.rs/), `just`, and the native
build tools required by your platform. The checked-in `rust-toolchain.toml`
selects the supported Rust toolchain.

Clone this repository and build only the Raw package:

```shell
git clone https://github.com/bproject07/Codex-Source.git
cd Codex-Source/codex-rs
cargo build --release -p codex-raw
```

The release executable is created at:

```text
Windows: codex-rs\target\release\codex-raw.exe
Unix:   codex-rs/target/release/codex-raw
```

When Rust dependencies change, regenerate and verify the repository Bazel lock
from the repository root:

```shell
just bazel-lock-update
just bazel-lock-check
```

Build output under `target/` is intentionally not committed.

Contributors should also run the focused validation from `codex-rs`:

```shell
just test -p codex-raw
just fix -p codex-raw
just fmt
```

## Debug with VS Code on Windows

Install the VS Code CodeLLDB and rust-analyzer extensions, then open the checked
in workspace with:

```text
Open Codex Source in VS Code.cmd
```

The launcher adds the default Rustup and Scoop MinGW locations to `PATH` and
selects `1.95.0-x86_64-pc-windows-gnu`. The workspace deliberately disables
whole-workspace rust-analyzer checks, build scripts, and proc-macro loading so
opening this large repository does not compile unrelated crates such as V8.

The Run and Debug menu provides:

- the existing `Codex TUI: Cargo launch` profile;
- `Codex Raw: API server (Windows GNU, debug :8081)`;
- `Codex Raw: One-shot prompt (Windows GNU)`;
- `Codex Raw: App server (Windows GNU, JSONL)`;
- `Codex Raw: Attach to running process`.

Selecting a Codex Raw launch profile and pressing F5 builds only the
`codex-raw` binary with full debug information. The API profile uses
`127.0.0.1:8081`, leaving a deployed server on port 8080 untouched. Keep
deployed executables under `dist/` rather than running the copy under `target/`,
because Windows cannot replace a running executable during the next F5 build.

## Install

Codex Raw must remain a separate executable. Do not rename it to `codex` or copy
it over an existing Codex installation.

### Windows PowerShell

Run this after the release build from `codex-rs`:

```powershell
$rawInstallDir = Join-Path $env:LOCALAPPDATA 'Programs\codex-raw'
New-Item -ItemType Directory -Force -Path $rawInstallDir | Out-Null
Copy-Item '.\target\release\codex-raw.exe' `
  (Join-Path $rawInstallDir 'codex-raw.exe')
$rawExe = Join-Path $rawInstallDir 'codex-raw.exe'
& $rawExe --version
```

Use `$rawExe` directly or add that directory to your user `PATH` if you want the
`codex-raw` command to be available in new terminals.

### Linux and macOS

Run this after the release build from `codex-rs`:

```shell
install -d "$HOME/.local/bin"
install -m 0755 target/release/codex-raw "$HOME/.local/bin/codex-raw"
"$HOME/.local/bin/codex-raw" --version
```

Ensure `$HOME/.local/bin` is on your user `PATH`. This installation does not
touch a system or user `codex` executable.

### Update the installed binary

Check the current version and look for a newer stable release:

```shell
codex-raw --version
codex-raw update --check
```

Install the verified platform binary from this repository's latest immutable
GitHub release:

```shell
codex-raw update
```

For a root-owned Linux installation, use `sudo codex-raw update`. Linux x64 and
ARM64, Windows x64, and macOS Intel release binaries are supported. The updater
requires an exact stable SemVer release from `bproject07/Codex-Source`, verifies
both GitHub's SHA-256 asset digest and the exact `SHA256SUMS` entry, stages and
checks the new binary, and retains the previous binary as a rollback backup.
It refuses same-version installs, downgrades, draft/prerelease releases, and
releases that GitHub does not mark immutable.

On Windows, a detached helper proves that it owns the same updater byte-range
lock before the parent exits. It uses Windows Restart Manager to request a
graceful shutdown only from processes whose identity and full image path match
the exact installed `codex-raw.exe`; it never force-kills or kills by name. All
waits are bounded and failures leave the update closed, with a status file for
diagnosis. After verification, Restart Manager restarts eligible registered
processes and services; a restart is also attempted after replacement failure
or rollback, and restart errors are reported. On Linux, an active
`codex-raw-api.service` is stopped and restarted only when its `MainPID` is
verified to use the exact binary being updated. If startup or verification
fails, Raw attempts a best-effort rollback only after safely stopping the
failed service; rollback or recovery errors are reported. Enable immutable
releases in the public repository before publishing updater-compatible releases.

Self-update is available from `codex-raw 0.1.0` onward. Any pre-`0.1.0`
installation, including one reporting `0.0.0`, needs one manual verified
replacement with an updater-enabled release. Until the public repository contains its first
immutable release and exact platform assets, only the local selection,
checksum, extraction, replacement, and rollback tests can run; a real GitHub
fetch cannot be claimed as an end-to-end update test.

### Verify that the standard Codex installation is unchanged

The build creates only `target/release/codex-raw` (or
`codex-raw.exe`). The install examples copy only that file into a new
`codex-raw` directory or filename. They never write to the path returned for
the standard `codex` command.

On Windows PowerShell, compare the two executable locations:

```powershell
$standardCodex = Get-Command codex -ErrorAction SilentlyContinue
$rawCommand = Get-Command codex-raw -ErrorAction SilentlyContinue

$standardCodex.Source
$rawCommand.Source
```

If the Raw install directory is not on `PATH`, compare it directly:

```powershell
(Get-Command codex -ErrorAction SilentlyContinue).Source
$rawExe
```

On Linux or macOS:

```shell
command -v codex
command -v codex-raw
```

The paths must be different. The state directories are also different:

```text
Standard Codex: ~/.codex
Codex Raw:      ~/.codex-raw
```

Building, installing, signing in, refreshing a token, or logging out through
`codex-raw` does not write to the standard executable or its state directory.

## Authentication and isolated state

Sign in once with ChatGPT:

```shell
codex-raw login
codex-raw status
```

The login opens a local callback server and a browser-based sign-in flow.
Credentials, the model cache, and the Raw ownership marker are stored under:

```text
~/.codex-raw
```

Raw deliberately uses file-backed credential storage inside that isolated
directory. Treat `~/.codex-raw/auth.json` as a plaintext secret: restrict access
to your user, never copy it into a repository or support report, and remove it
with `codex-raw logout` when it is no longer needed. The
[official Codex authentication documentation](https://learn.chatgpt.com/docs/auth#login-caching)
also describes local login caching and automatic ChatGPT token refresh.

You can select a different, dedicated directory:

```shell
codex-raw --home /path/to/new/raw-state login
```

or set `CODEX_RAW_HOME` for the current process environment.

Codex Raw refuses known Codex state directories and descendants. It also
refuses an existing directory unless it contains the valid Raw ownership
marker. The path guard is lexical; do not use a symbolic link or junction to
point a Raw home at another application's state directory.

Raw accepts its own ChatGPT login only. API-key and arbitrary-header auth modes
are disabled. Before `status` and every model request, the shared Codex auth
manager checks whether the access token needs to be refreshed. Refreshed
credentials are written only to the Raw home. There is no background daemon.

If a refresh token is expired, revoked, or invalid, sign in again:

```shell
codex-raw logout
codex-raw login
```

Logging out removes only the Raw credentials.

## One-shot CLI

```shell
codex-raw --help
codex-raw hello
codex-raw "Explain this in one sentence."
codex-raw send hello
codex-raw -m gpt-5.4-mini hello
codex-raw --model gpt-5.4-mini send "Reply briefly"
```

An unrecognized subcommand is treated as prompt text, which makes
`codex-raw hello` equivalent to `codex-raw send hello`. Multiple prompt
arguments are joined with one space.

If `--model` is omitted, Raw uses the active default from the account's model
catalog. Model availability can change with account entitlements and the remote
catalog.

The assistant text is streamed to stdout. Status information and the final
upstream usage object are written to stderr:

```text
[usage] {"input_tokens":7,"cached_input_tokens":0,"output_tokens":13,...}
```

The process exits non-zero for authentication, transport, stream, or
missing-usage failures.

## Persistent app-server

Start the JSONL process:

```shell
codex-raw -m gpt-5.4-mini app-server
```

The process reads one JSON object per line from stdin and writes one JSON object
per line to stdout. Every line is flushed immediately. This is a deliberately
small subset of the Codex app-server v2 protocol; it does not include a
`"jsonrpc":"2.0"` field.

Initialize the process:

```json
{"id":1,"method":"initialize","params":{"clientInfo":{"name":"example","version":"1.0.0"}}}
{"method":"initialized"}
```

Create an ephemeral protocol thread:

```json
{"id":2,"method":"thread/start","params":{"model":"gpt-5.4-mini","ephemeral":true}}
```

Start a turn with exactly one non-empty text item:

```json
{"id":3,"method":"turn/start","params":{"threadId":"raw-thread-1","input":[{"type":"text","text":"hello"}]}}
```

The server returns the matching request response before lifecycle
notifications. A successful turn emits:

```text
turn/started
item/started                 user message
item/completed               user message
item/started                 assistant message
item/agentMessage/delta      zero or more streamed text deltas
item/completed               complete assistant message
thread/tokenUsage/updated    exact upstream usage
turn/completed
```

Supported methods are `initialize`, `initialized`, `thread/start`, and
`turn/start`. Numeric and string request IDs are accepted. Unsupported methods
return `-32601`; invalid parameters return `-32602`.

The protocol thread is only a client handle. Raw does not send previous turns
back to the model, so sequential turns remain model-stateless. The persistent
process reuses authentication state, model metadata, the HTTP client, and its
connection pool. It checks token refresh before every turn.

In app-server mode, stdout is protocol-only. Clients must read stderr
concurrently so a full diagnostic pipe cannot block the child process.

## OpenAI-compatible HTTP API

Start the HTTP server:

```shell
codex-raw -m gpt-5.4-mini api-server \
  --listen 127.0.0.1:8080 \
  --max-concurrency 16
```

On PowerShell, the same command fits on one line:

```powershell
codex-raw -m gpt-5.4-mini api-server --listen 127.0.0.1:8080 --max-concurrency 16
```

The server runs in the foreground and stops on `Ctrl+C`. Loopback is the safe
default and can run without inbound authentication for local SDK compatibility.
To protect loopback or to listen on any non-loopback address, configure a
strong random token. Prefer `CODEX_RAW_API_TOKEN` so the secret is not recorded
in shell history or process arguments:

```powershell
$tokenBytes = [byte[]]::new(32)
[Security.Cryptography.RandomNumberGenerator]::Fill($tokenBytes)
$env:CODEX_RAW_API_TOKEN = [Convert]::ToHexString($tokenBytes)
codex-raw -m gpt-5.4-mini api-server --listen 0.0.0.0:8080 --max-concurrency 16
```

Raw refuses a non-loopback bind when no token is configured. With a token,
`/readyz` and every `/v1` route require `Authorization: Bearer TOKEN`;
`/healthz` remains open for process liveness checks. The built-in server still
uses plain HTTP, so place network-facing traffic behind a hardened reverse proxy
that enforces TLS, connection/header timeouts, and connection limits. Firewall
the Raw port so only that proxy or explicitly trusted clients can reach it.
Remote clients must use the machine's real LAN IP or DNS name instead of
`0.0.0.0`. Tokens must contain 32-512 RFC 6750 bearer-token characters; the
example above generates a 64-character hexadecimal token.

### Endpoints

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/healthz` | Process liveness; always unauthenticated |
| `GET` | `/readyz` | Current Raw login readiness; returns 503 when unavailable |
| `GET` | `/v1/models` | Text model catalog plus `gpt-image-2` |
| `POST` | `/v1/responses` | Responses JSON or typed SSE |
| `POST` | `/v1/chat/completions` | Chat Completions JSON or SSE |
| `POST` | `/v1/images/generations` | Subscription-backed base64 image generation |

### PowerShell requests

PowerShell can alter quotes in native JSON arguments, so these examples pipe
the exact body to `curl.exe`. They assume the default token-free loopback mode:

```powershell
curl.exe --silent http://127.0.0.1:8080/readyz
curl.exe --silent http://127.0.0.1:8080/v1/models
'{"model":"gpt-5.4-mini","input":"hello"}' | curl.exe --silent http://127.0.0.1:8080/v1/responses -H "Content-Type: application/json" --data-binary '@-'
'{"model":"gpt-5.4-mini","messages":[{"role":"user","content":"hello"}]}' | curl.exe --silent http://127.0.0.1:8080/v1/chat/completions -H "Content-Type: application/json" --data-binary '@-'
```

Generate and save an image with the ChatGPT subscription login:

```powershell
$image = Invoke-RestMethod `
  -Method Post `
  -Uri http://127.0.0.1:8080/v1/images/generations `
  -ContentType "application/json" `
  -Body '{"prompt":"A crystal observatory above a storm","model":"gpt-image-2","quality":"high","size":"1536x1024"}'

$outputPath = Join-Path (Get-Location) "codex-raw-image.png"
[IO.File]::WriteAllBytes(
  $outputPath,
  [Convert]::FromBase64String($image.data[0].b64_json)
)
$outputPath
```

Streaming Responses request:

```powershell
'{"model":"gpt-5.4-mini","input":"hello","stream":true,"parallel_tool_calls":false}' | curl.exe -N http://127.0.0.1:8080/v1/responses -H "Content-Type: application/json" --data-binary '@-'
```

For a token-protected server, add the bearer header:

```powershell
curl.exe --silent http://127.0.0.1:8080/v1/models `
  -H "Authorization: Bearer $env:CODEX_RAW_API_TOKEN"
```

### POSIX shell requests

```shell
curl -sS http://127.0.0.1:8080/readyz
curl -sS http://127.0.0.1:8080/v1/models
curl -sS http://127.0.0.1:8080/v1/responses \
  -H 'Content-Type: application/json' \
  --data '{"model":"gpt-5.4-mini","input":"hello"}'
curl -sS http://127.0.0.1:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  --data '{"model":"gpt-5.4-mini","messages":[{"role":"user","content":"hello"}]}'
curl -sS http://127.0.0.1:8080/v1/images/generations \
  -H 'Content-Type: application/json' \
  --data '{"prompt":"A crystal observatory above a storm","model":"gpt-image-2","quality":"high","size":"1536x1024"}'
```

For a protected server, add
`-H "Authorization: Bearer $CODEX_RAW_API_TOKEN"` to each `/readyz` or `/v1`
request.

### OpenAI Python SDK

The local server works with the standard client shape:

```python
import os

from openai import OpenAI

client = OpenAI(
    api_key=os.environ.get("CODEX_RAW_API_TOKEN", "raw-local"),
    base_url="http://127.0.0.1:8080/v1",
)

response = client.responses.create(
    model="gpt-5.4-mini",
    input="hello",
)

print(response.output_text)
print(response.usage)
```

`raw-local` is only a placeholder required by the SDK constructor for the
token-free loopback mode. When bearer authentication is configured, the SDK
sends `CODEX_RAW_API_TOKEN` as its authorization value. Raw never forwards that
inbound token upstream; upstream authentication always comes from the isolated
ChatGPT login.

### Supported request surface

The Responses endpoint accepts:

- optional `model`, otherwise the server default;
- `input` as a string or an explicit array of text messages, reasoning items,
  function calls, and matching function-call outputs;
- caller-supplied `instructions`;
- function tools;
- `tool_choice` values `none`, `auto`, and `required`;
- `parallel_tool_calls`, `reasoning.effort`, `service_tier`, and `stream`;
- `store` omitted, `null`, or `false`.

The Chat Completions endpoint accepts text and function-tool-loop `system`,
`developer`, `user`, `assistant`, and `tool` messages, function tools, `n=1`,
`stream`, `stream_options.include_usage`, `reasoning_effort`, `service_tier`,
and `store` omitted, `null`, or `false`.

`tool_choice` defaults to `auto` when function tools are supplied and to `none`
otherwise. `parallel_tool_calls` defaults to true, except that Responses Lite
models force it to false. Known reasoning effort names are `none`, `minimal`,
`low`, `medium`, `high`, `xhigh`, `max`, and `ultra`; other non-empty names are
forwarded for future compatibility. Model and account support still determines
whether an effort or `service_tier` is accepted upstream.

The Image Generations endpoint accepts:

- required non-empty `prompt`;
- `model`, defaulting to `gpt-image-2`;
- `n` from 1 through 4;
- `quality` values `auto` (default), `low`, `medium`, and `high`;
- `size`, defaulting to `auto`, or a model-supported `WIDTHxHEIGHT`;
- `background` values `auto` (default), `opaque`, and `transparent` on models
  that support native transparency;
- `response_format` omitted or set to `b64_json`.

For `gpt-image-2`, requested dimensions must use edges divisible by 16, a
maximum edge of 3840 pixels, an aspect ratio no greater than 3:1, and 655,360 to
8,294,400 total pixels. The model does not support native
`background=transparent`. Raw returns base64 image data and resolved metadata;
the current low-level image response does not retain upstream image usage or
`output_format`.

Unknown non-null parameters, multimodal input, non-function tools,
`model_verbosity`/`text.verbosity`, `temperature`, `top_p`, output-token limits,
reasoning summaries, structured output, `store:true`, `previous_response_id`,
unmatched tool outputs, and duplicate tool outputs are rejected with an
OpenAI-shaped error instead of being silently ignored. The maximum request body
is 2 MiB. When the concurrency limit is full, the server returns HTTP 429.

Responses streams end with `response.completed`. Chat streams end with
`[DONE]`. Responses usage is carried by the terminal response object; Chat
stream usage is emitted when `stream_options.include_usage` is requested.

## Function tools

Codex Raw transports function definitions and function-call items, but the
calling application owns the implementation and execution loop:

```text
client -> sends a function schema
model  -> returns a function_call and JSON arguments
client -> executes the function locally
client -> sends a matching function_call_output
model  -> uses the result and returns final text
```

Example first Responses request:

```json
{
  "model": "gpt-5.4-mini",
  "instructions": "Call get_weather exactly once.",
  "input": "What is the weather in Sofia?",
  "tools": [
    {
      "type": "function",
      "name": "get_weather",
      "description": "Return weather for a city",
      "parameters": {
        "type": "object",
        "properties": {"city": {"type": "string"}},
        "required": ["city"],
        "additionalProperties": false
      },
      "strict": true
    }
  ],
  "tool_choice": "required",
  "parallel_tool_calls": false
}
```

After executing the requested function, the client sends the original input,
the complete previous `response.output`, and a matching result:

```json
{
  "type": "function_call_output",
  "call_id": "<call_id from the model>",
  "output": "{\"temperature\":24,\"unit\":\"C\"}"
}
```

Replaying the complete previous output is important for reasoning models because
it can contain opaque encrypted reasoning items. For that reason, use
`/v1/responses` for reasoning-model tool loops. The Chat message format cannot
reliably preserve those opaque items.

## Verification

The crate's unit and protocol tests cover:

- Raw home ownership and protected-directory rejection;
- minimal standard and Responses Lite request construction;
- app-server parsing, lifecycle ordering, stable IDs, and usage mapping;
- strict Responses and Chat request validation;
- image request defaults, validation, and response mapping;
- loopback and non-loopback listen-scope classification;
- bearer-token route coverage and protected readiness behavior;
- function schema translation and tool-call correlation;
- non-streaming response and usage shapes;
- Responses content lifecycle and typed SSE events;
- Chat role, text, tool, usage, and `[DONE]` streaming behavior.

Run the test suite from `codex-rs`:

```shell
just test -p codex-raw
```

With a logged-in Raw home and a running API server, run the SDK smoke test:

```shell
uv run --with 'openai>=2,<3' python \
  raw-cli/scripts/openai_sdk_smoke.py \
  --base-url http://127.0.0.1:8080/v1 \
  --model gpt-5.4-mini \
  --include-tools
```

The smoke test exercises Responses and Chat in streaming and non-streaming
modes. With `--include-tools`, it also performs a complete Responses function
call and output round trip. When the server is protected, set
`CODEX_RAW_API_TOKEN`; the script uses it without printing it.

## Recorded performance

These figures are recorded observations from July 19, 2026, not universal
latency guarantees. Model scheduling, network conditions, account routing,
cache state, generated output, and reasoning behavior can materially change a
result.

### Standard Codex compared with Raw one-shot

The control used Codex CLI `0.144.6` with `exec`, `--ephemeral`,
`--ignore-user-config`, `--ignore-rules`, a read-only sandbox, an empty temporary
directory, and the exact prompt `hello`. Raw used a new one-shot process with
the same prompt. These were single paired observations, so they demonstrate the
context difference but are not a statistical benchmark.

| Model selection | Client | Wall time | Input tokens | Cached input | Output | Reasoning |
|---|---|---:|---:|---:|---:|---:|
| Active default | Standard Codex | 6.588 s | 11,816 | 8,960 | 11 | 0 |
| Active default | Codex Raw | 1.589 s | 7 | 0 | 11 | 0 |
| `gpt-5.4-mini` | Standard Codex | 6.235 s | 10,199 | 3,456 | 30 | 22 |
| `gpt-5.4-mini` | Codex Raw | 0.979 s | 7 | 0 | 13 | 0 |

In those samples, Raw was 4.15x faster for the active default and 6.37x faster
for `gpt-5.4-mini`, while reporting dramatically fewer input tokens. The
generated outputs and reasoning token counts were not identical, so the
latency ratio should not be interpreted as a controlled model-performance
comparison. The input-token difference is the more direct evidence that the
Codex agent context was not included.

### One-shot compared with persistent app-server

This paired test used `gpt-5.4-mini`, one warm-up turn, 10 measured turns per
mode, and alternating execution order. TTFT was measured to the first non-empty
text delta; total latency ended at process exit for one-shot or
`turn/completed` for app-server.

| Mode | Runs | Median TTFT | p95 TTFT | Median total | p95 total |
|---|---:|---:|---:|---:|---:|
| Raw one-shot | 10 | 1,107.567 ms | 2,962.096 ms | 1,325.743 ms | 3,302.678 ms |
| Raw app-server | 10 | 873.251 ms | 3,667.947 ms | 1,136.576 ms | 3,936.025 ms |

The persistent process reduced median TTFT by 21.2% and median total latency by
14.3%. Its p95 was worse in this small sample, so persistence improves the
typical warm sequential case but does not guarantee that every turn is faster.
All 20 measured requests reported 7 input and 13 output tokens.

### Persistent app-server compared with local API

This test used the same executable, model, and prompt in two persistent
processes. Each mode had one warm-up followed by 10 alternating measured turns.

| Mode | Runs | Median TTFT | p95 TTFT | Median total | p95 total |
|---|---:|---:|---:|---:|---:|
| Raw app-server | 10 | 898.61 ms | 3,295.47 ms | 1,145.78 ms | 3,635.60 ms |
| Raw api-server | 10 | 851.13 ms | 1,424.93 ms | 1,048.66 ms | 3,260.80 ms |

The API process was 5.3% lower in median TTFT and 8.5% lower in median total for
this sample. The difference is small enough that later runs can reverse the
ordering. The useful conclusion is that loopback HTTP and SSE do not erase the
benefit of reusing the Raw runtime.

Reproduce the app-server/API paired benchmark from `codex-rs`:

```powershell
.\raw-cli\scripts\benchmark_persistent.ps1 `
  -Exe .\target\release\codex-raw.exe `
  -Model gpt-5.4-mini `
  -Prompt hello `
  -Runs 10
```

Compare medians and tail percentiles across multiple alternating runs. Do not
draw conclusions from a single request.

## Security model

- Never commit or share `.codex-raw/auth.json`.
- Do not print or copy access, ID, or refresh tokens.
- Do not point the Raw home at another application's state directory.
- Do not bypass the ownership guard with a symbolic link or junction.
- Prefer `CODEX_RAW_API_TOKEN` over `--api-token`; command-line values can be
  visible in process listings and shell history.
- Token-free mode is for loopback only. Any local process that can reach the
  port can use the signed-in account's text and image allowance.
- A configured token protects `/readyz` and `/v1`, but it is one shared secret,
  not per-user authorization. `/healthz` intentionally remains open.
- A browser `Origin` rejection is defense in depth, not authentication.
- Non-loopback access requires a token, but the built-in server is plain HTTP.
  Use a hardened reverse proxy with TLS, connection/header timeouts, and
  connection limits, then firewall direct access to Raw.
- Do not expose Raw directly to the public internet or use it as a multi-tenant
  service.
- Use `codex-raw logout` and sign in again if credentials may be compromised.

Codex Raw does not bypass account permissions or platform policies. Use it only
under the repository license and the terms that apply to the signed-in account.

## Keep the fork up to date

The isolation of `codex-rs/raw-cli` is intentional. New Codex revisions can be
merged into the fork without modifying an installed Raw binary until the
updated source has passed its own validation.

An upstream update is not guaranteed to be automatic. Codex Raw compiles
against internal Codex crates, so an upstream type or protocol change can
require a small adaptation inside `raw-cli`. The compiler and the Raw test suite
are the primary compatibility checks.

### 1. Configure the official repository as upstream

Keep your fork as `origin` and add the official Codex repository as
`upstream`. This is a one-time setup:

```shell
git remote -v
git remote add upstream https://github.com/openai/codex.git
git fetch --no-tags upstream main
```

If `upstream` already exists, do not add it again; verify its URL with
`git remote -v`. To reduce the risk of an accidental push to the official
repository, an optional local safeguard is:

```shell
git remote set-url --push upstream DISABLED
```

Only fetch and merge from `upstream`. Push your maintained branch to your own
`origin`.

### 2. Create an isolated upgrade branch

Start from the currently working `main` branch:

```shell
git switch main
git status --short
git fetch --no-tags upstream main
git switch -c upgrade/codex-raw-YYYYMMDD
git merge --no-ff upstream/main
```

Use `upstream/main` for the latest development revision. For a more controlled
release, merge a specific upstream release tag or commit instead.

The temporary upgrade branch keeps the last working commit easy to recover
without creating another source-tree copy. Do not begin with unexplained
uncommitted changes.

### 3. Resolve conflicts without losing Raw registration

Most custom source is under `codex-rs/raw-cli`. The shared files that normally
need attention are:

```text
codex-rs/Cargo.toml
codex-rs/Cargo.lock
MODULE.bazel.lock
```

Preserve the `raw-cli` workspace member and its dependency declarations while
accepting unrelated upstream workspace changes. Do not resolve the entire
workspace manifest with a blanket `ours` or `theirs` choice. Regenerate lock
data from the merged manifests instead of copying an old lock block by hand.

Compile-sensitive areas to inspect after every update are:

- Responses request fields and serialization;
- image request/response types and `ImagesClient` routing;
- response stream completion, text, and function-call events;
- ChatGPT auth manager and token refresh behavior;
- model catalog selection and Responses Lite metadata;
- response item and token usage types;
- app-server v2 wire shapes;
- function-call argument delta behavior.

Keep compatibility changes local to `codex-rs/raw-cli` whenever possible. The
standard Codex executable and agent pipeline should remain unmodified.

### 4. Run the complete Raw validation

From `codex-rs`:

```shell
just fmt
just test -p codex-raw
just fix -p codex-raw
cargo build --release -p codex-raw
```

From the repository root, when dependencies or Bazel inputs changed:

```shell
just bazel-lock-update
just bazel-lock-check
```

Then use the newly built executable without installing it over the current Raw
version:

```shell
codex-rs/target/release/codex-raw --version
codex-rs/target/release/codex-raw status
codex-rs/target/release/codex-raw -m gpt-5.4-mini hello
```

On Windows PowerShell:

```powershell
.\codex-rs\target\release\codex-raw.exe --version
.\codex-rs\target\release\codex-raw.exe status
.\codex-rs\target\release\codex-raw.exe -m gpt-5.4-mini hello
```

Start the candidate API on a different loopback port, run `/readyz`,
`/v1/models`, Responses, Chat Completions, Image Generations, streaming, and a
function-tool round trip. Run the SDK smoke test and the paired latency
benchmark before promotion.

### 5. Promote only the verified candidate

After the upgrade branch passes all checks, integrate it into the maintained
branch and push it to your fork:

```shell
git switch main
git merge --ff-only upgrade/codex-raw-YYYYMMDD
git push origin main
```

Build the final release from that exact commit. Stop any running Raw server,
replace the deployed executable, verify health and the public routes, and remove
superseded builds from `dist/`. Keep only the current deploy binary; Git commits
and tags are the rollback source of truth. Because installation targets only
the `codex-raw` filename and Raw home, upgrading Raw still does not overwrite
the standard `codex` executable or `~/.codex` state.

If the candidate fails, switch back to the previous commit and rebuild it. Do
not promote a build merely because the merge completed successfully.

## License

This fork retains the Apache-2.0 license in `LICENSE`. Codex Raw is an isolated
extension built on the Codex source tree. See `NOTICE` for retained notices and
`UPSTREAM.md` for the exact upstream base. The Codex Raw name identifies this
fork only; it is not presented as an official OpenAI product or as a replacement
for the standard Codex agent.
