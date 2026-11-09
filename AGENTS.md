# Agent instructions

N8 Tray is a public repo: everything here is readable by anyone, so never commit secrets, personal paths beyond the documented examples, or details of private apps beyond what their example manifests show.

## What lives here

- `spec/`: the contract (manifest, status protocol, resource reporting, host lease), its JSON Schemas, example manifests and fixtures.
  The JSON Schemas in `spec/schema/` are normative; the Rust types in `crates/n8tray-contract` must accept exactly what they accept, and the fixture tests enforce it.
- `crates/`: the Rust workspace (the contract crate and, later, the host).
- `docs/DECISIONS.md`: why things are the way they are. Add an entry when you make a decision a later reader would question.

## Rules

- Change the contract only through a pull request that updates the spec, the schema, the fixtures and the Rust types together.
  A change that could reject a manifest or status document valid today is a new contract major (section "Versioning" in the spec).
- Never use the em dash; write a plain "-". `npm run lint` checks it.
- Long Markdown: one sentence per line.
- Checks before a pull request: `npm run lint` and `npm test` (they run `cargo fmt --check`, `cargo clippy -D warnings`, the text check and every test).
- CI runs on GitHub-hosted runners (Linux, Windows, macOS); never on a self-hosted machine.
- Squash-merge pull requests once CI is green; the pull request title becomes the commit message, so it must be a Conventional Commit.

<!-- release-kit -->
## Commits and releases

- Write every commit message as a Conventional Commit: `type(scope): subject`, with type one of feat, fix, perf, security, refactor, docs, test, build, ci, chore, style, revert. A hook and a CI check reject anything else; see RELEASING.md.
- Use `feat` and `fix` only for changes a user would notice: they become the release notes and the public build log.
- Write those subjects as patch notes, for users: what they can now do or what works better, not the file, library or test you touched. When the subject must stay technical, add a `Release-note: <plain sentence>` line to the body, or `Release-note: skip` for a change no user would notice.
- Don't bump versions, edit CHANGELOG.md or create tags by hand: `npm run release` does all three.
- Land changes through pull requests: a pre-push hook blocks pushing straight to main or a release branch, force pushes and moving release tags. Don't get around it with `--no-verify` or `RELEASE_KIT_ALLOW_DIRECT_PUSH=1` (that is for emergencies the owner asks for, and a Direct push alarm reports it either way).
<!-- /release-kit -->
