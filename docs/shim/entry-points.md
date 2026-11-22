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

### Windows: arguments and results (stage 2, part 3)

- **Fact:** With the proxy logging the raw argument registers and
  results, the game again ran normally to its main menu and exited.
  - `CreateInterface` call 1: `rcx` pointed to the text
    `SteamClient017`, `rdx` = 0, `r8` = 0, `r9` = 1; the result (`rax`) was
    a non-zero address.
  - `CreateInterface` call 2: `rcx` pointed to the text
    `SteamClient020`, `rdx` = 0, `r8` = 4, `r9` = 4; the result was a
    non-zero address.
  - `Steam_BGetCallback`: `rcx` was 1 in every logged call; `rdx` and `r8`
    held addresses near each other on the stack. In the 50 logged calls
    the result's lowest byte was 0 (48 times) or 1 (once); once `rax` was
    `0xffffff00` (lowest byte 0, upper bits not zero).
  - `Steam_FreeLastCallback`: `rcx` was 1 in every call; `rax` was 0 every
    time. Its first call came right after the only logged
    `Steam_BGetCallback` whose lowest result byte was 1.
  - No integer register of `Steam_BGetCallback` or
    `Steam_FreeLastCallback` pointed to printable text.
  - `Steam_ReleaseThreadLocalMemory` and `Steam_GetAPICallResult` were not
    called.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  `reborn-observe arm` with the proxy from `crates/shim-observe` (part 3:
  raw argument and result registers, text from readable memory), stage 2.
  Official client running, first test account.
- **Recorder:** the maintainer (see above).
- **Evidence** (excerpt; floating-point registers left out, see below):

  ```text
  0 ms ThreadId(1) CreateInterface call 1 rcx=0x7ffdb8de2da0 rdx=0x0 r8=0x0 r9=0x1 rcx->"SteamClient017"
  17 ms ThreadId(1) CreateInterface return 1 rax=0x7ffceac307d8
  17 ms ThreadId(1) CreateInterface call 2 rcx=0x7ffdb8de2dd0 rdx=0x0 r8=0x4 r9=0x4 rcx->"SteamClient020"
  17 ms ThreadId(1) CreateInterface return 2 rax=0x7ffceac307f0
  73 ms ThreadId(1) Steam_BGetCallback call 1 rcx=0x1 rdx=0xff176eeef0 r8=0xff176eee90 r9=0x374
  74 ms ThreadId(1) Steam_BGetCallback return 1 rax=0x1
  74 ms ThreadId(1) Steam_FreeLastCallback call 1 rcx=0x1 rdx=0xf r8=0x245c5226300 r9=0x36c
  74 ms ThreadId(1) Steam_FreeLastCallback return 1 rax=0x0
  74 ms ThreadId(1) Steam_BGetCallback call 2 rcx=0x1 rdx=0xff176eeef0 r8=0xff176eee90 r9=0x70
  74 ms ThreadId(1) Steam_BGetCallback return 2 rax=0xffffff00
  ```

- **Left out on purpose:** the floating-point registers. They are not
  arguments in these calls but hold leftovers, and in some calls one held
  the account's SteamID (and another a piece of the game's text). Raw
  register logs can carry account data; they stay on the recorder's
  computer and are redacted before anything comes here.
- **Meaning:** not known. Which arguments the entry points really take,
  their types and what the returned addresses are is not yet observed;
  calls on the returned objects are stage 3 (its own ADR, ADR 0038).

### Windows: what `Steam_BGetCallback` hands the game (step 4b)

- **Fact:** With the proxy logging the first 16 bytes at `rdx` and `r8`
  after each `Steam_BGetCallback` call whose result's lowest byte was 1
  (ADR 0042), three games ran normally, Coin Maker Simulator also into a
  game and back to the menu.
  - Such calls: 14 in Cave Crawlers, 24 in Bongo Cat, 19 in Coin Maker
    Simulator; `Steam_FreeLastCallback` was called exactly as often in
    each game.
  - At `rdx`, in every one of them: first a four-byte value 1 (the same
    as the lowest four bytes of `rcx` of the call in these three games;
    not in general: Dark Deception and Gravitas poll with `rcx` 2 and
    still get 1, see `meanings.md`, loop iteration 9b), then a four-byte
    number, then eight bytes that look like an address in the game's
    process. Bytes are little endian; the numbers seen, with how often:

    | Number | Cave Crawlers | Bongo Cat | Coin Maker Simulator |
    | --- | --- | --- | --- |
    | 304 | 1 | | |
    | 336 | 2 | | |
    | 703 | | 6 | 11 |
    | 1101 | 1 | 1 | |
    | 1102 | 3 | 4 | |
    | 1222 | | 2 | 2 |
    | 1281 | | 4 | 4 |
    | 4700 | 2 | 3 | |
    | 4701 | 1 | 1 | |
    | 4702 | 1 | 1 | |
    | 1040011 | 2 | 1 | 1 |
    | 1040044 | 1 | 1 | 1 |

  - After each call with the number 703, the next entry point the game
    called was `Steam_GetAPICallResult`, then `Steam_FreeLastCallback`;
    after every other one it was `Steam_FreeLastCallback` directly.
  - At `r8`, the 16 bytes showed no pattern shared by the three games.
    Whether the client wrote them is not known.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Bongo Cat (3419430)
  and Coin Maker Simulator Demo (5191630), Windows 11 64-bit,
  `reborn-observe arm` (without `--read-only`) with the proxy from
  `crates/shim-observe` (step 4b of ADR 0042). Official client running,
  first test account.
- **Recorder:** the maintainer (see above).
- **Evidence** (excerpt, Coin Maker Simulator; floating-point registers
  left out):

  ```text
  719 ms ThreadId(1) Steam_BGetCallback return 3 wrote rdx->[01000000 01050000 4001739c d4010000] r8->[00000000 00000000 01000000 00000000]
  720 ms ThreadId(1) Steam_BGetCallback return 5 wrote rdx->[01000000 bf020000 c004639c d4010000] r8->[00000000 00000000 01000000 00000000]
  720 ms ThreadId(1) Steam_GetAPICallResult call 1 rcx=0x1 rdx=0xb72601e4ae2e78d3 r8=0x1d4c60ca1b0 r9=0x318
  720 ms ThreadId(1) Steam_GetAPICallResult return 1 rax=0x0
  720 ms ThreadId(1) Steam_FreeLastCallback call 5 rcx=0x1 rdx=0x0 r8=0x1d232ff4800 r9=0x16
  ```

- **Meaning:** not known. What the number and the eight bytes after it
  are, and what lies beyond the first 16 bytes, is not observed; reading
  further needs a new ADR (ADR 0042). ADR 0050 is that ADR; see step 4d
  below.

### Windows: what the callback header points to (step 4d, ADR 0050)

- **Fact:** in an agent run of Cave Crawlers (2026-11-22, `observe-22072.log`,
  stays local), all 14 `Steam_BGetCallback` calls that returned 1 had, at
  `rdx` bytes 16 to 20, a number from 1 to 784 that, together with the
  address in bytes 8 to 16, made a readable range; bytes 20 to 24 were 0
  every time. Number (bytes 4 to 8, hex) and that size, with how often:

  | Number | Size | Calls |
  | --- | --- | --- |
  | `0x44e` | 16 | 3 |
  | `0x44d` | 24 | 1 |
  | `0x130` | 16 | 2 |
  | `0x150` | 12 | 2 |
  | `0x125c` | 8 | 2 |
  | `0x125d` | 4 | 1 |
  | `0x125e` | 1 | 1 |
  | `0xfde8b` | 260 | 1 |
  | `0xfdeac` | 784 | 1 |

  The same number always came with the same size. Only sizes and SHA-256
  hashes were logged (no number is listed yet).
- **Recorded, recorder:** as above, agent run under ADR 0044.
- **Meaning:** not confirmed. The observation fits bytes 8 to 16 being the
  address of the callback's data and bytes 16 to 20 its size; giving a
  number a meaning needs a candidate from a public page and an
  experiment (ADR 0042, 0050).

### Windows: `Steam_GetAPICallResult` (stage 2 and 4b)

- **Fact:** Bongo Cat called it 6 times and Coin Maker Simulator 11 times
  (Cave Crawlers never), each time right after a `Steam_BGetCallback`
  whose bytes at `rdx` held the number 703. `rcx` was 1 in Coin Maker
  Simulator; in Bongo Cat its lowest four bytes were 1 and the upper four
  varied. `rdx` held a value that does not look like an address, `r8` an
  address, and `r9` a small number (8, 16, 24, 32 or 792; 16 most
  often). The result was 0 or 1 (Bongo Cat 3 and 3, Coin Maker
  Simulator 3 and 8).
- **Recorded, recorder:** as above.
- **Meaning:** not known.
