# Where games load `steamclient` from

Stage 0 of ADR 0038. Facts only from recordings; see `README.md` for the
format.

### Windows: the path comes from the registry

- **Fact:** Before loading, the game process reads, under
  `HKCU\Software\Valve\Steam\ActiveProcess`, the value `pid` (`REG_DWORD`),
  then `SteamClientDll` (`REG_SZ`, path of `steamclient.dll`), and a few
  seconds later `pid`, `SteamClientDll`, `SteamClientDll64` (`REG_SZ`,
  path of `steamclient64.dll`) and `pid` again. Right after that it loads
  `steamclient64.dll` from exactly the path in `SteamClientDll64`
  (`C:\Program Files (x86)\Steam\steamclient64.dll`, the official client's
  folder). No other place is tried: there is no access to a
  `steamclient` file in the game's folder or anywhere else.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  Process Monitor (Microsoft Sysinternals; version not noted), filter
  "Path contains `steamclient`" or "Path contains `Software\Valve`",
  stage 0. The official client was running and signed in with the first
  test account.
- **Recorder:** the maintainer; confirmed on 2026-11-21 that they never
  accepted the Steamworks SDK License (never downloaded the SDK, no
  Steamworks partner access).
- **Evidence** (`Cave Crawlers.exe` only; the process number is
  replaced by `<pid>`):

  ```text
  RegQueryValue  SUCCESS  HKCU\Software\Valve\Steam\ActiveProcess\pid               REG_DWORD  <pid>
  RegQueryValue  SUCCESS  HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll    REG_SZ     C:\Program Files (x86)\Steam\steamclient.dll
  RegQueryValue  SUCCESS  HKCU\Software\Valve\Steam\ActiveProcess\pid               REG_DWORD  <pid>
  RegQueryValue  SUCCESS  HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll    REG_SZ     C:\Program Files (x86)\Steam\steamclient.dll
  RegQueryValue  SUCCESS  HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll64  REG_SZ     C:\Program Files (x86)\Steam\steamclient64.dll
  RegQueryValue  SUCCESS  HKCU\Software\Valve\Steam\ActiveProcess\pid               REG_DWORD  <pid>
  CreateFile     SUCCESS  C:\Program Files (x86)\Steam\steamclient64.dll
  Load Image     SUCCESS  C:\Program Files (x86)\Steam\steamclient64.dll
  ```

- **Meaning:** what `pid` is checked for is not known (it held the
  official client's process number in this recording; ADR 0012 notes that
  Valve's BSD-licensed Proton helper `steam_helper/steam.cpp` writes it).
- **Also seen, not understood:** about seven seconds after loading, the
  game process tried to delete the values `InstallDescription` and
  `InstallSequence` under `HKLM\Software\Valve\Steam\Apps\0`; both did not
  exist (`NAME NOT FOUND`). Not needed for loading.

### Windows: who loads it, and what follows

- **Fact:** The game's `steam_api64.dll` is loaded from
  `<game>\Cave Crawlers_Data\Plugins\x86_64\steam_api64.dll`, and right
  after it `steamclient64.dll` from the official client's folder. After
  that, `tier0_s64.dll` and `vstdlib_s64.dll` are loaded from the same
  folder (`C:\Program Files (x86)\Steam\`). Then the debug message
  `[S_API] SteamAPI_Init(): Loaded 'C:\Program Files (x86)\Steam\steamclient64.dll' OK.`
  appears.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  WinDbg 1.2611.4001.0 (Microsoft), module load events (`ModLoad`) and
  debug output, five runs with the same order. Official client running,
  first test account.
- **Recorder:** the maintainer (see above).
- **Evidence:**

  ```text
  ModLoad: ...   <game>\Cave Crawlers_Data\Plugins\x86_64\steam_api64.dll
  ModLoad: ...   C:\Program Files (x86)\Steam\steamclient64.dll
  ModLoad: ...   C:\Program Files (x86)\Steam\tier0_s64.dll
  ModLoad: ...   C:\Program Files (x86)\Steam\vstdlib_s64.dll
  [S_API] SteamAPI_Init(): Loaded 'C:\Program Files (x86)\Steam\steamclient64.dll' OK.
  ```

- **Meaning:** not known. Whether `tier0_s64.dll` and `vstdlib_s64.dll`
  are needed by `steamclient64.dll` itself is not recorded; a shim would
  not load them unless an observation shows the game needs them.

### Consequence for stage 2

A proxy is only loaded on Windows if `SteamClientDll64` points to it.
That value belongs to the official client's state, which ADR 0038 does
not allow Reborn to change silently. How the observation proxy gets
loaded on Windows (for example: the user sets the value by hand for one
session and restores it) needs a decision before stage 2.

### Linux, Windows games under Proton: same registry path inside the prefix

- **Fact:** Before the game starts, Proton copies
  `~/.local/share/Steam/legacycompat/steamclient.dll` and
  `.../legacycompat/steamclient64.dll` into the game's Wine prefix as
  `C:\Program Files (x86)\Steam\steamclient.dll` and `...\steamclient64.dll`.
  The prefix's registry (`pfx/user.reg`) holds, under
  `HKCU\Software\Valve\Steam\ActiveProcess`, the values `PID`,
  `SteamClientDll`, `SteamClientDll64` and `SteamPath`, with
  `SteamClientDll64` = `C:\Program Files (x86)\Steam\steamclient64.dll`,
  the same key and value the game read on Windows. In the game process,
  Wine loads the game's `steam_api64.dll` (native), then
  `C:\windows\system32\lsteamclient.dll` (Proton's built-in module), then
  `C:\Program Files (x86)\Steam\steamclient64.dll` (native, the copied
  file). Earlier, another process loads and unloads `lsteamclient.dll`;
  next to it Proton logs that it sets up the Steam registry.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030, a Windows-only game),
  Linux VM with the official client and Proton (versions not noted),
  stage 0. The copy: `strace -f -e trace=openat,execve` as launch option
  (the game itself did not start under `strace`). The loads:
  `PROTON_LOG=1 WINEDEBUG=+loaddll` as launch option. The registry: the
  prefix's `user.reg` after the run. First test account.
- **Recorder:** the maintainer (see above).
- **Evidence** (user name replaced by `<user>`):

  ```text
  openat(AT_FDCWD, "/home/<user>/.local/share/Steam/legacycompat/steamclient64.dll", O_RDONLY|O_CLOEXEC) = 11
  openat(AT_FDCWD, "/home/<user>/.local/share/Steam/steamapps/compatdata/2143030/pfx/drive_c/Program Files (x86)/Steam/steamclient64.dll", O_WRONLY|O_CREAT|O_TRUNC|O_CLOEXEC, 0666) = ...

  [Software\\Valve\\Steam\\ActiveProcess]
  "PID"=dword:00000020
  "SteamClientDll"="C:\\Program Files (x86)\\Steam\\steamclient.dll"
  "SteamClientDll64"="C:\\Program Files (x86)\\Steam\\steamclient64.dll"
  "SteamPath"="C:\\Program Files (x86)\\Steam"

  0164:trace:loaddll:build_module Loaded L"S:\\steamapps\\common\\Cave Crawlers\\Cave Crawlers_Data\\Plugins\\x86_64\\steam_api64.dll" at ...: native
  0164:trace:loaddll:build_module Loaded L"C:\\windows\\system32\\lsteamclient.dll" at ...: builtin
  0164:trace:loaddll:build_module Loaded L"C:\\Program Files (x86)\\Steam\\steamclient64.dll" at ...: native
  ```

- **Left out on purpose:** Proton's log also printed `fixme` lines from
  its `steamclient` channel. They come from Proton's `lsteamclient`, which
  is built from Steamworks SDK headers (ADR 0012), and name things from
  there, so they are not used or copied.
- **Meaning:** not known why `lsteamclient.dll` loads before
  `steamclient64.dll`. Whether the game reads `ActiveProcess` under
  Proton too was not recorded (no registry trace).
- **Consequence (not decided):** for Windows games under Proton, the
  `steamclient64.dll` a game loads is chosen by a registry value inside
  the game's prefix. When Reborn runs a game through Proton itself, that
  prefix is Reborn's own (ADR 0016, ADR 0017), not the official client's.

### Linux, native games

- **Fact (from ADR 0012):** `steam_api` tries a local `steamclient.so`,
  then `~/.steam/sdk64/steamclient.so`.
- **Recorded:** not yet in this format. The first test account owns no
  native Linux game yet.

## What `ActiveProcess\pid` is for (experiment 6, ADR 0048)

### Plan (2026-11-21, before the runs)

`docs/shim/loading.md` records that games read
`HKCU\Software\Valve\Steam\ActiveProcess\pid` before they load
`steamclient64.dll`, and that it held the official client's process ID.
What they check it for is not known. ADR 0048 needs to know what `pid`
should hold in shim mode.

Setup: official client running and signed in; `rebornd` running (its
process ID is the replacement value). For each of Linsips (4301370),
Baldi's Basics Classic Remastered (1712830) and Plant Therapy (2505120):
one normal proxy run, then one run with `reborn-observe arm --pid
<rebornd's process ID>`; the old value is restored on disarm.
`reborn-observe status`-like checks of `pid` before and after each run
show whether the official client rewrote it meanwhile.

**Prediction (if `pid` is only read, or only checked for a running
process):** in the replaced-`pid` runs each game loads the proxy, reaches
its first screen, closes on a close request, and calls nothing that its
normal runs (this normal run and the earlier ones in `coverage.md`) did
not, and `SteamUtils010` position 9 returns its app ID. A game that does
not load the proxy, stops, or calls positions no normal run called is a
mismatch; then `pid` must keep pointing at something else, and a new ADR
decides.

If the official client rewrites `pid` during the run, the experiment
tells nothing and is noted as such.

### Result (agent runs, ADR 0044, after the plan was committed at 17:09:36)

- `pid` held the official client's process ID before every run. In the
  three replaced-`pid` runs `reborn-observe` wrote `rebornd`'s process ID,
  and at disarm `pid` still held it, so the official client did not
  rewrite it during a run; disarm put the old value back each time.
- All six runs loaded the proxy, reached the first screen and closed on a
  close request.
- In the replaced-`pid` runs no game called any position that none of
  its normal runs called, none missed a position its normal runs called
  in the first 2 seconds, and position 9 of `SteamUtils010` returned the
  game's app ID (4301370, 1712830, 2505120). **Matches** in all three
  games.

So far a game behaves the same when `pid` names a running process other
than the official client. Under ADR 0048 `pid` gets `rebornd`'s process
ID in shim mode, once the maintainer gives the OK (ADR 0044). Not
covered: a `pid` that names no running process, and systems without the
official client, which this experiment could not test.
