# N8 Tray contract, version 2

This is the contract between N8 Tray (the host) and the apps it shows (modules).
An app that follows it runs on its own, with its own tray icon if it has one, and is picked up by N8 Tray when N8 Tray runs, with no code shared between them.

The contract has four parts:

1. A **manifest** the app installs: who it is, how to start, stop and query it, what it needs and what it may cost ([section 3](#3-the-manifest)).
2. A **status document** the app serves over loopback HTTP or writes to a file: its state, its components and the resources of every process it owns ([section 4](#4-the-status-document)).
3. A **status protocol**: how the host reads status, invokes actions and pushes memory pressure ([section 5](#5-the-status-protocol)).
4. A **host lease** the host writes, so an app hides its own tray icon while it is hosted ([section 8](#8-the-host-lease)).

The JSON Schemas in [`schema/`](schema) are normative for the shape of every document.
This text is normative for everything a schema cannot express; [section 11](#11-validation-rules-beyond-the-schema) lists those rules with the error codes the conformance kit reports.
The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are used as in RFC 2119.

## 1. Terms

- **Host**: N8 Tray, or the conformance kit's test tray, which plays the host in tests.
- **App** or **module**: a program that installs a manifest.
  Apps are independent: they are installed, updated and run without the host.
- **Built-in**: a module compiled into the host (today only WSL Memory).
  It is described by a manifest the host ships, with status type `builtin`.
- **Component**: a part of an app with its own state, such as one voice engine.
- **Owned process**: a process the app started, or a descendant of its main process.
- **Timestamp**: an RFC 3339 date and time in UTC, ending in `Z`, with 0 to 9 fraction digits.
  Writers SHOULD write milliseconds (`2026-11-09T18:04:11.412Z`).
- **Id**: `[a-z0-9]` followed by up to 47 of `[a-z0-9-]`, unique per host, stable forever.

## 2. Where things live

Everything is per user and needs no administrator rights.

| What | Windows | macOS | Linux |
|---|---|---|---|
| Manifests (the modules folder) | `%LOCALAPPDATA%\N8Tray\modules\` | `~/Library/Application Support/N8Tray/modules/` | `$XDG_DATA_HOME/n8tray/modules/` (default `~/.local/share/n8tray/modules/`) |
| Runtime folder: tokens, status files, the lease | `%LOCALAPPDATA%\N8Tray\run\` | `~/Library/Application Support/N8Tray/run/` | `$XDG_RUNTIME_DIR/n8tray/` |
| Apps the host installs | `%LOCALAPPDATA%\N8Tray\apps\` | `~/Library/Application Support/N8Tray/apps/` | `~/.local/share/n8tray/apps/` |
| The host's private tools (uv) | `%LOCALAPPDATA%\N8Tray\tools\` | `~/Library/Application Support/N8Tray/tools/` | `~/.local/share/n8tray/tools/` |

- The runtime folder MUST be readable and writable by the current user only (an ACL with only the user and SYSTEM on Windows, mode `0700` elsewhere).
  Whoever creates it first, host or app, sets that.
- On Linux without `XDG_RUNTIME_DIR` there is no runtime folder: apps run standalone and are not hosted.
- **Overrides for tests.** When the environment variable `N8TRAY_RUN_DIR` is set, apps and hosts MUST use it as the runtime folder, and when `N8TRAY_MODULES_DIR` is set, installers and hosts MUST use it as the modules folder.
  This is how an app is tested against the test tray without touching the real folders.
- An app's manifest is the file `<id>.json` in the modules folder.
  The app (its installer, or N8 Tray when it installs the app) writes it on install and removes it on uninstall.

## 3. The manifest

Schema: [`schema/manifest.schema.json`](schema/manifest.schema.json).
Examples: [`examples/`](examples).

```json
{
  "contract": 2,
  "id": "voicedock",
  "name": "VoiceDock",
  "version": "1.3.0",
  "installDir": "%N8TRAY_APPS%/voicedock/1.3.0",
  "status": { "type": "http", "url": "http://127.0.0.1:7477/n8/v2/status" },
  "run": { "program": "bin/voicedock-hub.exe", "args": ["--managed"], "stop": { "type": "http" } },
  "trayIcon": "hide-when-hosted"
}
```

### 3.1 Fields

| Field | Required | Meaning |
|---|---|---|
| `contract` | yes | Always `2` for this version. |
| `id` | yes | The app's id (section 1). It MUST match the manifest's file name. |
| `name` | yes | Display name, 1 to 40 characters. |
| `version` | yes | The app's version, SemVer. |
| `installDir` | yes, except built-ins | Absolute path of the installed app, after placeholders (3.2). Every relative path in the manifest resolves inside it. |
| `status` | yes | How the host reads status: `http`, `file` or `builtin` (3.3). |
| `group`, `description`, `icon` | no | Hub grouping (up to 32 characters), one line (up to 200) and a relative path to a `.png` or `.ico`. |
| `platforms` | no | Any of `windows`, `macos`, `linux`; default all three. On another OS the host shows "Not available on this computer" and never starts the app. |
| `run` | no | How the host starts and stops the app (3.4). Without it the host only shows the app and never starts or stops it. |
| `actions` | no | The only operations the host may invoke besides start and stop (3.5). |
| `requires` | no | Dependencies by check id (3.6). |
| `lifecycle` | no | Demand rules and lifecycle features (3.7). |
| `resources` | no | Alert budget and hard cap (3.8). |
| `open` | no | Well-known things to open: `settings`, `logs`, `folder`, `readme` (3.9). |
| `menu` | no | Extra menu lines (3.9). |
| `owns` | no | Program globs that belong to the app (3.10). |
| `trayIcon` | no | `hide-when-hosted` (the app has its own icon and hides it while hosted, section 8) or `none` (default: the app never shows an icon). |
| `autoStartDefault` | no | Whether the host starts the app with itself until the user says otherwise; default `false`. |

The schema rejects unknown fields, so a typo fails `n8tray-contract check`.
Hosts, in contrast, ignore unknown fields they read at run time (section 10).

### 3.2 Paths and placeholders

- A **relative path** uses `/` or `\`, has no `..` segment, no drive letter and does not start with a separator.
  It resolves inside `installDir`; the host resolves symlinks first and rejects a path that leaves `installDir`.
- `installDir` and `owns` entries MAY start with one placeholder from this fixed list, and no other:

| Placeholder | Meaning |
|---|---|
| `%LOCALAPPDATA%` | Windows only: the user's local application data folder. |
| `%USERPROFILE%` | Windows only: the user's profile folder. |
| `%HOME%` | The user's home folder, on every OS. |
| `%N8TRAY_APPS%` | The host's apps folder (section 2). |
| `%N8TRAY_TOOLS%` | The host's private tools folder (section 2). |

- Installers SHOULD write absolute paths; placeholders exist so that checked-in examples and catalog entries work on every machine.

### 3.3 `status`

| Type | Fields | Meaning |
|---|---|---|
| `http` | `url` (required), `intervalSeconds` (1 to 60, default 3), `timeoutMs` (50 to 5000, default 300) | The app serves the status protocol (section 5) on loopback. `url` MUST be `http://127.0.0.1:<port>/n8/v2/status`; the other endpoints are found from it. The token is always the file `<id>.token` in the runtime folder. |
| `file` | `intervalSeconds` (1 to 60, default 3) | The app writes its status document to `<id>.status.json` in the runtime folder (5.2). |
| `builtin` | none | Only in manifests the host ships for its own built-ins. A host MUST reject a `builtin` manifest found in the modules folder. |

### 3.4 `run`

```json
"run": {
  "interpreter": "powershell",
  "script": "n8commentary.ps1",
  "args": [],
  "stop": { "type": "exec", "args": ["-Stop"], "graceSeconds": 12 }
}
```

| Field | Meaning |
|---|---|
| `program` | A relative path to the executable. Exactly one of `program` and `interpreter` is given. |
| `interpreter`, `script` | An interpreter from the fixed list below and the relative path of the script it runs. |
| `args` | Fixed arguments, an array of strings, at most 32. Never a command line: nothing is split or passed through a shell. |
| `cwd` | Relative working directory; default `installDir`. |
| `hidden` | Windows: start without a console window; default `true`. |
| `stop` | How the host stops the app. Default `{ "type": "terminate" }`. |

The interpreters and the exact command line the host builds:

| `interpreter` | Command line |
|---|---|
| `powershell` | `powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File <script> <args...>` (Windows PowerShell 5.1, Windows only) |
| `pwsh` | `pwsh -NoLogo -NoProfile -File <script> <args...>` |
| `node` | `node <script> <args...>`, with the `node` the host's dependency check found |

`stop.type`:

- `http`: the host invokes the `stop` action over HTTP (5.3).
- `exec`: the host runs the app's program or script again with `stop.args`, which asks the running instance to stop (for example through a named event).
- `terminate`: the host asks the OS to end the app's main process (a close request first, where the OS has one).

After `graceSeconds` (1 to 120, default 10) the host MAY force-stop processes, but only ones that pass ownership verification (section 7), and on Windows only inside the app's Job Object.

### 3.5 `actions`

```json
"actions": [
  { "id": "restart", "label": "Restart", "type": "http" },
  { "id": "engine.install", "label": "Set up {component}", "type": "http", "component": true, "progress": true, "confirm": true },
  { "id": "test-sound", "label": "Test marker sound", "type": "exec", "args": ["-TestSound"] }
]
```

| Field | Meaning |
|---|---|
| `id` | `[a-z0-9]`, then up to 47 of `[a-z0-9.-]`, unique in the manifest. `start` and `stop` are reserved for `run`. |
| `label` | 1 to 60 characters. MAY contain `{component}` only when `component` is `true`; the host puts the component's name there. |
| `type` | `http` (5.3) or `exec` (the app's program or script with `args`). `exec` needs `run`. |
| `args` | For `exec`: fixed arguments. When `component` is `true`, an argument MAY be exactly `{component}`, replaced by a component id the app itself listed in its status. No other substitution exists. |
| `component` | The action applies to one component at a time. |
| `progress` | The action may answer `202` with a job (5.3). |
| `confirm` | The host asks the user before invoking it. |

### 3.6 `requires`

```json
"requires": [ { "check": "uv", "for": ["kokoro", "pocket"], "why": "Runs the Python voice engines." } ]
```

`check` names one of the host's built-in dependency checks: `browser`, `uv`, `python`, `engine-env`, `node`, `wsl`, `obs`, `devguard`, `libfuse2`.
A manifest can only name a check, never a command, so it can never make the host run anything.
`for` lists the components that need it (omit when the whole app does); `why` is one line, up to 140 characters.

### 3.7 `lifecycle`

```json
"lifecycle": {
  "demand": { "processes": ["chrome", "msedge", "brave", "vivaldi"], "stopAfterMinutes": 10 },
  "features": ["pressure", "presence"]
}
```

- `demand`: the app is only needed while one of these processes runs (names without extension, `[a-z0-9._-]`, compared case-insensitively per OS).
  The host stops the app after `stopAfterMinutes` (1 to 1440, default 10) without one and starts it again when one appears.
  An app stopped this way is `idle` to the user, not a problem.
- `features`: `pressure` (the app accepts memory-pressure notices, 5.4) and `presence` (the app tracks its own clients and reports `presence` in its status).
  `pressure` needs `status.type` `http`.

### 3.8 `resources`

```json
"resources": { "budget": { "warnMb": 900, "maxMb": 1600, "cpuWarnPercent": 40, "sustainSeconds": 60 }, "hardCapMb": 2500 }
```

- `budget`: suggested alert thresholds for the app's whole process tree, which the user can override.
  Memory over `warnMb` for `sustainSeconds` is attention, over `maxMb` is a problem; CPU over `cpuWarnPercent` for two minutes is attention.
  Without a budget the host uses warn 500 MB, max 1500 MB and CPU 50%.
- `hardCapMb`: a ceiling the OS enforces for the whole tree where it can (a Windows Job Object, a Linux cgroup).
- `warnMb` MUST be below `maxMb`, and `maxMb` MUST NOT be above `hardCapMb`.

### 3.9 `open` and `menu`

- `open` has up to four keys, `settings`, `logs`, `folder` and `readme`; each value is a relative path or a loopback URL (`http://127.0.0.1:<port>/...`).
- `menu` lines are `{ "label", "type": "open-url", "url" }` (a loopback URL, or any `https://` URL, opened in the default browser), `{ "label", "type": "open-path", "path" }` (a relative path) or `{ "label", "type": "action", "action" }` (an id from `actions`).
- An app's own page (a loopback URL) opens in a separate window or the default browser, never with access to the host's commands.

### 3.10 `owns`

```json
"owns": [ { "program": "engines/piper/*/piper.exe" }, { "program": "%N8TRAY_TOOLS%/uv/python*", "argsContain": "voicedock" } ]
```

Program globs (`*` within a folder, `**` across folders) inside `installDir` or behind `%N8TRAY_TOOLS%`, with an optional substring the command line must contain.
`owns` is one input to ownership verification (section 7): it lets the host recognize a process whose parent already exited.
It is never a list of things to kill.

## 4. The status document

Schema: [`schema/status.schema.json`](schema/status.schema.json).
The same document is served over HTTP or written to the status file.

```json
{
  "contract": 2,
  "id": "voicedock",
  "version": "1.3.0",
  "updatedAt": "2026-11-09T18:04:11.204Z",
  "pid": 14820,
  "pidStartedAt": "2026-11-09T08:01:57.412Z",
  "features": ["events", "progress", "pressure", "presence"],
  "state": "ready",
  "summary": "Kokoro ready, Piper idle",
  "components": [
    { "id": "piper", "name": "Piper", "state": "ready", "detail": "8 voices, no process while idle" },
    { "id": "kokoro", "name": "Kokoro", "state": "ready", "detail": "Model loaded, unloads in 7 min" }
  ],
  "presence": { "lastSeenAt": "2026-11-09T18:03:40.000Z", "clients": 1, "activeEngine": "kokoro" },
  "resources": {
    "sampledAt": "2026-11-09T18:04:10.950Z",
    "intervalSeconds": 5,
    "processes": [
      { "pid": 14820, "startedAt": "2026-11-09T08:01:57.412Z", "parentPid": 9124, "role": "hub", "component": null, "rssMb": 31.4, "privateMb": 27.9, "cpuPercent": 0.1 }
    ]
  },
  "jobs": []
}
```

| Field | Required | Meaning |
|---|---|---|
| `contract`, `id`, `version` | yes | As in the manifest. |
| `updatedAt` | yes | When this document was produced. |
| `pid`, `pidStartedAt` | yes | The app's main process and its creation time. Together they identify it; a pid alone can be reused. |
| `state` | yes | The app's state (section 6): `ready`, `idle`, `starting`, `degraded`, `busy` or `failed`. |
| `summary` | yes | One line for the hub row, up to 120 characters. |
| `detail` | no | Why the state is what it is, up to 300 characters. SHOULD be given for `degraded` and `failed`. |
| `features` | no | Optional protocol features the app supports: `events`, `progress`, `pressure`, `presence`. |
| `components` | no | `{ id, name, state, detail? }`, where `state` is any state of section 6. |
| `presence` | no | For apps with the `presence` feature: `lastSeenAt` (timestamp or null), `clients` (count) and app-defined string fields. |
| `resources` | yes | Section 4.1. |
| `jobs` | no | Running and recently finished jobs (5.3): `{ id, action, component?, state: running \| done \| failed, percent?, message?, startedAt }`. |

### 4.1 Resources

Every app MUST report the resources of the processes it owns.

| Field | Required | Definition |
|---|---|---|
| `sampledAt` | yes | When the figures were taken. |
| `intervalSeconds` | yes | The interval CPU is averaged over: the app's sampling cadence, 1 to 60 s (2 to 10 recommended). |
| `processes[]` | yes, at least the main process | One entry per owned process the host can see. |
| `.pid`, `.startedAt` | yes | OS pid and creation time, to the millisecond where the OS gives it. |
| `.parentPid` | yes | As the OS reports it (0 when there is none). |
| `.role` | yes | `main`, `hub`, `engine`, `worker`, `helper` or `ui`. Display only. |
| `.component` | no | The component the process belongs to, or null. |
| `.exe` | no | The executable's path, relative to `installDir` where it is inside it. Display only. |
| `.rssMb` | yes | Working set on Windows, resident set size on macOS and Linux, in MiB. The "Memory" figure users see. |
| `.privateMb` | no | Private bytes on Windows, USS or `phys_footprint` elsewhere, where cheap. |
| `.cpuPercent` | yes | Share of the whole machine (all logical cores together are 100), averaged over `intervalSeconds`. `null` until a second sample exists. |
| `.gpu` | no | `{ memMb, percent }`, when the app knows them. |
| `.model` | no | For engines: `{ loaded, mb? }`, which explains a jump in memory. |
| `namespaces[]` | no | Processes the host cannot see, such as a daemon inside WSL: `{ namespace, pid, startedAt, role?, component?, exe?, rssMb, cpuPercent }`, where `namespace` is `wsl:<distro>`. Counted under that namespace, not under the host OS. |

- An app reports only processes it started or that descend from its main process.
  Threads and library helpers are part of their process, not separate entries.
- An app refreshes `resources` at least every 10 s while running.

## 5. The status protocol

### 5.1 HTTP

All endpoints live under the origin of `status.url`, on `127.0.0.1` only.

| Request | Answer |
|---|---|
| `GET /n8/v2/status` | `200` with the status document. |
| `POST /n8/v2/actions/<id>` with `{}` or `{ "component": "<component id>" }` | `200` with an action result `{ "outcome": "ok" \| "already" \| "refused" \| "failed", "message"? }`, or `202` with `{ "jobId" }` for a `progress` action. Schema: [`schema/action-result.schema.json`](schema/action-result.schema.json). |
| `POST /n8/v2/pressure` with `{ "level": "ok" \| "attention" \| "problem" }` | `200` with an action result. Only for apps with the `pressure` feature. |
| `GET /n8/v2/events` | Optional (`events` feature): server-sent events, one `event: status` with the whole document on every change. |

- Every request carries `Authorization: Bearer <token>`, where the token is the content of `<id>.token` in the runtime folder (section 9).
- `401` for a missing or wrong token, `403` for a request with an `Origin` header or a `Host` other than `127.0.0.1:<port>` or `localhost:<port>`, `404` for an unknown action, `400` for a malformed body or an unknown component.
- `GET /n8/v2/status` answers within `timeoutMs`; the app keeps the document ready rather than computing it per request.

### 5.2 Status file

- The app writes the status document to `<id>.status.json` in the runtime folder on every change and at least every 10 s.
- Writes are atomic: a temporary file in the same folder, then a rename over the old one.
- The document is valid only while its `pid` is alive with a creation time within 1 s of `pidStartedAt` and `updatedAt` is under 30 s old.
  A stale or orphaned file means the app is not running; the app need not delete it.
- Readers MUST accept a UTF-8 byte order mark (Windows PowerShell writes one) and MUST treat a half-written or unparsable file as no status.
- File-status apps have no channel for actions or pressure: their actions are `exec`.

### 5.3 Actions and jobs

- `ok`: done. `already`: nothing to do, the app was already in the asked state. `refused`: not allowed now (for example low memory); `message` says why. `failed`: tried and failed; `message` says why.
- A `progress` action MAY answer `202` with a `jobId`; the job then appears in the status's `jobs` with its `percent` and `message` until it is `done` or `failed`, and stays listed for at least 60 s after it ends.
- `exec` actions report through their exit code (0 is `ok`, anything else `failed`) and the next status.

### 5.4 Memory pressure

The host computes one system memory pressure level (`ok`, `attention`, `problem`, with hysteresis) and pushes every change to apps with the `pressure` feature, and pushes the current level again when such an app starts.
What an app does with it is its own policy; VoiceDock's hub, for example, unloads idle models at `attention` and refuses to start heavy engines at `problem`.

## 6. The status vocabulary

One set of states for apps and components, so every hub row reads the same way.

| State | Meaning | Host level | Words users see | Used by |
|---|---|---|---|---|
| `ready` | Running and doing its job | ok | Ready | app, component |
| `idle` | Stopped or unloaded on purpose to save memory; starts on the next request | ok | Sleeping, starts when needed | app, component |
| `starting` | Process up, not ready yet (loading a model) | attention | Starting | app, component |
| `degraded` | Running, but a component it should run is not ready | attention | Ready, with one line naming what is not | app, component |
| `busy` | Installing, updating or stopping | attention | Installing, Updating, Stopping | app, component |
| `stopped` | Installed, not running | off, or problem if it should run | Installed but not running | host, component |
| `failed` | Crashed or cannot start; `detail` says why | problem | Stopped: reason | app, component |
| `not-installed` | Known but not installed | off | Not set up | host, component |
| `needs-setup` | Installed, but a required dependency is missing | off | Needs a dependency, see Checks | host, component |
| `unsupported` | Cannot run on this OS or CPU | hidden | Not available on this computer | host, component |

- An app reports only the states marked "app" for itself; `stopped`, `not-installed`, `needs-setup` and `unsupported` are what the host concludes when there is no live status, and components may use every state.
- The host's levels are ok, attention, problem and off; the tray shows the worst level of all apps.
- Whether an app should run is the host's intent (the user's choice, or a `demand` rule), never the app's.
  An app stopped by a `demand` rule is shown `idle`, not `stopped`.

## 7. Ownership and measurement

The host measures every process it attributes to an app itself and verifies that the app owns it, so an app can neither hide nor inflate what it costs.

### 7.1 What the app MUST do

- Report every process it owns that the host can see, with `pid`, `startedAt` and `parentPid` as the OS reports them.
- Never report a process it does not own.
- Report processes the host cannot see (inside WSL) under `namespaces`, never under `processes`.

### 7.2 How the host verifies

1. **Identity.** The pid exists and its creation time is within 1 s of `startedAt`; otherwise the pid was reused and the entry is ignored.
2. **Tree.** The process is the app's root process (the one the host started, or the status `pid` of an app found running) or a descendant of it, walked through parent pids with creation times checked at each step: a parent must be older than its child.
3. **Containment, where the OS has it.** Windows: every app the host starts runs in its own Job Object, and membership confirms ownership even after the parent exited. Linux: the app's systemd user scope, through the process's cgroup. macOS: process group and parent chain only.
4. **Location.** The executable is inside `installDir` or the host's tools folder, or matches an `owns` glob. An entry that passes this step but not steps 1 to 3 is shown as "unverified", is not counted and is never acted on.
5. **Nothing hidden.** The host walks the tree itself. A descendant the app did not report is still counted and flagged "unreported child".

- The figures users see are the host's own measurement for every verified process it can see; the app's figures are used for `namespaces`, `gpu` and `model`.
- When the two differ by more than 25% for a minute, the host notes it in the details panel; it is a bug in the app's sampler, not an alert.
- The host force-stops only verified processes (section 3.4).

## 8. The host lease

The host tells apps that it hosts them, so no app ever shows a second icon.
This is the format N8 Tray 0.26.0 and later already writes (Phase 0 of the rebuild) and VoiceDock 1.2 reads.
Schema: [`schema/lease.schema.json`](schema/lease.schema.json).

```json
{ "contract": 2, "host": "n8tray", "version": "1.0.0", "pid": 9124,
  "pidStartedAt": "2026-11-09T08:01:40.118Z", "heartbeat": "2026-11-09T18:04:10.002Z",
  "hosting": ["voicedock", "n8commentary", "n8hotkeys", "leadscraper"] }
```

### 8.1 The host

- Writes `host.json` in the runtime folder every 5 s, atomically (temporary file, then rename), starting with its first status tick.
- `hosting` lists the ids of the apps it shows; an app the user set to "use its own icon", or turned off, is not listed.
- Never deletes the lease on exit: it goes stale, and every app's own icon comes back.
  (Deleting it would let an app's own tray win the race at sign-in and start a second owner for the same server.)
- Holds one host at a time: the Tauri host holds `host.lock` in the runtime folder; on Windows it also checks the PowerShell tray's `Local\N8Tray_SingleInstance` mutex.

### 8.2 The app

An app with `trayIcon: "hide-when-hosted"` reads the lease at start and every 5 s:

1. **Hosted** when the lease lists its id, `heartbeat` is less than 15 s from now in either direction (a heartbeat slightly in the future is clock noise) and the process `pid` is alive.
   Readers SHOULD also check that the process's creation time is within 1 s of `pidStartedAt` where that is cheap.
   While hosted, the app shows no icon and leaves to the host anything the host owns (VoiceDock leaves its servers to it).
2. **Own** otherwise: the app shows its icon and runs standalone.
   After the host quits or crashes, the icon comes back within about 15 s.
3. **Waiting**, only at start: a stale lease that lists the app usually means the host is starting at sign-in too, so the app waits up to 30 s for a fresh lease before showing its icon.
   Once the app has been hosted or has shown its icon, it never waits again.

- A missing, unreadable, half-written or non-object lease is no lease. Readers MUST accept a UTF-8 byte order mark.
- Hosting is about the icon: an app keeps working, and keeps serving its status, either way.
- The command-line switch `-NoTray` that apps had before the lease stays an alias for "never show an icon".

## 9. Security

- **Loopback only.** Status URLs are `http://127.0.0.1:<port>/...`; apps bind `127.0.0.1`, never `0.0.0.0`.
- **A token per app.** The app writes 32 random bytes as 64 lowercase hex characters to `<id>.token` in the runtime folder at every start, before it serves, atomically, readable by the user only.
  The host reads it when it needs it and again after a `401`.
  The token never appears on a command line, in a URL, in a log or in a status document; apps compare it in constant time.
- **The control plane** (`/n8/v2/*`) refuses any request with an `Origin` header and any `Host` but `127.0.0.1:<port>` or `localhost:<port>`, which stops web pages and DNS rebinding.
- **An app's data plane** (for VoiceDock `/tts`, `/voices`, `/engines`, `/presence`) is the app's own business, outside `/n8/v2/`; it MUST NOT allow anything the control plane protects (installing, stopping, reading private data).
- **No command strings.** A manifest has programs that resolve inside `installDir` (after symlinks), an interpreter from a fixed list, and fixed argument arrays.
  The only substitution is `{component}`, an id the app itself listed, matching the id rule.
  Dependency checks are the host's own code; a manifest only names one.
- **Ownership before force.** The host force-stops only verified processes (section 7).
- **Not covered:** malware running as the same user can write a manifest; that is the trust level of the Startup folder.

## 10. Versioning

- `contract` is an integer major version.
  A host supports its own major and the one before it, and shows an app with a newer major as "Needs a newer N8 Tray".
- **Within version 2**, changes are additions only: new optional fields and new `features` flags.
  Hosts ignore unknown fields at run time, so an app can adopt a newer minor field without breaking an older host.
  The authoring check (`n8tray-contract check`) is strict and rejects unknown fields, so typos are caught; an app pins the kit version it is checked against.
- A change that would make a document valid today invalid, or change what a field means, is a new major.
- **Version 1** is today's `modular.json` (`schemaVersion` 1 or 2), which the PowerShell tray reads.
  It is accepted during the migration: `n8tray-contract check` validates it against [`schema/modular-v1.schema.json`](schema/modular-v1.schema.json), and the Tauri host reads it through an adapter until the PowerShell tray retires, then drops it.

| Version 1 (`modular.json`) | Seen by the host as |
|---|---|
| `id`, `name`, `group`, `description`, `autoStartDefault` | The same fields. |
| `entry` (a `.ps1`), `launchArgs` | `run` with `interpreter: "powershell"`, `script` and `args`. |
| `monitor` (`type: "process"`, `match`, `requiredArgs`) | No status document: the state is `ready` while a matching process runs, else `stopped`. |
| `health[]` | One component per entry, `ready` when its health URL answers, else `failed`. |
| `settings`, `log` | `open.settings`, `open.logs`. |
| `tray.icon` | `icon`. |

## 11. Validation rules beyond the schema

`n8tray-contract check` reports each problem with a code.
`schema` covers every JSON Schema violation; the others are:

| Code | Applies to | Rule |
|---|---|---|
| `id-file-name` | manifest | The file name (without `.json`) matches `id`, unless the file is named `manifest.json` or `modular.json`. |
| `path-escapes` | manifest | A relative path is absolute, has a drive letter or has a `..` segment. |
| `placeholder` | manifest | A path uses a placeholder outside the list in 3.2, or one in the middle of a path. |
| `install-dir` | manifest | `installDir` is not absolute after its placeholder. |
| `status-url` | manifest | `status.url` is not `http://127.0.0.1:<port>/n8/v2/status` with a port from 1 to 65535. |
| `builtin-run` | manifest | A `builtin` manifest has `run`, `installDir` or `exec` actions, or a non-builtin one has no `installDir`. |
| `action-id` | manifest | Two actions share an id, or an action uses `start` or `stop`. |
| `action-exec` | manifest | An `exec` action without `run`, `args` on an `http` action, or an `http` action or `http` stop without `status.type` `http`. |
| `component-placeholder` | manifest | `{component}` in a label or argument of an action that is not `component: true`, or an argument that contains it with other text. |
| `menu-action` | manifest | A menu line names an action that is not declared. |
| `menu-url` | manifest | An `open-url` or `open` URL that is neither loopback HTTP nor HTTPS (`open` URLs must be loopback). |
| `budget` | manifest | `warnMb` is not below `maxMb`, or `maxMb` is above `hardCapMb`. |
| `feature-transport` | manifest | `pressure` without `status.type` `http`. |
| `main-process` | status | `resources.processes` does not list the status `pid`. |
| `duplicate-pid` | status | A pid appears twice in `processes`. |
| `self-parent` | status | A process names itself as its parent. |
| `start-order` | status | A process started after `resources.sampledAt` (1 s tolerance), or `pidStartedAt` differs from the main process's `startedAt` by more than 1 s. |
| `unknown-component` | status | A process or job names a component the status does not list (only when `components` is given). |
| `duplicate-id` | status | Two components or two jobs share an id. |
| `lease-start-order` | lease | `heartbeat` is earlier than `pidStartedAt` (1 s tolerance). |

`n8tray-contract probe` checks a running app; its checks are described in the conformance kit's README.
