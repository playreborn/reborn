## What and why

<!-- What does this PR change and why? Link the issue: Closes #... -->

## Affected crates

<!-- e.g. reborn-cm, reborn-cli -->

## Checklist

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass locally
- [ ] New behaviour has tests
- [ ] Protocol code cites its source and has a replay test with a redacted fixture (or this PR has no protocol code)
- [ ] No tokens, passwords, e-mail addresses or unredacted Steam IDs anywhere in this PR
- [ ] Commits are signed off (`git commit -s`)
- [ ] If AI tools were used: I have read, run and understood every line

## Shim (`steamclient` replacement) only

<!-- Delete this section if the PR does not touch the shim. -->

- [ ] I have never downloaded or used the Steamworks SDK, and I did not use files distributed under its license (ADR 0012)
- [ ] Every interface detail in this PR has a provenance entry in `docs/shim/` (observation recording or public documentation, not memory)

## Needs maintainer review?

<!-- Tick if this touches login/tokens, keyring, file writes/deletes, unsafe code, or acts on a user's account. -->

- [ ] Yes, this touches a sensitive area (see AGENTS.md § 7)
