# N8 Tray

One tray for Windows, macOS and Linux that hosts independent local apps through an open contract.

An app that implements the contract installs a small manifest, reports its status and what it costs in memory and CPU, and runs on its own with its own tray icon.
When N8 Tray runs, it picks the app up, shows it in one hub with every other app, and the app's own icon steps aside.
Nothing is installed without the user asking.

## Status

This repository is new and is being built in phases.

| Phase | What | State |
|---|---|---|
| 1 | The contract: specification, JSON Schema, fixtures and a conformance kit | In progress |
| 2 to 5 | The Tauri app on Windows: host core, measurement, hub, checks and setup | Next |
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
  n8tray-contract/   the contract's Rust types and validation
docs/
  DECISIONS.md       why things are the way they are
```

Start with [`spec/contract-v2.md`](spec/contract-v2.md).

## Development

Needs Rust (the version in `rust-toolchain.toml`, installed by rustup on first use) and Node 22 or later.

```bash
npm install        # release-kit and the git hooks
npm run lint       # cargo fmt --check, clippy with warnings as errors, the text check
npm test           # every test in the workspace
```

Commits follow Conventional Commits and land through pull requests; see [RELEASING.md](RELEASING.md) and [AGENTS.md](AGENTS.md).

## License

[MIT](LICENSE)
