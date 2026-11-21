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

### Windows: which entry points are called, how often (stage 2, part 2)

- **Fact:** With the observation proxy loaded instead of
  `steamclient64.dll` (forwarding every call unchanged), the game ran
  normally to its main menu and exited normally. From start to exit:
  - `CreateInterface` was called twice (at 0 ms and 16 ms after the
    first call);
  - `Steam_BGetCallback` was called very often, about every 3 ms after
    the first 70 ms (the first 50 calls are logged one by one);
  - `Steam_FreeLastCallback` was called 13 times, at irregular times;
  - `Steam_ReleaseThreadLocalMemory` and `Steam_GetAPICallResult` were
    not called;
  - every call came from the same thread.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  `reborn-observe arm` with the proxy from `crates/shim-observe` (part 2:
  logs calls, not arguments), stage 2. Official client running, first
  test account. The real DLL needed `LOAD_WITH_ALTERED_SEARCH_PATH`:
  a plain `LoadLibraryW` failed with error 126 because its own folder
  holds DLLs it needs (`loading.md`).
- **Recorder:** the maintainer (see above).
- **Evidence** (excerpt of `observe-<pid>.log`, path shortened):

  ```text
  0 ms ThreadId(1) CreateInterface call 1
  loading the real DLL from C:\Program Files (x86)\Steam\steamclient64.dll
  16 ms ThreadId(1) CreateInterface call 2
  69 ms ThreadId(1) Steam_BGetCallback call 1
  69 ms ThreadId(1) Steam_FreeLastCallback call 1
  69 ms ThreadId(1) Steam_BGetCallback call 2
  90 ms ThreadId(1) Steam_BGetCallback call 3
  ...
  221 ms ThreadId(1) Steam_BGetCallback call 50
  378 ms ThreadId(1) Steam_FreeLastCallback call 2
  ...
  3497 ms ThreadId(1) Steam_FreeLastCallback call 13
  ```

- **Meaning:** not known. The names suggest polling for callbacks and
  releasing them, but names are not a source; arguments and results are
  for part 3.
