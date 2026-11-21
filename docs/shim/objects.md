# Objects games get from `steamclient`

Stage 3 of ADR 0038, planned in ADR 0040. Facts only from recordings;
see `README.md` for the format.

### Windows: shape of what `CreateInterface` returns (step 3a)

- **Fact:** For both results of `CreateInterface` in Cave Crawlers, the
  first eight bytes at the returned address hold an address inside
  `steamclient64.dll`, and from there consecutive eight-byte values are
  addresses inside `steamclient64.dll` that Windows marks as executable:
  - `SteamClient017`: first field points to `steamclient64+0x133a5e8`;
    37 consecutive such values;
  - `SteamClient020`: first field points to `steamclient64+0x133aa08`;
    42 consecutive such values.

  The game ran normally to its main menu and exited normally.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  `reborn-observe arm` with the proxy from `crates/shim-observe`
  (step 3a: reads, after `VirtualQuery`, only the first field of the
  result and the pointer values it leads to; asks Windows whether those
  addresses are executable; no code bytes read), stage 3. Official
  client running, first test account. The module's range came from
  `GetModuleInformation`.
- **Recorder:** the maintainer (see `loading.md`).
- **Evidence** (floating-point registers left out, see
  `entry-points.md`):

  ```text
  0 ms ThreadId(1) CreateInterface call 1 rcx=0x7ffdb81b2da0 rdx=0x0 r8=0x0 r9=0x1 rcx->"SteamClient017"
  16 ms ThreadId(1) CreateInterface return 1 rax=0x7ffceac307d8
  16 ms ThreadId(1) CreateInterface result 1 shape: first field -> steamclient64+0x133a5e8, 37 entries point into its executable memory
  17 ms ThreadId(1) CreateInterface call 2 rcx=0x7ffdb81b2dd0 rdx=0x0 r8=0x4 r9=0x4 rcx->"SteamClient020"
  17 ms ThreadId(1) CreateInterface return 2 rax=0x7ffceac307f0
  17 ms ThreadId(1) CreateInterface result 2 shape: first field -> steamclient64+0x133aa08, 42 entries point into its executable memory
  ```

- **Meaning:** not known. The counts are what was counted: they stop at
  the first value that is not an address in executable memory, so
  whether 37 and 42 are the tables' sizes is not known. Whether the game
  calls through these tables, and with which arguments, is step 3b
  (ADR 0040).
- **Consequence for step 3b:** a stand-in table needs at least 37 and 42
  entries; the shape ADR 0040 requires for 3b was seen.

### Windows: calls through the returned objects (step 3b)

- **Fact:** With stand-in objects handed out for both results of
  `CreateInterface` (calls forwarded to the same position of the real
  object's table), the game ran normally to its main menu and exited.
  - Through the stand-in for `SteamClient017`: no calls.
  - Through the stand-in for `SteamClient020`: 33 calls, all from the
    same thread, at positions 0, 1, 2, 4, 5, 12, 23 and 34 (all below the
    42 counted entries). In every one of them `rcx` held the stand-in's
    address, so the object is passed in `rcx`.
  - Position 12, called 25 times: `r8` was 1, `rdx` 0 or 1, and `r9`
    pointed to a text each time; the result was a non-zero address each
    time, and the same text always gave the same address. The texts:
    `SteamUser020`, `SteamFriends017`, `SteamUtils009`,
    `SteamMatchMaking009`, `SteamMatchMakingServers002`,
    `SteamNetworking006`, `SteamNetworkingSockets008`,
    `SteamNetworkingUtils003`, `SteamParties002`, `SteamInput001`,
    `SteamController007`, `STEAMAPPS_INTERFACE_VERSION008`,
    `STEAMINVENTORY_INTERFACE_V003`, `STEAMMUSIC_INTERFACE_VERSION001`,
    `STEAMPARENTALSETTINGS_INTERFACE_VERSION001`,
    `STEAMREMOTEPLAY_INTERFACE_VERSION001`,
    `STEAMREMOTESTORAGE_INTERFACE_VERSION014`,
    `STEAMSCREENSHOTS_INTERFACE_VERSION003`,
    `STEAMUGC_INTERFACE_VERSION014`,
    `STEAMUSERSTATS_INTERFACE_VERSION011`, `STEAMVIDEO_INTERFACE_V002`.
  - Position 5, called once: `rdx` = 1, `r8` = 1, `r9` pointed to
    `SteamUser020`; the result was the same address position 12 gave for
    `SteamUser020`.
  - Positions 0 and 2 were called once each right at the start (results
    1), position 34 twice (results 0), and positions 4, 1 and 23 once each
    when the game exited (results `0x13`, `0xffffff01`, 1).
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  `reborn-observe arm` (without `--read-only`) with the proxy from
  `crates/shim-observe` (step 3b), stage 3. Official client running,
  first test account.
- **Recorder:** the maintainer (see `loading.md`).
- **Evidence** (excerpt; floating-point registers left out, see
  `entry-points.md`; "(rcx held ...)" is the proxy's note that it put the
  real object into `rcx`, whose new value the line shows):

  ```text
  18 ms ThreadId(1) CreateInterface result 2 replaced by stand-in 1 (0x21870671950) for SteamClient020
  18 ms ThreadId(1) SteamClient020#0 call 1 (rcx held the stand-in, now the real object) rcx=0x7ffceac307f0 rdx=0x0 r8=0x216017460c0 r9=0xe
  53 ms ThreadId(1) SteamClient020#0 return 1 rax=0x1
  54 ms ThreadId(1) SteamClient020#12 call 1 (rcx held the stand-in, now the real object) rcx=0x7ffceac307f0 rdx=0x0 r8=0x1 r9=0x7ffdb68423a0 r9->"SteamUtils009"
  54 ms ThreadId(1) SteamClient020#12 return 1 rax=0x217880802c0
  54 ms ThreadId(1) SteamClient020#5 call 1 (rcx held the stand-in, now the real object) rcx=0x7ffceac307f0 rdx=0x1 r8=0x1 r9=0x7ffdb6842380 r9->"SteamUser020"
  54 ms ThreadId(1) SteamClient020#5 return 1 rax=0x217881c0360
  67 ms ThreadId(1) SteamClient020#12 call 18 (rcx held the stand-in, now the real object) rcx=0x7ffceac307f0 rdx=0x1 r8=0x1 r9=0x7ffdb6842380 r9->"SteamUser020"
  67 ms ThreadId(1) SteamClient020#12 return 18 rax=0x217881c0360
  ```

- **Meaning:** not known. What `rdx` and `r8` mean at position 12, what
  the other positions do, and what the returned addresses are (the texts
  suggest more objects of the same kind) is not observed yet; objects
  returned by these calls are step 3c, which needs its own ADR
  (ADR 0040).

### Windows: calls through objects returned through stand-ins (step 3c)

- **Fact:** With stand-ins also handed out for the results of calls
  through stand-ins that passed a text and returned the step 3a shape
  (ADR 0041), the game ran normally to its main menu and exited.
  - 23 stand-ins were made (room for 32): the two for `CreateInterface`
    and one for each of the 21 texts passed at position 12 of
    `SteamClient020` (position 5 with `SteamUser020` got the same
    stand-in as position 12 with that text). Results were replaced 28
    times; the same text always got the same stand-in. No result was
    left as it was for lack of room or table entries.
  - In every call through these stand-ins `rcx` held the stand-in's
    address. No other argument register (`rdx`, `r8`, `r9`) of any call,
    and no register of a call to the exported entry points, held a
    stand-in's address.
  - Calls through the new stand-ins, by position (50 means at least 50,
    since the proxy logs only the first 50 calls of each kind):

    | Object | Positions (calls) |
    | --- | --- |
    | `SteamUtils009` | 9 (2), 14 (50) |
    | `SteamUser020` | 2 (6), 12 (1) |
    | `SteamFriends017` | 43 (2), 44 (1) |
    | `SteamInput001` | 2 (50) |
    | `SteamController007` | 2 (50) |
    | `STEAMAPPS_INTERFACE_VERSION008` | 7 (1) |
    | `STEAMINVENTORY_INTERFACE_V003` | 1 (1), 6 (1), 20 (1), 21 (3) |
    | `STEAMREMOTESTORAGE_INTERFACE_VERSION014` | 0 (4), 1 (1), 13 (1), 15 (1), 21 (1), 22 (1) |
    | `STEAMUSERSTATS_INTERFACE_VERSION011` | 0 (1), 4 (2), 6 (50), 10 (3) |

    The other 13 objects (among them `SteamClient017`) got no calls.
  - Calls that passed texts: `STEAMREMOTESTORAGE_INTERFACE_VERSION014`
    positions 0, 1, 13 and 15 with the game's save file names in `rdx`
    (`MainPlayerData.json`, once `BackUp_MainPlayerData.json` at
    position 0); `STEAMUSERSTATS_INTERFACE_VERSION011` position 4 with
    `PrestigeLevel` and `SingleRunDamage`, position 6 with
    `Keep_PotionEffect` in `rdx`; `SteamFriends017` position 43 with
    `steam_display` in `rdx` and `#Status` or `Main Menu` in `r8`. None
    of their results had the step 3a shape, so none got a stand-in.
  - `SteamUser020` position 2: `rdx` held an address on the caller's
    stack and the result was that same address.
- **Recorded:** 2026-11-21, Cave Crawlers (2143030), Windows 11 64-bit,
  `reborn-observe arm` (without `--read-only`) with the proxy from
  `crates/shim-observe` (step 3c), stage 3. Official client running,
  first test account.
- **Recorder:** the maintainer (see `loading.md`).
- **Evidence** (excerpt; floating-point registers left out, see
  `entry-points.md`):

  ```text
  61 ms ThreadId(1) SteamClient020#12 result 1 shape: first field -> steamclient64+0x13493c0, 34 entries point into its executable memory
  61 ms ThreadId(1) SteamClient020#12 result 1 replaced by stand-in 2 (0x1e116e3fba0) for SteamUtils009
  61 ms ThreadId(1) SteamUtils009#9 call 1 (rcx held the stand-in, now the real object) rcx=0x1df1c4d02c0 rdx=0x7ffdb50821b8 r8=0x1ddb5a04bc0 r9=0xd
  62 ms ThreadId(1) SteamClient020#5 result 1 replaced by stand-in 3 (0x1e116e3fc60) for SteamUser020
  62 ms ThreadId(1) SteamUser020#2 call 1 (rcx held the stand-in, now the real object) rcx=0x1df1c600360 rdx=0xbfc2b0edc8 r8=0x7ffdb50825b8 r9=0xc
  62 ms ThreadId(1) SteamUser020#2 return 1 rax=0xbfc2b0edc8
  147 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 call 1 (rcx held the stand-in, now the real object) rcx=0x1df1c4d0450 rdx=0x1e004237a60 r8=0xbfc2b0f080 r9=0x7ffe3c011890 rdx->"Keep_PotionEffect"
  148 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 return 1 rax=0x1
  2521 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#0 call 1 (rcx held the stand-in, now the real object) rcx=0x1df1c4d0430 rdx=0x1e123519790 r8=0x1e1186bf020 r9=0xaf4b rdx->"MainPlayerData.json"
  2528 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#0 return 1 rax=0x1
  ```

- **Meaning:** not known. The table shows which positions Cave Crawlers
  uses, not what they do; the texts (save file names, stat names, rich
  presence keys) hint at it, but that is not a source. What `SteamUser020`
  position 2 writes to the address in `rdx` was not read.

### Windows: two more games, newer `SteamClient` versions (stage 3)

- **Fact:** With the same proxy as step 4b (stand-ins on), Bongo Cat and
  Coin Maker Simulator ran normally, Coin Maker Simulator also into a
  game and back to the menu.
  - `CreateInterface` was called three times in each: `SteamClient017`,
    then twice `SteamClient021` (Bongo Cat) or `SteamClient023` (Coin
    Maker Simulator). Both calls with the same text gave the same object
    (the same stand-in). Counted table entries: 37 for `SteamClient017`
    as in Cave Crawlers, 40 for `SteamClient021`, 38 for
    `SteamClient023`.
  - Unlike `SteamClient020`, where every text went to position 12, these
    two were asked for most objects at a position of their own, with the
    text in `r9` (in `r8` at position 9):

    | Text | `SteamClient021` | `SteamClient023` |
    | --- | --- | --- |
    | `SteamUser023` | 5 | 5 |
    | `SteamFriends018` | 8 | 8 |
    | `SteamUtils010` | 9, 12 | 9, 12 |
    | `SteamMatchMaking009` | 10 | 10 |
    | `SteamMatchMakingServers002` | 11 | 11 |
    | `STEAMUSERSTATS_INTERFACE_VERSION013` | 13 | 13 |
    | `STEAMAPPS_INTERFACE_VERSION008` / `009` | 15 (008) | 15 (009) |
    | `SteamNetworking006` | 16 | 16 |
    | `STEAMREMOTESTORAGE_INTERFACE_VERSION016` | 17 | 17 |
    | `STEAMSCREENSHOTS_INTERFACE_VERSION003` | 18 | 18 |
    | `SteamMatchGameSearch001` | 19 | not asked |
    | `STEAMHTTP_INTERFACE_VERSION003` | 24 | 23 |
    | `STEAMUGC_INTERFACE_VERSION021` | 26 | 25 |
    | `STEAMMUSIC_INTERFACE_VERSION001` | 27 | 26 |
    | `STEAMMUSICREMOTE_INTERFACE_VERSION001` | 28 | not asked |
    | `STEAMHTMLSURFACE_INTERFACE_VERSION_005` | 29 | 27 |
    | `STEAMINVENTORY_INTERFACE_V003` | 33 | 31 |
    | `STEAMVIDEO_INTERFACE_V007` | 34 | 32 |
    | `STEAMPARENTALSETTINGS_INTERFACE_VERSION001` | 35 | 33 |
    | `SteamInput006` | 12, 36 | 12, 34 |
    | `SteamParties002` | 37 | 35 |
    | `STEAMREMOTEPLAY_INTERFACE_VERSION003` / `004` | 38 (003) | 36 (004) |
    | `SteamNetworkingSockets012`, `SteamNetworkingUtils004`, `SteamNetworkingMessages002`, `SteamController008`, `STEAMTIMELINE_INTERFACE_V004` | 12 | 12 |

    So the same text can sit at different positions in different
    `SteamClient` versions (`STEAMHTTP_INTERFACE_VERSION003`: 24 and
    23). Asking twice for the same text, at either position, gave the
    same object.
  - At these positions `rdx` and `r8` were 1 in Coin Maker Simulator
    (at position 9 only `rdx`). In Bongo Cat their lowest four bytes were
    1 and the upper four bytes varied.
  - Every result of these calls (32 in Bongo Cat, 30 in Coin Maker
    Simulator) had the step 3a shape and got a stand-in: 29 stand-ins in Bongo Cat and 27 in Coin Maker Simulator
    (room for 32). In every call through a stand-in `rcx` held the
    stand-in's address; no other register ever held one.
  - Coin Maker Simulator called position 22 of
    `STEAMUSERSTATS_INTERFACE_VERSION013` three times with a stat name in
    `rdx`; the result was not readable memory, so it got no stand-in.
- **Recorded:** 2026-11-21, Bongo Cat (3419430) and Coin Maker Simulator
  Demo (5191630), Windows 11 64-bit, `reborn-observe arm` (without
  `--read-only`) with the proxy from `crates/shim-observe` (step 4b),
  stage 3. Official client running, first test account.
- **Recorder:** the maintainer (see `loading.md`).
- **Evidence** (excerpt, Coin Maker Simulator; floating-point registers
  left out):

  ```text
  19 ms ThreadId(1) CreateInterface result 2 shape: first field -> steamclient64+0x133ae18, 38 entries point into its executable memory
  59 ms ThreadId(1) SteamClient023#5 call 1 (rcx held the stand-in, now the real object) rcx=0x7ffceac30808 rdx=0x1 r8=0x1 r9=0x7ffdb8dec3e0 r9->"SteamUser023"
  68 ms ThreadId(1) SteamClient023#9 call 1 (rcx held the stand-in, now the real object) rcx=0x7ffceac30808 rdx=0x1 r8=0x1d4c6182500 r9=0x0 r8->"SteamUtils010"
  80 ms ThreadId(1) SteamClient023#23 call 1 (rcx held the stand-in, now the real object) rcx=0x7ffceac30808 rdx=0x1 r8=0x1 r9=0x1d4c61821c0 r9->"STEAMHTTP_INTERFACE_VERSION003"
  81 ms ThreadId(1) SteamClient023#23 result 1 replaced by stand-in 12 (0x1d31a06dcc0) for STEAMHTTP_INTERFACE_VERSION003
  ```

- **Meaning:** not known. The 32 stand-ins are nearly used up by newer
  games; a game asking for more objects gets the real ones for the rest
  (the log says so).

### Windows: the same `SteamClient` version, asked in two ways (stage 3)

- **Fact:** Two more games, recorded by an AI agent (ADR 0044):
  - Baldi's Basics Classic Remastered asked for `SteamClient020`, like
    Cave Crawlers, but unlike Cave Crawlers it asked for most objects at
    a position of its own, with the text in `r9` (`r8` at position 9),
    as Bongo Cat and Coin Maker Simulator did with 021 and 023: for
    example `SteamUser021` at 5, `STEAMUSERSTATS_INTERFACE_VERSION012`
    at 13, `STEAMREMOTESTORAGE_INTERFACE_VERSION016` at 17,
    `STEAMHTTP_INTERFACE_VERSION003` at 24; seven texts went to 12. Its
    `SteamClient020` table had 42 counted entries.
  - Linsips asked for `SteamClient023`, like Coin Maker Simulator, but
    asked for every object at position 12 (text in `r9`), except
    `SteamUser023`, which it asked for at 5 and again at 12.
  - So whether a game uses position 12 or a position per interface
    depends on the game, not on the `SteamClient` version. Asking at 12
    and at the interface's own position gave the same object (Linsips,
    `SteamUser023`: one stand-in).
  - Both games ran normally to their first screen and closed on a close
    request. Baldi's Basics used 29 stand-ins, Linsips 9.
- **Recorded:** 2026-11-21, Baldi's Basics Classic Remastered (1712830)
  and Linsips (4301370), Windows 11 64-bit, `reborn-observe arm` with
  the step 4c proxy, stage 3. Official client running, first test
  account.
- **Recorder:** an AI agent on the maintainer's computer, on the
  maintainer's instruction (ADR 0044).
- **Evidence** (excerpt; floating-point registers and the `rcx` note
  left out):

  ```text
  20 ms ThreadId(1) CreateInterface result 2 shape: first field -> steamclient64+0x133aa08, 42 entries point into its executable memory
  20 ms ThreadId(1) CreateInterface result 2 replaced by stand-in 1 (0x1d77de19c20) for SteamClient020
  63 ms ThreadId(1) SteamClient020#5 call 1 rcx=0x7ffceac307f0 rdx=0x1 r8=0x1 r9=0x7ffdb68473b0 r9->"SteamUser021"
  74 ms ThreadId(1) SteamClient023#5 call 1 rcx=0x7ffceac30808 rdx=0x1 r8=0x1 r9=0x7ffdb684c3e0 r9->"SteamUser023"
  ```

  The first three lines are Baldi's Basics, the last is Linsips.

- **Meaning:** not known.
