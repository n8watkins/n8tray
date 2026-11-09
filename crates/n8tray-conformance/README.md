# N8 Tray conformance kit

Two tools for making an app follow the [N8 Tray contract](../../spec/contract-v2.md) and keeping it that way in CI:

- **`n8tray-contract`**: validates contract documents (`check`), checks a running app against its manifest (`probe`), and plays the host (`host`, the test tray).
- **`n8tray-fake-module`**: a small app that follows the contract, for testing hosts and for comparing against.

## Install

Needs Rust (every GitHub-hosted runner has it).

```bash
cargo install --locked --git https://github.com/n8watkins/n8tray --tag v0.1.0 n8tray-conformance
```

Pin the tag: the authoring check is strict, so a newer kit may know fields an older one does not (spec section 10).

## `n8tray-contract check`

```bash
n8tray-contract check path/to/voicedock.json        # one file
n8tray-contract check spec/examples                 # every .json in a folder
n8tray-contract check --kind status status.json     # skip detection
n8tray-contract check --json manifest.json          # machine-readable
```

It validates manifests, status documents, leases, action results and contract 1 `modular.json` files against the JSON Schemas, then applies the rules of spec section 11.
Each problem has a code (`schema`, `path-escapes`, `budget` and so on).
A manifest's file name must match its id, except `manifest.json` and `modular.json`.
Exit code 0 means valid, 1 invalid, 2 a file could not be read.

## `n8tray-contract probe`

```bash
n8tray-contract probe ~/.local/share/n8tray/modules/voicedock.json
n8tray-contract probe voicedock.json --run-dir "$RUN" --timeout-ms 2000 --action restart
```

It reads the manifest, finds the app's status the way the host does, and reports each check as PASS, WARN or FAIL (exit code 1 when anything fails).

| Check | What it verifies |
|---|---|
| `manifest` | The manifest is valid. |
| `token-file` | HTTP apps: `<id>.token` exists in the runtime folder, is 64 lowercase hex characters and is not readable by other users (Unix). |
| `auth` | HTTP: no token and a wrong token both get `401`. |
| `origin` | HTTP: a request with an `Origin` header gets `403`. |
| `host` | HTTP: a request with a foreign `Host` header gets `403`. |
| `unknown-action` | HTTP: an undeclared action gets `404`. |
| `status` | HTTP: `GET /n8/v2/status` answers `200` with a valid status document within the timeout. |
| `no-secrets` | HTTP: the token does not appear in the status document. |
| `pressure` | Apps with the `pressure` feature: it is in the status's features, and `POST /n8/v2/pressure` answers with a valid action result. |
| `action` | With `--action`: the HTTP action answers with a valid action result. |
| `status-file` | File apps: `<id>.status.json` is valid and under 30 s old. |
| `identity` | The status's id is the manifest's (a different version is a warning). |
| `resources` | `resources.sampledAt` is recent: within three intervals plus 5 s. |
| `main-process` | The status pid is alive and started when `pidStartedAt` says. |
| `ownership` | Every reported process is alive, started when it says, and descends from the main process (spec 7.2, steps 1 and 2). |
| `unreported` | Warning: child processes of the main process that `resources` does not list (spec 7.2, step 5). |
| `presence` | Apps with the `presence` feature report `presence`. |
| `components` | Warning: component actions without `components` in the status. |

The runtime folder is `--run-dir`, else `N8TRAY_RUN_DIR`, else the per-OS folder (spec section 2).
Process creation times come from a process table with whole-second precision, so identity checks allow 1 to 2 s.

## `n8tray-contract host`: the test tray

```bash
n8tray-contract host --run-dir "$RUN" --hosting voicedock            # until stopped
n8tray-contract host --run-dir "$RUN" --hosting voicedock --once     # one lease, then exit
n8tray-contract host --run-dir "$RUN" --hosting voicedock --for-seconds 30
n8tray-contract host --run-dir "$RUN" --hosting voicedock --freeze-after-seconds 10   # a hung host
```

It writes `host.json` every 5 s exactly as N8 Tray does (spec section 8), so an app's lease handling can be tested without N8 Tray:
start the test tray, start the app with `N8TRAY_RUN_DIR` pointing at the same folder, check that it shows no icon, stop the test tray, and check that the icon comes back within about 15 s.
A lease from `--once` goes stale at once (its process is gone); one from `--freeze-after-seconds` goes stale after 15 s while its process lives on.

## `n8tray-fake-module`

```bash
n8tray-fake-module --run-dir "$RUN" --manifest-out "$RUN/../modules/fake.json"             # HTTP status
n8tray-fake-module --run-dir "$RUN" --status file --children 2                            # a status file, two workers
n8tray-fake-module --run-dir "$RUN" --status file --follow-lease --lease-poll-ms 200       # prints "tray: hosted|own|waiting"
```

It reports its real resources and can misbehave on request: `--eat-mb` (memory), `--lie-pid` (claims a process it does not own), `--hide-children` (does not report its workers).
Its HTTP control plane is a reference for the checks of spec section 9, and its lease handling for spec 8.2.
`--help` lists every option.

## In an app's CI

A Node app on Linux (VoiceDock, for example):

```yaml
- run: cargo install --locked --git https://github.com/n8watkins/n8tray --tag v0.1.0 n8tray-conformance
- run: n8tray-contract check manifest/voicedock.json
- name: Probe the running hub against the test tray
  env:
    N8TRAY_RUN_DIR: ${{ runner.temp }}/n8tray-run
  run: |
    n8tray-contract host --hosting voicedock --for-seconds 120 &
    node servers/hub/server.js --managed &
    sleep 3
    n8tray-contract probe manifest/voicedock.json
```

A Windows PowerShell app (N8 Commentary, N8 Hotkeys, Lead Scraper) does the same on `windows-latest` with `shell: pwsh` and `Start-Process` for the background steps.
