# Contributing to Reborn

Thanks for helping! This document explains how to contribute. The rules in
[AGENTS.md](AGENTS.md) apply to everyone, human or AI.

## Ways to help

| Area | Experience needed |
| --- | --- |
| Compatibility reports (which game works how well) | none |
| Translations | none |
| Documentation and guides | little |
| GUI components | Rust basics |
| Parser edge cases (VDF, ACF) | Rust basics |
| Platform code (paths, processes, Proton) | intermediate |
| Protocol and content engine | advanced |
| `steamclient` replacement (shim) | expert |

Look for issues labelled `good first issue` or `help wanted`. For larger ideas,
open an RFC issue first so we can agree on the approach before you write code.

## Development setup

1. Install Rust via [rustup](https://rustup.rs). The toolchain is pinned in
   `rust-toolchain.toml` and installs itself on first build.
2. Windows: install the Visual Studio C++ Build Tools.
3. Optional: `cargo install cargo-deny` for license and advisory checks.

Before opening a PR, all of these must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

## Test accounts

Use a **secondary Steam account** for any experiment against Valve's servers.
Never put real tokens, passwords, Steam IDs or e-mail addresses into issues,
PRs, fixtures, logs or AI prompts. Automated tests never talk to live servers.

## Protocol code

The Steam protocol is undocumented. Protocol code is only merged if it

- cites its source (reference implementation file/line or a fixture),
- comes with a replay test against recorded, redacted traffic, and
- uses generated protobufs only.

See [AGENTS.md § 4](AGENTS.md#4-protocol-code-rules-most-important-section).

## The `steamclient` replacement (shim): clean room

The shim must not be built from Valve's Steamworks SDK (ADR 0012,
[legal assessment](docs/legal/shim-assessment.md)). If you work on the shim:

- You have never downloaded or used the Steamworks SDK, and you do not
  read files distributed under its license (Proton `lsteamclient/`,
  `steamworks-sys`, the `sdk/` folders of emulators).
- Every interface detail you add has a provenance entry in `docs/shim/`:
  an observation recording of a game you own, or Valve's public API
  documentation for what a function means. Never copy its text.
- Your PR includes the clean-room statement from the PR template.

Have you used the SDK before? You are still welcome everywhere else in
Reborn (daemon, GUI, protocol, content engine), just not in the shim.

## AI-assisted contributions

Using AI tools is welcome. You are responsible for every line you submit:
you have read it, run it and can explain it. Unreviewed bulk PRs will be closed.

## Commits and sign-off (DCO)

- Use [Conventional Commits](https://www.conventionalcommits.org):
  `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `build:`, `chore:`.
- Sign off every commit with `git commit -s`. This certifies the
  [Developer Certificate of Origin](https://developercertificate.org): you
  wrote the change or have the right to submit it under the project's licenses.
- We use short-lived feature branches off `main` and squash-merge PRs.

## Reviews

Changes to login/tokens, keyring access, file writes or deletes, `unsafe`
code, the shim, or anything acting on a user's account need a review by a
maintainer. Say so in the PR description.

## Code of conduct

Everyone participating in this project follows our
[Code of Conduct](CODE_OF_CONDUCT.md).
