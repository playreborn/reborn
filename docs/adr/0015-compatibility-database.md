# 0015: Compatibility database

- **Status:** Proposed
- **Date:** 2026-11-20

## Context

Plan section 8.5 defines compatibility tiers 0-5 and says the data lives as
one TOML file per app in `compat/`, maintained through compatibility-report
issues, so that people without programming skills can contribute. M2 stage 4
(release 0.1) needs the database, a report template and a way for Reborn to
show a game's tier.

The M0 skeleton already had a `compat/_template.toml` (fields `level`,
`platform` = windows/linux/steamos, `runtime` = native/proton-version) and a
`compat-report.yml` issue form, but no loader and no validation.

## Problem

Which file schema, how strict the loader is, and how reports turn into a
single tier per game and platform.

## Alternatives considered

1. **One big file (TOML or JSON) for all apps:** easy to load, but every
   contribution edits the same file, which causes merge conflicts and long
   diffs.
2. **One TOML file per app (plan 8.5):** small, reviewable pull requests,
   and the file name doubles as the key. Needs a directory loader.
3. **SQLite or an external service:** not reviewable in pull requests and
   would need hosting; rejected.
4. **Lenient loader that skips invalid files:** robust at runtime, but a
   broken contribution would vanish silently.
5. **Strict loader that fails on any invalid file, checked in CI:** broken
   contributions are caught in review.

## Decision

One TOML file per app, `compat/<app_id>.toml`, with `app_id`, `name` and one
or more `[[report]]` entries (`tier`, `platform` = `windows` |
`linux-native` | `linux-proton`, `reborn_version`, optional `proton_version`,
`date` as `"YYYY-MM-DD"`, optional `notes`). The schema is documented in
`compat/README.md` and replaces the M0 template: "tier" matches the plan's
"Stufe", and the platform names say what was run (native build or Proton)
instead of a separate `runtime` field; SteamOS desktop mode counts as Linux.

`reborn-core::compat` loads one file or a directory strictly: unknown keys,
tiers outside 0-5, unknown platforms, invalid dates, `proton_version` on a
non-Proton report and an `app_id` that differs from the file name are
errors, and one invalid file fails the whole load. A unit test loads the
repository's `compat/` directory, so CI rejects broken files. The best known
tier per platform is the highest reported tier; among equal tiers the newest
report is shown.

The repository ships no real entries yet, only `compat/example.toml.sample`,
which the loader ignores and a test keeps valid. Entries must come from real
tests with Reborn; the issue form (`compatibility-report.yml`) asks for
exactly the schema's fields and a confirmation of that.

## Consequences

- Contributors fill in an issue form; maintainers copy the fields into a
  file. A bot could automate this later.
- `toml` (MIT OR Apache-2.0) becomes a dependency of `reborn-core`.
- Strictness means a typo blocks CI until fixed; error messages name the file,
  the report number and the field.
- "Highest tier" can hide a regression reported later. The full report list
  stays available, so a frontend can show the newest report as well.
- The database is read from a directory; how it is shipped with releases or
  updated at runtime (bundled, downloaded) is not decided here.
