# Decisions

Why N8 Tray is built the way it is.
Newest last; an entry is never rewritten, only superseded by a later one that says so.

## D1. A public repo for the host and the contract (2026-11-09)

The rebuilt N8 Tray (Tauri 2, Windows, macOS and Linux) and its contract live in this public repo, `n8watkins/n8tray`, under the MIT license.
The PowerShell tray and the private modules (N8 Commentary, N8 Hotkeys) moved to the private repo `n8watkins/n8tray-modules`, which held the `n8tray` name until this repo took it.
Lead Scraper stays private in its own repo.
Public, because apps such as VoiceDock that anyone can install must be able to read the contract and run its conformance kit.

## D2. The contract first, alone (2026-11-09)

Phase 1 (the contract: spec, JSON Schema, fixtures, conformance kit) is merged and tagged before any other work starts.
Four tracks then build against it in parallel: the tray app, VoiceDock's engines, the adopters and the other platforms.
After that the contract changes only through a pull request that updates the spec, schema, fixtures and types together, so no track builds on a moving target.

## D3. Rust for the workspace, release-kit for releases (2026-11-09)

The host and the contract crate are Rust, as the plan's Tauri app needs.
release-kit (package mode) owns commits, versions, notes and releases; the root `package.json` exists for it and runs the Cargo checks.
The workspace version lives once, in `Cargo.toml` `[workspace.package]`, and `.release.json` lists it and its `Cargo.lock` entries as version files.

## D4. Our own Dependabot file (2026-11-09)

`.github/dependabot.yml` started as release-kit's and adds a Cargo entry.
It drops the kit's marker line so a re-run of apply keeps it instead of replacing it and losing Cargo; apply still checks the kit's rules on it.
When a kit release changes its Dependabot file, merge the change in by hand.

## D5. The JSON Schemas are normative; authoring is strict, run time is lenient (2026-11-09)

The schemas in `spec/schema/` define the shape of every document, and the specification adds only what a schema cannot express, as rule codes (section 11).
The Rust types must read exactly what the schemas accept; the fixture tests check that every valid fixture reads back unchanged through them.
The schemas reject unknown fields so that `n8tray-contract check` catches typos, while hosts ignore unknown fields at run time so that minor additions within contract 2 never break an older host.

## D6. Built-ins are described by manifests too (2026-11-09)

WSL Memory is Rust code inside the host, but it is described by a manifest with `status.type` `builtin`, which the host ships and never accepts from the modules folder.
Every hub row then comes from one kind of description, and the example set covers all five apps.

## D7. Fewer knobs than the plan's draft (2026-11-09)

The plan's draft manifest had `status.tokenFile` and `run.mode`.
The token file is always `<id>.token` and the status file always `<id>.status.json` in the runtime folder, and the only run mode is a long-running app, so both fields are gone: one less way to get a manifest wrong.
A field can be added later within contract 2 when a second value exists.

## D8. Test overrides for the folders (2026-11-09)

`N8TRAY_RUN_DIR` and `N8TRAY_MODULES_DIR` override the runtime and modules folders, and apps must honor `N8TRAY_RUN_DIR`.
That is what lets an app be tested against the conformance kit's test tray in CI without touching the real folders.

## D9. Phase 0's lease is the contract's lease (2026-11-09)

The lease N8 Tray 0.26.0 writes and VoiceDock 1.2 reads (heartbeat every 5 s, fresh for 15 s either way, host pid alive, a 30 s grace at start, never deleted) is section 8 of the specification unchanged.
A fixture holds exactly what the PowerShell tray writes, so the schema can never drift from the tray that runs today.

## D10. The conformance kit is one crate, installed with Cargo from a tag (2026-11-09)

`crates/n8tray-conformance` builds both tools, `n8tray-contract` (check, probe, the test tray) and `n8tray-fake-module`, so one install gives an adopter everything, and the contract library stays free of the HTTP server and process-table dependencies the tools need.
Adopters install it with `cargo install --locked --git https://github.com/n8watkins/n8tray --tag vX.Y.Z n8tray-conformance`, which works on every GitHub-hosted runner without a release pipeline; prebuilt binaries can come with the Tauri release work.
The probe reads process creation times through sysinfo, which gives whole seconds, so its identity check allows for that; the host will use the precise OS calls.

## D11. One OS crate with unsafe code, and only there (2026-11-09)

The workspace forbids unsafe code.
`crates/n8tray-platform` is the exception: it denies unsafe code too, but allows it in `src/win/`, where each Windows API is wrapped once behind a safe function, and in two `libc::kill` calls on Unix.
`n8tray-core` keeps the workspace's `forbid` and takes the OS only through those safe functions, so its logic is tested with data.
The process snapshot on Windows is one `NtQuerySystemInformation(SystemProcessInformation)` call per pass, the way Task Manager reads processes: pid, parent, creation time to 100 ns, CPU times, working set and private bytes for every process at once.
macOS and Linux read the same through sysinfo until their phases.

## D12. Every app the host starts runs in a named Job Object (2026-11-09)

On Windows the host creates the job `Local\N8Tray.App.<id>` and starts the app suspended, puts it in the job, then resumes it, so nothing the app starts can escape the job.
The job has no kill-on-close: apps keep running when N8 Tray exits, and after a restart the host opens the job by name again.
`resources.hardCapMb` becomes the job's memory limit.
A force stop ends the job, never a process outside it; an app the host did not start (found running) is only ever asked to stop.

## D13. The host watches only while the PowerShell tray runs (2026-11-09)

When `Local\N8Tray_SingleInstance` exists, the new host reads every app's status but writes no lease and starts, stops or restarts nothing, so the two trays never fight over an app.
Replacing the old tray (spec 8.1, plan section 15) comes with the switch-over in Phase 9.
With `N8TRAY_HOME`, `N8TRAY_RUN_DIR` or `N8TRAY_MODULES_DIR` set, the folders are not the ones the old tray uses, so the host ignores it; that is how a build is tried next to the running tray.

## D14. "Start with N8 Tray" applies when an app is first found (2026-11-09)

An app whose start-with-N8-Tray setting is on (the manifest's `autoStartDefault` until the user changes it) starts when the host starts, and also when its manifest appears while the host runs, so installing an app makes it run without restarting the tray.
It applies once per app per host run: an app the user stopped stays stopped.
An app that exits unexpectedly while it should run is restarted after 5 s, 30 s and 2 min, and the host gives up at the fourth exit in 10 minutes, as the PowerShell tray did for N8 Hotkeys.

## D15. TypeScript types generated with ts-rs (2026-11-09)

The plan named specta for the bindings between Rust and the window's TypeScript.
The snapshot types in `n8tray-core` derive `ts_rs::TS` instead: ts-rs is stable, needs no Tauri plugin, and keeps `n8tray-core` free of Tauri.
`cargo test` writes the files to `app/ui/src/bindings/`, and CI fails when they differ from what is committed, so a mismatch fails the build as the plan asks.

## D16. A host folder of its own, and `N8TRAY_HOME` (2026-11-09)

Besides the contract's folders (modules, runtime), the host keeps `settings.json`, `status.json`, `logs/` and `history/` in `%LOCALAPPDATA%\N8Tray` on Windows, `~/Library/Application Support/N8Tray` on macOS, and the XDG config and state folders on Linux.
`N8TRAY_HOME` moves all of it, and the contract's folders, under one folder.
The status file is the snapshot the window shows, rewritten every 10 s and on every level change, so tests and scripts can read the host's state without a window.

## D17. The window exists only while it is used (2026-11-09)

The tray icon and its menu are native and need no WebView.
The window (and its WebView2 processes) is created when it opens, hidden when it closes or loses focus, and destroyed 60 s after it was hidden, so its memory goes back to the system; reopening within 60 s is instant, later it takes a moment to create.
The menu is rebuilt only when its shape changes (an app or an action appears or goes); otherwise only the texts and states that changed are updated, so an open menu does not flicker.

## D18. The window gets four commands and no plugins (2026-11-09)

`capabilities/hub.json` gives the window `get_snapshot`, `module_action`, `open_item` and `hide_window`, and the snapshot event, nothing else: no file system, shell, HTTP or opener plugin.
`open_item` takes an app id and the key of an entry the manifest declared, never a path or URL, so the window cannot make N8 Tray open anything an app did not declare.
The only plugin is single-instance, used in Rust: starting N8 Tray again opens the window, `--hide` closes it and `--exit` ends the running copy.

## D20. One measurement pass, at the window's pace (2026-11-09)

The host measures every process with one OS snapshot per pass: every 3 s while the window is open or while something is happening (a start, a stop, an action, a planned restart, an app waiting for its demand process), every 10 s otherwise.
CPU is the CPU time a process used between two passes over the wall time and every logical processor, as Task Manager shows it; the first pass after a process appears has none ("measuring").
GPU counters, the most expensive read, are taken at most every 10 s, and only on Windows.
The figures users see are the host's for every process it counts; an app's own figures are used only for processes inside WSL and, when the host has no counters, for GPU, and a gap over 25% for a minute becomes a note in the panel.
A test measures one pass's CPU cost on every CI runner and fails above 0.1% of the machine at the 10 s pace (0.5% for debug builds).

## D21. Two memory levels from one reading (2026-11-09)

System memory has the PowerShell tray's thresholds and hysteresis: attention below 3.0 GB free, problem below 1.5 GB, back only 10% above the threshold (3.3 and 1.65 GB).
The level shown in the tray dwells 10 s both ways, as before.
The level pushed to apps with the `pressure` feature enters after the same 10 s but recovers only after 60 s, so an engine is not stopped and started again every few seconds while memory hovers (plan 5.5).
A problem notifies once per episode, as do an app's memory alerts; CPU alerts only color the row.

## D22. History in memory and in day files (2026-11-09)

Every app and the tray itself keep 15 minutes in memory at 10 s resolution (the maximum memory and average CPU of each 10 s), sent with the snapshot for the sparklines.
On disk, one row per app per minute goes to `history/YYYY-MM-DD.csv` (UTC days): minute, app, memory, private memory, CPU, GPU memory and process count, kept for `history.keepDays` days (7).
CSV keeps the files readable without N8 Tray and cheap to append.

## D19. A lean WebView2, and handing memory back when idle (2026-11-09)

Measured on the owner's PC (Windows 11, WebView2 157), with the window open:

| WebView2 arguments | Processes | Working set | Private |
|---|---|---|---|
| Tauri's defaults | 6 | 344 MB | 157 MB |
| plus `--in-process-gpu` | 5 | 297 MB | 118 MB |
| plus `--disable-gpu --in-process-gpu --renderer-process-limit=1 --disable-background-networking` | 5 | 275 MB | 90 MB |

The window is a small static page, so it needs no GPU process and at most one renderer: the last row is the default.
`N8TRAY_WEBVIEW_ARGS` replaces the arguments, for measuring.
Most of WebView2's working set is shared, mapped runtime code; its private bytes are what it costs the system.

The tray hands its unused resident pages back to Windows (EmptyWorkingSet) 30 s after start and after its window is destroyed: start-up and the WebView touch far more memory than an idle tray keeps.
Measured, the idle tray's working set went from 33 MB to 5.4 MB (12.8 MB private), and back to 5.6 MB 70 s after the window closed.

The tray's own figures are its process and its window's processes, never the apps it started, although those are its child processes too.

## D23. App icons may be SVG (2026-11-09)

The hub shows each app's own icon, so users recognize the app at a glance, as the PowerShell tray's hand-drawn icons did.
Contract 2 allowed only `.png` and `.ico`; it now also allows `.svg` (spec 3.11), the preferred format because it stays sharp at every display scale.
This widens what is valid and makes nothing invalid, so it stays within contract 2; a kit release before it rejects an SVG icon in `n8tray-contract check`, and an app checked against it pins that kit.
The host never had a release that read manifests, so no installed host rejects such a manifest.
The host reads an icon itself (at most 256 KB) and the window shows it only as an image, so an SVG's scripts or external references never run; a missing or unusable icon shows the generic app glyph.
`spec/examples/icons/` has the icons for the five example apps, redrawn in SVG from the PowerShell tray's artwork: a speaker for VoiceDock, a keyboard for N8 Hotkeys, a camera for N8 Commentary, a briefcase for Lead Scraper and a memory chip for WSL Memory.

## D24. The window's budget is private memory; both figures are shown (2026-11-09)

Plan 3.6 gives N8 Tray budgets: idle with no window at most 30 MB and 0.2% CPU, with the window open at most 150 MB including the WebView, and back under 35 MB within 70 s of closing it.
The budget metric is private memory (private bytes on Windows): what N8 Tray alone costs the system.
The working set also counts WebView2's shared, mapped runtime code, which other WebView2 and Edge processes share, so it overstates the cost and moves with whatever else runs (D19).
The status details show both figures for the tray and for its window, and the snapshot carries `windowPrivateMb` next to `windowMb`.
`scripts/tray-budget.mjs` measures a release build in its own folders from the host's status file and fails CI on Windows when a figure is over budget; on Linux and macOS it reports the figures, since the budgets were set for the owner's Windows PC and WebKitGTK and WKWebView cost differently.

## D25. The hub (2026-11-09)

The window follows the mockup the owner approved on 2026-11-09, with the apps' own icons instead of letter tiles.
Each row has one action, Stop while the app runs and Start while it does not; Restart is in the app's panel and the right-click menu.
Its views are the hub, an app's panel (state, components, the last 15 minutes, processes, what to open, its settings), status details, settings and resource history (24 hours and 7 days from the day files).
The window is a view: Rust sends the snapshot and answers a fixed list of commands (`capabilities/hub.json`); which actions are possible, levels and texts are the host's, and the window only lays them out and words them.

It opens 2 px from the tray icon, away from the taskbar, inside the work area of the icon's monitor, on every taskbar edge, with an auto-hidden taskbar and from the overflow flyout: the PowerShell tray's rules, ported as `n8tray_core::placement` and unit-tested per edge.
Its height follows its content and it grows away from the taskbar; a window taller than the work area scrolls.
It is created hidden and shown only once it has drawn itself and reported its height (`fit_window`), so it appears in one piece; if it does not draw within 2.5 s it is shown anyway.
On Windows it is a tool window (no Alt-Tab or taskbar entry) with Windows 11's rounded corners.
A frameless Tauri window on Windows keeps invisible resize borders (7 px left and right, 7 px below on the owner's PC), so the placement positions the visible frame that DWM reports (`DWMWA_EXTENDED_FRAME_BOUNDS`), not the window rectangle; measured on the owner's PC, the visible edge sits exactly 2 px from the icon and the taskbar.
Focus loss hides only a window that is shown: a window still being created can lose a focus it never had, and must still appear.
Keyboard: Tab and Shift+Tab through every control, arrows between rows and within a row, Enter and Space, Escape back to the hub and then closed.
High Contrast uses the system's colors through `forced-colors`, and each level stays a distinct dot.

The pictures for review come from Playwright against recorded snapshots (`app/ui/preview/`), the replacement for the PowerShell tray's `-Render`; the real window is tested through WebDriver on Windows and Linux.
