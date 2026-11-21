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
