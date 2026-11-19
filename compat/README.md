# Compatibility database

How well each game works with Reborn. One TOML file per app, named
`<app_id>.toml`, following [`_template.toml`](_template.toml). Reports come
in through the *Compatibility report* issue template and are added here.

## Levels

| Level | Meaning |
| --- | --- |
| 0 | Does not start |
| 1 | Starts only with the official client as fallback |
| 2 | Starts without the official client, singleplayer playable |
| 3 | Additionally achievements and cloud work |
| 4 | Additionally multiplayer works |
| 5 | No known differences to the official client |

Games with kernel-level anti-cheat are marked as unsupported until proven otherwise.
