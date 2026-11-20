# Proton on Linux

How Reborn finds installed Proton builds and builds the command that runs a
Windows game through one. Code: `crates/platform/src/proton.rs`. Nothing in
that module starts a process or writes a file; spawning is the launcher's job.

## Sources

| Reference | Version | License | Use |
| --- | --- | --- | --- |
| [ValveSoftware/Proton](https://github.com/ValveSoftware/Proton) | branch `proton_10.0`, commit `e91ca2be` | BSD-3-Clause (`LICENSE.proton`) | read only |
| [Open-Wine-Components/umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) | commit `e2b203a1` | GPL-3.0 | read only |

Files read: Proton's `proton` script, `README.md`,
`compatibilitytool.vdf.template` and `toolmanifest_runtime.vdf`;
umu-launcher's `umu/umu_run.py`, `umu/umu_consts.py` and
`umu/umu_runtime.py`. Line numbers below refer to these versions.

## Discovery

Search roots are parameters (`SearchRoots`), so tests use temporary
directories. `SearchRoots::linux_defaults(home, xdg_data_home)` gives:

| Root | Kind | Source |
| --- | --- | --- |
| `~/.steam/root` | Steam installation | Proton `README.md` line 141 |
| `~/.steam/steam` | Steam installation | Proton `README.md` line 293 |
| `~/.steam/root/compatibilitytools.d` | custom tools | Proton `README.md` lines 141, 198 |
| `$XDG_DATA_HOME/Steam/compatibilitytools.d` (default `~/.local/share`) | custom tools | `umu_consts.py` lines 82-86, 104 |

`SearchRoots::from_environment()` fills these from `HOME` and
`XDG_DATA_HOME` and adds Reborn's own library, because Reborn installs
Proton like any other app (plan section 9.2).

Rules:

1. **Libraries.** Every Steam installation is a library itself; more come
   from its `steamapps/libraryfolders.vdf`, read with `reborn-formats`.
   Duplicates (for example `~/.steam/root` and `~/.steam/steam` pointing to
   the same folder) are removed by canonical path.
2. **Valve builds.** In each library, every directory
   `steamapps/common/Proton*` that contains a `proton` file is a Proton build
   (`README.md` line 293 for the location, lines 208-216 for the layout). Its
   name is the directory name.
3. **Custom builds.** Every subdirectory of a `compatibilitytools.d`
   directory that contains a `proton` file is a Proton build. If it has a
   `compatibilitytool.vdf`, the `display_name` of the first tool under
   `compatibilitytools` / `compat_tools` is its name, as umu-launcher does
   (`umu_runtime.py` lines 556-563); otherwise the directory name
   (`umu_runtime.py` line 565). Tools whose `from_oslist` does not contain
   `windows` are skipped (layout from `compatibilitytool.vdf.template`).
4. **Version.** The first line of the build's `version` file, as opaque text
   (Proton only logs it: `proton` lines 525 and 1847).
5. **Required runtime.** `require_tool_appid` in `toolmanifest.vdf`
   (`toolmanifest_runtime.vdf`; read the same way by `umu_runtime.py` lines
   553-554). Proton 10.0 declares 1628350, the Steam Linux Runtime 3.0
   "sniper" (`umu_runtime.py` line 524).

Missing directories are skipped silently. Unreadable directories and broken
`libraryfolders.vdf`, `compatibilitytool.vdf` or `toolmanifest.vdf` files are
returned as problems next to the results, so one broken tool does not hide
the others.

## Command

`proton_command` returns program, arguments, environment and working
directory:

```text
<proton dir>/proton <verb> <executable> <game arguments...>
```

| Item | Value | Source |
| --- | --- | --- |
| verb | `run` (default) or `waitforexitandrun` | `proton` lines 2009-2020; umu's default is `waitforexitandrun` (`umu_run.py` lines 233-236) |
| executable | absolute Linux path | `umu_run.py` line 254 ("otherwise Proton will fail") |
| `STEAM_COMPAT_DATA_PATH` | per-app directory chosen by the caller; the Wine prefix is its `pfx/` | `proton` lines 1987-1989 (required), 585-588 (`pfx/`) |
| `STEAM_COMPAT_CLIENT_INSTALL_PATH` | official client directory, or empty | `proton` line 949 (read unconditionally), lines 1036-1047 (files copied only if present); umu sets it empty (`umu_run.py` lines 843, 1039-1041) |
| `SteamAppId`, `SteamGameId` | the app ID | `proton` lines 871, 1195 (per-game workarounds); `umu_run.py` lines 281-282 |
| working directory | the game's working directory | Proton does not change it; the program path must therefore be absolute, since Proton locates its files from its script path (`proton` line 1991) |

All paths must be absolute; relative paths are rejected.

## Not covered

Left out because the sources above do not settle them:

- **Steam Linux Runtime.** Builds that declare `require_tool_appid` are meant
  to run inside that runtime's container (pressure-vessel). The command is
  not wrapped yet; whether a build works without it is unverified.
- **Steamworks.** `proton run` starts the game through Proton's `steam.exe`
  helper (`proton` lines 1965-1973). How that helper reaches a Steam client
  (plan section 8 assumes Proton's `lsteamclient` and the native
  `steamclient.so`, to be verified in M4) is not verified here, so Steamworks
  games may still need the official client until the shim exists.
- **Further variables.** `STEAM_COMPAT_APP_ID`, `STEAM_COMPAT_INSTALL_PATH`,
  `STEAM_COMPAT_LIBRARY_PATHS`, `STEAM_COMPAT_CONFIG`,
  `STEAM_COMPAT_SHADER_PATH` and others are not set: the values the official
  client uses could not be verified from these sources.
- **Manifest files directly in `compatibilitytools.d`** (instead of in a tool
  subdirectory, `compatibilitytool.vdf.template` lines 7-13) are not read.
