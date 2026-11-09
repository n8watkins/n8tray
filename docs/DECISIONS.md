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
