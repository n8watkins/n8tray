# N8 Tray

One tray for Windows, macOS and Linux that hosts independent local apps through an open contract.

An app that implements the contract installs a small manifest, reports its status and what it costs in memory and CPU, and runs on its own with its own tray icon.
When N8 Tray runs, it picks the app up, shows it in one hub with every other app, and the app's own icon steps aside.
Nothing is installed without the user asking.

## Status

This repository is new and is being built in phases.

| Phase | What | State |
|---|---|---|
| 1 | The contract: specification, JSON Schema, fixtures and a conformance kit | Done (v0.1.0) |
| 2 | The host core and tray on Windows: discovery, supervision in Job Objects, status, lease, menu, window | Done |
| 3 | Measurement: CPU, memory, GPU, history, alerts, memory pressure, demand rules | Done |
| 4 | The hub: every app with its own icon, panels, status details, settings, resource history; placed at the icon on every taskbar edge; keyboard, screen readers, High Contrast | Done |
| 5 | Checks and opt-in setup, self-update | Next |
| 11, 12 | macOS and Linux | Later |

The PowerShell N8 Tray that runs on the owner's PC today lives in a private repo until the Tauri app replaces it.

## Layout

```text
spec/
  contract-v2.md     the contract: manifest, status protocol, vocabulary, resources, ownership, security, lease
  schema/            its JSON Schemas (normative for the shape of every document)
  examples/          manifests for VoiceDock, Commentary, Hotkeys, Lead Scraper and WSL Memory
  fixtures/          valid and invalid documents every implementation is tested against
crates/
  n8tray-contract/       the contract's Rust types and validation
  n8tray-conformance/    the conformance kit: the n8tray-contract CLI (check, probe, the test tray)
                         and n8tray-fake-module
  n8tray-platform/       the OS layer: process snapshots, Job Objects, memory, GPU counters,
                         start at sign-in
  n8tray-core/           the host without a window: discovery, intent, supervision, status,
                         ownership, the lease, settings; tested against the fake module
app/
  src-tauri/             the tray app (Tauri 2): icon with a level badge, native menu, window,
                         single instance, start at sign-in
  ui/                    the window (Preact and TypeScript): a view of the host's snapshot;
                         preview/ runs it against recorded snapshots for screenshots
docs/
  DECISIONS.md       why things are the way they are
```

Start with [`spec/contract-v2.md`](spec/contract-v2.md).
To make an app follow the contract, use the [conformance kit](crates/n8tray-conformance/README.md).

## Development

Needs Rust (the version in `rust-toolchain.toml`, installed by rustup on first use) and Node 22 or later.

```bash
npm install        # release-kit and the git hooks
npm run lint       # cargo fmt --check, clippy with warnings as errors, the text check
npm test           # every test in the workspace
```

The tray app embeds its window, so build the window before Cargo builds the app:

```bash
npm ci --prefix app/ui && npm run build --prefix app/ui
cargo run -p n8tray            # the tray; N8TRAY_HOME=/tmp/n8 keeps it in its own folders
```

`n8tray --exit` ends a running N8 Tray (apps keep running), `n8tray --hide` closes its window, and starting it again (or with `--show`) opens the window.

The window's own checks:

```bash
cd app/ui
npm test           # view models and keyboard (Vitest)
npm run screens    # every view, light, dark and High Contrast, against recorded snapshots;
                   # pictures in app/ui/screens/, plus the keyboard path and an accessibility scan
npm run preview    # the same window in a browser at http://localhost:1430/?fixture=busy&view=hub
```

`node app/ui/tests/e2e/webdriver.mjs` drives the real window through tauri-driver, and `node scripts/tray-budget.mjs --app <n8tray>` measures N8 Tray's memory and CPU against its budgets (D24).

Commits follow Conventional Commits and land through pull requests; see [RELEASING.md](RELEASING.md) and [AGENTS.md](AGENTS.md).

## License

[MIT](LICENSE)
