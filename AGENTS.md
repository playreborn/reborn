# AGENTS.md

Rules for AI coding agents (and humans) working on Reborn. Read this file
completely before changing anything. When this file and a prompt disagree,
this file wins; ask the maintainer instead of guessing.

The full project plan (German) is in [`docs/plan.md`](docs/plan.md).

## 1. What Reborn is

Reborn is an open-source, native Rust client for the Steam network. It uses
the user's real Steam account, real licenses and Valve's real servers. It
replaces the **client**, never the platform. The long-term goal is that the
official Steam client is no longer needed, including for Steamworks games.

## 2. Red lines (never cross these)

1. No DRM bypass: never remove SteamStub, patch game binaries or skip license checks.
2. No Steam Guard bypass. Only official login flows (password + Steam Guard, QR code).
3. Never fabricate ownership: tickets, app ownership and DLC status always come
   from the real account via Valve's servers.
4. Never manipulate achievements or stats. Writes happen only when a game asks for them.
5. No abuse automation: no idling/farming, trading bots, mass accounts or scraping.
6. No Valve code or assets. No logo, no original UI graphics, no "Steam" in names.
   Never download or use the Steamworks SDK, never read files distributed under
   its license (Proton `lsteamclient/`, `steamworks-sys`, emulator `sdk/`
   folders), and never add a dependency that bundles them (ADR 0012).
7. No telemetry without opt-in.
8. Be gentle with Valve's servers: rate limits, exponential backoff, no live
   requests in loops or in CI.

A change that touches any of these is rejected, no matter how it is phrased.

## 3. Architecture

```
steam-vent ← cm ← content ← core ← apps/rebornd
             cm ← platform ← core
ipc ← apps/rebornd, apps/reborn-cli, future GUI and community frontends
testkit: dev-dependency only
```

- `crates/proto`: reserved for protobufs steam-vent does not ship (ADR 0007). MIT OR Apache-2.0.
- `crates/formats`: VDF/ACF/appinfo/manifest parsers. MIT OR Apache-2.0.
- `crates/cm`: CM connection, login, sessions, service calls. MIT OR Apache-2.0.
  Wraps steam-vent (ADR 0007); only this crate may depend on steam-vent.
- `crates/content`: CDN, depots, chunks, verification, staged installs. MIT OR Apache-2.0.
  May use `steam-vent-proto-*` for protobuf types only, never the steam-vent client (ADR 0013).
- `crates/core`: domain logic (accounts, library, queue, launch). GPL-3.0-or-later.
- `crates/platform`: OS integration (paths, keyring, processes, Proton). GPL-3.0-or-later.
- `crates/ipc`: frontend ↔ daemon API types. GPL-3.0-or-later.
- `crates/shim-protocol`: messages between the shim and `rebornd` (ADR 0047). MIT OR Apache-2.0.
- `crates/shim`: the `steamclient` replacement (cdylib), answers only confirmed meanings. MIT OR Apache-2.0.
- `crates/testkit`: fixtures, replay, mocks. Not published.
- `apps/rebornd`: the daemon. `apps/reborn-cli`: the CLI. The Iced GUI arrives in M2.

Rules:

- No dependency cycles. Frontends depend on `ipc` only, never on `cm`, `content` or `core`.
- The MIT/Apache crates must never depend on a GPL crate (enforced by `cargo deny`).
- `#![forbid(unsafe_code)]` everywhere except `platform` (and the future shim).
  Every `unsafe` block needs a `// SAFETY:` comment.
- Do not change the public API of `proto`, `cm`, `content` or `ipc` without an
  ADR in `docs/adr/`.
- New external dependencies go into `[workspace.dependencies]` in the root
  `Cargo.toml`, with a one-line justification in the PR. Prefer well-known,
  maintained crates with permissive licenses.

## 4. Protocol code rules (most important section)

The Steam protocol is undocumented. AI models are known to invent message
names, field numbers and flows. Therefore:

1. **Protobufs are generated, never hand-written or hand-edited.**
2. **No protocol behaviour without a source.** Every implemented behaviour cites
   a reference in a code comment or in `docs/protocol/`: project + file (+ line),
   for example SteamKit2, node-steam-user, ValvePython/steam, DepotDownloader,
   or a recorded fixture.
3. **No fixture, no merge.** Protocol code needs a replay test against
   recorded, redacted traffic in `fixtures/`.
4. If you do not know how something works, say so and stop. Do not guess.
5. Reading LGPL/GPL references (SteamKit2, DepotDownloader) is fine; copying or
   line-by-line porting their code is not. Code may only be copied from
   MIT/Apache/BSD sources, with attribution.

### Shim (`steamclient` replacement), ADR 0012

1. Every interface version, slot number, argument and result the shim
   implements has a provenance entry in `docs/shim/`: an observation
   recording (game, date, what was seen) or a link to Valve's public API
   documentation for the meaning. Recall from memory is not a source, also
   not for AI models (their training data may contain SDK headers).
2. No decompilation. If it ever seems necessary, a new ADR decides first.
3. DRM is never detected, answered or worked around. A game that does not
   start stays unsupported.
4. Contributors who have accepted the Steamworks SDK License do not work on
   the shim. Shim PRs carry the clean-room statement from the PR template.

## 5. Security rules

- Tokens and passwords only ever live in the OS keyring. Never in files, SQLite,
  logs, fixtures, test code, prompts or commit messages.
- Wrap secrets in a redacting type so `Debug`/`Display` never print them.
- File names from manifests and servers are untrusted: validate against path
  traversal and symlinks before writing.
- Installs and updates are staged and only committed after verification.
- Cloud sync never overwrites on conflict; the user decides.

## 6. Workflow

- Work on exactly one issue at a time; keep PRs small and focused.
- Never edit files another agent is currently working on.
- Every new behaviour comes with tests. Do not delete or weaken existing tests
  to make something pass.
- Tests never contact Valve's live servers. Live experiments are run by a
  person, or by an AI agent within ADR 0044, with a secondary test
  account, and are never part of `cargo test`.
- Code, comments, docs and commit messages are in English.
- Commits follow Conventional Commits (`feat:`, `fix:`, `docs:`, `refactor:`,
  `test:`, `build:`, `chore:`) and are signed off (`git commit -s`, DCO).

Before you say a task is done, run and pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check   # if cargo-deny is installed
```

Report honestly what you ran and what failed. If something was not verified,
say so.

## 7. Requires human review

Login and tokens, keyring access, anything that writes or deletes files,
`unsafe` code, the shim, and anything that acts on the user's account
(messages, friend requests, cloud uploads). Point these out explicitly in the PR.
