# Compatibility database

How well each game works with Reborn. Reborn shows the best known tier for a
game on your platform, and the project uses the database to decide which
missing features matter most (plan section 8.5, ADR 0015).

Contributing a report needs no programming: open a
[Compatibility report](https://github.com/playreborn/reborn/issues/new?template=compatibility-report.yml)
issue and a maintainer turns it into an entry here.

**Reports must come from real tests.** Only report what you actually played
with Reborn on your own account, with the Reborn version you name. Do not copy
results from other sites (ProtonDB, forums, store pages) or from the official
client, and do not guess. An entry with made-up data is removed.

## Tiers

Every tier includes everything below it.

| Tier | Meaning |
| --- | --- |
| 0 | Does not start |
| 1 | Starts, but only with the official client as fallback |
| 2 | Starts without the official client, singleplayer playable |
| 3 | Additionally achievements and cloud |
| 4 | Additionally multiplayer |
| 5 | No known differences to the official client |

Games with kernel-level anti-cheat are marked as unsupported until proven otherwise.

## Files

One TOML file per app, named `<app_id>.toml`, where `<app_id>` is the number
from the game's store URL (`store.steampowered.com/app/<app_id>/`), written
without leading zeros. [`example.toml.sample`](example.toml.sample) shows the
format; it is an example, not a real report. Only files ending in `.toml` are
part of the database.

```toml
app_id = 12345            # must match the file name
name = "Some Game"        # for humans; not checked against Steam

[[report]]                # one entry per test, newest first
tier = 2                  # 0-5, see above
platform = "linux-proton" # "windows" | "linux-native" | "linux-proton"
reborn_version = "0.1.0"  # Reborn version or commit hash
proton_version = "Proton 9.0-4"  # optional, only for linux-proton
date = "2026-11-20"       # YYYY-MM-DD, quoted, the day of the test
notes = "Singleplayer works; cloud not tested."  # optional, free text
```

| Field | Required | Rules |
| --- | --- | --- |
| `app_id` | yes | Integer; equal to the number in the file name. |
| `name` | yes | Not empty. |
| `report` | yes | At least one `[[report]]` entry. |
| `report.tier` | yes | Integer 0 to 5. |
| `report.platform` | yes | `windows`, `linux-native` (native Linux build) or `linux-proton` (Windows build through Proton). SteamOS desktop mode counts as Linux; mention it in `notes`. |
| `report.reborn_version` | yes | Not empty. A version such as `0.1.0` or a commit hash. |
| `report.proton_version` | no | Only allowed with `linux-proton`. As shown by the Proton build, e.g. `Proton 9.0-4` or `GE-Proton9-20`. |
| `report.date` | yes | A quoted, valid date `YYYY-MM-DD`. |
| `report.notes` | no | Free text: what works, what does not, workarounds, launch options. |

Unknown keys are errors, so typos are caught. `reborn-core`'s loader
(`crates/core/src/compat.rs`) validates every file here in `cargo test`, so a
broken file fails CI.

For each platform the best known tier of an app is the highest tier among its
reports for that platform; with equal tiers, the newest report is shown.

## Adding a report (maintainers)

1. Check that the issue describes a real test with a named Reborn version.
2. Create or extend `<app_id>.toml` with the fields from the issue; put the
   new report first.
3. Run `cargo test -p reborn-core compat` and reference the issue in the commit.
