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
