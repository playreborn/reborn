# Shim provenance

Every interface detail the `steamclient` replacement (the shim) uses has
an entry here with its source (ADR 0012, ADR 0038, AGENTS.md § 4).
Allowed sources:

- an observation recording: a game the recorder owns, run with the
  official client, observed while running with a generic tool or with
  Reborn's observation proxy;
- Valve's public documentation on its website, for what something means
  (linked, not copied).

Not allowed: the Steamworks SDK or any file distributed under its
license, decompiled or disassembled code, and memory (also not an AI
model's).

## Entry format

One file per topic (e.g. `loading.md`, `entry-points.md`). Each fact:

```markdown
### <short name of the fact>

- **Fact:** what was seen, as plain as possible.
- **Recorded:** <date>, <game (app ID)>, <operating system>, <tool and
  version>, <stage of ADR 0038>.
- **Recorder:** confirms they never accepted the Steamworks SDK License.
- **Evidence:** the relevant log lines, redacted (no account names,
  SteamIDs, paths with user names, tickets).
- **Meaning:** link to Valve's public documentation, if any.
```

## Topics

| File | What | Stage |
| --- | --- | --- |
| `loading.md` | where games load `steamclient` from | 0, Windows and Proton recorded; native Linux not yet |
| `entry-points.md` | which entry points games ask for | 1, 2 and 4b (callbacks), Windows recorded; Linux not yet |
| `objects.md` | what `CreateInterface` returns and how it is used | 3a, 3b and 3c recorded (Windows, three games) |
| `meanings.md` | what recorded calls mean: candidates and experiments (ADR 0042) | 4a, first three confirmed (Windows) |
| `tools/` | scripts for the public tools used in stages 0 and 1, and how to use `reborn-observe` (stage 2) | |
