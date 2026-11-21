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
