# The N8 Tray contract

- [`contract-v2.md`](contract-v2.md): the specification, version 2.
- [`schema/`](schema): the JSON Schemas (draft 2020-12), normative for the shape of every document:
  - `manifest.schema.json`: the manifest an app installs.
  - `status.schema.json`: the status document an app serves or writes.
  - `action-result.schema.json`: the answer to an action or a pressure notice.
  - `lease.schema.json`: the host lease.
  - `modular-v1.schema.json`: contract 1, today's `modular.json`, accepted during the migration.
- [`examples/`](examples): manifests for VoiceDock, N8 Commentary, N8 Hotkeys, Lead Scraper and the WSL Memory built-in.
  They describe each app as it will be once it adopts the contract, so some switches they name (such as `-Stop`) arrive with that adoption.
- [`fixtures/`](fixtures): documents every implementation is tested against.

## Fixtures

`fixtures/<kind>/valid/*.json` must validate.
`fixtures/<kind>/invalid/<code>.<name>.json` must fail with `<code>`, where `<code>` is `schema` or a rule code from section 11 of the specification.
`<kind>` is `manifest`, `status`, `lease`, `action-result` or `modular-v1`.

The Rust tests in `crates/n8tray-contract/tests/fixtures.rs` run every fixture and example on every pull request, and fail when section 11 documents a rule no fixture covers.
Another implementation (an app's own reader, for example) can reuse the fixtures the same way.
