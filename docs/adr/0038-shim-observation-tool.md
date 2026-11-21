# 0038: The shim observation tool

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

ADR 0012 decides that the `steamclient` replacement (the shim) is built
only from observation and Valve's public documentation, and that a
logging proxy, run by the user with their own game and the official
client, records what games ask for. It needs `unsafe`, human review and
this ADR. The legal assessment (`docs/legal/shim-assessment.md`) bases
this on § 69d(3) UrhG: the rightful user may observe, study and test a
program while running it.

What is known so far (ADR 0012, observed):

- Linux: `steam_api` tries a local `steamclient.so`, then
  `~/.steam/sdk64/steamclient.so`.
- Windows: Valve's BSD-licensed Proton helper writes
  `HKCU\Software\Valve\Steam\ActiveProcess\pid`. Where `steam_api` reads
  the path of `steamclient64.dll` is **not confirmed**.

Nothing is known yet about the entry points of `steamclient` from an
allowed source. Memory is not a source (AGENTS.md § 4).

## Problem

How Reborn learns what games ask of `steamclient` without the SDK,
without decompiling, without modifying Valve's or the game's files, and
without recording secrets.

## Alternatives considered

1. **Write the full proxy now:** it would need to know entry points,
   calling conventions and object layouts in advance, which is exactly
   what is not known from an allowed source.
2. **Grow it in stages, each one only using what the previous stage
   recorded:** slower, but every step has a source.
3. **Read the export table of the official `steamclient` statically:**
   reading the names in a binary's export directory is not translating
   code, but it is not observation while running either. Rejected by the
   maintainer (see below).

## Decision

Option 2, in these stages. Each stage records its results in
`docs/shim/` with game, date and what was seen (format:
`docs/shim/README.md`), and each later stage only uses recorded facts.

0. **Where is `steamclient` loaded from (no Reborn code).** The user runs
   a game they own with the official client while a generic, public
   monitoring tool (on Windows: Process Monitor from Microsoft
   Sysinternals; on Linux: `strace -f -e trace=file`) records which files
   and registry keys the game opens. Recorded: the path of
   `steamclient(64)`, the registry values read, in which order.
1. **Which entry points are asked for.** Recorded while running, e.g.
   which names the game's `steam_api` asks the operating system for in
   `steamclient` (`GetProcAddress` on Windows, `dlsym` on Linux), with a
   generic API monitoring tool. Still no Reborn code.
2. **The logging proxy.** A separate crate (`crates/shim-observe`, a
   `cdylib`, `unsafe` allowed with `// SAFETY:` comments) that is loaded
   instead of `steamclient`, loads the real one, and forwards every entry
   point recorded in stage 1 unchanged, logging each call. It is placed
   only where stage 0 shows `steamclient` is looked for first, never by
   modifying Valve's or the game's files; if that is not possible, a new
   ADR decides. What it logs:
   - entry point, thread, order, return value;
   - arguments as raw values; a value is logged as text only if it
     points to readable memory holding printable ASCII (generic, no
     knowledge of types);
   - buffers by length and SHA-256 only, never their content, so tickets
     and other secrets cannot end up in a log.
   Logs stay on the user's computer; the user checks and redacts them
   before anything goes into `docs/shim/`.
3. **Objects and slots.** Whether and how calls on returned objects are
   observed is decided by a new ADR once stages 0 to 2 have results.

Always:

- Only games the user owns, with the official client running and signed
  in; developers use secondary accounts (ADR 0012).
- DRM is never detected, answered or worked around (red line 1). A game
  that does not start with the proxy is not investigated further.
- Nobody who has accepted the Steamworks SDK License works on this.
- Nothing is decompiled or disassembled.

## Decided by the maintainer (2026-11-21)

- Entry point names come only from observation while a game runs
  (stage 1). The export table of the official `steamclient` is not read
  statically (option 3 rejected).
- The first test subject is Cave Crawlers (app 2143030), owned by the
  first test account. It keeps cloud files through the Steam interface
  (`docs/protocol/cloud.md`).

## Consequences

- Stages 0 and 1 need no code and no human review of `unsafe`; they need
  the user's time and public tools.
- Stage 2 is the first `unsafe` code outside `reborn-platform` and needs
  human review (AGENTS.md section 7).
- Progress is slow by design; every interface detail in the shim can be
  traced to a recording.
