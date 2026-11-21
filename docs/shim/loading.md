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

### Consequence for stage 2

A proxy is only loaded on Windows if `SteamClientDll64` points to it.
That value belongs to the official client's state, which ADR 0038 does
not allow Reborn to change silently. How the observation proxy gets
loaded on Windows (for example: the user sets the value by hand for one
session and restores it) needs a decision before stage 2.

### Linux

- **Fact (from ADR 0012):** `steam_api` tries a local `steamclient.so`,
  then `~/.steam/sdk64/steamclient.so`.
- **Recorded:** not yet in this format; to be recorded with
  `strace -f -e trace=file` on the Linux VM.
