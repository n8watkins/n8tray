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
