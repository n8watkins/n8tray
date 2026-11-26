# Decisions

Why N8 Tray is built the way it is.
Newest last; an entry is never rewritten, only superseded by a later one that says so.

## D1. A public repo for the host and the contract (2026-11-09)

The rebuilt N8 Tray (Tauri 2, Windows, macOS and Linux) and its contract live in this public repo, `n8watkins/n8tray`, under the MIT license.
The PowerShell tray and the private modules (N8 Commentary, N8 Hotkeys) moved to the private repo `n8watkins/n8tray-modules` (renamed `n8watkins/n8tray-v1` and archived on 2026-11-11), which held the `n8tray` name until this repo took it.
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

Note (2026-11-26): WSL Memory was dropped before it shipped (D34); the `builtin` status type stays in contract 2.

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
The window (and its WebView2 processes) is created when it opens, hidden when it closes or loses focus, and destroyed 10 s after it was hidden, so its memory goes back to the system; reopening within 10 s is instant, later it takes about 0.3 s to create and draw.
It was 60 s until 0.2.1; measured on the owner's PC on 2026-11-10, the hidden window kept about 100 MB private (six processes, 280 MB working set) for that whole minute, while creating it again took 270 to 350 ms plus about 60 ms to draw.
The owner reopens it within seconds when he does at all, so 10 s keeps that instant and frees the memory 50 s sooner.
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

Note (2026-11-26): D34 removes the level shown in the tray and its notification; the level pushed to apps stays as described here.

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

Note (2026-11-26): the WSL Memory icon left with its example manifest (D34); four example icons remain.

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
The window measures its content during layout, not in an animation frame, since a hidden WebView runs no frames; the native window gets the page's background color, so the first frame shown already matches.
Focus loss hides only a window that is shown: a window still being created can lose a focus it never had, and must still appear.
Keyboard: Tab and Shift+Tab through every control, arrows between rows and within a row, Enter and Space, Escape back to the hub and then closed.
High Contrast uses the system's colors through `forced-colors`, and each level stays a distinct dot.

The pictures for review come from Playwright against recorded snapshots (`app/ui/preview/`), the replacement for the PowerShell tray's `-Render`; the real window is tested through WebDriver on Windows and Linux.

## D26. No frozen prototypes in the window (2026-11-09)

`app.security.freezePrototype` was on since Phase 2, and the window never drew: Preact assigns `constructor` on every component it creates, which throws once `Object.prototype` is frozen, so the window stayed at "Starting".
It is off.
The window loads only N8 Tray's own bundled code under a strict CSP (`default-src 'self'`), with no remote content and no app code, so freezing the prototype guarded against little.
The WebDriver tests run the real window in CI, so a runtime failure like this one fails a pull request now.

## D27. Updates install on a click, signed with N8 Tray's own key (2026-11-10)

The host looks for an update at start (after a minute) and once a day, as before; Install in the window or the tray menu now installs it in place instead of opening the release page.
Tauri's updater plugin downloads it from the channel's feed and verifies its minisign signature against the public key in `tauri.conf.json` before anything runs; the window has no updater permission, only the `install_update` command, and the host shows the progress in the snapshot.
On Windows the per-user NSIS installer runs passively and starts N8 Tray again with the same arguments; the host stops first, so the lock and the lease are released.
The private key and its password are GitHub Actions secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) and live in the owner's password store with an offline backup; losing the key means installed copies can never take another in-app update.

The installer is built and signed by `app-release.yml`, which runs after release-kit's Release workflow has published a release: release-kit verifies and publishes on Linux, and an NSIS installer needs Windows.
Its build job holds the key with read access only; a separate publish job, without the key, uploads the installer as `N8Tray_<version>_x64-setup.exe`, its `.sig`, a checksum file and `latest.json`, and never replaces a file already on the release.
Re-running release-kit's Release workflow after that fails, since it sees files it did not build; retry the app's part with "Run workflow" on App release instead.
Only Windows is built until the macOS and Linux phases.

CI builds two installers signed with a throwaway key (this version and the next patch) and updates one to the other end to end on Windows, through the real window.
`N8TRAY_UPDATE_FEED` points every channel at another feed for that test; the test builds trust the throwaway key and allow a plain-HTTP feed through `--config`, so the shipped app still accepts only HTTPS and only the real key.
The beta channel keeps its feed address but has no feed yet, because release-kit cuts only plain `vX.Y.Z` releases.

## D28. The release check leaves the app crate to CI and App release (2026-11-10)

release-kit's Release workflow verifies a tag by running `npm run release:check` on a bare Ubuntu runner, and offers no way to install system packages first.
The Tauri app (`n8tray`) needs the GTK, GLib and WebKit development packages to compile on Linux, so v0.2.0's Release run failed and nothing was published; the tag stays as it is and the next release carries its notes.
`release:check` now runs the format and text checks on everything and clippy, the tests and the build on every crate but the app (`npm run check:no-app`).
The app is still checked on every pull request (lint, tests and the real window on all three OSes, CI result being required), and App release builds it for real on Windows before anything is signed.
A CI job runs `npm run release:check` on a bare runner too, so a change that would break a release fails its pull request.
If release-kit gains a way to install system packages before its checks, this can go back to the full `lint`, `test` and `build`.

## D29. One list by state, one watch line, the tray's cost in private memory (2026-11-10)

The owner's first look at 0.2.1 found the hub's categories pointless and its watch notice confusing.
With four or five apps, the manifests' groups (Audio, Commentary, Hotkeys, Watchers) put one app under each heading: the headings said nothing a user acts on.
The hub is now one list in the host's order: what runs or is starting first, then what wants to run and does not (it needs a look), then what is stopped, then what is not installed or cannot run here, by name within each (`supervisor::list_order`).
The order is the host's, so the tray menu follows it too; the manifest's `group` stays in the contract and the snapshot and is no longer shown.
A pending action counts as running, so an app moves up as soon as it starts and down only once it has stopped.

While the PowerShell N8 Tray runs (D13), the window says so once: "Your PowerShell N8 Tray is managing these apps. This app is watching only; nothing here starts or stops apps.", with "Learn more" opening the why in place.
That state is how the two trays share the PC during the switch-over, not a fault: it no longer raises the level to "Needs attention" or replaces the status reason, so the pill and the tray icon keep showing the apps' and memory's health, and the tooltip carries a "Watching only" line.
While watching, rows show no Start or Stop button, since none would do anything.
A low-memory reason reads "Low memory"; the meter under it shows the figures.

Note (2026-11-26): D34 removes the PC's memory from the level, the pill and the tray icon, and the "Low memory" reason with it; they show the apps' health only.

The hub showed the tray's window as its working set (about 290 MB on the owner's PC), most of which is WebView2 runtime code shared with Edge (D19, D24).
The N8 Tray row and the status card's "N8 ... in all" now count the tray and its window by private memory, what they cost the system (about 115 MB with the window open); the status details still show both figures.

## D30. The icons are the owner's n8builds artwork (2026-11-25)

N8 Tray's icons are the owner's own n8builds artwork, not drawings made for the tray: a redraw tuned for 16 px was tried and dropped, because his originals shrunk to 16, 24 and 32 px stay legible and he wanted his artwork.
`tools/icons/make-icons.py` makes every file from three sources without redrawing anything; each is only cut out and resized:

- The app icon (every `icon.ico` entry from 16 to 256 px, `icon.png`, `32x32.png`, `128x128.png`, `128x128@2x.png`, and N8 Tray's own icon in the hub) is the Flow monogram on its bordered dark tile (`n8-flow-app.png`, the owner's pick), with the black outside its rounded corners made transparent along the border's own edge.
  At 16 and 20 px the blue border softens into a faint ring; the n8 still reads.
- The tray on a dark taskbar is the glowing logo (`n8builds-logo.png`), cropped to the mark with its black background keyed out (the art is light over black, so alpha is the brightest channel); its glow fades out close to the letters so it does not smear at 16 px.
- The tray on a light taskbar is the cyan and navy monogram (`n8-monogram-light.png`), already transparent, so only cropped; the white n of the logo would vanish there.

The tray follows the taskbar's colour live (owner, 2026-11-25).
On Windows the taskbar's colour is `SystemUsesLightTheme` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`, not `AppsUseLightTheme`, which a window's theme events follow and which can differ.
N8 Tray reads it at start and watches the key with `RegNotifyChangeKeyValue` on a thread of its own (`n8tray_platform::theme`), which sleeps until the registry signals a change: no polling, and no window needed.
The `WM_SETTINGCHANGE` "ImmersiveColorSet" broadcast was the other way; Tauri does not pass the tray's hidden window's messages on, so it would need a window of our own just to receive one message.
macOS and Linux keep the dark-taskbar art until they get a signal of their own.
The level dot over it (`badge.rs`) shows only on trouble (owner, 2026-11-25): healthy is the logo alone, because the dot covers most of the 8; amber, red and grey are as before, on either art.

## D31. The status card no longer shows the system's memory (2026-11-25)

The status card at the top of the window had a "Windows 8.9 / 31.1 GB free" row with a bar under its headline.
The owner asked for it gone, because Pulse, his system monitor, now shows the PC's memory.
The card keeps its headline, the "N8 ... MB in all" line (N8's own apps, not the system) and "Details".
Only the row and its meter were removed from the interface (`memoryMeter`, its markup and its styles).
The system memory reading itself stays, since it still feeds the status level and its "Low memory" reason, the low-memory notification, the tray tooltip, the status report and the free and total figures in "Details".
Per-app memory is untouched.

Note (2026-11-26): D34 removes everything this entry kept except the reading behind the pressure pushed to apps.

## D32. Pinned apps: hide what you do not use, never what runs (2026-11-25)

The owner approved a "pin" design so he can hide apps he does not use without deleting them.
Every hosted app is pinned or unpinned (`pinned` in the app's settings, `Command::SetPinned`); that is separate from whether it runs.
Pinned apps show in the main view; unpinned apps leave it and sit behind one line, such as "2 more", which opens the full list.
Nothing is deleted and the app's settings stay.

Hiding is about what is seen, never about what runs:

- An unpinned app that runs is on the line: "2 more · 1 running, 40 MB", or "2 more · 1 running" while its memory is not measured yet, and "2 more" while none runs.
  The memory is what each row shows (measured plus what the app reports inside WSL), written as the rest of N8 Tray writes memory (GB from 1000 MB).
- An unpinned app in trouble is back in the main view with its warning (`AppView.inMainView`), and is then not counted on the line.
  Trouble is read from the state, not the level: degraded, failed, a problem such as stopped while it should run, or an active resource alert.
  Starting and busy are normal work and pull nothing in, although their level is attention too.
  A turned-off app is never in trouble.
- An app pulled in by trouble stays in the main view for 30 s after the trouble clears, counted from the last snapshot that saw it, so a flapping app does not jump in and out.
  The host keeps this hold (`pins::TroubleHold` in the host's state) and drives it with the snapshot's clock, which tests pass in.
- The level, its reason, the tray icon's dot and N8's memory total count every app, pinned or not.
- `HostSnapshot.running` and `total` stay the hub's figures, every app pinned or not, as before: the shipped window and the copied status read them.
  The new window's headline uses `pinnedRunning` and `pinnedTotal`, pinned apps only; an app pulled in by trouble is shown but not counted there.
  `pinnedText` is that headline ready-made: "2 of 2 running", "Nothing pinned" when no app in the hub is pinned, or "No apps", never "0 of 0 running".
- The tooltip names the running apps the main view shows, pinned or pulled in by trouble, and counts the rest: "Running: N8 Hotkeys · 2 more running", or "2 more running" alone.
  That count is always `more.running`, and it stays whole when the names are cut.

`HostSnapshot.more` is `null` when every app is in the main view, never a line with a count of 0, so a view shows the line exactly when it has something to say.
Pinned applies only to apps shown in N8 Tray: "Show in N8 Tray" (`ownIcon`) and "Turned on" (`enabled`) keep their meaning.
An app with its own icon, or one hidden because it cannot run on this computer, is in no view of the hub and never on the line.
A turned-off app is still in the hub, as it is today, so unpinned it is counted on the line ("1 more") and listed behind it.
`SetPinned` for an id no manifest has saves an entry for it, as `SetOwnIcon` does, and shows nothing.

N8's memory total counts what N8 Tray measures, not memory an app reports inside WSL, while a row and the line add the reported part.
So the line's figure can be larger than its share of the total for an app that runs inside WSL; the total was left as it is, because the shipped window shows it.
Keeping an unused app stopped is the existing "Start with N8 Tray" switch (`autoStart`), not part of the pin.

A settings file without the key reads as pinned, so no migration was needed and the settings schema stays at version 1.
An older N8 Tray keeps the key through a load and a save (unknown keys are preserved), so going back a version loses no choice.
The core publishes the data now and the native window will show it; today's window does not read the new fields, and with every app pinned (the default) the snapshot reads as before apart from the new fields themselves.

## D33. The native shell: Slint next to the Tauri app until the switch (2026-11-25)

The owner decided N8 Tray becomes a native Slint app with no WebView, for memory, and that it must end up looking exactly like today's.
Only the shell and the window are replaced (`app/src-tauri` and `app/ui`); the UI-free crates stay as they are.
Lane N0 adds the skeleton as its own crate and binary, `native/` and `n8tray-native.exe`, so the Tauri app keeps shipping untouched until a later switch.

Its shape:

- `native/` is a Cargo workspace of its own with its own `Cargo.lock`, excluded from the root workspace, until the switch.
  In the shared workspace, Slint moved shared dependencies of the shipping app (icu, image, moxcms, zbus) and the root Lint and Test jobs checked a Tauri build with the native crate's features unified in, which is not the one that ships.
  Its path dependencies on `n8tray-core` and `n8tray-platform` stay members of the root workspace; only CI's Native job compiles Slint, and it runs the crate's format, lint and tests (`npm run check:native`).
  The exe's version resource takes the root workspace's version, which release-kit keeps.
  A release bumps N8 Tray's own crates, which `native/Cargo.lock` records, so the Native job and `check:native` first resolve the lock plainly (`cargo metadata`), which updates only those path entries and keeps every other dependency as committed.
  (Until 2026-11-26 they ran `cargo update -p` on the three crates, which also re-resolved those crates' dependencies and flipped the lock between two valid states on every run.)
- It runs only in a home of its own: without `N8TRAY_HOME`, or with it naming the installed N8 Tray's own folder, it exits with code 3 before it writes anything, its log included, and says why on stderr.
  In the real home it would become the host of the user's apps (status, history, lease) and the installed app would find the host lock taken at sign-in.
  `N8TRAY_RUN_DIR` and `N8TRAY_MODULES_DIR` are ignored: everything stays under the home.
- One `Shell` on the UI thread holds the host's handle, the latest snapshot, the tray icon and the window, if there is one.
  Host events, the taskbar's colour, tray and menu clicks and requests from a later copy are posted to the UI thread with `slint::invoke_from_event_loop` and handled one at a time.
- The tray is `tray-icon` 0.27 with `muda` menus, Pulse's version; with a lockfile of its own the GTK it needs on Linux no longer meets Tauri's.
  The menu is a line-for-line port of today's (same items, order, labels, states and in-place updates), the badge is today's drawing on plain RGBA, and the process lets its menus follow the system's dark mode exactly as Tauri's window library does (Windows 10 build 17763 or later, uxtheme from System32).
  "Start at sign-in" shows its state but is disabled: the setting it saves would make the installed app rewrite the real sign-in entry.
  If the tray icon cannot be created, N8 Tray exits, as the Tauri app does.
- The window's life (show, hide, the 400 ms guard after a blur, generations, waiting for the native window, drop or keep) is plain state in `life.rs`, tested without a toolkit; `window.rs` carries out its steps.
  A native window that never appears is dropped after 2 s.
- A panic on the UI thread unwinds out of the event loop to `main`, where the shell is no longer borrowed: the host is stopped and the tray icon removed as on a normal exit, then the process exits with code 101.
  The crate denies unsafe code, each of its own modules forbids it, and only Slint's generated module may allow it.
- The commands are plain Rust functions with today's validation: the window passes ids and keys only, never a path, a URL or a command line (D18).
- Slint is pinned to 1.19.1, Pulse's version, on the winit backend with the `accessibility` feature.
  Slint re-activates a window each time it shows it, so the window is shown once off screen and never activated (owned by a hidden window, so it never has a taskbar button or an Alt-Tab entry), then shown, hidden and placed only through Win32 (`n8tray_platform::window`).
  Its frame insets are 0 on the owner's PC (a frameless winit window keeps no invisible border); the placement still subtracts them, as for the Tauri window (D25).
- Single instance is a named mutex plus one named event per request (show, hide, exit) in `n8tray_platform::single`.
  The name is derived from the N8 Tray home, so the native shell never meets the installed app's lock, and two test homes never meet each other.
  The first copy opens the events before it takes the mutex, so a request from a copy started at the same moment is never lost; if the mutex cannot be created at all, the copy runs as the first one and logs it.
- Until the switch the native shell never writes the sign-in entry (the installed app owns it), shows no notifications (they are logged) and opens the release page for Install update instead of installing in place (D27).
  It runs on Windows only; on macOS and Linux it is compiled and tested in CI.
- Nothing users get contains the native shell: `native/scripts/check-release.mjs` checks that the installer bundles no native file (`bundle.externalBin`, `bundle.resources`), that the bundle's build step is unchanged, and that App release's commands (not its comments) run `tauri build` and nothing that builds the native shell.
  It is tested against bad samples in the same CI step.
- Left for the lanes that build the screens: the copied code (`logs.rs`, `recent.rs`, `open.rs`, the menu model, the badge drawing and the command checks) should move to a crate both shells share, or go with the Tauri app at the switch.
  Placement across monitors of different scale shows one frame at the wrong size before the second placement corrects it, and is not tested yet.

Measured on the owner's PC on 2026-11-25 with `native/scripts/measure.ps1` (no input; the foreground window never changed), each run in its own `N8TRAY_HOME` hosting the conformance kit's fake module, the placeholder window 380 by 104 px.
Figures are the tray process's private working set (Task Manager's Memory column) and, after the slash, its private bytes, in MB, the median of three readings.
The PC was quiet at the start (CPU 6%) and busy at the end (87%, other agents building); memory figures do not depend on that.

| Run | Closed, 36 s after start | Window open | Hidden 5 s | 16 s after hiding | After 4 more opens |
|---|---|---|---|---|---|
| Drop, `software` | 2.39 / 13.6 | 4.07 / 14.7 | 4.07 / 14.7 | 3.01 / 15.1 | 0.66 / 14.9 |
| Drop, `skia-software` | 2.35 / 13.3 | 4.03 / 14.4 | 4.09 / 14.4 | 2.94 / 14.7 | 0.61 / 14.7 |
| Keep, `software` | 2.50 / 13.7 | 4.05 / 14.3 | 4.05 / 14.3 | 3.21 / 15.7 | 0.89 / 15.5 |
| Keep, `skia-software` | 2.51 / 13.4 | 4.11 / 14.9 | 4.12 / 14.9 | 3.10 / 15.6 | 0.87 / 15.5 |
| Drop, a build with `software` only | 2.41 / 13.3 | 3.57 / 13.8 | 3.58 / 13.8 | 2.76 / 14.4 | 0.71 / 14.3 |

The installed Tauri N8 Tray 0.5.0 read 4.9 to 5.3 / 18.4 to 18.6 at the same time, closed, and its window costs about 66 MB more in its WebView processes.
So the native shell is about half today's cost closed, and its open window adds about 1.6 MB to the one process instead of starting WebView2.

What the figures decide:

- **The window is dropped 10 s after it hides (D17 stays).**
  Dropping returns about 0.6 to 0.8 MB of private bytes that a kept window holds, and five opens and closes in a row grew nothing.
  Creating the window again took 1 to 6 ms, and opening it measured the same either way (206 to 228 ms from starting a `--show` copy to the window on screen), so dropping costs users nothing.
  The real screens will hold more than the placeholder (text, icons, models), which widens the gap.
- **The renderer is `software`, the default when `N8TRAY_RENDERER` names none.**
  With the placeholder both renderers cost the same within a few hundred KB, but a binary that links Skia loads `OPENGL32.dll` and `dxcore.dll` from the start, even when it draws with `software`; the `software`-only build loads neither (37 modules closed instead of 43 to 44), is 19.3 MB instead of 24.7 MB, and its open window is 0.5 MB smaller.
  Scribe's Slint trial found Skia keeping 5.6 MB of caches after its window closed against 1.8 MB for `software`.
  The `skia-software` feature stays buildable only until the real hub screen has been compared with today's window pixel for pixel in both renderers; if `software` text matches as well, the switch builds without Skia.

The memory script keeps a window on screen about a second per open, shown without activation; it never took the focus in any run.

## D34. The PC's memory leaves N8 Tray (2026-11-26)

The owner decided that whole-PC memory belongs in a system monitor, not in N8 Tray: "Definitely removing the low memory entirely".
D31 had already taken the "Windows ... GB free" row off the status card; this removes the rest.

For users: the tray dot no longer turns amber or red when the PC is low on memory, the "Low memory notifications" setting is gone, and "Details" no longer shows the PC's free memory.
A system monitor is now the place to watch the whole PC's memory; N8 Tray keeps showing what its own apps use.

- Low free memory no longer raises N8 Tray's level: the tray dot turns amber or red only for the apps, and the "Low memory" reason is gone (this supersedes the shown level in D21 and the reason in D29).
- The low-memory notification and its "Low memory notifications" setting are gone.
- The tray tooltip and the tray menu's status line no longer carry "Windows 4.2/31.1 GB free"; they carry N8's own figure, the one the status card shows, in both shells.
  The menu line reads "N8 Tray: Healthy · 310 MB in all", so "N8" is not said twice; the tooltip's last line reads "N8 310 MB in all" and is the first line dropped when the tooltip runs out of room, before "Watching only".
- "Details" no longer has the free and total line, and Copy status no longer has the PC's figures.
- The snapshot's `memory` keeps only `n8Mb` and `text` ("N8 310 MB in all"); `freeGb`, `totalGb`, `level` and `system` are gone, and `settings.memoryNotifications` with them.
  The shape `memory.n8Mb` was kept rather than moved to the top level, so the native window's lanes and the fixtures change as little as possible.

What stays, because it is N8 Tray acting for its apps rather than showing the PC:

- The memory pressure pushed to apps with the `pressure` feature (spec 5.4), with D21's thresholds, 10 s in and 60 s out, and the "Pressure sent" line in "Details".
  It needs the system's free memory, so the platform sampler (`n8tray_platform::memory::system_memory`) stays, read only for this; the level only goes to apps, and the log records it with the free memory whenever it changes, since that reading is the pressure's input.
- `memory.attentionBelowGiB` and `memory.problemBelowGiB` in the settings, which set that pressure.
- Every app's memory, CPU and graphics figures, history, process tables, budgets and their alerts ("over its 1 GB warning"), and "N8 ... in all".

WSL Memory was planned as a built-in (D6): Rust code in the host, described by a manifest with status type `builtin`, showing WSL's free memory and a count of dev servers from devguard's status file.
The host never shipped it; only its example manifest, its icon and the interface's sample rows existed, so no user had the row.
The owner dropped it ("do we need the wsl memory in n8tray i think not right?"), so the example, the icon and the sample rows are gone, and the PowerShell tray's `wslMemory` section is no longer imported.
The `builtin` status type stays in contract 2: removing it would reject manifests valid today, which needs a new contract major, and it costs nothing while no built-in exists.

Settings files need no new schema version.
A file from 0.6.0 loads without an error or a warning: `memory.notify` is kept as it is under `memory`, and app settings or a `wslMemory` section for WSL Memory are kept as unknown entries, as before.
A file this version saves still reads in 0.6.0, which takes a missing `notify` as off.

## D35. Compact rows and a window height limit: two global settings (2026-11-26)

The owner approved two settings for the native window.
This entry adds them to the core only, as D32 did for pins, so a later lane can draw them.
Today's Tauri window does not read or change either.

- `compact` is a boolean, off by default.
  Normal shows two lines per app; compact shows one.
- `windowHeight` is one of three presets, `short` (400 px), `medium` (500 px) or `tall` (600 px), `short` by default.
  It is the height the window stops growing at before its app list scrolls.
- Both are global, not per app, and sit at the top level of the settings file as `compact` and `windowHeight`.
- A file without them reads as the defaults, so there is no new schema version.
  An older build keeps both keys in its unknown entries through a load and a save, so going back a version and forward again loses no choice.
- An unknown `windowHeight` in a hand-edited file (a wrong name, a number, a null) reads as `short`, and the file is not a settings error.
  This differs from the update channel, where an unknown value makes the whole file an error: a cosmetic limit should never switch off saving for every other setting.
  The next save writes `short`.
- `SettingsView` carries `compact`, `windowHeight` and `windowHeightPx`, the pixel value, so a shell does not repeat the mapping.
- The host takes `Command::SetCompact` and `Command::SetWindowHeight`, saved through the same path as the other global settings.
- The native shell's `set_setting` takes `compact`, and its `set_window_height` takes only the three preset names.
  The Tauri app's setting list has neither, as it has no `pinned`: the difference is deliberate, and a test on each side says so.
- The Tauri window receives the same snapshot, so its data gains the three new `settings` fields, which no view reads.
  Its copied status text (`report.rs`) does not mention them.

## D36. The native kit is a base six view lanes can build on alone (2026-11-26)

An independent review of the native window kit (PR 45) found that a view lane could not build its screen without editing shared files, and that the shell differed from today's window in ways a lane would copy.
This entry records how the kit answers it; `native/ui/README.md` is the rulebook the lanes follow.

- One way for a view to reach N8 Tray, `view::intent`: settings and host commands by key, Start at sign-in, an app's actions (armed through `view::confirm`), N8 Tray's own targets by name, an app's `open` and `menu` items, installing an update, a history read on a worker thread with the reply on the UI thread, recent warnings, the status text and the clipboard.
  Views still pass only ids and keys (D18); `view::intent::resolve` maps each request through the same validation as the tray menu and the Tauri commands, and refuses the rest.
  Until the native shell installs updates in place, "install" and "try again" open the release page, as the tray menu's Install update does (D27).
  The clipboard is Win32's (`n8tray_platform::clipboard`); elsewhere copying says it is not available yet.
  Copy status copies the same text as today: `native/src/report.rs` is a copy of the Tauri app's, kept equal by a test until the switch removes that one.
- A view's types live in its model file, `views/<name>-model.slint`, exported whole through a chain of `export *` from `main.slint` (Slint allows one per file).
  A struct a view declares is then a plain Rust type; one a model file did not export would reach Rust only as a deprecated alias, which `-D warnings` refuses.
  Model files hold no component, because exporting a component that is not a window is a build warning.
- The router follows today's window: every open by the tray icon shows the hub (`hub.rs` emits "hub" on every show); `show_view` opens another view with only the hub behind it, so the tray's status line opens the status details; a panel or one app's history whose app leaves the snapshot falls back to the hub, and the way back forgets gone apps; any view change and hiding the window disarm every armed action, as today's buttons keep their armed state in the view that shows them; a new view puts the keyboard focus at its top, where a screen reader hears its title (`Nav.title`).
- Compact and the window height (D35) are applied from the snapshot on every fill and every reveal, and the window is fitted after each fill.
- A focus request by app id (`RowFocus.to-id`) stays pending until a row takes it, and a row checks for one when it is created: after a reorder, the moved rows are new instances that exist only from the next layout, which a `changed` handler never sees.
- Each view module lists its golden pictures (`SHOTS`: fixture, app, density, height), rendered in the shell's own window, so a lane adds states without touching the gallery's lists or the shared fixtures.
- Release builds compile `ui/main.slint` alone; debug and test builds compile `ui/debug.slint`, which adds the widget gallery in the same compilation, so both share their Rust types.
  The Native CI job checks the Rust code Slint generated for each build: the test build's must have the gallery's component (so the check can fail) and the release build's must not.
  Measured on Windows (release, LTO, stripped): the exe is 27,452,416 bytes, 24,064 smaller than main's CI build (27,476,480).
  Link-time optimisation had already dropped most of the gallery from release builds, since nothing there creates it: neither exe holds the gallery's words, so looking in the exe could never fail, and the check reads the generated code instead.
- The scroll area brings a focused control into view; only `in-bar` keeps a bar's control from moving the list, since the position check cannot tell a bar's control from content once the list is scrolled.
  So every control of a view is in a bar or in its `ScrollArea`, and `view::gallery::tests::tab_through` checks a view for it.
- Table cells wrap as `td` does, with number columns keeping their width beside a long name, and key-value values wrap instead of eliding (a fixed `height` on any text in a row capped the row at one line).

The window's height changes while it shows (content that grows or shrinks) move its top and its height in one `SetWindowPos`, so a window by a bottom taskbar keeps its bottom on the taskbar, and a refresh that changes nothing no longer calls it at all.
Whether one frame at the old size shows in between depends on when Slint draws relative to that call; it can only be seen on a real desktop with a real taskbar, which this lane did not use (headless renders only).
Check it when the hub lane lands: open the window with a list that grows (start an app) and watch the bottom edge.
