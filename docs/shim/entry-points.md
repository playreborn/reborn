# Entry points games ask `steamclient` for

Stage 1 of ADR 0038. Facts only from recordings; see `README.md` for the
format. Names come only from observation while a game runs (ADR 0038,
decided by the maintainer); the export table of `steamclient` is not
read.

### Windows: five names, each asked once at start

- **Fact:** After `steamclient64.dll` was loaded, the game process asked
  `GetProcAddress` for exactly five names in that module, each once, in
  this order:
  1. `CreateInterface`
  2. `Steam_ReleaseThreadLocalMemory`
  3. `Steam_BGetCallback`
  4. `Steam_FreeLastCallback`
  5. `Steam_GetAPICallResult`

  All five came right after the debug message
  `[S_API] SteamAPI_Init(): Loaded 'C:\Program Files (x86)\Steam\steamclient64.dll' OK.`
  (written by the game's `steam_api64.dll`). No other name and no ordinal
  was asked for in `steamclient64.dll` from start to the main menu and
  back out.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  WinDbg 1.2611.4001.0 (Microsoft), script
  `docs/shim/tools/windbg-getprocaddress.wds` started on
  `ld:steamclient64.dll`, stage 1. The official client was running and
  signed in with the first test account. Only calls of the operating
  system function were observed; the code of `steamclient` was not
  looked at.
- **Recorder:** the maintainer; confirmed on 2026-11-21 that they never
  accepted the Steamworks SDK License.
- **Evidence** (module handle of `steamclient64.dll` in this run:
  `00007ffd'acc90000`; each call shows twice because it passed both
  breakpoints, `KERNEL32!GetProcAddressStub` and then
  `KERNELBASE!GetProcAddressForCaller`):

  ```text
  [S_API] SteamAPI_Init(): Loaded 'C:\Program Files (x86)\Steam\steamclient64.dll' OK.
  gpa 00007ffdacc90000 CreateInterface
  gpa 00007ffdacc90000 CreateInterface
  gpa 00007ffdacc90000 Steam_ReleaseThreadLocalMemory
  gpa 00007ffdacc90000 Steam_ReleaseThreadLocalMemory
  gpa 00007ffdacc90000 Steam_BGetCallback
  gpa 00007ffdacc90000 Steam_BGetCallback
  gpa 00007ffdacc90000 Steam_FreeLastCallback
  gpa 00007ffdacc90000 Steam_FreeLastCallback
  gpa 00007ffdacc90000 Steam_GetAPICallResult
  gpa 00007ffdacc90000 Steam_GetAPICallResult
  ```

- **Meaning:** not known yet. The names alone say nothing about
  arguments, results or calling convention; that is for stage 2 (the
  logging proxy) to observe.
- **Not covered:** other games, other `steam_api` versions, 32-bit games,
  Linux, and names asked later than the main menu.
