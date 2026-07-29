# Reproducible benchmarks

This page publishes Codex Raw measurements together with the exact release,
binary, workload, environment, calculation method, and raw samples that
produced them.

> [!CAUTION]
> These results are recorded observations, not performance guarantees. Model
> routing, account state, network conditions, cache state, generated output,
> and upstream behavior can materially change latency and token usage.

## Published results

Each row is bound to a named immutable release, exact binary digest, sanitized
raw samples, and a provenance record. The two releases were measured on the
same workstation with the same harness, account, model, prompt, and workload,
but on different dates. They are dated observations, not a controlled
before/after performance comparison.

| Record ID                               | Release            | Mode         | Model          | Measured runs | Median TTFT |    p95 TTFT | Median total |   p95 total |
| --------------------------------------- | ------------------ | ------------ | -------------- | ------------: | ----------: | ----------: | -----------: | ----------: |
| `raw-0.1.1-app-windows-x64-20260729-01` | `codex-raw-v0.1.1` | `app-server` | `gpt-5.4-mini` |            10 |   815.77 ms | 3,022.92 ms |    983.73 ms | 3,292.14 ms |
| `raw-0.1.1-api-windows-x64-20260729-01` | `codex-raw-v0.1.1` | `api-server` | `gpt-5.4-mini` |            10 |   704.79 ms | 1,900.71 ms |    916.01 ms | 2,170.69 ms |
| `raw-0.1.0-app-windows-x64-20260728-01` | `codex-raw-v0.1.0` | `app-server` | `gpt-5.4-mini` |            10 |   701.73 ms |   969.14 ms |    904.12 ms | 1,125.31 ms |
| `raw-0.1.0-api-windows-x64-20260728-01` | `codex-raw-v0.1.0` | `api-server` | `gpt-5.4-mini` |            10 |   658.40 ms | 2,024.18 ms |    874.14 ms | 2,251.11 ms |

## Codex Raw 0.1.1 — Windows x64

This run used the immutable Windows x64 asset for `codex-raw 0.1.1`. One
uninterrupted invocation completed exactly two warm-up turns and twenty
measured turns. It performed no retries and had no failed or timed-out request.

### 0.1.1 artifact identity

| Field                    | Exact observed value                                                                                                                            |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Release                  | [`codex-raw-v0.1.1`](https://github.com/bproject07/Codex-Source/releases/tag/codex-raw-v0.1.1), release ID `361516591`, published and immutable |
| Tag object               | `42336d4b4f7710aa6ec64f41556263d64d0695de`                                                                                                      |
| Tag commit               | `4d814d6e7578a833bb355665862c0f4b83678f89`                                                                                                      |
| Asset                    | `codex-raw-0.1.1-x86_64-pc-windows-msvc.zip`, 12,163,128 bytes                                                                                  |
| Asset SHA-256            | `f883916658fd6b4e1d549dbb50a532c72d9f9efafbdf7dfb7ee129740ff1057d`                                                                              |
| Extracted binary SHA-256 | `0bb4a1d8b2d624d59b7023bc9256cb5643a5c5f9cf46f6b90bf21aeea6026e0c`                                                                              |
| Version output           | `codex-raw 0.1.1`                                                                                                                               |
| Target                   | `x86_64-pc-windows-msvc`                                                                                                                        |
| Benchmark harness        | `codex-rs/raw-cli/scripts/benchmark_persistent.ps1` from the tagged commit                                                                      |
| Harness Git blob         | `2c92994556a50dd2c83be9c844ad41d141a6fbc8`                                                                                                      |
| Harness blob SHA-256     | `95184d60b7bf753d97d1868263bdb369a3049b3a7f3d6d04127e81e0529e36a1`                                                                              |
| Harness checkout SHA-256 | `a3b17cd9c9c3445bfbe07c530cb8884f3a24f755c772dcc4822368525edc16` (observed CRLF Windows checkout)                                                |

The downloaded archive digest matched both GitHub's asset digest and the
release's `SHA256SUMS` record before extraction.

### 0.1.1 environment and workload

| Field                | Exact observed value                                                                  |
| -------------------- | ------------------------------------------------------------------------------------- |
| Machine              | VirtualBox virtual machine (`innotek GmbH`)                                           |
| CPU exposed to guest | 12th Gen Intel Core i7-12700, 4 physical/logical guest cores                          |
| Memory               | 12,776,771,584 bytes (about 11.90 GiB)                                                |
| Operating system     | Microsoft Windows 11 Pro, version/build `10.0.26200`                                  |
| Architecture         | x64 OS and x64 process                                                                |
| PowerShell           | `7.6.3`                                                                               |
| Local time zone      | `FLE Standard Time`                                                                   |
| Network              | Local workstation connection; route and upstream model placement were not controlled |
| Start UTC            | `2026-07-29T05:07:02.1434541Z`                                                        |
| End UTC              | `2026-07-29T05:07:27.2418112Z`                                                        |

The observed VM wall-clock interval is inconsistent with the monotonic
samples: finish minus start is 242.5499 ms shorter than the sum of the
sequential measured-turn durations alone. This can occur when a VM wall clock
is corrected during a run, but no cause is asserted here. The timestamps are
retained as observed metadata only. Every reported latency comes from the
harness's monotonic stopwatch and does not use the wall clock.

The workload and calculation method were identical to the `0.1.0` run:
`gpt-5.4-mini`, public prompt `hello`, one app-server and one loopback
api-server, one warm-up per mode, ten measured turns per mode, sequential
requests, no retries, and a 120-second per-operation timeout. Reasoning effort
and service tier were omitted so the executable defaults applied. The prompt's
UTF-8 SHA-256 was
`2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824`.

### 0.1.1 detailed results

| Mode         | Metric |      Median |         p90 |         p95 |   Minimum |     Maximum |        Mean |
| ------------ | ------ | ----------: | ----------: | ----------: | --------: | ----------: | ----------: |
| `app-server` | TTFT   |   815.77 ms | 2,390.88 ms | 3,022.92 ms | 622.08 ms | 3,022.92 ms | 1,215.59 ms |
| `app-server` | Total  |   983.73 ms | 2,588.05 ms | 3,292.14 ms | 826.19 ms | 3,292.14 ms | 1,419.76 ms |
| `api-server` | TTFT   |   704.79 ms | 1,484.27 ms | 1,900.71 ms | 517.10 ms | 1,900.71 ms |   899.22 ms |
| `api-server` | Total  |   916.01 ms | 1,693.17 ms | 2,170.69 ms | 722.06 ms | 2,170.69 ms | 1,114.33 ms |

| Invariant or outcome                       | Observed value                                                                                               |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| Successful warm-up turns                   | 2 of 2                                                                                                       |
| Successful measured turns                  | 20 of 20                                                                                                     |
| Failed / timed-out turns                   | 0 / 0                                                                                                        |
| Input tokens                               | exactly 7 in every measured turn                                                                             |
| Output lengths                             | 19 responses had 32 output characters; one API response had 29 output characters                             |
| Output text and fingerprints               | deliberately not retained                                                                                    |
| Cached-input, output, and reasoning tokens | not retained by the harness; no values are inferred                                                          |

With only ten samples per mode, nearest-rank p95 is the maximum observed
sample. Two app-server samples took about 2.39 and 3.02 seconds to first text,
which materially increased that mode's p90, p95, and mean. The API median was
110.97 ms lower for TTFT and 67.72 ms lower for total latency in this run, but
the small sample and uncontrolled upstream conditions do not establish a
general speed advantage or a release-to-release regression.

### 0.1.1 published raw data

| File                                                                                                                                                             | SHA-256                                                            |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| [`benchmarks/data/codex-raw-v0.1.1-windows-x64-20260729T050702Z.samples.json`](benchmarks/data/codex-raw-v0.1.1-windows-x64-20260729T050702Z.samples.json)       | `a56015e18430d0c220d0d2f04fb4d826b1d03c02ea47628fb9225ca2d607e0f5` |
| [`benchmarks/data/codex-raw-v0.1.1-windows-x64-20260729T050702Z.provenance.json`](benchmarks/data/codex-raw-v0.1.1-windows-x64-20260729T050702Z.provenance.json) | `52cc3e24478313f8bdf2d40d51aaba9ede70c1847641f00c18a3b4bbadc77860` |

The samples file contains one sanitized row for every measured turn. It
contains order, mode, TTFT, total latency, input tokens, and output length. It
contains no response text or output fingerprint: unsalted hashes of
low-entropy outputs can reveal the text they are intended to hide. It also
contains no hostname, account identifier, credential, token, private address,
or absolute local path.

### Reproduce the 0.1.1 aggregate run

From a clone of this repository, download and verify the exact release asset,
then create a detached worktree for the exact tagged harness:

```powershell
$archive = 'codex-raw-0.1.1-x86_64-pc-windows-msvc.zip'
$expected = 'f883916658fd6b4e1d549dbb50a532c72d9f9efafbdf7dfb7ee129740ff1057d'

Invoke-WebRequest `
  "https://github.com/bproject07/Codex-Source/releases/download/codex-raw-v0.1.1/$archive" `
  -OutFile $archive
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
  throw 'release asset digest mismatch'
}

Expand-Archive $archive -DestinationPath .\codex-raw-0.1.1
$exe = Resolve-Path `
  '.\codex-raw-0.1.1\codex-raw-0.1.1-x86_64-pc-windows-msvc\codex-raw.exe'
& $exe --version

git fetch origin tag codex-raw-v0.1.1
git worktree add --detach .\codex-raw-0.1.1-source codex-raw-v0.1.1
$source = Resolve-Path '.\codex-raw-0.1.1-source'
if ((git -C $source rev-parse HEAD).Trim() -ne
    '4d814d6e7578a833bb355665862c0f4b83678f89') {
  throw 'tag commit mismatch'
}
if ((git -C $source rev-parse `
    'HEAD:codex-rs/raw-cli/scripts/benchmark_persistent.ps1').Trim() -ne
    '2c92994556a50dd2c83be9c844ad41d141a6fbc8') {
  throw 'benchmark harness blob mismatch'
}
$harness = Join-Path $source `
  'codex-rs\raw-cli\scripts\benchmark_persistent.ps1'

pwsh -File $harness `
  -Exe $exe `
  -Model gpt-5.4-mini `
  -Prompt hello `
  -Runs 10 `
  -TimeoutSeconds 120
```

This command uses the caller's existing Codex Raw browser-login state and
performs 22 live model turns. Do not automate a retry after a partial failure,
because some turns may already have completed.

## Codex Raw 0.1.0 — Windows x64

The first release-bound benchmark was recorded from the immutable Windows x64
asset for `codex-raw 0.1.0`. One uninterrupted invocation completed exactly
two warm-up turns and twenty measured turns. It performed no retries and had
no failed or timed-out request.

### Artifact identity

| Field                    | Exact observed value                                                                                                                            |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Release                  | [`codex-raw-v0.1.0`](https://github.com/bproject07/Codex-Source/releases/tag/codex-raw-v0.1.0), release ID `361408465`, published and immutable |
| Tag object               | `3942faacc179e129c88960b259b1f2421b2da890`                                                                                                      |
| Tag commit               | `99cc140866c4f66da6d8f283a013e6b43e4458e9`                                                                                                      |
| Asset                    | `codex-raw-0.1.0-x86_64-pc-windows-msvc.zip`, 12,228,846 bytes                                                                                  |
| Asset SHA-256            | `c5551071b41245c33d6406a2a2f20c95fc8ff91bcd7468ab41b701a30c661d08`                                                                              |
| Extracted binary SHA-256 | `7a5d8b8a1b7144a2a6efd619b53a477a251acc19c14a6e06f85e8e23baa15208`                                                                              |
| Version output           | `codex-raw 0.1.0`                                                                                                                               |
| Target                   | `x86_64-pc-windows-msvc`                                                                                                                        |
| Benchmark harness        | `codex-rs/raw-cli/scripts/benchmark_persistent.ps1` from the tagged commit                                                                      |
| Harness Git blob         | `2c92994556a50dd2c83be9c844ad41d141a6fbc8`                                                                                                      |
| Harness blob SHA-256     | `95184d60b7bf753d97d1868263bdb369a3049b3a7f3d6d04127e81e0529e36a1`                                                                              |
| Harness checkout SHA-256 | `a3b17cd9c9c3445bfbe07c530cb8884f3f9a24f755c772dcc4822368525edc16` (observed CRLF Windows checkout)                                                |

The archive digest independently matched both GitHub's asset digest and the
release's `SHA256SUMS` record before extraction.

### Environment

| Field                | Exact observed value                                                                 |
| -------------------- | ------------------------------------------------------------------------------------ |
| Machine              | VirtualBox virtual machine (`innotek GmbH`)                                          |
| CPU exposed to guest | 12th Gen Intel Core i7-12700, 4 physical/logical guest cores                         |
| Memory               | 12,776,771,584 bytes (about 11.90 GiB)                                               |
| Operating system     | Microsoft Windows 11 Pro, version/build `10.0.26200`                                 |
| Architecture         | x64 OS and x64 process                                                               |
| PowerShell           | `7.6.3`                                                                              |
| Local time zone      | `FLE Standard Time`                                                                  |
| Network              | Local workstation connection; route and upstream model placement were not controlled |
| Start UTC            | `2026-07-28T22:30:30.6465170Z`                                                       |
| End UTC              | `2026-07-28T22:30:56.3387855Z`                                                       |

No hostname, account identifier, credential, token, private address, or
response text is present in the published data.

### Workload and method

| Field                           | Exact value                                                              |
| ------------------------------- | ------------------------------------------------------------------------ |
| Interfaces                      | persistent JSONL `app-server` and local HTTP `api-server`                |
| Model                           | `gpt-5.4-mini`                                                           |
| Prompt                          | `hello`                                                                  |
| Prompt UTF-8 SHA-256            | `2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824`       |
| Reasoning effort / service tier | omitted; executable defaults applied                                     |
| Concurrency                     | one request at a time; API server `--max-concurrency 1`                  |
| Warm-up                         | one successful turn per mode, fixed order: app then API                  |
| Measured                        | 10 turns per mode, 20 total                                              |
| Measured order                  | sequential pairs; the first mode alternated for each pair                |
| Timeout                         | 120 seconds per operation                                                |
| Retries                         | none                                                                     |
| TTFT                            | client monotonic stopwatch start to first non-empty assistant text delta |
| Total latency                   | same start to terminal turn/completion event                             |
| Median                          | arithmetic mean of the two middle sorted values                          |
| p90 / p95                       | nearest rank, index `ceil(p × n) - 1` in the zero-based sorted values    |

The tagged harness normally prints only aggregate statistics. A PowerShell
line breakpoint immediately after its measured loop copied the already
completed sample objects to JSON. It therefore did not run inside a measured
request or change its stopwatch boundaries. The tagged harness itself was not
modified.

### Detailed results

| Mode         | Metric |    Median |         p90 |         p95 |   Minimum |     Maximum |        Mean |
| ------------ | ------ | --------: | ----------: | ----------: | --------: | ----------: | ----------: |
| `app-server` | TTFT   | 701.73 ms |   857.32 ms |   969.14 ms | 591.13 ms |   969.14 ms |   738.38 ms |
| `app-server` | Total  | 904.12 ms | 1,047.71 ms | 1,125.31 ms | 756.16 ms | 1,125.31 ms |   920.72 ms |
| `api-server` | TTFT   | 658.40 ms |   947.07 ms | 2,024.18 ms | 595.88 ms | 2,024.18 ms |   813.98 ms |
| `api-server` | Total  | 874.14 ms | 1,165.71 ms | 2,251.11 ms | 728.89 ms | 2,251.11 ms | 1,009.37 ms |

| Invariant or outcome                       | Observed value                                                                                               |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| Successful warm-up turns                   | 2 of 2                                                                                                       |
| Successful measured turns                  | 20 of 20                                                                                                     |
| Failed / timed-out turns                   | 0 / 0                                                                                                        |
| Input tokens                               | exactly 7 in every measured turn                                                                             |
| Output fingerprints                        | 19 responses shared one SHA-256 fingerprint and had 32 output characters; one app response had another fingerprint and 29 output characters |
| Output text                                | deliberately not retained                                                                                    |
| Cached-input, output, and reasoning tokens | not retained by the `0.1.0` harness; no values are inferred                                                  |

With only ten samples per mode, nearest-rank p95 is the maximum observed
sample. One API request took 2,024.18 ms to first text and 2,251.11 ms total,
which explains that mode's p95 and mean. The API median was 43.34 ms lower for
TTFT and 29.99 ms lower for total latency, but this single small run does not
establish a general speed advantage. One response also differed in length and
fingerprint, so the result is not presented as an exact output-equivalence
claim.

### Published raw data

| File                                                                                                                                                             | SHA-256                                                            |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| [`benchmarks/data/codex-raw-v0.1.0-windows-x64-20260728T223030Z.samples.json`](benchmarks/data/codex-raw-v0.1.0-windows-x64-20260728T223030Z.samples.json)       | `d084f60c3d7dac252a8490f4783f7d03e62809cb600eed997b184cb8a8cfbcbe` |
| [`benchmarks/data/codex-raw-v0.1.0-windows-x64-20260728T223030Z.provenance.json`](benchmarks/data/codex-raw-v0.1.0-windows-x64-20260728T223030Z.provenance.json) | `c23d438caac0d8af9cc60c3a46751c278252e0bbea2a1b1ab52ba69a196b68d9` |

The samples file contains one row for each measured turn, including global
order, mode, TTFT, total latency, input tokens, output length, and a SHA-256
fingerprint of the output. The two warm-up turns completed successfully but
their timings were discarded by the tagged `0.1.0` harness before capture;
this limitation is recorded in both JSON files.

### Reproduce the 0.1.0 aggregate run

From a clone of this repository, download and verify the exact release asset,
then create a detached worktree for the exact tagged harness:

```powershell
$archive = 'codex-raw-0.1.0-x86_64-pc-windows-msvc.zip'
$expected = 'c5551071b41245c33d6406a2a2f20c95fc8ff91bcd7468ab41b701a30c661d08'

Invoke-WebRequest `
  "https://github.com/bproject07/Codex-Source/releases/download/codex-raw-v0.1.0/$archive" `
  -OutFile $archive
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
  throw 'release asset digest mismatch'
}

Expand-Archive $archive -DestinationPath .\codex-raw-0.1.0
$exe = Resolve-Path `
  '.\codex-raw-0.1.0\codex-raw-0.1.0-x86_64-pc-windows-msvc\codex-raw.exe'
& $exe --version

git fetch origin tag codex-raw-v0.1.0
git worktree add --detach .\codex-raw-0.1.0-source codex-raw-v0.1.0
$source = Resolve-Path '.\codex-raw-0.1.0-source'
if ((git -C $source rev-parse HEAD).Trim() -ne
    '99cc140866c4f66da6d8f283a013e6b43e4458e9') {
  throw 'tag commit mismatch'
}
if ((git -C $source rev-parse `
    'HEAD:codex-rs/raw-cli/scripts/benchmark_persistent.ps1').Trim() -ne
    '2c92994556a50dd2c83be9c844ad41d141a6fbc8') {
  throw 'benchmark harness blob mismatch'
}
$harness = Join-Path $source `
  'codex-rs\raw-cli\scripts\benchmark_persistent.ps1'

pwsh -File $harness `
  -Exe $exe `
  -Model gpt-5.4-mini `
  -Prompt hello `
  -Runs 10 `
  -TimeoutSeconds 120
```

This command uses the caller's existing Codex Raw browser-login state and
performs 22 live model turns. Do not automate a retry after a partial failure,
because some turns may already have completed.

## Publication rules for future results

- Use an asset downloaded from a named, published, immutable release, never an
  unnamed local build.
- Record the tag, full commit, asset name, archive and binary SHA-256, exact
  version output, harness identity, environment, model, prompt, ordering,
  warm-ups, measured runs, timeouts, and calculation methods.
- Preserve failed and timed-out attempts. Never discard an outlier without
  retaining and explaining it.
- Publish sanitized per-sample data. Never publish credentials, tokens,
  account identifiers, private prompts, private infrastructure addresses,
  absolute local paths, or response text that was not explicitly public.
- Do not publish unsalted output fingerprints for private or low-entropy
  outputs; they can reveal the original text. Prefer omitting the fingerprint
  unless the output corpus itself is explicitly public.
- State unavailable fields as unavailable; do not reconstruct or infer token
  values that the measured client did not retain.
- Comparisons must identify every client artifact and use the same machine,
  account, model, prompt, and time window. Report output and token differences
  beside latency.
- Treat every result as a dated observation, not a promise about future
  latency or upstream service behavior.
