# Meanings of recorded calls

Step 4a of ADR 0042. A meaning is a **candidate** when a page of Valve's
public documentation describes a function whose parameters and result
fit a recording, and **confirmed** only after an experiment planned
here in advance matched its prediction. Positions come only from the
recordings in `objects.md`; the order of functions on a documentation
page is never used. Descriptions are in Reborn's own words; the pages
are linked, not copied. The shim implements only confirmed meanings.

## `STEAMREMOTESTORAGE_INTERFACE_VERSION014`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamRemoteStorage>,
  read 2026-11-21; it opened without signing in or accepting an
  agreement. Its constants name the version text
  `STEAMREMOTESTORAGE_INTERFACE_VERSION014`, the same one Cave Crawlers
  asked for.
- **Recording:** Cave Crawlers (2143030), Windows, 2026-11-21, the step
  3c and step 4b runs (`objects.md`). At start the game called, in this
  order, positions 13, 21, 22, 15 and 1; position 0 it called later,
  while playing and at exit.

### Position 0: confirmed `FileWrite` (experiment 1)

- **Recorded:** `rdx` pointed to a file name (`MainPlayerData.json`, once
  `BackUp_MainPlayerData.json`), `r8` held an address, `r9` a number
  (`0xaf4b` = 44875 every time); the result was 1. Called while playing
  and twice at exit (the backup name first).
- **Fit:** the page documents a function that takes a file name, the
  address of the bytes to write and their count, and reports success as
  true or false. Of the documented functions that take a file name, an
  address and a count, it is the only one that reports success: the
  reading one returns a byte count (it would have been 44875, as at
  position 1), and the asynchronous writing one returns a call handle,
  which 1 does not look like.

### Position 1: confirmed `FileRead` (experiment 1)

- **Recorded:** once at start: `rdx` pointed to `MainPlayerData.json`,
  `r8` held an address, `r9` was 44875; the result was 44875, the same
  as `r9`.
- **Fit:** the page documents a function that takes a file name, the
  address of a buffer and how many bytes to read into it, and returns
  how many bytes it read.

### Position 15: confirmed `GetFileSize` (experiment 1)

- **Recorded:** once at start, just before position 1: `rdx` pointed to
  `MainPlayerData.json`; the result was 44875, the number the game then
  passed to position 1.
- **Fit:** the page documents a function that takes a file name and
  returns the file's size in bytes. The other documented functions that
  take only a file name return true or false, a platform bit field, a
  timestamp or a handle; 44875 used as a buffer size fits a size best.

### Shadow mode for positions 15 and 1 (ADR 0055 stage 1; planned, not run)

Added 2026-11-22; no live run yet.

- **What:** the proxy's `SHADOWS` lists position 15 as
  `Shadow::FileSize` and position 1 as `Shadow::FileRead`. When the call
  starts, the proxy copies the file name at `rdx` (at most 1024 bytes of
  UTF-8, `VirtualQuery`-checked) and, for position 1, the buffer address
  in `r8` and the size in `r9`. After the official call returns, a worker
  thread asks `rebornd`'s read-only shadow session `CloudFileSize { name
  }` or `CloudRead { name, max: r9 }`. `rebornd` answers from the
  session's separate **shadow folder**
  (`<Reborn data>/shadow-cloud/<account>/<app>/`), which it filled once
  when `arm` registered the app: one app info, one cloud listing and one
  SHA-1-checked download per file a game can read through the API (at
  most 64 files and 64 MiB; at most one fill a minute, one at a time;
  `reborn_core::shadow_cloud`, `docs/protocol/cloud.md`). Nothing is
  written in the official client's folders or the cloud.
- **Compared:** position 15: the low 32 bits of `rax` with Reborn's size.
  Position 1, by size and SHA-1 only (ADR 0055): the official result (a
  byte count, at most the low 32 bits of `r9` and 16 MiB) and the SHA-1
  of that many bytes at `r8`, hashed on the game's thread right after the
  return in 64 KiB pieces and only if `VirtualQuery` shows the whole range
  readable, with the count and SHA-1 of
  Reborn's bytes. A file Reborn does not have is not compared (the
  official answer for a missing file is not recorded).
- **Logged:** only `shadow match`, `shadow mismatch`, `shadow: no Reborn
  answer (<reason>)` or `shadow: skipped, <reason>`. No file names and no
  sizes: the call line shows `rcx`, `rdx` and `r8` as raw addresses, says
  `r9 hidden (account data)` and shows no register text (no file name at
  `rdx`, no old buffer contents at `r8`); the return line says `rax
  hidden (account data)`; the shadow line never carries the file name, a
  size or contents. The same holds for `BIsSubscribed`. The game always
  gets the official client's result;
  neither position is in `ANSWERS`. Position 0 (`FileWrite`) is never
  shadowed (ADR 0055: writes go to answer mode in stage 3).
- **Prediction for a live run** (Cave Crawlers, 2143030, which calls 15
  then 1 with `MainPlayerData.json` at start): `rebornd` signed in to the
  test account, the official client running; `reborn-observe arm --app
  2143030`; `rebornd`'s log says the shadow folder was filled (2 written);
  both positions log `shadow match`. A mismatch is expected if the
  official client's local file differs from the cloud version at arm
  time (for example a save not yet uploaded); it is then noted here, not
  explained away.
- **First run:** experiment 33, match (end of this file).

### Position 13: candidates `FileExists`, `FilePersisted`, `FileForget`

- **Recorded:** once at start, first of the five: `rdx` pointed to
  `MainPlayerData.json` (`r8` and `r9` did not look like arguments); the
  result was 1.
- **Fit:** the page documents several functions that take only a file
  name and return true or false. One of them deletes the file locally;
  the game read the file right after, so that one does not fit. Three
  remain: whether the file exists, whether it is kept in the cloud, and
  one that removes it from the cloud but keeps it on disk.

### Positions 21 and 22: not known

- **Recorded:** once each at start, between positions 13 and 15: `rdx`
  was 15, `r8` an address, `r9` 33 (position 21) or 9 (position 22); the
  result was 1. In experiment 1 `rdx` was 15 again, `r9` 645 and 4.
- No candidate yet: no recorded call returned 15, so what that argument
  is cannot be told from the recordings.

### Experiment 1 (planned and run 2026-11-21)

Cave Crawlers with the step 4b proxy, as before.

1. Before starting: write down the size in bytes and the time of
   `MainPlayerData.json` in the game's cloud folder of the official
   client (`userdata\<account>\2143030\remote\`; the account folder
   stays out of the docs).
2. Play until the save changes (some progress), then quit normally.
3. After quitting: write down size and time again.

Predictions from the page:

- **Position 15 (`GetFileSize`):** its result at start equals the size
  from step 1.
- **Position 1 (`FileRead`):** `r9` and its result at start equal the
  size from step 1.
- **Position 0 (`FileWrite`):** the last call with `MainPlayerData.json`
  before exit has `r9` equal to the size from step 3, and result 1; the
  file's time in step 3 is later than in step 1.
- **Position 13:** result 1, as all three candidates predict for a file
  that exists and is in the cloud. Telling them apart needs a later
  experiment.

A prediction that does not match leaves the candidate as it is, with
the mismatch noted here.

**Result** (run by the maintainer, 2026-11-21, `observe-<pid>.log` of
the step 4b proxy; the game ran normally). Making progress was not
possible over the remote connection, so the recorder changed game
settings instead; the save changed all the same.

- Step 1: 44875 bytes; step 3: 44877 bytes, written about 20 minutes
  later.
- Position 15: result 44875. **Matches.**
- Position 1: `r9` 44875, result 44875. **Matches.**
- Position 0: seven calls; the last one with `MainPlayerData.json`,
  right before exit, had `r9` = 44877 and result 1. **Matches.** The
  earlier calls had, in order, 44875, 46785, 44875, 44875, 44877, and
  44877 with the backup name just before the last call; all returned
  1.
- Position 13: result 1. **Matches** all three candidates, as expected.

Positions 0, 1 and 15 are confirmed for
`STEAMREMOTESTORAGE_INTERFACE_VERSION014`; position 13 stays a
candidate.

Evidence (excerpt; floating-point registers and the `rcx` note left
out):

```text
1085 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#15 call 1 rdx=0x1e8ac9a0500 r8=0x1e51f3e8340 r9=0x8101010101010100 rdx->"MainPlayerData.json"
1085 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#15 return 1 rax=0xaf4b
1086 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#1 call 1 rdx=0x1e8ac9a06a0 r8=0x1e8a397f020 r9=0xaf4b rdx->"MainPlayerData.json"
1086 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#1 return 1 rax=0xaf4b
176310 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#0 call 7 rdx=0x1e8e3019fb0 r8=0x1e8db5b2020 r9=0xaf4d rdx->"MainPlayerData.json"
176317 ms ThreadId(1) STEAMREMOTESTORAGE_INTERFACE_VERSION014#0 return 7 rax=0x1
```

## `STEAMUSERSTATS_INTERFACE_VERSION011`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamUserStats>,
  read 2026-11-21; it opened without signing in or accepting an
  agreement. Its constants name the version text
  `STEAMUSERSTATS_INTERFACE_VERSION011`, the same one Cave Crawlers
  asked for. Bongo Cat and Coin Maker Simulator asked for
  `STEAMUSERSTATS_INTERFACE_VERSION013`, which the page does not name;
  their calls are not covered here.
- **Recording:** Cave Crawlers (2143030), Windows, 2026-11-21, the
  experiment 1 run with the step 4b proxy. The game called positions 0
  (once), 4 (3 times), 6 (50 000 times or more), 10 (5 times) and 12
  (50 times or more; the proxy logs the first 50 calls of each kind,
  then every 10 000th). The step 3c run, which ended at the main menu,
  has no calls at position 12.

### Position 12: confirmed `GetAchievementDisplayAttribute` (experiment 2)

- **Recorded:** 50 logged calls between about 12 and 19 seconds after
  start. Each passed a name in `rdx` (34 different names, for example
  `PlayWith10Players` and `Hit_Into_Spikes`) and the text `name` in
  `r8`. Each result was a different address, mostly 0x20 above the one
  before; none had the step 3a shape.
- **Fit:** the page documents one function that takes an achievement
  name and a second text and returns text: a display attribute of the
  achievement, picked by the second text, and `name` is one of the keys
  it lists (the achievement's name in the player's language). The other
  documented functions that take an achievement name return true or
  false, a number or an image handle. Different addresses close
  together fit short texts better.

### Position 6: candidates that read a value by name

- **Recorded:** about 300 calls a second, from 120 ms to the end of the
  run. Every logged call passed `Keep_PotionEffect` in `rdx`, the same
  address on the game's stack in `r8` and the same value in `r9`
  (`0x7ffe4c781890`); the result was 1 every time.
  `Keep_PotionEffect` is also one of the names the game passed to
  position 12.
- **Fit:** the page documents several functions that take a name and
  the address of a place for the answer, and report success as true or
  false: the unlock state of an achievement, the player's own value of
  a stat (one for whole numbers, one for floating-point numbers), the
  share of all players who unlocked an achievement, and the total of a
  global stat. One more also takes a second address for the unlock
  time; whether `r9` is an argument is not known, so it is not ruled
  out. Which of them it is cannot be told from the recording: what the
  client wrote to the address in `r8` was not read. That the same name
  went to position 12 hints at an achievement, but a hint is not a
  source.
- **Experiment 2** ruled out the share of players; the other candidates
  remain (see its result below).
- **Further run** (2026-11-21, an AI agent on the maintainer's computer,
  on the maintainer's instruction, ADR 0044; the game ran to its menu
  only): 55 logged calls at position 6, all with `Keep_PotionEffect`;
  the 16 bytes after each were `009b0ad4 a0020000 e4f959f8 fc7f0000`.
  The first byte was 0 again, bytes 2 to 8 differed from experiment 2.
  That fits a call that writes only the first byte, but it was not a
  planned experiment, so it changes no status.

### Position 4: candidates that set a value by name

- **Recorded:** three calls: `PrestigeLevel` at about 1 second,
  `SingleRunDamage` at about 2 and 122 seconds. `r8` was 0 every time,
  `r9` the same value each time (`0x7ffe4c781810`); the result was 1.
- **Fit:** the page documents three functions that take a name and one
  or two numbers and report success as true or false: setting a stat to
  a value (one for whole numbers, one for floating-point numbers), and
  updating an average-rate stat (with floating-point numbers). The
  whole-number one fits `r8` = 0 directly; the other two carry their
  numbers in floating-point registers, which are not evaluated here,
  so they are not ruled out. A fourth one, showing progress towards an
  achievement, takes two counts; `r9` does not look like a count, so it
  fits less well. The page says such a set only changes the client's
  copy until another call sends it to the server.
- These are writes the game asked for; the proxy only passes them on
  (red line 4).

### Positions 0 and 10: not known

- **Recorded:** position 0 once at 87 ms and position 10 first at about
  1 second, both with `rdx` = 15, an address in `r8` and `r9` = 807
  (`0x327`); the result was 1. This is the same pattern as positions 21
  and 22 of `STEAMREMOTESTORAGE_INTERFACE_VERSION014` (`rdx` = 15, an
  address, a number). The other four calls at position 10 (two at about
  2 seconds, two at about 122 seconds) had the same address in `rdx` and `r8` and another
  address in `r9`; the result was 1.
- No candidate: without knowing what 15 is, no documented function fits
  better than another.

### What confirming these needs

The proxy logs texts only for arguments (ADR 0038) and the bytes behind
an address only after `Steam_BGetCallback` (ADR 0042). None of the
candidates above can be confirmed with that:

- **Position 12:** the text at the returned address. A prediction would
  be that it matches the achievement's name as the official client
  shows it in Cave Crawlers' achievement list.
- **Position 6:** the first bytes at the address in `r8` after the call
  returns. A prediction for the unlock state would be one byte, 0 or 1,
  matching whether the test account has unlocked `Keep_PotionEffect`
  in the official client; a stat would fill four bytes.
- **Position 4:** no experiment yet; it needs a way to see the stat's
  value that does not change it.

Reading either needs a new ADR before any experiment is planned here
(ADR 0043).

### Experiment 2 (planned and run 2026-11-21)

Cave Crawlers with the step 4c proxy (ADR 0043), which logs the result
text of position 12 and the first 16 bytes at `r8` after position 6.
Nothing on the account is changed; the game is only played as usual.

1. Before starting: open Cave Crawlers' achievements in the official
   client and keep the list at hand (names in the client's language,
   and which ones are unlocked).
2. Start the game, play until the game shows its achievements or for
   about a minute, then quit normally. (In experiment 1, position 12
   was first called at about 12 seconds.)
3. After quitting: from the log, take the `result->"..."` lines of
   position 12 and the `after r8->[...]` lines of position 6.

Predictions from the page:

- **Position 12 (`GetAchievementDisplayAttribute`):** every logged call
  with the key `name` returns text, and each text is the name of one of
  the achievements in the official client's list from step 1, in the
  client's language. Different achievement names in `rdx` give different
  texts.
- **Position 6:** the candidates write different amounts to the address
  in `r8`:
  - unlock state of an achievement: the first byte is 0 or 1, and it is
    1 exactly if the achievement whose display name position 12 returns
    for `Keep_PotionEffect` is unlocked in the list from step 1;
  - the player's own stat value: four bytes, a whole number or a
    floating-point number;
  - the share of players who unlocked an achievement: four bytes, a
    floating-point number between 0 and 100;
  - a global stat total: eight bytes.
  The proxy cannot see which bytes the call wrote and which were there
  before. If the achievement is locked and the first byte is 0, that
  does not tell the unlock state apart from a stat of 0, and the
  candidates stay as they are.

The account's achievement state is noted here only as a match or a
mismatch with the prediction (ADR 0043).

**Result** (run by the maintainer, 2026-11-21, `observe-<pid>.log` of
the step 4c proxy; the game ran normally for about three minutes).
Playing was not possible over the remote connection, so the game stayed
in its menus; position 12 was first called at about 88 seconds.

- **Position 12:** 50 logged calls with 34 different achievement names,
  all with the key `name`. 48 returned text the proxy could read, 33
  different texts for the 33 names (for example `PlayWith10Players` →
  "The Whole Squad", `Hit_Into_Spikes` → "THIS IS SPARTA!",
  `Keep_PotionEffect` → "Potion Addict", `SloshKill_Normal` → "Tier 1
  Crawler"). The recorder found these four in the official client's
  achievement list for Cave Crawlers. The two calls with `EmptyTorch`
  showed `result: no text`: the proxy logs only 2 to 64 printable ASCII
  characters, so a longer name or one with other characters looks like
  this; what it returned is not known. **Matches**, with that one name
  not checked.
- **Position 6:** 55 logged calls (the 50 000th at about 169 seconds),
  all with `Keep_PotionEffect` and the same `r8`; every time the 16
  bytes after the call were `00a5ddca f4010000 e4f96bfa fc7f0000`. The
  first byte is 0, which the recorder compared with the state of
  "Potion Addict" in the official client: **matches** the unlock
  state. Read as four bytes, the start is a negative floating-point
  number, outside 0 to 100: **does not match** the share of players,
  which is ruled out. The next three bytes are not 0, so a stat of 0
  written as four bytes does not fit either; a stat with another value,
  and a global total in eight bytes, are not ruled out by the
  predictions. (The first eight bytes read as an address in the range
  this run's heap addresses had, which hints at a leftover value with
  only the first byte written, but a hint is not a source.)

Position 12 is confirmed as `GetAchievementDisplayAttribute` for
`STEAMUSERSTATS_INTERFACE_VERSION011`. Position 6 stays a candidate:
unlock state of an achievement, the player's own stat value, or a
global stat total. A later run after "Potion Addict" has been unlocked
by playing normally would tell the unlock state apart (first byte 1).

Evidence (excerpt; floating-point registers and the `rcx` note left
out):

```text
124 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 call 1 rcx=0x1f48a130450 rdx=0x1f68e52b7b0 r8=0x2aa7aef070 r9=0x7ffe058c1890 rdx->"Keep_PotionEffect"
124 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 return 1 rax=0x1
124 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 return 1 after r8->[00a5ddca f4010000 e4f96bfa fc7f0000]
87673 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 call 1 rcx=0x1f48a130450 rdx=0x1f68ef88c20 r8=0x1f68efe3df0 r9=0x7ffe058c1990 rdx->"PlayWith10Players" r8->"name"
87673 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 1 rax=0x1f6901b0ec2
87673 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 1 result->"The Whole Squad"
87674 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 call 5 rcx=0x1f48a130450 rdx=0x1f68ef88ae0 r8=0x1f68efe3f40 r9=0x7ffe058c1990 rdx->"EmptyTorch" r8->"name"
87675 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 5 rax=0x1f6901b0f42
87675 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 5 result: no text
87682 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 call 32 rcx=0x1f48a130450 rdx=0x1f68ef88d60 r8=0x1f68efe3df0 r9=0x7ffe058c1990 rdx->"Keep_PotionEffect" r8->"name"
87682 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 32 rax=0x1f6901c02a2
87682 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 32 result->"Potion Addict"
169079 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 return 50000 after r8->[00a5ddca f4010000 e4f96bfa fc7f0000]
```

Evidence (excerpt, `observe-<pid>.log` of experiment 1; floating-point
registers and the `rcx` note left out):

```text
87 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#0 call 1 rcx=0x1e6a5ec0450 rdx=0xf r8=0x1e51f3eb580 r9=0x327
88 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#0 return 1 rax=0x1
120 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 call 1 rcx=0x1e6a5ec0450 rdx=0x1e8acc6a890 r8=0x7393cef100 r9=0x7ffe4c781890 rdx->"Keep_PotionEffect"
120 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 return 1 rax=0x1
1100 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#4 call 1 rcx=0x1e6a5ec0450 rdx=0x1e8ac98e320 r8=0x0 r9=0x7ffe4c781810 rdx->"PrestigeLevel"
1100 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#4 return 1 rax=0x1
1101 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#10 call 1 rcx=0x1e6a5ec0450 rdx=0xf r8=0x1e51f3e9f00 r9=0x327
1101 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#10 return 1 rax=0x1
11870 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 call 1 rcx=0x1e6a5ec0450 rdx=0x1e8ad3bc8d0 r8=0x1e8ac98ed40 r9=0x7ffe4c781990 rdx->"PlayWith10Players" r8->"name"
11870 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 1 rax=0x1e8aea80ec2
11870 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 call 2 rcx=0x1e6a5ec0450 rdx=0x1e8ad3bc4d0 r8=0x1e8ac98ee90 r9=0x7ffe4c781990 rdx->"Hit_Into_Spikes" r8->"name"
11871 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#12 return 2 rax=0x1e8aea80ee2
169724 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION011#6 call 50000 rcx=0x1e6a5ec0450 rdx=0x1e8e3019dd0 r8=0x7393cef100 r9=0x7ffe4c781890 rdx->"Keep_PotionEffect"
```

## `SteamUtils009` and `SteamUtils010`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamUtils>, read
  2026-11-21. Its constants name the version text `SteamUtils009`, the
  one Cave Crawlers asked for. Bongo Cat, Coin Maker Simulator, Baldi's
  Basics and Linsips asked for `SteamUtils010`, which the page does not
  name; for them the entry is a candidate only (ADR 0042, versions).
- **Recording:** all five recorded games call position 9 about twice at
  start and position 14 about 50 to 90 times (`coverage.md`).

### Position 9: confirmed `GetAppID` for `SteamUtils010` (experiment 3); candidate for `SteamUtils009`

- **Recorded:** the result was, in every recorded game, that game's own
  app ID: Cave Crawlers `0x20b336` (2143030, `SteamUtils009`), Bongo Cat
  `0x342d26` (3419430), Coin Maker Simulator `0x4f37ce` (5191630),
  Baldi's Basics `0x1a22be` (1712830), Linsips `0x41a23a` (4301370). The
  argument registers held leftovers that changed between games (`rdx`
  0 or an address, `r9` small numbers).
- **Fit:** the page documents a function that takes no arguments and
  returns the app ID of the current process. Of the other documented
  functions without arguments that return a 32-bit number (lengths,
  counts, seconds, server time), none would equal the app ID in five
  games.

### Experiment 3 (planned 2026-11-21 12:48, before reading the runs)

The agent's batch of stage 3 runs (ADR 0044) was running in the
background with these games (correction: its first game started at
12:47:52, not at about 12:35 as first written): Paper Lily - Chapter 1 (2220360), Please, Touch The
Artwork 2 (2600140), Cube Escape: Paradox (909080), Katawa Shoujo
(3068300), Higurashi When They Cry Hou - Ch.1 (310360), SCP –
Containment Breach (2178380), Travellin Cats in Paris (2313250), An
Arcade Full of Cats (2368470). Their logs had not been read when this
plan was written.

**Prediction from the page:** in every one of these games that loads the
proxy and asks for `SteamUtils009` or `SteamUtils010`, the first result
of position 9 is that game's app ID. A game that does not load the proxy
or does not call position 9 counts neither way. A single game where the
result differs is a mismatch.

**Result** (read after the plan was committed at 12:48:44; agent runs,
ADR 0044). Three of the eight games loaded the proxy and called
position 9 of `SteamUtils010`; in all three the first result was the
game's app ID: Paper Lily `0x21e148` (2220360), Please, Touch The
Artwork 2 `0x27accc` (2600140), An Arcade Full of Cats `0x2423d6`
(2368470). **Matches.** The other five count neither way: four are
32-bit games (Katawa Shoujo and SCP loaded `steamclient.dll`, which the
64-bit proxy does not replace; Travellin Cats in Paris and Cube Escape
ship only a 32-bit `steam_api.dll`), and Higurashi waited for a
language choice before loading Steam.

The maintainer gave the OK on 2026-11-21 (ADR 0044): position 9 is
confirmed as `GetAppID` for `SteamUtils010`. For `SteamUtils009` it
stays a candidate: no `SteamUtils009` game was part of experiment 3.

Evidence (excerpt; floating-point registers and the `rcx` note left
out; one game per pair of lines, in the order above):

```text
63 ms ThreadId(1) SteamUtils010#9 call 1 rcx=0x2705a7802e0 rdx=0x7ffdaf9041d0 r8=0x2707b621500 r9=0xd
63 ms ThreadId(1) SteamUtils010#9 return 1 rax=0x21e148
61 ms ThreadId(1) SteamUtils010#9 call 1 rcx=0x1d7c26702e0 rdx=0x7ffdaf9041d0 r8=0x1d535b872c0 r9=0xd
61 ms ThreadId(1) SteamUtils010#9 return 1 rax=0x27accc
61 ms ThreadId(1) SteamUtils010#9 call 1 rcx=0x14245f202e0 rdx=0x0 r8=0x14251867ec0 r9=0xc
61 ms ThreadId(1) SteamUtils010#9 return 1 rax=0x2423d6
```

**Further run** (not part of experiment 3, which did not list it):
Gravitas (1067310) asked for `SteamUtils009`, like Cave Crawlers, and
position 9 returned `0x10492e` (1067310), its app ID. It changes no
status.

```text
64 ms ThreadId(1) SteamUtils009#9 call 1 rcx=0x265b1df02e0 rdx=0x7ffdaef341d0 r8=0x265ad201680 r9=0xd
64 ms ThreadId(1) SteamUtils009#9 return 1 rax=0x10492e
```

## Positions most games share: further candidates

Read 2026-11-21 from the pages linked below; recordings from the 14
games in `coverage.md`. None of these is confirmed; each needs an
experiment, and the open question is written down with it.

### `SteamController007` and `SteamController008` position 2: candidate `RunFrame`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamController>;
  its constants name `SteamController007` (Cave Crawlers, Outcore). Ten
  games asked for `SteamController008`, which the page does not name.
- **Recorded:** called about as often as the game draws frames (50 to 66
  logged calls within the first seconds, then every 10 000th), always
  with `rdx` = 0; the result register held 0 or the same leftover
  address in every call of a game, which fits a function without a
  result.
- **Fit:** the page documents exactly one function without arguments
  and without a result: it brings the interface up to date with the
  latest controller input, optional for lower latency.
- **Open:** a function without arguments or result leaves little to
  predict. An experiment would need a visible effect, for example
  controller input arriving only when it runs; not planned yet.

### `SteamInput006` position 3: candidate `RunFrame`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamInput>; its
  constants name `SteamInput007`. Ten games asked for `SteamInput006`
  (and two for `SteamInput001`), which the page does not name.
- **Recorded:** called about as often as frames are drawn, `rdx` 0 or 1
  (the first call of Baldi's Basics had 1), result register 0 or a
  leftover address.
- **Fit:** the page documents a function to call every frame that takes
  one reserved true/false argument and returns nothing.
- **Open:** as for `SteamController`.

### `SteamUser020` to `SteamUser023` position 2: candidate `GetSteamID`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamUser>; its
  constants name `SteamUser019`, which no recorded game asked for. All
  14 games called position 2 of `SteamUser020`, `021`, `022` or `023`,
  once at start.
- **Recorded:** `rdx` held an address on the game's stack and the result
  was that same address.
- **Fit:** the page documents two functions without arguments that
  return something other than a number or true/false: the account's
  Steam ID, and a user handle. Microsoft's x64 calling convention
  returns a value that does not fit into a register through an address
  the caller passes and gives that address back as the result, which is
  what was recorded.
- **Open:** confirming it means reading what the client wrote to that
  address, which would be the account's Steam ID. ADR 0043 does not
  allow a read entry for a call that could hand back an account
  identifier; that needs its own ADR first.

### Shadow mode for `GetAppID` (ADR 0046)

Three agent runs on 2026-11-21 with `reborn-observe arm --app <id>`:
Linsips, Paper Lily - Chapter 1 and Chronigma. Every logged call at
position 9 of `SteamUtils010` gave `shadow match`: the official client's
result and Reborn's answer (the app ID Reborn started the game with)
were the same. The games ran normally and closed on a close request.
Under ADR 0046 this is the three-game condition for answer mode; answer
mode also needs the maintainer's OK.

```text
79 ms ThreadId(1) SteamUtils010#9 return 1 shadow match: client 4301370, Reborn 4301370
67 ms ThreadId(1) SteamUtils010#9 return 1 shadow match: client 2220360, Reborn 2220360
60 ms ThreadId(1) SteamUtils010#9 return 1 shadow match: client 4051150, Reborn 4051150
```

### Experiment 4: answer mode for `GetAppID` (planned 2026-11-21, before the runs)

The maintainer gave the OK for answer mode (ADR 0046). In these runs the
proxy does not forward position 9 of `SteamUtils010` to the official
client; the game gets Reborn's answer, the app ID given to
`reborn-observe arm --app`. Every other call goes to the official client
as before.

Games: Baldi's Basics Classic Remastered (1712830), Plant Therapy
(2505120) and An Arcade Full of Cats (2368470), each already recorded in
a normal stage 3 run.

**Prediction:** in each game, (1) the log shows position 9 answered by
Reborn with the game's app ID, (2) the game loads the proxy, reaches the
same first screen and closes on a close request as in its stage 3 run,
and (3) the set of interface versions and positions it calls in the
first run seconds is the same as in its stage 3 run. A game that
behaves differently is a mismatch.

**Result of experiment 4** (agent runs, 14:28 to 14:33, after the plan
was committed at 14:27:34):

- (1) **Matches** in all three: position 9 was answered by Reborn and
  returned the game's app ID (`0x1a22be`, `0x2639a0`, `0x2423d6`).
- (2) **Matches** in all three: each game loaded the proxy, reached its
  first screen and closed on a close request.
- (3) **Matches** for Plant Therapy only. Baldi's Basics also called
  position 6 of `STEAMUSERSTATS_INTERFACE_VERSION012` (at 443 ms, with
  the text `ACH_NULL`), and An Arcade Full of Cats also called positions
  23 and 28 of it (at about 30 seconds); neither had these calls in its
  stage 3 run. **Does not match.**

Whether answering `GetAppID` caused the extra calls or whether they vary
from run to run anyway (they come later, and stats arrive from the
server asynchronously) cannot be told from one normal run per game; the
prediction did not allow for run-to-run differences. Answer mode for
`GetAppID` is therefore **not confirmed** as harmless; it stays in the
proxy's list only for planned experiments.

```text
62 ms ThreadId(1) SteamUtils010#9 call 1 answered by Reborn, not forwarded
62 ms ThreadId(1) SteamUtils010#9 return 1 rax=0x1a22be
64 ms ThreadId(1) SteamUtils010#9 call 1 answered by Reborn, not forwarded
64 ms ThreadId(1) SteamUtils010#9 return 1 rax=0x2639a0
62 ms ThreadId(1) SteamUtils010#9 call 1 answered by Reborn, not forwarded
62 ms ThreadId(1) SteamUtils010#9 return 1 rax=0x2423d6
```

### Experiment 5 (planned 2026-11-21, before the runs): how much normal runs vary

Baldi's Basics and An Arcade Full of Cats, two normal runs each (no
`--app`, so no answer and no shadow), then two answer-mode runs each,
all with the same waiting time (30 seconds after the `[S_API]` line).

**Prediction:** if answering `GetAppID` does not change what the game
does, then every interface version and position called in an
answer-mode run is also called in at least one of the three runs of
that game without answer mode (its two new normal runs and its stage 3
run; the experiment 4 runs are not counted), and the other way round for the
positions called within the first 2 seconds. A position seen only in
answer-mode runs, or a position within the first 2 seconds missing from
them, is a mismatch.

**Result of experiment 5** (agent runs, after the plan and its wording
fix were committed):

- All eight runs loaded the proxy, reached the first screen and closed
  normally. In the four answer-mode runs position 9 was answered by
  Reborn (twice per run).
- Baldi's Basics: no position appeared only in an answer-mode run, and
  none of the positions called in the first 2 seconds of the normal runs
  was missing from them. **Matches.** The two new normal runs called
  positions 0 and 6 of `STEAMUSERSTATS_INTERFACE_VERSION012`, which the
  stage 3 run had not: the calls that made experiment 4 mismatch also
  vary between normal runs.
- An Arcade Full of Cats: the same. **Matches.** The extra positions 23
  and 28 of experiment 4 (at about 30 seconds) appeared in none of the
  eight runs.

Answer mode for `GetAppID` matched experiment 5 in both games, which
with Plant Therapy in experiment 4 makes three games. The maintainer
gave the OK on 2026-11-21 (ADR 0044): answering `GetAppID` at position 9
of `SteamUtils010` with the app ID Reborn started the game with is
confirmed for the shim.

### `SteamUtils010` position 14: not known

- **Recorded:** called about as often as frames are drawn; the first
  call of each game had an address in `r8`, later ones `r8` = 1; the
  result was `0xffffffff` every time in all games.
- No candidate: none of the functions on the `ISteamUtils` page that
  take no or simple arguments obviously gives the same 32-bit value
  every frame in every game.

## `SteamClient020`, `SteamClient021` and `SteamClient023`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamClient>, read
  2026-11-22. It opened without signing in. Unlike the other pages it
  names no version constant, so which `SteamClient` version it describes
  is not known; every entry here is for the version text recorded, and
  positions come only from `objects.md` and the raw logs.
- **Recording:** the 20 games of `coverage.md`; the examples below are
  from Cave Crawlers (020), Baldi's Basics (020) and Cartapli (023).
- In all calls `rcx` held the object. Handles appear in the low 32 bits
  of `rdx`, `r8` and `r9`; the upper 32 bits were often leftovers
  (`0xcf00000001`), so only the low 32 bits are compared below.

### Position 0: candidate `CreateSteamPipe`

- **Recorded:** the first call through every `SteamClient` object, in
  every game; `rdx` 0, `r9` a small number; the result was 1.
- **Fit:** the page documents a function without arguments that opens a
  communication pipe to the client and returns its handle.

### Position 2: candidate `ConnectToGlobalUser`

- **Recorded:** the second call; `rdx` 1 (the result of position 0),
  `r8` an address on the stack; the result was 1.
- **Fit:** the page documents a function that takes a pipe handle and
  returns the handle of the user signed in to the client.

### Per-interface positions and position 12: candidates `GetISteam…` and `GetISteamGenericInterface`

- **Recorded:** `rdx` 1 or 0, `r8` 1, `r9` an interface version text
  (`objects.md`); the result an object. Position 9 differs: `rdx` 1 and
  the text in `r8`. Position 12 took every kind of text; the positions
  of their own took one kind each (for example `STEAMHTTP_…` only at 24
  of 020 and 021, 23 of 023).
- **Fit:** the page documents one function per interface that takes a
  user handle, a pipe handle and a version text and returns that
  interface; one generic function with the same arguments for any
  interface; and one for `ISteamUtils` that takes only a pipe handle and
  a version text. The two-argument shape matches position 9, where only
  `SteamUtils…` texts were passed. Which documented function belongs to
  which position number is not taken from the page; it follows only
  from which texts a position was called with.

### At exit (Cave Crawlers, `SteamClient020`): positions 4, 1, 23

- **Recorded:** right before the game exited, in this order: position 4
  with `rdx` 1, `r8` 1 (result register leftover `0x13`); position 1
  with `rdx` 1 (result low byte 1); position 23 with `rdx` 0 (result 1).
- **Fit (candidates):** position 4 `ReleaseUser` (takes a pipe and a user
  handle, returns nothing); position 1 `BReleaseSteamPipe` (takes a pipe
  handle, returns true or false); position 23
  `BShutdownIfAllPipesClosed` (no arguments, returns true or false).

### Position 34 (020) and 30 (023): candidate `SetWarningMessageHook`

- **Recorded:** once at start; `rdx` an address in the range where
  Windows loads program modules (for example `0x7ffe4c783820`); result
  0.
- **Fit:** the page documents a function that takes a pointer to a
  function and returns nothing. Not part of experiment 7: the proxy
  does not check whether the address is code.

### Experiment 7 (planned 2026-11-22, before the runs)

Four agent runs (ADR 0044) with the step 4c proxy and `GetAppID`
answered by Reborn as usual: Plant Therapy (020), Outcore (020),
Chronigma (021) and Cartapli (023). Each game runs to its first screen
and is closed by a close request.

**Predictions from the page**, on the low 32 bits, for each game:

1. The first call through the `SteamClient` object is position 0; call
   its result `P`.
2. The next call is position 2 with `rdx` = `P`; call its result `U`.
3. Every call of position 9 has `rdx` = `P` and a `SteamUtils…` text in
   `r8`.
4. Every call of position 12 and of the per-interface positions has
   `r8` = `P` and `rdx` = `U` or 0.
5. For the `SteamClient020` games, if the game calls positions 4, 1 and
   23 when it closes: position 4 has `rdx` = `P` and `r8` = `U`,
   position 1 has `rdx` = `P`, and position 23 comes after position 1.
   A game that does not call them at close counts neither way for this
   point.

A single call that breaks 1 to 4, or a call at close that breaks 5, is a
mismatch for that position.

**Result of experiment 7** (agent runs after the plan was committed at
02:02:33; checked with a small script over the raw logs):

- Plant Therapy (020), Outcore (020), Chronigma (021), Cartapli (023):
  points 1 to 4 **match** in all four, without a single mismatch. Point
  5 **matches** in both `SteamClient020` games: each called positions 4,
  1 and 23 at close, in that order, with the predicted handles.
- Outcore first waited in the official client's launch-option window;
  the agent chose the default option and clicked Play (ADR 0049).
- **Limit:** in all four games both handles were 1 (`P` = `U` = 1), so
  the runs cannot tell the pipe handle from the user handle; points 2 to
  5 would also have matched with the two swapped. Telling them apart
  needs a run in which the two differ, which has not been seen.

```text
observe-50924.log SteamClient020: P=1 U=1 | 1: match | 2: match | 3: 1 calls | 4: 31 calls | mismatches: 0
    5: close calls [4, 1, 23] -> match
observe-47724.log SteamClient020: P=1 U=1 | 1: match | 2: match | 3: 0 calls | 4: 26 calls | mismatches: 0
    5: close calls [4, 1, 23] -> match
observe-11304.log SteamClient021: P=1 U=1 | 1: match | 2: match | 3: 1 calls | 4: 31 calls | mismatches: 0
observe-34580.log SteamClient023: P=1 U=1 | 1: match | 2: match | 3: 1 calls | 4: 29 calls | mismatches: 0
```

Evidence (Plant Therapy; floating-point registers and the `rcx` note
left out):

```text
16 ms ThreadId(1) SteamClient020#0 call 1 rcx=0x7ffceac307f0 rdx=0x0 r8=0x252f5714bc0 r9=0xe
55 ms ThreadId(1) SteamClient020#0 return 1 rax=0x1
55 ms ThreadId(1) SteamClient020#2 call 1 rcx=0x7ffceac307f0 rdx=0x1 r8=0x7ffdb1f1efc0 r9=0x1c
56 ms ThreadId(1) SteamClient020#2 return 1 rax=0x1
90305 ms ThreadId(1) SteamClient020#4 call 1 rcx=0x7ffceac307f0 rdx=0x1 r8=0x1 r9=0x255618547b0
90327 ms ThreadId(1) SteamClient020#4 return 1 rax=0x19
90327 ms ThreadId(1) SteamClient020#1 call 1 rcx=0x7ffceac307f0 rdx=0x1 r8=0x252f5711bc0 r9=0x1c
90327 ms ThreadId(1) SteamClient020#1 return 1 rax=0xffffff01
90327 ms ThreadId(1) SteamClient020#23 call 1 rcx=0x7ffceac307f0 rdx=0x0 r8=0x252f5711bc0 r9=0x1c
90327 ms ThreadId(1) SteamClient020#23 return 1 rax=0x1
```

Status: positions 0 (`CreateSteamPipe`), 2 (`ConnectToGlobalUser`), 9
(`GetISteamUtils`), 12 (`GetISteamGenericInterface`) and the
per-interface `GetISteam…` positions for `SteamClient020`, `021` and
`023`, and positions 4, 1, 23 (`ReleaseUser`, `BReleaseSteamPipe`,
`BShutdownIfAllPipesClosed`) for `SteamClient020`, matched experiment 7;
the maintainer gave the OK on 2026-11-22 (ADR 0044), so they are
confirmed for these versions. Which handle is the pipe
and which the user stays open (limit above).

## `STEAMAPPS_INTERFACE_VERSION008`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamApps>, read
  2026-11-22; its constants name `STEAMAPPS_INTERFACE_VERSION008`, the
  version the recorded games asked for.
- **Recording:** the games of `coverage.md` that called it: positions 7
  (Cave Crawlers, Please, Touch The Artwork 2, Outcore, Plant Therapy), 6
  (Plant Therapy), 4 (Linsips, Fallout Shelter) and 0 (An Arcade Full of
  Cats, Gravitas, Dark Deception, A Park Full of Cats).
- These are ownership calls. Under ADR 0046 their answers always come
  from Valve's servers through `rebornd`, never from the proxy or the
  shim on their own; a meaning here only says what is asked.

### Position 7: candidates `BIsDlcInstalled`, `BIsSubscribedApp`

- **Recorded:** `rdx` an app ID that is not the game's own: 2444550 in
  Cave Crawlers, 2544540 in Plant Therapy. Their store pages, read
  2026-11-22, say they are DLC of exactly these games ("Cave Crawlers:
  Expedition Pass", "Plant Therapy: Queens"). The result was 0. The test
  account owns no DLC (`reborn-cli library --all`, 2026-11-22).
- **Fit:** the page documents functions that take one app ID and return
  true or false: whether a DLC is owned and installed, whether the user
  is subscribed to an app, and whether an app is installed. For a DLC
  that is neither owned nor installed all three give false; telling the
  first two apart needs a DLC that is owned but not installed, which the
  test account does not have. `BIsAppInstalled` fits less well, since
  the game asks about its own DLC.

### Position 6: candidates `BIsSubscribedApp`, `BIsAppInstalled`

- **Recorded:** once in Plant Therapy, `rdx` 2505120, the game's own app
  ID; result 1.
- **Fit:** the same three functions; for the game itself, owned and
  installed, "subscribed" and "installed" both give true, while "DLC
  owned and installed" makes little sense for a base game.

### Position 0: candidate `BIsSubscribed`

- **Recorded:** once at start; `rdx` 1, `r8` a value that is not an
  address (`0x7ffffffffffffffc`); result 1.
- **Fit:** the page documents several functions without arguments that
  return true or false. For a game the account owns permanently, only
  "subscribed to the current app" is expected to be true; "from a free
  weekend", "from family sharing", "cyber cafe", "low violence" and "VAC
  banned" are expected to be false for the test account.
- **Shadow mode** (ADR 0046 as amended by ADR 0055, stage 1; added
  2026-11-22, not run yet): the proxy's `SHADOWS` lists position 0 as
  `Shadow::Subscribed`. After the official client's call returns, a
  worker thread in the proxy asks `rebornd` (`AppOwnership` for the app
  given to `reborn-observe arm --app`, on the read-only shadow session
  `arm` registered) and compares the answer with the lowest byte of
  `rax`. The log line says only `shadow match` or `shadow mismatch`
  (account data), or `shadow: no Reborn answer (<reason>)`; the call's
  own return line then says `rax hidden (account data)` instead of the
  value, so the two lines together do not give Reborn's answer. The game
  always gets the official client's result; position 0 is never in
  `ANSWERS`. Position 7 is not shadowed: it is narrowed to two meanings,
  not confirmed as one, and the shadow session knows no DLC yet.

### Position 4: candidates `GetCurrentGameLanguage`, `GetAvailableGameLanguages`

- **Recorded:** once at start in Linsips and Fallout Shelter; result an
  address inside `steamclient64.dll`'s range.
- **Fit:** the page documents two functions without arguments that
  return text: the language the user set, and the comma-separated list
  of the game's languages. Telling them apart needs the text at the
  result (ADR 0043); not planned yet.

### Experiment 8 (planned 2026-11-22, before the runs)

Agent runs (ADR 0044) of Cave Crawlers and Plant Therapy (position 7),
and of An Arcade Full of Cats, Dark Deception and A Park Full of Cats
(position 0), each to the first screen.

**Predictions:**

1. Every call of position 7 has in `rdx` an app ID whose store page says
   it is DLC of that game, and the result is 0 (the account owns no DLC).
2. Every call of position 0 returns 1 in its lowest byte.

A call that breaks 1 or 2 is a mismatch.

**Result of experiment 8** (agent runs after the plan was committed at
02:34:20):

- (1) **Matches.** Cave Crawlers asked about 2444550 again (result 0).
  Plant Therapy went through eight app IDs again and again (2544540,
  2664530, 2783580, 2997420, 3078370, 3308530, 3406390, 3761200; every
  logged call returned 0), and A Park Full of Cats asked twice about
  2673390 (result 0). The store page of each of these nine IDs, read one
  at a time on 2026-11-22, says it is DLC of exactly the game that asked
  (for example "Plant Therapy: Cat Art Pack", "A Park Full of Cats:
  Haunted Ride").
- (2) **Matches.** Position 0 returned 1 in An Arcade Full of Cats, Dark
  Deception and A Park Full of Cats.

What the experiment shows is limited by the account: it cannot tell
"DLC owned and installed" from "subscribed to the app" at position 7,
and it rules the other no-argument functions out at position 0 only by
their expected value for this account. Both stay candidates in that
sense; position 7 is narrowed to two functions and position 0 to one; the
maintainer gave the OK on 2026-11-22 (ADR 0044): position 0 is confirmed
as `BIsSubscribed`, position 7 as one of `BIsDlcInstalled` and
`BIsSubscribedApp` (the same answer for every DLC the account does not
own).

Evidence (excerpt; floating-point registers and the `rcx` note left
out; Cave Crawlers, Plant Therapy, Dark Deception):

```text
1168 ms ThreadId(1) STEAMAPPS_INTERFACE_VERSION008#7 call 1 rcx=0x29e9db90520 rdx=0x254d06 r8=0x29d5718d8c0 r9=0xd
1168 ms ThreadId(1) STEAMAPPS_INTERFACE_VERSION008#7 return 1 rax=0x0
98 ms ThreadId(1) STEAMAPPS_INTERFACE_VERSION008#7 call 2 rcx=0x1a1e0620a60 rdx=0x28a852 r8=0x19f169acf00 r9=0x3c
98 ms ThreadId(1) STEAMAPPS_INTERFACE_VERSION008#7 return 2 rax=0x0
77 ms ThreadId(1) STEAMAPPS_INTERFACE_VERSION008#0 call 1 rcx=0x1c27dc40a40 rdx=0x1 r8=0x7ffffffffffffffc r9=0x13
77 ms ThreadId(1) STEAMAPPS_INTERFACE_VERSION008#0 return 1 rax=0x1
```

## `SteamUtils010` positions 5 and 6: candidates `GetImageSize`, `GetImageRGBA`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamUtils>, read
  again 2026-11-22 (it names `SteamUtils009`; candidate only for 010, as
  for `GetAppID`).
- **Recorded:** Please, Touch The Artwork 2, stage 3 run: four pairs, each
  first position 5 then position 6 with the same small number in `rdx`
  (3, 3, 11, 10). Position 5 got two stack addresses 8 bytes apart in
  `r8` and `r9`; position 6 got a heap address in `r8` and `0x21100` =
  135424 in `r9`; both returned 1.
- **Fit:** the page documents a function that takes an image handle and
  two places for width and height and reports success, and one that
  takes the same handle, a buffer and the buffer's size and fills it
  with the image as RGBA; the page says to size the buffer as width ×
  height × 4. 135424 = 184 × 184 × 4.

### Experiment 9 (planned 2026-11-22, before the runs)

Two agent runs of Please, Touch The Artwork 2 to its first screen.

**Predictions:**

1. Every call of position 6 follows a call of position 5 with the same
   `rdx` (low 32 bits), with no other position 5 or 6 call in between.
2. The size in `r9` of every position 6 call is a multiple of 4, and a
   quarter of it is a square number (a square image, as an avatar or an
   icon is; a non-square image would not break the meaning, only this
   prediction, and is noted).
3. Both return 1 in their lowest byte.

**Result of experiment 9** (two agent runs after the plan was committed
at 02:57:11):

- (1) **Matches.** In both runs four pairs, each position 5 then position
  6 with the same handle (3, 10, 3, 11), nothing in between.
- (2) **Matches.** Every position 6 call had `r9` = 135424 = 4 × 184².
- (3) **Matches.** All 16 calls returned 1.

Positions 5 and 6 of `SteamUtils010` matched `GetImageSize` and
`GetImageRGBA`; the maintainer gave the OK on 2026-11-22 (ADR 0044), so
they are confirmed for `SteamUtils010`. Only one game
called them; whether the handles name avatars or other images is not
known (the handles come from other interfaces, not covered here).

Evidence (excerpt; floating-point registers and the `rcx` note left
out):

```text
2055 ms ThreadId(1) SteamUtils010#5 call 1 rcx=0x26739b502e0 rdx=0x3 r8=0xf1ee58df40 r9=0xf1ee58df38
2055 ms ThreadId(1) SteamUtils010#5 return 1 rax=0x1
2055 ms ThreadId(1) SteamUtils010#6 call 1 rcx=0x26739b502e0 rdx=0x3 r8=0x26738c10020 r9=0x21100
2056 ms ThreadId(1) SteamUtils010#6 return 1 rax=0x1
```

## `SteamUtils009` and `SteamUtils010` position 14: candidate `GetCurrentBatteryPower`

- **Page:** <https://partner.steamgames.com/doc/api/ISteamUtils>, read
  2026-11-22 (names `SteamUtils009`; candidate only for 010).
- **Recorded:** 14 of the 20 games call position 14 of `SteamUtils010`,
  four call it on `SteamUtils009`, about as often as frames are drawn.
  The first call had an address in `r8`, later ones 1; the result was
  `0xffffffff` every time, in every game, on the maintainer's desktop
  computer.
- **Fit:** of the functions without arguments on the page, one returns a
  single byte: the battery level in percent, or 255 when the computer is
  on AC power. The others return times and counts that change, an app
  ID, small enum values, or true/false; none would give a lowest byte of
  255 in every call of every game.

### Experiment 10 (planned 2026-11-22, before the runs)

Agent runs of Linsips, Chronigma and Cartapli (`SteamUtils010`) and of
Outcore (`SteamUtils009`) to the first screen, on the same desktop
computer, which runs on AC power.

**Prediction:** every logged call of position 14 returns 255 in its
lowest byte. Checked by `tools/shim/experiments/010_battery_power.py`.

**Limit, known in advance:** a desktop on AC power cannot show the
battery case (0 to 100); only a laptop on battery could.

**Result of experiment 10** (agent runs after the plan was committed at
04:03:12): Linsips, Chronigma and Cartapli **match**: every logged call of
position 14 returned 255 in its lowest byte. Outcore was not run (its
launch-option window was answered too late, the run timed out), so
`SteamUtils009` position 14 stays a candidate. Under ADR 0051 position 14
of `SteamUtils010` is confirmed as `GetCurrentBatteryPower`, with the
known limit that only the AC-power case was observed. The review pass
noted that this prediction repeats the observation that led to the
candidate (it could only have failed if the behaviour changed); the
meaning rests mainly on the elimination of the other functions on the
page. A run on a laptop on battery would make it a real test.

```text
observe-46356.log: 108 logged calls, 0 not 255 -> match
observe-18812.log: 64 logged calls, 0 not 255 -> match
observe-34072.log: 66 logged calls, 0 not 255 -> match
experiment 10: match
```

```text
78 ms ThreadId(1) SteamUtils010#14 call 1 rcx=0x19f0f8f02e0 rdx=0x0 r8=0x19f0bee1500 r9=0x1a
79 ms ThreadId(1) SteamUtils010#14 return 1 rax=0xffffffff
```

## `STEAMUSERSTATS_INTERFACE_VERSION011/012` position 0: candidates `RequestCurrentStats`, `StoreStats`

- **Recorded:** once early in eleven games (65 to 2339 ms after start),
  with leftovers in `rdx`, `r8`, `r9` that differ between games; the
  result was 1 every time.
- **Fit:** the `ISteamUserStats` page (read 2026-11-21, names version
  011) documents two functions without arguments that report success as
  true or false: one asks for the current user's stats and achievements,
  one sends changed stats to the server. A call at start, before any
  stat is set, fits the first better, but nothing recorded so far tells
  them apart; telling them apart probably needs the callbacks of ADR 0050.
  No experiment planned yet.

## `SteamClient021` and `SteamClient023`: the calls at close

- **Recorded** (the reference runs of `coverage.md`): every `SteamClient021`
  game called positions 4, 1 and 23 when it closed, every
  `SteamClient023` game positions 4, 1 and 22, in that order, with the
  same handles as the `SteamClient020` games (experiment 7). Position 32
  (021) and 30 (023) took a function address at start, like 34 in 020.
- **Fit:** the same functions as for `SteamClient020`: `ReleaseUser`,
  `BReleaseSteamPipe`, `BShutdownIfAllPipesClosed`
  (<https://partner.steamgames.com/doc/api/ISteamClient>); 32 and 30 are
  candidates for `SetWarningMessageHook`, not tested.

### Experiment 11 (planned 2026-11-22, before the runs)

Agent runs of Chronigma and Fallout Shelter (021) and Linsips and
Cartapli (023), each closed by a close request.

**Prediction:** at close, position 4 with `rdx` = `P` and `r8` = `U`, then
position 1 with `rdx` = `P` returning true, then position 23 (021) or 22
(023) returning true (`P`, `U` as in experiment 7). Checked by
`tools/shim/experiments/011_close_calls.py`.

**Result of experiment 11** (agent runs after the plan was committed at
04:33:39): **match** in all four games; checked by the script.

```text
observe-35536.log SteamClient021: P=1 U=1 close calls [4, 1, 23] -> match
observe-45648.log SteamClient021: P=1 U=1 close calls [4, 1, 23] -> match
observe-21344.log SteamClient023: P=1 U=1 close calls [4, 1, 22] -> match
observe-11536.log SteamClient023: P=1 U=1 close calls [4, 1, 22] -> match
experiment 11: match
```

The logs: `observe-35536.log` Chronigma, `observe-45648.log` Fallout
Shelter (both 021), `observe-21344.log` Linsips, `observe-11536.log`
Cartapli (both 023); Windows 11, 2026-11-22, step 4c proxy with
`GetAppID` answered by Reborn; recorded by the agent (ADR 0044).

Evidence (Linsips, the close calls; floating-point registers and the
`rcx` note left out):

```text
88945 ms ThreadId(1) SteamClient023#4 call 1 rcx=0x7ffceac30808 rdx=0x1 r8=0x1 r9=0x1
88945 ms ThreadId(1) SteamClient023#4 return 1 rax=0x5
88945 ms ThreadId(1) SteamClient023#1 call 1 rcx=0x7ffceac30808 rdx=0x1 r8=0x27ee0a23c00 r9=0x1c
88945 ms ThreadId(1) SteamClient023#1 return 1 rax=0xffffff01
88945 ms ThreadId(1) SteamClient023#22 call 1 rcx=0x7ffceac30808 rdx=0x0 r8=0x27ee0a23c00 r9=0x1c
88945 ms ThreadId(1) SteamClient023#22 return 1 rax=0x1
```

Under ADR 0051, positions 4, 1 and 23 of `SteamClient021` and 4, 1 and
22 of `SteamClient023` are confirmed as `ReleaseUser`,
`BReleaseSteamPipe` and `BShutdownIfAllPipesClosed`.

## More candidates from the gap list (loop iteration 3)

Pages: `ISteamUserStats` (read 2026-11-21, `RequestUserStats` checked
again 2026-11-22), `ISteamUser` (2026-11-21),
<https://partner.steamgames.com/doc/api/ISteamFriends> (2026-11-22),
`ISteamInput` (2026-11-22). Correction made before the runs: the first
version of this list named pages that had not been re-read for these
entries; `RequestUserStats` and the `ISteamFriends` entry were then
checked on their pages, the other entries rest on the pages as read on
the dates given. Each entry
is a candidate unless an experiment below confirms it.

- **`STEAMUSERSTATS_INTERFACE_VERSION012` positions 0 and 10:** position 0
  once early, position 10 once when the game closed (An Arcade Full of
  Cats, One's Own Exile, A Park Full of Cats), both returning 1 with
  leftovers in the registers. The page documents two functions without
  arguments that return true or false: asking for the current stats, and
  sending changed stats to the server. Candidates: 0 `RequestCurrentStats`,
  10 `StoreStats`.
- **`STEAMUSERSTATS_INTERFACE_VERSION013` position 15:** once early in
  three games; `rdx` held the test account's own Steam ID (the value is
  left out here, ADR 0050), the result looked like a random 64-bit
  number. The page documents a function that takes a Steam ID and returns
  a call handle for an asynchronous request of that user's stats.
  Candidate: `RequestUserStats`. Not tested: confirming the handle needs
  `Steam_GetAPICallResult` and the callbacks.
- **`SteamInput006` position 0:** once at start, before any other
  `SteamInput006` call, `rdx` 0 or 1, result 1. The page names an
  initialization function that takes one true/false argument ("call
  RunFrame yourself") and returns true or false. Candidate: `Init`.
- **`SteamInput006` position 6:** every frame in three games; `rdx` an
  address, result 0. The page documents a function that fills a buffer
  with the handles of connected controllers and returns how many; no
  controller was connected. Candidate: `GetConnectedControllers`.
- **`SteamInput001` position 2:** every frame in four games, `rdx` an
  address, the result register a leftover. Only a guess from the shape:
  a per-frame function like `RunFrame` of `SteamInput006` position 3.
  `SteamInput001` is an older version than the one the `ISteamInput` page
  describes, and the address in `rdx` is not explained by that; no page
  backs this entry yet.
- **`SteamUser023` position 1:** once early, no argument used, result 1.
  The `ISteamUser` page documents several functions without arguments
  that return true or false; whether the client is connected to Steam is
  the one a game would ask first. Candidate: `BLoggedOn` (also
  `BIsBehindNAT` and the phone and two-factor checks fit the shape).
- **`SteamFriends017` position 0:** once early, no argument used, result
  an address in the heap. The `ISteamFriends` page documents one function
  without arguments that returns text: the user's display name.
  Candidate: `GetPersonaName`. That text is account data, so it is not
  read (ADR 0043).

### Experiment 12 (planned 2026-11-22, before the runs)

Agent runs of An Arcade Full of Cats, One's Own Exile and A Park Full of
Cats (UserStats012) and of Baldi's Basics, Chronigma and HoloCure
(`SteamInput006`), each closed by a close request after about 30
seconds.

**Predictions** (`tools/shim/experiments/012_stats_and_input_init.py`):

- A: position 0 of `STEAMUSERSTATS_INTERFACE_VERSION012` is called once,
  within the first 3 seconds, returning true; position 10 once, at least
  20 seconds later (at close), returning true.
- B: position 0 of `SteamInput006` is the first `SteamInput006` call, is
  called once and returns true.

**Result of experiment 12** (agent runs after the plan was committed;
logs: `observe-33776` An Arcade Full of Cats, `observe-612` One's Own
Exile, `observe-50224` A Park Full of Cats, `observe-46700` Baldi's
Basics, `observe-43372` Chronigma, `observe-8424` HoloCure):

```text
observe-33776.log A: position 0 at [63] ms, position 10 at [88250] ms -> match
observe-33776.log B: first SteamInput006 call is position 3, 0 position-0 calls -> MISMATCH
observe-612.log A: position 0 at [63] ms, position 10 at [87666] ms -> match
observe-50224.log A: position 0 at [63] ms, position 10 at [87886] ms -> match
observe-50224.log B: first SteamInput006 call is position 3, 0 position-0 calls -> MISMATCH
observe-46700.log A: position 0 at [144] ms, position 10 at [] ms -> MISMATCH
observe-46700.log B: first SteamInput006 call is position 0, 1 position-0 calls -> match
observe-43372.log B: first SteamInput006 call is position 0, 1 position-0 calls -> match
observe-8424.log A: position 0 at [63] ms, position 10 at [] ms -> MISMATCH
observe-8424.log B: first SteamInput006 call is position 0, 1 position-0 calls -> match
experiment 12: MISMATCH
```

The games were closed about 86 to 89 seconds after the proxy started,
not after about 30 seconds as planned (the waits of `run.sh` add up);
no verdict depends on that.

The script as committed reports **MISMATCH**, so nothing is confirmed
(ADR 0051 needs a script-checked match). Why, without changing the
script afterwards:

- In the games each prediction named, both matched: A in An Arcade Full
  of Cats, One's Own Exile and A Park Full of Cats; B in Baldi's Basics,
  Chronigma and HoloCure.
- The script applied both predictions to every log. Outside the named
  games: Baldi's Basics and HoloCure called position 0 of
  `STEAMUSERSTATS_INTERFACE_VERSION012` but not position 10 at close (a
  game that changed no stat may well not store them), and An Arcade Full
  of Cats and A Park Full of Cats used `SteamInput006` without ever
  calling position 0 (`Init` is not needed by every game, or happens
  elsewhere).
- Lesson for the loop: the check script must encode which games each
  prediction is about. A new experiment with such a script is needed to
  confirm these positions; until then they stay candidates.

### Experiment 13 (planned 2026-11-22, before the runs)

Experiment 12 again, with the lesson from its result: each prediction is
checked only in the games it names, and a named game whose log lacks the
interface counts as a mismatch. Agent runs of An Arcade Full of Cats,
One's Own Exile and A Park Full of Cats (part A) and of Baldi's Basics
Classic Remastered, Chronigma and HoloCure (part B), each closed by a
close request after about 30 seconds or more.

**Predictions** (`tools/shim/experiments/013_stats_and_input_init_scoped.py`,
each log given as `A:<log>` or `B:<log>`):

- A: position 0 of `STEAMUSERSTATS_INTERFACE_VERSION012` is called once,
  within the first 3 seconds, returning true; position 10 once, at least
  20 seconds later (at close), returning true. Candidates: 0
  `RequestCurrentStats`, 10 `StoreStats`.
- B: position 0 of `SteamInput006` is the first `SteamInput006` call, is
  called once and returns true. Candidate: `Init`.

The experiment matches only with three logs per part, all matching.

**Result of experiment 13** (agent runs after the plan was committed at
07:46:59; logs: `observe-42400` An Arcade Full of Cats, `observe-47612`
One's Own Exile, `observe-28456` A Park Full of Cats, `observe-50248`
Baldi's Basics, `observe-51752` Chronigma, `observe-2896` HoloCure):
**match**, checked by the script.

```text
observe-42400.log A: position 0 at [47] ms, position 10 at [87929] ms -> match
observe-47612.log A: position 0 at [46] ms, position 10 at [88220] ms -> match
observe-28456.log A: position 0 at [46] ms, position 10 at [87758] ms -> match
observe-50248.log B: first SteamInput006 call is position 0, 1 position-0 calls -> match
observe-51752.log B: first SteamInput006 call is position 0, 1 position-0 calls -> match
observe-2896.log B: first SteamInput006 call is position 0, 1 position-0 calls -> match
part A: 3 logs, match
part B: 3 logs, match
experiment 13: match
```

The games were closed about 88 seconds after the proxy started (the
waits of `run.sh` add up), as in experiment 12; no verdict depends on
that. Under ADR 0051, positions 0 and 10 of
`STEAMUSERSTATS_INTERFACE_VERSION012` are confirmed as
`RequestCurrentStats` and `StoreStats`, and position 0 of `SteamInput006`
as `Init`. Not in the shim yet: neither interface has an object there, and
an honest answer to `RequestCurrentStats` needs the stats from Valve's
servers and the callbacks (ADR 0050).

**Correction (loop iteration 10a, after a review pass):** the plan listed
two functions without arguments, but the `ISteamUserStats` page also
documents `ResetAllStats` (one `bool`, returns `bool`, "automatically
calls StoreStats"). Experiment 13 did not rule it out for position 10:
it was called once at close and returned true, which `ResetAllStats`
would do too, and `rdx` was 1 in all three games, which also fits a
`bool` argument. Position 10 of `STEAMUSERSTATS_INTERFACE_VERSION012` is
therefore **narrowed to `StoreStats` or `ResetAllStats`**, not confirmed
(`status.json` changed accordingly). Position 0 (`RequestCurrentStats`)
is not affected.

## `STEAMINVENTORY_INTERFACE_V003`: result handles (loop iteration 6)

Page read 2026-11-22: <https://partner.steamgames.com/doc/api/ISteamInventory>.

- **Recorded** (reference runs of Cave Crawlers and Bongo Cat):
  - Position 6: `rdx` an address on the stack, result 1 (Cave Crawlers 7
    calls, Bongo Cat 1). In Cave Crawlers three calls in a row were
    followed by position 1 calls whose `rdx` held 2, 3 and 4.
  - Position 1: `rdx` a small number in its low 32 bits (0 to 7), `r8` 0 or
    an address, `r9` an address, result 1. Bongo Cat called it twice for
    the same number, first with `r8` 0, then with an address.
  - Position 3 (Bongo Cat): `rdx` the same kind of small number, the
    result's low 32 bits a number that read as a Unix time is the day of
    the reference run.
- **Fit:** the page documents `GetAllItems(SteamInventoryResult_t
  *pResultHandle)`, which returns true and "a new result handle" through
  the pointer; `GetResultItems(resultHandle, pOutItemsArray,
  punOutItemsArraySize)`, where a NULL array returns "the number of
  elements the array needs to hold"; and `GetResultTimestamp(resultHandle)`,
  which returns "Unix epoch time". `GetItemsByID` also writes a handle, but
  takes a count as its third argument, and `r9` held an address in Cave
  Crawlers. Candidates: 6 `GetAllItems`, 1 `GetResultItems`, 3
  `GetResultTimestamp`.

### Experiment 14 (planned 2026-11-22, before the runs)

The proxy now reads, after the call returns (ADR 0043), the first 16
bytes at `rdx` for position 6 and at `r9` for position 1. Agent runs of
Cave Crawlers and Bongo Cat, each closed by a close request.

**Predictions** (`tools/shim/experiments/014_inventory_handles.py`):

- A (both games): position 6 is called at least once; every logged call
  returns true, and the first four bytes at `rdx` after it are a handle
  below 0x10000 that a later position 1 call has in the low 32 bits of
  `rdx`.
- B (Bongo Cat): position 3 is called at least once; each result's low
  32 bits are a Unix time within one day of the log's last write, and its
  `rdx` is a handle also given to position 1.

Position 1 is recorded, not predicted (the item count it writes is read
but not part of a prediction). Each log must be recorded by `run.sh` for
its game in `starts.log`, one log per game.

**Result of experiment 14** (agent runs after the plan was committed at
08:33:53; logs recorded by `run.sh` in `starts.log`: `observe-31272`
Cave Crawlers, `observe-47280` Bongo Cat): **match**, checked by the
script.

```text
observe-31272.log (Cave Crawlers) A: 1 position-6 calls, handles [7] -> match
observe-47280.log (Bongo Cat) A: 1 position-6 calls, handles [5] -> match
observe-47280.log (Bongo Cat) B: 7 position-3 calls, result minus log time [-87, -87, -87, -87, -87, -87, -27] s -> match
experiment 14: match
```

Cave Crawlers called position 6 once this time (seven times in its
reference run).

**Review finding (before merge):** the predictions do not tell
`GetAllItems` apart from other functions on the page that write "a new
result handle" through `SteamInventoryResult_t *pResultHandle` and return
true: `GrantPromoItems(pResultHandle)` has the same signature, and
`TriggerItemDrop` and `AddPromoItem` take an item definition as a second
argument (in the new Bongo Cat log `r8` held a small number). Likewise
`SerializeResult(resultHandle, pOutBuffer, punOutBufferSize)` has the
shape of `GetResultItems`. So, although the script reported a match:

- position 3 is confirmed as `GetResultTimestamp`: it is the only
  function on the page that takes a result handle and returns a `uint32`,
  and the result was a Unix time 27 to 87 seconds before the log's last
  write;
- position 6 stays a candidate: `GetAllItems`, `GrantPromoItems`,
  `TriggerItemDrop` or `AddPromoItem` (it writes a result handle);
- position 1 stays a candidate: `GetResultItems` or `SerializeResult`.

Telling them apart needs another experiment. Lesson for the loop: before
planning, list every function on the page with the recorded shape, not
only the best fit. Not in the shim yet: it has no inventory object, and
an honest answer needs the account's inventory from Valve's servers.

## `STEAMREMOTESTORAGE_INTERFACE_VERSION016` position 13 (loop iteration 7)

Page: <https://partner.steamgames.com/doc/api/ISteamRemoteStorage>
(re-read 2026-11-22).

- **Recorded** (reference runs): Bongo Cat and Linsips called it once
  early (about 0.7 and 1.5 seconds), the only
  `STEAMREMOTESTORAGE_INTERFACE_VERSION016` call of the run, with a file
  name as text in `rdx` (`presets.json`, `progress.dat`); the result was
  0 both times. In Bongo Cat `r8` held bytes of the file name, not an
  address.
- **Fit:** every function on the page that takes only a file name:
  `FileExists`, `FilePersisted`, `FileForget`, `FileDelete` (true or
  false), and `GetFileSize`, `GetFileTimestamp`, `GetSyncPlatforms`,
  `FileShare`, `FileWriteStreamOpen` (other results, 0 possible for a
  missing file). Position 13 of version 014 was narrowed to the first
  three, but nothing shows that 016 keeps 014's order, and here no call
  follows that would rule `FileDelete` out. All stay candidates.
- **No experiment yet**, and no shim answer: several candidates change
  what is stored or synced.

## UserStats: reading a value by name (loop iteration 7)

Page: <https://partner.steamgames.com/doc/api/ISteamUserStats> (re-read
2026-11-22).

- **`STEAMUSERSTATS_INTERFACE_VERSION012` position 6:** One's Own Exile
  called it 16 times early with a name as text in `rdx` (for example
  `Adornments`), the same `r8` address each time, result 1. Fallout
  Shelter called it 35 times, 27 of them with a name as text in `rdx` and
  `r9` the length of the name plus one (a leftover), all with result 0.
- **`STEAMUSERSTATS_INTERFACE_VERSION013` position 5:** Cartapli called it
  19 times within 5 ms at start, each with an achievement-like name as
  text in `rdx` (`CLEAR_TUTORIAL_ACHIEVEMENT` and so on), the same `r8`
  address each time, `r9` the length of the name plus one; 12 results
  were 1, 7 were 0. Bongo Cat called it once with `PET_1`, result 1.
- **Fit, for both:** the functions on the page that take a name and an
  output pointer and return true or false: `GetAchievement`, `GetStat`
  (two kinds), `GetGlobalStat` (two kinds),
  `GetAchievementAchievedPercent`. (Experiment 2 ruled the last one out
  for position 6 of version 011, not for these versions.)
  `GetAchievementAndUnlockTime` needs a pointer in `r9`, which held a
  length in Fallout Shelter and Cartapli. If `r8` were a leftover too,
  the name-only writes `SetAchievement`, `ClearAchievement` and
  `SetStat` (float, value in a floating-point register) would also fit;
  a constant `r8` across 19 different names fits an output slot better,
  but nothing recorded rules them out. All stay candidates; telling them
  apart needs the bytes written at `r8` (ADR 0043, 0053).

## `SteamUtils009` positions 9 and 14 (loop iteration 8)

Page re-read 2026-11-22: <https://partner.steamgames.com/doc/api/ISteamUtils>.

- **Recorded** (reference runs, `coverage.md`): position 9 in all five
  `SteamUtils009` games (Cave Crawlers, Outcore, UNBEATABLE [white
  label], Gravitas, Dark Deception), position 14 in four of them, both
  without arguments; the same positions as `GetAppID` and
  `GetCurrentBatteryPower` in `SteamUtils010` (experiments 3 and 10).
- **Every function on the page without arguments** (LOOP step 3):
  `BOverlayNeedsPresent`, `GetAppID`, `GetConnectedUniverse`,
  `GetCurrentBatteryPower`, `GetEnteredGamepadTextLength`,
  `GetIPCCallCount`, `GetIPCountry`, `GetSecondsSinceAppActive`,
  `GetSecondsSinceComputerActive`, `GetServerRealTime`,
  `GetSteamHardwareDefaultConfig`, `GetSteamUILanguage`,
  `InitFilterText`, `IsOverlayEnabled`, `IsRunningOnSteamHardware`,
  `IsRunningUnderProton`, `IsSteamChinaLauncher`,
  `IsSteamInBigPictureMode`, `IsSteamRunningInVR`,
  `IsSteamRunningOnSteamDeck`, `IsVRHeadsetStreamingEnabled`,
  `StartVRDashboard`.
  - Only `GetAppID` is described as returning the app ID; a result equal
    to the app ID in four different games rules the others out (the
    true/false ones return 0 or 1, the counters and times are not equal
    to four different app IDs, the texts are addresses).
  - Only `GetCurrentBatteryPower` documents the value 255 ("Returns 255
    when the user is on AC power"). The two hardware enums have no values
    on the page; that 255 stands for one of them on a desktop is not
    ruled out by the page, only by the same reading as experiment 10.

### Experiment 15 (planned 2026-11-22, before the runs)

Agent runs of Cave Crawlers, Gravitas, Dark Deception and UNBEATABLE
[white label] (Outcore needs its launch dialog and is left out), each
closed by a close request. `run.sh` arms with `--app`; answer mode covers
only `SteamUtils010`, so `SteamUtils009` calls reach the official client.

**Predictions** (`tools/shim/experiments/015_utils009.py`):

- A (all four): position 9 is called, and every result's low 32 bits are
  the game's app ID.
- B (Cave Crawlers, Dark Deception, UNBEATABLE): position 14 is called,
  and every result's lowest byte is 255.

**Result of experiment 15** (agent runs after the plan was committed at
11:14:42; logs recorded by `run.sh` in `starts.log`: `observe-48520`
Cave Crawlers, `observe-22464` Gravitas, `observe-9812` Dark Deception,
`observe-54100` UNBEATABLE [white label]): **match**, checked by the
script.

```text
observe-48520.log (Cave Crawlers) A: 2 calls, results [2143030] -> match
observe-48520.log (Cave Crawlers) B: 52 calls, lowest bytes [255] -> match
observe-22464.log (Gravitas) A: 5 calls, results [1067310] -> match
observe-9812.log (Dark Deception) A: 5 calls, results [332950] -> match
observe-9812.log (Dark Deception) B: 63 calls, lowest bytes [255] -> match
observe-54100.log (UNBEATABLE [white label]) A: 2 calls, results [1290490] -> match
observe-54100.log (UNBEATABLE [white label]) B: 61 calls, lowest bytes [255] -> match
experiment 15: match
```

Gravitas wrote no `[S_API]` debug line, so the first run's log was not
recorded in `starts.log`; `run.sh` now also records a log by the game's
process ID, and Gravitas was started a second time (the first log,
`observe-50860`, is not used). Under ADR 0051, positions 9 and 14 of
`SteamUtils009` are confirmed as `GetAppID` and `GetCurrentBatteryPower`
(for 14 with the caveat written above). Added to the shim (same answers
as for 010) and to shadow mode; a shadow run of Dark Deception
(`observe-53604`) matched in all 68 logged calls (5 at position 9, 63 at
position 14).

**Corrections from the review (before merge):** the plan said the two
hardware enums have no values on the page; it does list them
(`ESteamHardwareType` 0 to 3, `ESteamHardwareDefaultConfig` 0 to 7), so
255 rules out `IsRunningOnSteamHardware` and
`GetSteamHardwareDefaultConfig` too, and the caveat for position 14 does
not apply. The counters and times (`GetIPCCallCount`,
`GetSecondsSince…`, `GetServerRealTime`) are ruled out because their
lowest byte would not stay 255 over 52 to 63 calls in three games, the
text results because they are addresses, `StartVRDashboard` because a
void function in every frame of three games does not fit, and the
true/false ones because they return 0 or 1. The second Gravitas run used
the changed `run.sh`.

## Callback 0x44d: stats received (loop iteration 9)

Pages read 2026-11-22: <https://partner.steamgames.com/doc/api/ISteamUserStats>
(callback structs, no numeric IDs on the page) and
<https://partner.steamgames.com/doc/api/steam_api> (`k_EResultOK` = 1,
`k_EResultFail` = 2; the layout of a game ID is not described).

- **Recorded** (step 4d logs of 2026-11-22: Cave Crawlers, Gravitas, Dark
  Deception, all `STEAMUSERSTATS_INTERFACE_VERSION011`): one callback
  with number 0x44d per run, data size 24, 250 to 310 ms after the first
  call of `STEAMUSERSTATS_INTERFACE_VERSION011` position 0 (candidate
  `RequestCurrentStats` or `StoreStats`).
- **Every callback on the page, by size** (fields as listed there,
  `uint64` and `CSteamID` 8 bytes, `EResult` and `int` 4): 24 bytes fit
  `UserStatsReceived_t` (`m_nGameID`, `m_eResult`, `m_steamIDUser`) and
  `LeaderboardScoresDownloaded_t` (two handles and a count); 16 bytes fit
  `UserStatsStored_t`, `GlobalStatsReceived_t` and
  `GlobalAchievementPercentagesReady_t`; the others are larger or
  smaller. Callbacks of other pages could also have 24 bytes; the
  predictions below are about content, which rules those out unless they
  also start with the app ID followed by a result code and the same
  8-byte value in every game.
- **Candidates:** 0x44d `UserStatsReceived_t`; position 0 of
  `STEAMUSERSTATS_INTERFACE_VERSION011` `RequestCurrentStats` (the page:
  its result arrives as `UserStatsReceived_t`).

### Experiment 20 (planned 2026-11-22, before the runs)

The proxy logs the first 64 bytes of callbacks 0x44d and 0x44e (ADR
0050, 0054). 0x44d carries a Steam ID in bytes 16 to 24 if the candidate
is right; that value stays in the local log, the script only compares
it. 0x44e (16 bytes) is recorded, not predicted. Agent runs of Cave
Crawlers, Gravitas and Dark Deception, each closed after about 30
seconds.

**Predictions** (`tools/shim/experiments/020_stats_received_callback.py`):

- A: one 0x44d callback per game, size 24, bytes 0 to 4 the game's app
  ID, bytes 8 to 12 equal to 1 or 2.
- B: bytes 16 to 24 are the same in all three games.
- C: the callback comes after the first call of
  `STEAMUSERSTATS_INTERFACE_VERSION011` position 0, at most 5 seconds
  later.

**Result of experiment 20** (agent runs after the plan was committed at
12:05:59; logs recorded by `run.sh`: `observe-42504` Cave Crawlers,
`observe-31124` Gravitas, `observe-47852` Dark Deception): **mismatch**,
as scripted.

```text
observe-42504.log (Cave Crawlers) A: 1 callbacks 0x44d, size [24], app ID matches, result 1 -> match
observe-42504.log (Cave Crawlers) C: callback 265 ms after the first position-0 call -> match
observe-31124.log (Gravitas) A: 1 callbacks 0x44d, size [24], app ID matches, result 1 -> match
observe-31124.log (Gravitas) C: callback 268 ms after the first position-0 call -> match
observe-47852.log (Dark Deception) A: 1 callbacks 0x44d, size [24], app ID matches, result 1 -> match
observe-47852.log (Dark Deception) C: callback 285 ms after the first position-0 call -> match
B: bytes 16..24 the same in 3 logs -> MISMATCH
experiment 20: MISMATCH
```

A and C matched (the result code was 1 in all three). B did not: the
8 bytes at 16 to 24 differ between games. Looked at afterwards, without
printing any value: bytes 12 to 20 are the same in all three games and
only bytes 20 to 24 differ. That fits the user's Steam ID at 12 to 20
(the fields packed on 4 bytes) and 4 bytes of leftover at the end, not
the layout the plan assumed. Nothing is confirmed; 0x44d stays a
candidate for `UserStatsReceived_t`, position 0 of
`STEAMUSERSTATS_INTERFACE_VERSION011` a candidate for
`RequestCurrentStats`.

### Experiment 21 (planned 2026-11-22, before the runs)

Experiment 20 again, with the layout its result suggested, on fresh runs
of the same three games (they are the only `…VERSION011` games).

**Predictions** (`tools/shim/experiments/021_stats_received_layout.py`):
A and C as in experiment 20; B: bytes 12 to 20 are the same in all three
games and their upper four bytes are not zero.

**Result of experiment 21** (agent runs after the plan was committed at
12:15:06; logs recorded by `run.sh`: `observe-752` Cave Crawlers,
`observe-47412` Gravitas, `observe-52328` Dark Deception): **match**,
checked by the script.

```text
observe-752.log (Cave Crawlers) A: 1 callbacks 0x44d, size [24], app ID matches, result 1 -> match
observe-752.log (Cave Crawlers) C: callback 309 ms after the first position-0 call -> match
observe-47412.log (Gravitas) A: 1 callbacks 0x44d, size [24], app ID matches, result 1 -> match
observe-47412.log (Gravitas) C: callback 290 ms after the first position-0 call -> match
observe-52328.log (Dark Deception) A: 1 callbacks 0x44d, size [24], app ID matches, result 1 -> match
observe-52328.log (Dark Deception) C: callback 305 ms after the first position-0 call -> match
B: bytes 12..20 the same in 3 logs -> match
experiment 21: match
```

Under ADR 0051:

- callback number **0x44d (1101)** is confirmed as `UserStatsReceived_t`:
  24 bytes, the app ID in bytes 0 to 4 (bytes 4 to 8, the rest of the
  game ID, were not checked), the result (`EResult`) in 8 to 12, and in
  12 to 20 the same 8 bytes in every game, read as the user's Steam ID;
- position 0 of `STEAMUSERSTATS_INTERFACE_VERSION011` is confirmed as
  `RequestCurrentStats` (the page: its result arrives as
  `UserStatsReceived_t`; `StoreStats` answers with `UserStatsStored_t`).

Not in the shim yet: it does not deliver callbacks (experiment 19, loop
iteration 9b, plans that protocol), and the stats themselves have to come
from Valve's servers through `rebornd`.

**Review findings (before merge):**

- The size list in the plan assumed natural alignment (8 + 4 + 4 padding
  + 8 = 24). With the Steam ID at 12 to 20 the fields take 20 bytes; why
  the size is 24 (4 bytes at the end that differ between games) is not
  explained by the observation. The 16-byte candidates for 0x44e rest on
  the same assumption and are only a guess until 0x44e is tested.
- Prediction B only shows the same 8 bytes in three games. Before the
  shim writes this field, an experiment compares bytes 12 to 20 with the
  account's own Steam ID seen elsewhere in the same log (printing only
  match or mismatch).
- Scripts from now on also reject logs recorded before their plan
  commit (`LOOP.md`).

**Shadow mode** (ADR 0046 as amended by ADR 0055, stage 1:
`RequestCurrentStats` "compared by 0x44d's size and a hash of its
confirmed fields"; added 2026-11-22, **planned, not run yet**): when
`Steam_BGetCallback` returns 1 with a 0x44d header, the proxy reads the
payload size and only payload bytes 0..12, and hands them to its shadow
worker thread (the bounded queue of the `BIsSubscribed` shadow; never on
the game's thread). The worker sends `RequestStats` on the read-only
shadow session `reborn-observe arm --app` registered and waits up to
30 s for `rebornd`'s `StatsReceived`. **Match** means: size 24, bytes
0..4 equal to the app given to `arm` (the session's own app, the only
one `RequestStats` answers), and bytes 8..12 equal to the `EResult` of
`StatsReceived`. The fields are compared directly rather than by a
hash; only the verdict is logged, so the effect is the same. The log
line says only `shadow match`, `shadow mismatch` (not which field), or
`shadow: no Reborn answer (<reason>)`, never the values. The game always
gets the official callback unchanged.

Left out on purpose:

- bytes 12..20 (read as the user's Steam ID): not compared until the
  experiment from the review findings above has compared them with the
  account's own Steam ID (ADR 0055, stage 0). The comparison does not
  read bytes 12..20; the step-4b line still logs the 0x44d payload
  locally (ADR 0050, 0054), as before;
- bytes 4..8 (never checked) and 20..24 (unexplained);
- achievement names: 0x44d carries none; ADR 0055 shadows them
  separately, through `GetAchievementDisplayAttribute`.

A limit to keep in mind for the run: a 0x44d that answers a
`RequestUserStats` for another user (experiment 22) would be compared
with the account's own stats and may show a mismatch. The three
`…VERSION011` games, Cave Crawlers, Gravitas and Dark Deception, got
exactly one 0x44d per run in experiments 20 and 21.

### Experiment 30 (planned 2026-11-22, before the runs; loop iteration 14a)

The first live run of the stats shadow above, in Cave Crawlers
(2143030), Gravitas (1067310) and Dark Deception (332950), one fresh
`run.sh` run each, armed with `reborn-observe arm --app <id>`.

**Conditions for the runs.** `rebornd` is signed in to the test account
with the stored login while the official client runs and stays signed in
to the same account (the maintainer allowed such shadow runs on
2026-11-22, ADR 0055 stage 1). The agent stops at once, and the run does
not count, if Steam asks for Steam Guard or any other sign-in step, or
if either client is logged off or kicked (ADR 0055, "Two sign-ins at
once"; LOOP.md "Stop and ask the maintainer"). Each game stays running
for at least a minute after its start, so that the shadow worker (which
waits up to 30 s for `StatsReceived`) can write its line before the game
exits. `rebornd` is shut down after the runs.

**What the proxy writes** (`crates/shim-observe/src/proxy.rs`,
`observe_return` and `answer_asks`; `daemon.rs`,
`compare_stats_received`): the step-4b line
`<ms> ms ThreadId(<t>) Steam_BGetCallback return <n> wrote rdx->[…] …`
for a header naming 0x44d, and later, from the shadow worker,
`<ms> ms ThreadId(<t>) Steam_BGetCallback return <n> callback 0x44d <note>`
with the same `<ms>`, `<t>` and `<n>`, where `<note>` is
`shadow match`, `shadow mismatch`, `shadow: no Reborn answer (<why>)`,
`shadow: no app ID given`, `shadow: skipped, <why>` or
`shadow: no worker thread`.

**Predictions** (`tools/shim/experiments/030_stats_shadow.py`), per
game:

- A: the log has at least one step-4b line for callback 0x44d;
- B: every such line is followed by exactly one stats shadow line with
  the same thread and call number, and no stats shadow line is without
  one;
- C: every stats shadow line says exactly `shadow match` (no mismatch,
  no "no Reborn answer", nothing skipped).

All three games must match. The script rejects a log given twice, logs
not recorded by `run.sh` for these games (`observelog.app_of`) and logs
whose start (`observelog.start_of`) is earlier than the plan commit
(`PLAN_COMMITTED`, set in a follow-up commit). It prints only counts,
the kind of each note and match or MISMATCH: no payload bytes, IDs,
registers or reason texts.

**What a full match confirms** (ADR 0055 stage 1, `RequestCurrentStats`
"compared by 0x44d's size and … its confirmed fields"): in three games
the stats-received callback Reborn would deliver agrees with the official
one in size (24), app ID (bytes 0..4) and `EResult` (bytes 8..12), with
`rebornd`'s `StatsReceived` from Valve's servers. **What stays open:**

- bytes 12..20 (read as the user's Steam ID) are not compared; the
  experiment ADR 0055 stage 0 asks for (compare them with the account's
  own Steam ID, match or mismatch only) still has to run before the shim
  writes that field; bytes 4..8 and 20..24 stay unexplained;
- achievement names are not covered (0x44d has none); they need the
  separate `GetAchievementDisplayAttribute` shadow;
- answer mode for stats is not allowed by this result: it needs the
  maintainer's OK and, per game, every account-data call matched in
  shadow mode (ADR 0055 stage 2).

A mismatch or "no Reborn answer" is recorded as such; nothing is
changed in the shim to make it match without a new plan.

**Result of experiment 30** (agent runs by the main session after the
plan time 18:57:28, proxy built from PR #36's head `3d2e878`; `rebornd`
signed in with the stored login while the official client ran; logs
recorded by `run.sh`: `observe-52568` Cave Crawlers, `observe-59640`
Gravitas, `observe-62168` Dark Deception): **match**, checked by the
script.

```text
observe-52568.log (Cave Crawlers) A: 1 step-4b lines for callback 0x44d -> match
observe-52568.log (Cave Crawlers) B: per 0x44d [1 shadow line(s): 1], 0 shadow lines without a 0x44d -> match
observe-52568.log (Cave Crawlers) C: shadow notes {'match': 1} -> match
observe-52568.log (Cave Crawlers): match
observe-59640.log (Gravitas) A: 1 step-4b lines for callback 0x44d -> match
observe-59640.log (Gravitas) B: per 0x44d [1 shadow line(s): 1], 0 shadow lines without a 0x44d -> match
observe-59640.log (Gravitas) C: shadow notes {'match': 1} -> match
observe-59640.log (Gravitas): match
observe-62168.log (Dark Deception) A: 1 step-4b lines for callback 0x44d -> match
observe-62168.log (Dark Deception) B: per 0x44d [1 shadow line(s): 1], 0 shadow lines without a 0x44d -> match
observe-62168.log (Dark Deception) C: shadow notes {'match': 1} -> match
observe-62168.log (Dark Deception): match
experiment 30: match
```

As fixed beforehand: in three games the stats-received callback Reborn
would deliver agrees with the official one in size, app ID and
`EResult` (ADR 0055 stage 1). No logoff, kick or Steam Guard prompt
(none in `rebornd`'s local log); `rebornd` was shut down afterwards. The
"What stays open" items above stay open.

**Not in this plan, noted afterwards (2026-11-23):** after signing in,
`rebornd` (`watch_friends`, `apps/rebornd/src/daemon.rs`) sent one
`set_persona_state` to online, because Reborn's own-status setting
(`%APPDATA%\Reborn\config\own-status`) held `online`. That is one
status change of the account that the plan did not list; the maintainer
accepted it on 2026-11-23. Experiment 33 lists it under its conditions.

## Callback delivery: `Steam_BGetCallback`, `Steam_FreeLastCallback`, `Steam_GetAPICallResult` (loop iteration 9b)

Pages read 2026-11-22, both opened without signing in:
<https://partner.steamgames.com/doc/sdk/api> (callbacks, call results,
manual dispatch) and <https://partner.steamgames.com/doc/api/ISteamUtils>
(`GetAPICallResult`, `SteamAPICallCompleted_t`).

- **What the pages say, in Reborn's words:** a callback is a struct with a
  unique identifier and a small amount of data. Besides the automatic
  dispatch, there is a manual one that works like a window message loop:
  the game fetches the next available callback in a loop and dispatches it
  itself; the details are only in `steam_api.h`, which is excluded (red
  line 6), so the pages say nothing about a header, a size, freeing a
  callback or the order of calls. Call results go to one listener; a game
  can also ask `ISteamUtils` whether a call has finished and get its
  result. `GetAPICallResult` takes a call handle, a buffer, the buffer's
  size, the callback identifier the game expects and a pointer to a
  failure flag, and returns true if the handle is valid and the call is
  finished. `SteamAPICallCompleted_t` holds a call handle
  (`SteamAPICall_t`), a callback identifier (`int`) and the size of the
  finished result (`uint32`). The page does not give the width of
  `SteamAPICall_t`. That it is 64 bits, and so that the struct has 16
  bytes, is derived from the recorded `rdx` values of
  `Steam_GetAPICallResult`, which use all 64 bits; it is not read on a
  page. No page gives a callback's numeric identifier.
- **The flat export is on no public page.** The pages describe the
  `ISteamUtils` method `GetAPICallResult`, not the entry point
  `Steam_GetAPICallResult`, and not `Steam_BGetCallback` or
  `Steam_FreeLastCallback`. Linking the export to the method rests on the
  name and the recorded shape only. That the export takes one extra
  leading argument (`rcx`, a handle) before the method's parameters in
  their documented order is an **unverified inference** (ADR 0042: names
  and the order on a page are not sources for positions or registers).
- **Every `ISteamUtils` function that takes a call handle** (LOOP step 3):
  - `GetAPICallResult` (handle, buffer, buffer size, expected number,
    failure flag; true or false): fits `rdx` a 64-bit handle, `r8` a heap
    address, `r9` a size-like 8 to 792, result 0 or 1.
  - `IsAPICallCompleted` (handle, failure flag; true or false): fits if
    `r8` were the flag pointer and `r9` a leftover. Not ruled out by the
    registers alone.
  - `GetAPICallFailureReason` (handle only; an enum result): fits if `r8`
    and `r9` were leftovers and the value were 0 or 1. Not ruled out by the
    registers alone.
  - Only F tells them apart: if bytes 12..16 of the 703 payload (the size
    of the finished result) equal `r9` in every call, `r9` is an argument
    carrying that size, which only `GetAPICallResult` takes. Until F is
    checked, the meaning of `Steam_GetAPICallResult` stays a candidate.
- **Recorded** (logs with the step 4d `payload:` notes, recorded by
  `run.sh`: `observe-31272` and `observe-48520` Cave Crawlers,
  `observe-47280` Bongo Cat, `observe-9812` and `observe-53604` Dark
  Deception, `observe-54100` UNBEATABLE [white label], `observe-22464`
  Gravitas; raw logs stay local):
  - 119 headers; in every one bytes 0..4 were 1, bytes 16..20 a size of 1
    to 784 that made a readable range with the address in bytes 8..16, and
    bytes 20..24 were 0. The same number (bytes 4..8) always came with the
    same size, in all seven logs: 15 numbers, for example 703 with 16, 1101
    with 24, 1040044 with 784, 1080000 with 628.
  - After every header, exactly one `Steam_FreeLastCallback` call came on
    the same thread before that thread's next logged `Steam_BGetCallback`
    line; there were exactly as many frees as headers (14, 14, 26, 23, 21,
    2 and 19, in the order the logs are listed above).
  - `Steam_GetAPICallResult` (only Bongo Cat, 6 calls) came only after a
    header with the number 703 and before its free, exactly once after each
    of the six; `rcx` low 32 bits 1, `rdx` a 64-bit value, `r8` an address,
    `r9` 8 to 792. Three of the six `rdx` values were, earlier in the log,
    the result of a call through `STEAMUSERSTATS_INTERFACE_VERSION013`
    (positions 15 and 37) or `STEAMINVENTORY_INTERFACE_V003` (position 26);
    the other three appear nowhere else in the log.
  - **New:** Dark Deception (`SteamClient019`) and Gravitas
    (`SteamClient018`) poll from a second thread and alternate
    `Steam_BGetCallback` calls with `rcx` 1 and `rcx` 2, each with its own
    `rdx`/`r8` addresses. The handle 2 is the one these games pass to
    `SteamClient` positions 4 and 1 at close and in `r8` to position 6 with
    a `SteamGameServer012` text: in both games position 4 was called with
    `rdx` 2, `r8` 1 and position 6 with `rdx` 1, `r8` 2. So, in the
    `SteamClient` shapes of 018 and 019 (not confirmed for those
    versions), the user on pipe 2 is also handle 1. Headers from polls
    with `rcx` 2 still had bytes 0..4 = 1, and their numbers (1080000, 1080001) were never seen
    from polls with `rcx` 1. The frees after them had `rcx` 2.
  - The result of `Steam_BGetCallback` had lowest byte 0 or 1 every time
    (upper bits sometimes leftovers, such as `0xffffff00`).
  - `r8` of `Steam_FreeLastCallback` held the payload size in Dark
    Deception and an address in Bongo Cat: a leftover, not an argument.

Evidence (Dark Deception, thread 2; hashes, `r8` bytes and floating-point
registers left out):

```text
168 ms ThreadId(2) Steam_BGetCallback call 2 rcx=0x1 rdx=0xca29a7fd10 r8=0xca29a7fd50 r9=0x16
168 ms ThreadId(2) Steam_BGetCallback return 2 wrote rdx->[01000000 50010000 10001dc6 b0020000 0c000000 00000000] r8->[…] payload: size 12 sha256 …
169 ms ThreadId(2) Steam_FreeLastCallback call 2 rcx=0x1 rdx=0x0 r8=0xc r9=0x7ffe4c8fc100
169 ms ThreadId(2) Steam_BGetCallback call 5 rcx=0x2 rdx=0xca29a7fd70 r8=0xca29a7fdb0 r9=0x12
169 ms ThreadId(2) Steam_BGetCallback return 5 wrote rdx->[01000000 c17a1000 0000fdc6 b0020000 04000000 00000000] r8->[…] payload: size 4 sha256 …
169 ms ThreadId(2) Steam_FreeLastCallback call 4 rcx=0x2 rdx=0x0 r8=0x4 r9=0x7ffe4c8fc104
```

### Readings and their alternatives

| Part | Reading | Alternatives | Ruled out by |
| --- | --- | --- | --- |
| `rcx` of all three entry points | the handle of the pipe the game polls | the user handle; a constant | A2 (a pipe-2 poll freed with `rcx` 2) rules out a constant; pipe vs. user only via the `SteamClient` positions (2 is the handle released at close), not by this experiment |
| header bytes 0..4 | the user handle `U` | a copy of `rcx`; the pipe handle; a constant 1; a count; the user on that pipe | A rules out a copy of `rcx` (pipe-2 polls with bytes 0..4 = 1). It rules out the pipe handle provided `rcx` is the pipe, which comes from the 018/019 `SteamClient` shapes and is not confirmed for those versions. A constant 1 and "the user on that pipe" are **not** ruled out: `U` was 1 in every game, and in Dark Deception and Gravitas the user on pipe 2 is also 1 (position 4 with `rdx` 2, `r8` 1; position 6 with `rdx` 1, `r8` 2) |
| bytes 4..8 | the callback identifier | a sequence counter; a tag shared by several structs | C (same number, same size in five games) and E (`GetAPICallResult` only after 703) rule out a counter; a tag shared by structs of one size is not ruled out |
| bytes 8..16 | the address of the callback's data | another readable address | B only shows it is readable together with the size |
| bytes 16..20 | the size of the data | a count of items; flags | C rules out per-call values; a fixed per-type count is not ruled out |
| bytes 20..24 | padding (always 0) | the upper half of a 64-bit size | not distinguishable while sizes stay small |
| result of `Steam_BGetCallback` | true when a callback was handed over | a count of waiting callbacks | D (lowest byte only 0 or 1, one free per header) |
| `Steam_FreeLastCallback` | releases the callback just fetched, on that pipe | called on a timer; frees several at once | D (exactly one per header, before the next poll, never without a header) |
| `Steam_GetAPICallResult` | `GetAPICallResult`: fetches the result a 703 callback announced (`rcx` pipe, `rdx` call handle, `r8` buffer, `r9` its size; expected number and failure flag would be on the stack, not logged; the parameter order is the unverified inference above) | polling independent of callbacks; `IsAPICallCompleted`; `GetAPICallFailureReason` | E rules out independent polling (only in a 703 window, exactly one per 703); the other two functions only F |
| number 703 | `SteamAPICallCompleted_t` (16 bytes: handle, number, size) | any other 16-byte callback | only F: bytes 0..8 equal `rdx` and bytes 12..16 `r9` of the following `GetAPICallResult`. Needs 703 in `PAYLOAD_LISTED` (ADR 0050); per the page the struct carries a call handle, a number and a size, no account identifier |

### Experiment 19 (planned 2026-11-22, before the runs)

**Predictions** (`tools/shim/experiments/019_callback_delivery.py`; registers
compared on their low 32 bits):

- A: every header's bytes 0..4 equal `U`, the first result of
  `SteamClient` position 2 in the same log; across all logs, at least one
  header comes from a logged poll whose `rcx` differs from its bytes 0..4.
- A2: the free and any `GetAPICallResult` that belong to a header have the
  same `rcx` as the logged poll that fetched it.
- B: every header has 24 bytes, a `payload: size N sha256` note with `N` =
  bytes 16..20, and bytes 20..24 = 0.
- C: one size per number, in every log and across all logs.
- D: per thread, exactly one free after each header before the next logged
  poll, no free without an open header, and the poll result's lowest byte
  only 0 or 1.
- E: every `GetAPICallResult` call lies in a 703 window on the same
  thread, and every 703 window holds exactly one; at least one log has a
  703 callback.
- F (only once 703's first payload bytes are logged): bytes 0..8 = `rdx`
  (all 64 bits, on purpose) and bytes 12..16 = the low 32 bits of `r9` of
  that window's `GetAPICallResult`. Without them, F is reported as not
  checked and does not count.

Each log must be a different file recorded by `run.sh` in `starts.log`,
with step 4d notes; at least three different games. Logs with more than 50
frees or `GetAPICallResult` calls cannot be checked for D and E (the proxy
logs only the first 50 calls of each entry point) and count as a mismatch.
A2 must be checked at least once across all logs, and a `wrote` line
without a readable header is a mismatch. **Freshness:** a log whose
`start` line in `starts.log` (the last `start` of the same app before its
`log` line) is earlier than the commit time of the final plan is rejected;
the script holds that time as `PLAN_COMMITTED`, 2026-11-22 12:41:02 (commit
`ea39f6e`; local time, as `run.sh`
writes `starts.log`). `Steam_BGetCallback` call lines are logged only for calls 1 to 50 (and
every 10000th), so A2 covers only headers from those polls, and "before
the next poll" in D means the next *logged* poll line.

**Games to run:** Dark Deception (332950) and Gravitas (1067310) for the
two pipes (A, A2); Bongo Cat (3419430) and Coin Maker Simulator Demo
(5191630) for 703 and `GetAPICallResult` (E, and F once 703 is listed);
Cave Crawlers (2143030) as the plain case. Each runs to its first screen
and is closed by a close request.

**What a full match confirms** (for the recorded `SteamClient` versions
and the five games' `steam_api` builds):

- header bytes 4..8 are the callback number, bytes 8..16 the address of its
  data, and bytes 16..20 the size of that data (B, C, E);
- the poll and free order of D: a result with lowest byte 1 hands over one
  callback, and the game frees it with exactly one `Steam_FreeLastCallback`
  on the same `rcx` and thread before it polls again (D, A2);
- E's window rule: `Steam_GetAPICallResult` is called only while a 703
  callback is held, once per 703.

**What stays a candidate even on a full match:**

- header bytes 0..4: the user handle `U`, a constant 1, or the user on
  that pipe (all 1 in every recording);
- bytes 20..24: padding or the upper half of the size;
- 703 = `SteamAPICallCompleted_t`, and `Steam_GetAPICallResult` =
  `GetAPICallResult` rather than `IsAPICallCompleted` or
  `GetAPICallFailureReason`: both need F;
- whether `rcx` of the three entry points is the pipe or the user handle.

**Already seen:** the seven existing logs above give "match" for A to E
(F not checked). The predictions were written from these logs, so they
are not the test; only fresh runs after this plan is committed are.

**What the logs contradict in earlier notes:** `entry-points.md` (step 4b)
says bytes 0..4 are the same as the lowest four bytes of `rcx` of the
call (that entry now points here); with pipe-2 polls they are not (`rcx` 2, bytes 0..4 = 1). ADR 0042
and stage 2 say `rcx` was 1 in every `Steam_BGetCallback` call and every
call came from one thread; that held for Cave Crawlers, not for Dark
Deception and Gravitas. Dark Deception also calls
`Steam_ReleaseThreadLocalMemory` (at least 50 times, on both threads;
the proxy logs only the first 50), which stage 2 did not see.

**Result of experiment 19** (agent runs after the plan time 12:41:02;
logs recorded by `run.sh`: `observe-51204` Dark Deception, `observe-12256`
Gravitas, `observe-51224` Bongo Cat, `observe-25680` Coin Maker
Simulator Demo, `observe-52912` Cave Crawlers): **match** for A to E, F
not checked, checked by the script.

```text
observe-51204.log (app 332950) A: 21 headers, bytes 0..4 [1], P=1 U=1, 10 from a call whose rcx differs -> match
observe-51204.log (app 332950) A2: 15 frees and GetAPICallResult calls with a logged poll -> match
observe-51204.log (app 332950) B: size note equals bytes 16..20, bytes 20..24 zero, 0 unreadable headers -> match
observe-51204.log (app 332950) C: 8 numbers, 0 with more than one size -> match
observe-51204.log (app 332950) D: 21 frees for 21 headers, result lowest bytes [0, 1] -> match
observe-51204.log (app 332950) E: 0 GetAPICallResult calls, 0 callbacks 703 -> match
observe-51204.log (app 332950) F: 0 of 0 703 callbacks with payload bytes -> not checked
observe-12256.log (app 1067310) A: 21 headers, bytes 0..4 [1], P=1 U=1, 10 from a call whose rcx differs -> match
observe-12256.log (app 1067310) A2: 15 frees and GetAPICallResult calls with a logged poll -> match
observe-12256.log (app 1067310) B: size note equals bytes 16..20, bytes 20..24 zero, 0 unreadable headers -> match
observe-12256.log (app 1067310) C: 6 numbers, 0 with more than one size -> match
observe-12256.log (app 1067310) D: 21 frees for 21 headers, result lowest bytes [0, 1] -> match
observe-12256.log (app 1067310) E: 0 GetAPICallResult calls, 0 callbacks 703 -> match
observe-12256.log (app 1067310) F: 0 of 0 703 callbacks with payload bytes -> not checked
observe-51224.log (app 3419430) A: 26 headers, bytes 0..4 [1], P=1 U=1, 0 from a call whose rcx differs -> match
observe-51224.log (app 3419430) A2: 9 frees and GetAPICallResult calls with a logged poll -> match
observe-51224.log (app 3419430) B: size note equals bytes 16..20, bytes 20..24 zero, 0 unreadable headers -> match
observe-51224.log (app 3419430) C: 10 numbers, 0 with more than one size -> match
observe-51224.log (app 3419430) D: 26 frees for 26 headers, result lowest bytes [0, 1] -> match
observe-51224.log (app 3419430) E: 6 GetAPICallResult calls, 6 callbacks 703 -> match
observe-51224.log (app 3419430) F: 0 of 6 703 callbacks with payload bytes -> not checked
observe-25680.log (app 5191630) A: 20 headers, bytes 0..4 [1], P=1 U=1, 0 from a call whose rcx differs -> match
observe-25680.log (app 5191630) A2: 17 frees and GetAPICallResult calls with a logged poll -> match
observe-25680.log (app 5191630) B: size note equals bytes 16..20, bytes 20..24 zero, 0 unreadable headers -> match
observe-25680.log (app 5191630) C: 5 numbers, 0 with more than one size -> match
observe-25680.log (app 5191630) D: 20 frees for 20 headers, result lowest bytes [0, 1] -> match
observe-25680.log (app 5191630) E: 12 GetAPICallResult calls, 12 callbacks 703 -> match
observe-25680.log (app 5191630) F: 0 of 12 703 callbacks with payload bytes -> not checked
observe-52912.log (app 2143030) A: 14 headers, bytes 0..4 [1], P=1 U=1, 0 from a call whose rcx differs -> match
observe-52912.log (app 2143030) A2: 1 frees and GetAPICallResult calls with a logged poll -> match
observe-52912.log (app 2143030) B: size note equals bytes 16..20, bytes 20..24 zero, 0 unreadable headers -> match
observe-52912.log (app 2143030) C: 9 numbers, 0 with more than one size -> match
observe-52912.log (app 2143030) D: 14 frees for 14 headers, result lowest bytes [0, 1] -> match
observe-52912.log (app 2143030) E: 0 GetAPICallResult calls, 0 callbacks 703 -> match
observe-52912.log (app 2143030) F: 0 of 0 703 callbacks with payload bytes -> not checked
C across all logs: 15 numbers -> match
    number 304 (0x130): size [16]
    number 336 (0x150): size [12]
    number 703 (0x2bf): size [16]
    number 1101 (0x44d): size [24]
    number 1102 (0x44e): size [16]
    number 1222 (0x4c6): size [260]
    number 1281 (0x501): size [272]
    number 4700 (0x125c): size [8]
    number 4701 (0x125d): size [4]
    number 4702 (0x125e): size [1]
    number 1040011 (0xfde8b): size [260]
    number 1040015 (0xfde8f): size [20]
    number 1040044 (0xfdeac): size [784]
    number 1080000 (0x107ac0): size [628]
    number 1080001 (0x107ac1): size [4]
F not checked (703 payload bytes not logged)
experiment 19: match
```

Under ADR 0051 (for the recorded `SteamClient` versions and the five
games' `steam_api` builds) the readings listed above under "What a full match
confirms" are confirmed: header bytes 4 to 8 the callback number, 8 to 16
the address of its data, 16 to 20 its size; one callback per poll that
returns 1, freed by exactly one `Steam_FreeLastCallback` on the same `rcx`
and thread before the next poll; `Steam_GetAPICallResult` only while a
703 callback is held, once per 703. The readings under "What stays a
candidate" stay candidates. Not in the shim at the time: the shim's
`Steam_BGetCallback` aborted until PR #24; delivering callbacks needs
their contents, which come from `rebornd`.

**In the shim since PR #24** (`crates/shim/src/callbacks.rs`): the delivery
machinery, a queue per `rcx` value with the header above (bytes 0..4
written as 1 and 20..24 as 0, as recorded), result 1 or 0, and one free
per header (a free with nothing held returns 0, as Linsips recorded).
Nothing fills the queue yet, so no game gets a callback from the shim.

**Correction (loop iteration 10a, after a review pass):** a Linsips run
(`observe-55108`, app 4301370) recorded after experiment 19 and outside
its five games breaks two of its rules. D: the game called
`Steam_FreeLastCallback` 11 times for 10 headers, once after a poll that
returned 0, so "no free without an open header" is not a rule for every
game. E: it freed its one 703 callback without calling
`Steam_GetAPICallResult`, and the next callback was a 0x44d, so "exactly
one `GetAPICallResult` per 703" is not a rule for every game either. The
confirmation above stays scoped to the five games' builds. For the shim
this means `Steam_FreeLastCallback` must be harmless when nothing is held.
Whether Linsips ignored the 703 or received the result as an ordinary
callback is open; one run does not tell.

## `SteamUser`, `SteamFriends`, `SteamController` and `SteamInput` (loop iteration 9a)

Pages read 2026-11-22, each opened without signing in:
<https://partner.steamgames.com/doc/api/ISteamUser> (its constants name
`SteamUser019`), <https://partner.steamgames.com/doc/api/ISteamFriends>
(its constants name `SteamFriends015`; games asked for `017` and `018`),
<https://partner.steamgames.com/doc/api/ISteamController> (its constants
name `SteamController007`) and
<https://partner.steamgames.com/doc/api/ISteamInput>. No page describes
`SteamUser020` to `023`, `SteamFriends017`/`018`, `SteamController008`,
`SteamInput001` or `SteamInput006`; as before, a page gives the meaning of
a function, never a position. Recordings: the reference runs of
`tools/shim/games.json`, read with `tools/shim/observelog.py`. Steam IDs in
registers were compared inside scripts only; no value of one is written
here. Planning only: no game was run for this section.

### `SteamUser020` to `023` position 2: candidate `GetSteamID`, no experiment

- **Recorded:** all 20 games, once to 50 logged times; in every call the
  result was the address `rdx` held, an address on the game's stack that
  changes between call sites.
- **Every function on the page, by shape** (LOOP step 3):
  - returns a class through the caller's address, the recorded shape:
    `GetSteamID` (`CSteamID`), the only one;
  - return a number, enum, true/false or handle in the result register,
    which would not equal a fresh stack address in every call:
    `BeginAuthSession`, `BIsBehindNAT`, `BIsPhoneIdentifying`,
    `BIsPhoneRequiringVerification`, `BIsPhoneVerified`,
    `BIsTwoFactorEnabled`, `BLoggedOn`, `BSetDurationControlOnlineState`,
    `DecompressVoice`, `GetAuthSessionTicket`, `GetAuthTicketForWebApi`,
    `GetAvailableVoice`, `GetDurationControl`, `GetEncryptedAppTicket`,
    `GetGameBadgeLevel`, `GetHSteamUser`, `GetPlayerSteamLevel`,
    `GetUserDataFolder`, `GetVoice`, `GetVoiceOptimalSampleRate`,
    `InitiateGameConnection`, `RequestEncryptedAppTicket`,
    `RequestStoreAuthURL`, `UserHasLicenseForApp`;
  - take a Steam ID first, which `rdx` would then hold as a value (as
    `STEAMUSERSTATS_INTERFACE_VERSION013` position 15 shows), not a stack
    address: `AdvertiseGame`, `EndAuthSession`;
  - return nothing, so the result register is a leftover:
    `CancelAuthTicket`, `StartVoiceRecording`, `StopVoiceRecording`,
    `TerminateGameConnection`, `TrackAppUsageEvent`.
- **Why no experiment:** the functions without a result are ruled out only
  by the bytes the call writes to the address in `rdx`, which would be the
  test account's Steam ID. ADR 0043 and 0053 do not allow a read entry for
  a call whose candidate hands back an account identifier. A script that
  compares that value with later Steam ID arguments (for example
  `SteamFriends017` position 36 in Please, Touch The Artwork 2), printing
  only match or mismatch, needs its own ADR and the maintainer.

### `SteamUser023` and `SteamUser021` position 1: candidates, no experiment

- **Recorded:** `SteamUser023` position 1 once or twice early in Bongo Cat,
  An Arcade Full of Cats, One's Own Exile, A Park Full of Cats and Fallout
  Shelter, result 1; `rdx` held 0x18 or 1. `SteamUser021` position 1 once in
  HoloCure, `rdx` an address in the heap, result 1.
- **Every function on the page that fits a result of 1:** the no-argument
  true/false checks `BLoggedOn`, `BIsBehindNAT`, `BIsPhoneIdentifying`,
  `BIsPhoneRequiringVerification`, `BIsPhoneVerified`,
  `BIsTwoFactorEnabled`; `GetHSteamUser` (the user handle; the result of
  `ConnectToGlobalUser` was 1 in the runs of experiment 7);
  `GetPlayerSteamLevel` and `GetGameBadgeLevel` (a level of 1);
  `BSetDurationControlOnlineState` (true when set); `StartVoiceRecording`
  and `StopVoiceRecording` (a leftover of 1). With 0x18 or 1 in `rdx`,
  functions that write through a pointer in `rdx` (`GetAvailableVoice`) or
  read a text from it (`GetAuthTicketForWebApi`, `RequestStoreAuthURL`)
  would fault, so they are out for `023`; for `021` (one call, `rdx` a heap
  address) they are not, and `GetAuthTicketForWebApi` returns a ticket
  handle, so no read is planned for that position.
- **Why no experiment:** `BLoggedOn` and `GetHSteamUser` both return 1 for
  a signed-in client; only a run with the client offline would tell them
  apart, which changes the sign-in state (stop and ask, `LOOP.md`).

### `SteamUser020` position 12: candidate `GetVoiceOptimalSampleRate`

- **Recorded:** Cave Crawlers (reference run and two agent runs) and Outcore
  called it once early; the result was the same number every time, from
  11025 to 48000 (the value is not written here: until the meaning is
  settled it could be the account's Steam level), `rdx` 0xf in Cave
  Crawlers and a stack address in Outcore.
- **Every function on the page, by shape** (the arguments in Cave Crawlers
  were 0xf, a heap address and 0x33a):
  - return a number without needing a valid pointer:
    `GetVoiceOptimalSampleRate` (uint32, the voice decoder's native rate;
    the page gives 11025 to 48000 as the decoder's range),
    `GetPlayerSteamLevel`, `GetGameBadgeLevel`, `GetHSteamUser`,
    `GetDurationControl` (a call handle), `UserHasLicenseForApp` (an enum;
    `rdx` would be a Steam ID), `GetVoice(bWantCompressed, …)` (an enum;
    its second argument, `r8`, is the destination buffer), the true/false
    functions;
  - need a valid pointer in `rdx`, which 0xf is not:
    `InitiateGameConnection`, `GetAuthSessionTicket`,
    `GetAuthTicketForWebApi`, `GetAvailableVoice`, `DecompressVoice`,
    `GetUserDataFolder`, `GetEncryptedAppTicket`, `RequestEncryptedAppTicket`,
    `RequestStoreAuthURL`, `BeginAuthSession`;
  - return nothing, so the result register is a leftover:
    `StartVoiceRecording`, `StopVoiceRecording`, `CancelAuthTicket`,
    `EndAuthSession`, `AdvertiseGame`, `TerminateGameConnection`,
    `TrackAppUsageEvent`;
  - `GetSteamID` returns through the address in `rdx` (0xf).
- **Ruled out by the prediction below:** the true/false functions (the
  value is not 0 or 1), `GetHSteamUser` (the result is compared with the
  user handle), `GetDurationControl` (a call handle; those seen in
  `Steam_GetAPICallResult` are 64-bit values, not the same small number in
  two games and three runs), `GetGameBadgeLevel` (the page bounds it: "the
  regular badge (max level 5) and the foil badge (max level 1)"),
  `GetSteamID` (0xf is not writable).
- **Residual judgement (for the review):** `GetPlayerSteamLevel` has no
  upper limit on the page; it is ruled out only because a Steam level of
  at least 11025 for the test account is not credible. `UserHasLicenseForApp`
  (0xf is not a Steam ID) and `GetVoice` (it would write voice data into
  the heap buffer in `r8`) return enums whose values were not checked on
  the page; they are ruled out only because an enum value of at least
  11025 is not credible. The void functions are ruled out only because a
  leftover that is the same value in the decoder's range in two games and
  three runs is not credible. So a match
  of experiment 18 records position 12 as **narrowed** to
  `GetVoiceOptimalSampleRate`, not as confirmed.

### `SteamFriends017` and `SteamFriends018`

- **Recorded** (reference runs):
  - `017` position 43, Cave Crawlers: 14 logged calls, `rdx` a key as text
    (`steam_display`, `status`, `steam_player_group`,
    `steam_player_group_size`), `r8` a value as text (for example
    `#Status`, `Main Menu`) in 12 of them, result 1. Position 44 came
    directly before the first call of each group (no argument used, result
    an address in the heap).
  - `017` position 3, Please, Touch The Artwork 2 (once) and Fallout
    Shelter (13 times): `rdx` 4, result a small number. Position 4 followed:
    `rdx` an address on the stack and the result that same address, `r8`
    0, 1, 2, … below the position 3 result, `r9` 4.
  - `017` position 36, Please, Touch The Artwork 2: `rdx` a Steam ID; the
    first call (right after `SteamUser023` position 2) returned a small
    number that was then given to `SteamUtils010` positions 5 and 6
    (`GetImageSize`, `GetImageRGBA`, confirmed; `r9` of the latter 0x21100,
    a 184 by 184 by 4 byte buffer); the next calls, with the Steam IDs from
    position 4, returned -1 or a positive handle. After each -1 came position 37
    with the same Steam ID and `r8` 0, result 0.
  - `017` position 0 (Paper Lily, An Arcade Full of Cats, Dark Deception,
    UNBEATABLE, A Park Full of Cats): no argument used, result an address
    in the heap.
  - `018`, Bongo Cat, once each right after `SteamUser023` position 2, `rdx`
    a Steam ID: position 36 (`r8` 0) result 0, position 6 result an address
    in the heap, position 9 result 0.
- **Candidates, from the page (all 74 functions listed there checked by
  shape):**
  - **Position 43: `SetRichPresence`.** The only function on the page that
    takes two texts. An empty value removes the key (page), which fits the
    calls whose `r8` was not logged as text. Every other function taking a
    text has it together with a Steam ID, an app ID or an enum
    (`ActivateGameOverlay…`, `InviteUserToGame`, `ReplyToFriendMessage`,
    `SendClanChatMessage`, `GetFriendRichPresence`).
  - **Position 3: `GetFriendCount`, position 4: `GetFriendByIndex`.** 4 is
    `k_EFriendFlagImmediate` on the page. `GetFriendByIndex(iFriend,
    iFriendFlags)` returns a `CSteamID` through the caller's address (`rdx`),
    with the index in `r8` and the flags in `r9`. Functions of the same
    shape: `GetCoplayFriendCount`/`GetCoplayFriend` and
    `GetClanCount`/`GetClanByIndex` take no flags, so `rdx` of position 3
    and `r9` of position 4 would both be leftovers, equal to each other in
    every call; `GetFriendsGroupMembersCount(friendsGroupID)` has no index
    function returning a `CSteamID`; `GetFriendFromSourceByIndex`,
    `GetClanOfficerByIndex` and `GetChatMemberByIndex` would have a Steam ID
    in `r8`, not 0, 1, 2. **Residual judgement (for the review):**
    `GetClanCount`/`GetClanByIndex` and `GetCoplayFriendCount`/`GetCoplayFriend`
    are excluded only because a leftover in `rdx` and `r9` would not be 4
    every time; the prediction checks that `r9` follows `rdx` of position 3,
    which a script can check, but that the 4 is an argument is a judgement.
  - **Position 36: `GetLargeFriendAvatar`.** Only the three avatar
    functions return an image handle for `GetImageSize`/`GetImageRGBA`;
    only the large one documents -1 ("not loaded yet"). **Residual
    judgement:** the page gives 32 by 32 (small), 64 by 64 (medium) and
    128 by 128 (large) pixels, but the recorded buffer (184 by 184 by 4
    bytes) already contradicts the page's size for the large one, so the
    sizes on the page are not a reliable test. The prediction's buffer size
    larger than 64 by 64 by 4 bytes is kept, but it rules out the small and
    medium functions only by judgement. A -1 result, seen in the reference
    run, is not required (avatars may now be cached) and the script does
    not report whether one occurred, so a match records position 36 as
    **narrowed** to `GetLargeFriendAvatar`, not as confirmed.
  - **Position 37 and `018` position 36** (Steam ID and 0 in, 0 out):
    `RequestUserInformation(steamID, false)` ("false means that we already
    have all the details") fits the call after a -1 avatar, but so do
    `HasFriend` (flags 0), `BHasEquippedProfileItem`, `IsUserInSource`,
    `IsClanChatAdmin`, `GetFriendGamePlayed` and `GetClanActivityCounts`
    with a null pointer, `SetInGameVoiceSpeaking` (no result), and every
    function that takes only a Steam ID and returns 0 when `r8` is a
    leftover. No experiment.
  - **`018` position 6** (Steam ID in, heap address out): the functions
    taking a Steam ID that return text: `GetFriendPersonaName`,
    `GetPlayerNickname`, `GetClanName`, `GetClanTag`,
    `GetFriendPersonaNameHistory`, `GetFriendRichPresenceKeyByIndex`
    (`r8` as an index); `GetFriendRichPresence` would need a key as text in
    `r8`. Several return account data; no read, no experiment.
  - **`018` position 9** (Steam ID in, 0 out): every function on the page
    that takes a Steam ID and returns a number, enum or true/false can
    return 0 (for example `GetFriendRelationship`, `GetFriendPersonaState`,
    `GetFriendSteamLevel`, `GetFriendCoplayTime`, the avatar functions,
    `IsClanPublic`). No candidate stands out; no experiment.
  - **Positions 0 and 44** (no argument, heap address out): the page has
    one function without arguments that returns text, `GetPersonaName`,
    and one without arguments and without a result, `ClearRichPresence`,
    whose leftover can be an address. Position 44 coming directly before
    each group of `SetRichPresence` calls fits `ClearRichPresence`; nothing
    recorded rules out either for either position. Telling them apart
    needs the result text, which for `GetPersonaName` is account data
    (ADR 0043). No experiment. ADR 0056 now allows comparing position 0's
    text with the persona name without reading it out; see "Shadow
    comparison for `SteamFriends017` position 0" at the end. **Position 0
    is now confirmed as `GetPersonaName` (experiment 33, under A1)**;
    position 44 stays narrowed.

### `SteamController007`/`008` position 2 and `SteamInput001` position 2 / `SteamInput006` position 3: `RunFrame`

- **Recorded:** in all 18 games with these objects, the `SteamInput` call
  came first and was followed directly, on the same thread, by the
  `SteamController` call, about once per frame, mostly directly before a
  run of `Steam_BGetCallback` calls. `rdx` was 0, except `SteamInput006` in
  Baldi's Basics and HoloCure (1 in calls the games make themselves after
  `Init`) and `SteamInput001`, where `rdx` held one address per game. In
  most games both calls returned one and the same register value
  (0x7ffcea7d4dc0 in most, 0 in Chronigma); in Baldi's Basics and HoloCure
  the two differed.
- **Every function on the pages with the shape** (no argument or a
  true/false argument, called every frame):
  - `ISteamController` (34 functions): `RunFrame` (no result; "performed
    automatically by SteamAPI_RunCallbacks"), `Init` and `Shutdown`
    ("always returns true"). The other 31 take a controller handle, a name
    or an output pointer; with 0 in `rdx` those that return a value would
    return a handle, a count, a struct through `rdx` (0, which would
    fault) or true/false, and the void ones (`ActivateActionSet`,
    `ActivateActionSetLayer`, `DeactivateActionSetLayer`,
    `DeactivateAllActionSetLayers`, `StopAnalogActionMomentum`,
    `SetLEDColor`, `TriggerHapticPulse`, `TriggerRepeatedHapticPulse`,
    `TriggerVibration`) act on one controller.
  - `ISteamInput` (48 functions): `RunFrame(bReservedValue)` (no result,
    also "performed automatically by SteamAPI_RunCallbacks"),
    `EnableDeviceCallbacks` (no argument, no result),
    `EnableActionEventCallbacks(pCallback)` (no result; 0 in `rdx` would be
    a null callback), and the true/false functions `BNewDataAvailable`,
    `BWaitForData`, `Init` (confirmed as position 0 of `006`), `Shutdown`;
    the void ones that act on one controller are as above
    (`ActivateActionSet`, `ActivateActionSetLayer`,
    `DeactivateActionSetLayer`, `DeactivateAllActionSetLayers`,
    `StopAnalogActionMomentum`, `SetLEDColor`, `Legacy_TriggerHapticPulse`,
    `Legacy_TriggerRepeatedHapticPulse`, `TriggerSimpleHapticEvent`,
    `TriggerVibration`, `TriggerVibrationExtended`,
    `SetDualSenseTriggerEffect`); the rest return a value and take a
    handle, a name or a pointer.
- **What rules them out** (experiment 16):
  - A: a true/false function sets the lowest result byte to 0 or 1 in every
    call, so one call with another lowest byte rules out all of them.
  - D: no other position of the two interfaces is called in the game. Both
    pages say `Init` "must be called when starting use of" the interface,
    and a per-controller function needs a handle from
    `GetConnectedControllers` or `GetControllerForGamepadIndex` (other
    positions); a game that calls neither cannot be acting on a controller
    or setting up action callbacks every frame. That, and B (the pair
    comes directly before the callback pump) and C (the same leftover from
    both), fit the pages' statement that `SteamAPI_RunCallbacks` runs
    `RunFrame` on both interfaces.
  - **Residual judgement (for the review):** the void functions
    (`EnableDeviceCallbacks`, `EnableActionEventCallbacks(NULL)` and the
    per-controller ones called with handle 0) return a leftover just like
    `RunFrame` would, so they are ruled out by D and the `Init` rule, not
    by a byte a script reads.
- **`SteamInput001`:** no page describes that version, and the address in
  `rdx` is not explained by `RunFrame(bool)` (its lowest byte, 0xd0 or 0xc0,
  is not 1). A match records `SteamInput001` position 2 only as
  **narrowed** to a `RunFrame`-like per-frame call, not as confirmed.

### Experiment 16 (planned 2026-11-22, before the runs)

Agent runs (`tools/shim/run.sh <app id>`, each closed by a close request
after about 30 seconds or more) of Cave Crawlers, Dark Deception and
UNBEATABLE [white label] (`SteamController007` + `SteamInput001`) and of
Bongo Cat, Linsips and Cartapli : Fold Quest (`SteamController008` +
`SteamInput006`). No input.

**Predictions** (`tools/shim/experiments/016_run_frame.py`), for each game,
C the `SteamController` call and I the `SteamInput` call:

- A: C and I are each logged at least 20 times, and each returns, in at
  least one call, a lowest byte other than 0 and 1.
- B: every logged I call is followed directly, on its thread, by a C call,
  and at least 10 of these pairs are followed directly by
  `Steam_BGetCallback`.
- C: within a game, all logged C and I calls return one and the same value.
- D: within a game, no other position of the game's `SteamController` and
  `SteamInput` versions is called.

All six games are needed, one log each, recorded by `run.sh` in
`starts.log` and started at or after the plan commit (the script has its
time). A match confirms `SteamController007`/`008` position 2 and
`SteamInput006` position 3 as `RunFrame` (with the residual judgement
above, for the review) and narrows `SteamInput001` position 2.

### Experiment 17 (planned 2026-11-22, before the runs)

Agent runs of Cave Crawlers, Please, Touch The Artwork 2 and Fallout
Shelter, each closed by a close request after about 30 seconds or more. No
input; the calls predicted came within the first 3 seconds in the reference
runs (the Cave Crawlers main menu sets its rich presence at about 2
seconds).

**Predictions** (`tools/shim/experiments/017_friends.py`):

- A (Cave Crawlers): `SteamFriends017` position 43 is called; every call
  has a text in `rdx` and returns true; at least one also has a text in
  `r8`.
- B (Please, Touch The Artwork 2; Fallout Shelter): position 3 is called,
  always with 4 in `rdx`; position 4 is called, every call after a
  position 3 call, returning its `rdx`, with `r9` equal to `rdx` of the
  latest position 3 call and `r8` from 0 to below that call's result.
- C (Please, Touch The Artwork 2): position 36 is called; every result is
  -1, 0 or positive, at least one is positive, and every positive result is
  later given to `SteamUtils010` position 5 and to position 6 with a buffer
  larger than 64 by 64 by 4 bytes.

Friend counts, indices, Steam IDs and avatar results are compared inside
the script; for parts B and C only match or mismatch is printed (no call
counts, no handle counts, no result kinds, which could reveal the friend
count). All three games are needed, one log each, started at or after the
plan commit. A match confirms position 43 as `SetRichPresence` and
positions 3 and 4 as `GetFriendCount` and `GetFriendByIndex` (with the
residual judgement above, for the review), and narrows position 36 to
`GetLargeFriendAvatar`.

**Implementing** `SetRichPresence`, `GetFriendCount` and `GetFriendByIndex`
in the shim waits for the maintainer, even after a match: they act on or
hand out account and friends data (ADR 0051 keeps actions on the account
and friends with the maintainer).

### Experiment 18 (planned 2026-11-22, before the runs)

An agent run of Cave Crawlers (it can be the run of experiments 16 and 17).

**Prediction** (`tools/shim/experiments/018_voice_sample_rate.py`):
`SteamUser020` position 12 is called; every call returns one and the same
value from 11025 to 48000 (upper 32 bits zero), different from the user
handle `SteamClient020` position 2 returned. The script prints only that
verdict, not the value (it could be the account's Steam level). The log
must be started at or after the plan commit. Because of the residual
judgement above, a match records position 12 only as **narrowed** to
`GetVoiceOptimalSampleRate`, not as confirmed.

**Before the runs:** while planning, the three scripts were tried on the
reference logs and on earlier agent runs (`observe-48520`, `observe-31272`,
`observe-42504`, `observe-9812`, `observe-54100`, `observe-47280`) to check
that they read the logs correctly; those logs are not evidence for these
experiments, which are evaluated only on runs made after this plan is
committed. Each script rejects a log whose `start` line in `starts.log`
(read with the new `observelog.started_at`) is earlier than the plan commit
time written into the script (`LOOP.md`). No proxy read (`READS`) or
callback payload entry is added.

**Corrections from the review (before any run):** prediction D added to
experiment 16; parts B and C of experiment 17 print only match or
mismatch; experiment 18 prints no value; the residual judgements and the
"narrowed" outcomes above were written down; the void functions and
`GetVoice` were added to the list for position 12, and `GetGameBadgeLevel`
is ruled out by the page's limits (5 and 1).

**Results of experiments 16 to 18** (agent runs by the main session after
the plan time 12:42:38; logs recorded by `run.sh` in `starts.log`:
`observe-52912` Cave Crawlers (used for 16, 17 and 18), `observe-51828`
Dark Deception, `observe-55256` UNBEATABLE [white label], `observe-51224`
Bongo Cat, `observe-55108` Linsips, `observe-50768` Cartapli : Fold Quest,
`observe-42892` Please, Touch The Artwork 2, `observe-29128` Fallout
Shelter): all three **match**, checked by the scripts.

```text
observe-52912.log (Cave Crawlers) A: 53 SteamController007#2 and 53 SteamInput001#2 calls, a result byte other than 0/1: True/True -> match
observe-52912.log (Cave Crawlers) B: 53 pairs, 0 unpaired, 49 followed by Steam_BGetCallback -> match
observe-52912.log (Cave Crawlers) C: 1 distinct result values -> match
observe-52912.log (Cave Crawlers) D: other positions called: none -> match
observe-51828.log (Dark Deception) A: 68 SteamController007#2 and 68 SteamInput001#2 calls, a result byte other than 0/1: True/True -> match
observe-51828.log (Dark Deception) B: 68 pairs, 0 unpaired, 35 followed by Steam_BGetCallback -> match
observe-51828.log (Dark Deception) C: 1 distinct result values -> match
observe-51828.log (Dark Deception) D: other positions called: none -> match
observe-55256.log (UNBEATABLE [white label]) A: 67 SteamController007#2 and 67 SteamInput001#2 calls, a result byte other than 0/1: True/True -> match
observe-55256.log (UNBEATABLE [white label]) B: 67 pairs, 0 unpaired, 65 followed by Steam_BGetCallback -> match
observe-55256.log (UNBEATABLE [white label]) C: 1 distinct result values -> match
observe-55256.log (UNBEATABLE [white label]) D: other positions called: none -> match
observe-51224.log (Bongo Cat) A: 58 SteamController008#2 and 58 SteamInput006#3 calls, a result byte other than 0/1: True/True -> match
observe-51224.log (Bongo Cat) B: 58 pairs, 0 unpaired, 49 followed by Steam_BGetCallback -> match
observe-51224.log (Bongo Cat) C: 1 distinct result values -> match
observe-51224.log (Bongo Cat) D: other positions called: none -> match
observe-55108.log (Linsips) A: 127 SteamController008#2 and 127 SteamInput006#3 calls, a result byte other than 0/1: True/True -> match
observe-55108.log (Linsips) B: 127 pairs, 0 unpaired, 117 followed by Steam_BGetCallback -> match
observe-55108.log (Linsips) C: 1 distinct result values -> match
observe-55108.log (Linsips) D: other positions called: none -> match
observe-50768.log (Cartapli : Fold Quest) A: 72 SteamController008#2 and 72 SteamInput006#3 calls, a result byte other than 0/1: True/True -> match
observe-50768.log (Cartapli : Fold Quest) B: 72 pairs, 0 unpaired, 59 followed by Steam_BGetCallback -> match
observe-50768.log (Cartapli : Fold Quest) C: 1 distinct result values -> match
observe-50768.log (Cartapli : Fold Quest) D: other positions called: none -> match
experiment 16: match
```

```text
observe-52912.log (Cave Crawlers) A: 2 position-43 calls, 2 with two texts -> match
observe-42892.log (Please, Touch The Artwork 2) B: positions 3 and 4 checked -> match
observe-42892.log (Please, Touch The Artwork 2) C: position 36 checked -> match
observe-29128.log (Fallout Shelter) B: positions 3 and 4 checked -> match
experiment 17: match
```

```text
observe-52912.log (Cave Crawlers): one value from 11025 to 48000, not a user handle -> match
experiment 18: match
```

As the plan fixed beforehand, under ADR 0051:

- confirmed: `SteamController007` and `SteamController008` position 2 and
  `SteamInput006` position 3 as `RunFrame` (with the residual judgement
  written above); `SteamFriends017` position 43 `SetRichPresence`,
  3 `GetFriendCount`, 4 `GetFriendByIndex` (with the residual judgement
  written above);
- narrowed, still candidates: `SteamInput001` position 2 (a
  `RunFrame`-like per-frame call), `SteamFriends017` position 36
  (`GetLargeFriendAvatar`), `SteamUser020` position 12
  (`GetVoiceOptimalSampleRate`).

Not in the shim yet. `SetRichPresence`, `GetFriendCount` and
`GetFriendByIndex` wait for the maintainer even then (account and friends
data).

## More callbacks: 703 and 0x44e (loop iteration 10a)

Pages read 2026-11-22, both opened without signing in:
<https://partner.steamgames.com/doc/api/ISteamUtils> (callback structs,
`GetAPICallResult`, `IsAPICallCompleted`, `GetAPICallFailureReason`) and
<https://partner.steamgames.com/doc/api/ISteamUserStats> (callback and call
result structs; which functions trigger `UserStatsStored_t`; which
functions return `SteamAPICall_t`). No page gives a callback's numeric
identifier or a struct's packing.

**Logs looked at** (step 4d logs recorded by `run.sh`, raw logs stay
local): those of experiment 19 (`observe-51204`, `observe-12256`,
`observe-51224`, `observe-25680`, `observe-52912`), the runs after it
(`observe-51828` Dark Deception, `observe-55256` UNBEATABLE [white
label], `observe-55108` Linsips, `observe-50768` Cartapli : Fold Quest,
`observe-42892` Please, Touch The Artwork 2, `observe-29128` Fallout
Shelter), and the experiment 20 and 21 logs of Cave Crawlers. Payload
bytes were compared inside scripts only; nothing below quotes them.

**Packing.** Experiment 21 found the fields of 0x44d 4-byte packed (the
Steam ID at 12 to 20) and left the size 24 unexplained. Natural alignment
is therefore not assumed: a size below rules a struct out only if the
struct cannot fit in the recorded size under any packing. The predictions
rest on content and timing.

### 703 (0x2bf): candidate `SteamAPICallCompleted_t`

- **Recorded:** 703 callbacks (size 16) in Bongo Cat (6), Coin Maker (12),
  Fallout Shelter (29), Cartapli (4), Please, Touch The Artwork 2 (3) and
  Linsips (1). In all but Linsips each 703 window held one
  `Steam_GetAPICallResult` call. Its `r9` depended on where its `rdx`
  came from: a handle returned earlier by
  `STEAMUSERSTATS_INTERFACE_VERSION013` position 15 (candidate
  `RequestUserStats`) came with `r9` 24 (Bongo Cat, Cartapli); one from
  `…VERSION013` positions 22 and 37 and from `…VERSION012` position 23
  with 16; one from `…VERSION012` position 29 with 24; one from
  `STEAMINVENTORY_INTERFACE_V003` position 26 with 8. Handles with no
  earlier source in the log came with 32 or 792.
- **Linsips (`observe-55108`) breaks experiment 19's D and E** (see the
  correction note at the end of the loop iteration 9b section): it freed
  its one 703 callback without calling `Steam_GetAPICallResult` (the next
  callback was a 0x44d), and it called `Steam_FreeLastCallback` 11 times
  for 10 headers.
- **Every callback on the `ISteamUtils` page** (LOOP step 3; fields as the
  page lists them): `CheckFileSignature_t` (one enum),
  `GamepadTextInputDismissed_t` (`bool`, `uint32`), `LowBatteryPower_t`
  (`uint8`), and `FloatingGamepadTextInputDismissed_t`, `IPCountry_t`,
  `AppResumingFromSuspend_t` and `SteamShutdown_t` (no fields): none
  fills 16 bytes under any packing. `SteamAPICallCompleted_t`
  (`m_hAsyncCall` `SteamAPICall_t`, `m_iCallback` `int`, `m_cubParam`
  `uint32`) has 16 bytes with a 64-bit handle under 4-byte packing and
  natural alignment alike (8 + 4 + 4, no padding either way; the 64 bits
  are derived from the recorded `rdx` values, experiment 19, not read on
  a page). Callbacks of other pages could also have 16 bytes; F rules out
  every struct that does not hold, at 0..8, the handle the game then
  passes to `Steam_GetAPICallResult` and, at 12..16, the size it passes.
- **`Steam_GetAPICallResult` against the other `ISteamUtils` functions
  that take a call handle:** `IsAPICallCompleted` (handle, failure flag)
  and `GetAPICallFailureReason` (handle) take no size. Ruling them out is
  an inference from F: if `r9` always equals the size of the finished
  result, `r9` is read as an argument carrying that size, which only
  `GetAPICallResult` takes; a leftover that always equals the size is not
  strictly excluded.
- **Every `SteamAPICall_t` function on the `ISteamUserStats` page** (G):
  `RequestUserStats` (call result `UserStatsReceived_t`),
  `RequestGlobalStats` (`GlobalStatsReceived_t`),
  `RequestGlobalAchievementPercentages`
  (`GlobalAchievementPercentagesReady_t`), `FindLeaderboard` and
  `FindOrCreateLeaderboard` (`LeaderboardFindResult_t`),
  `DownloadLeaderboardEntries` and `DownloadLeaderboardEntriesForUsers`
  (`LeaderboardScoresDownloaded_t`), `UploadLeaderboardScore`
  (`LeaderboardScoreUploaded_t`), `AttachLeaderboardUGC`
  (`LeaderboardUGCSet_t`), `GetNumberOfCurrentPlayers`
  (`NumberOfCurrentPlayers_t`). Only `RequestUserStats` finishes as
  `UserStatsReceived_t` (1101, experiment 21).

### 0x44e (1102): candidate `UserStatsStored_t`

- **Recorded:** Cave Crawlers (`…VERSION011`) called position 10 three
  times (about 1.2 and 2.4 seconds after start, the first right after
  position 4, candidate `SetStat`); three 0x44e callbacks followed, 7,
  139 and 137 ms after the call of the same rank. Bongo Cat
  (`…VERSION013`) called position 9 five times (four within 2 ms at about
  1.45 s, one at about 61 s); five 0x44e callbacks followed, 1 to 7 ms
  after. All eight had size 16, bytes 0..4 the game's app ID, bytes 4..8
  zero, bytes 8..12 equal to 1, bytes 12..16 zero. No 0x44e in any log
  without those calls. Both trigger positions returned 1 and had no text
  argument. The lowest byte of `rdx` was 15 in the first Cave Crawlers
  call of every recording (later calls 0, 64 or 192) and 24 in every
  Bongo Cat call. (`…VERSION012` position 10, narrowed to `StoreStats` or
  `ResetAllStats` in experiment 13, see the correction there, was seen
  only at close in logs from before step 4d; those games polled no
  callback after it, so they cannot test 0x44e.)
- **Every callback and call result struct on the `ISteamUserStats` page**
  (LOOP step 3):
  - `UserStatsStored_t` (`m_nGameID` `uint64`, `m_eResult`): fits the
    content (game ID, then result). The page says `StoreStats` and
    `IndicateAchievementProgress` trigger it, and that `ResetAllStats`
    "automatically calls StoreStats"; all three return `bool`.
  - `GlobalStatsReceived_t` and `GlobalAchievementPercentagesReady_t`
    (the same two fields): fit the content too; the page names them as
    call results of `RequestGlobalStats` and
    `RequestGlobalAchievementPercentages`, which return `SteamAPICall_t`.
    C rules them out: the trigger's result has lowest byte 1 and upper
    32 bits zero, not a 64-bit handle (every recorded handle used the
    upper 32 bits), and no 703 between trigger and callback announces a
    finished result with number 1102.
  - `UserStatsReceived_t`: is 0x44d (experiment 21).
  - `UserAchievementStored_t` (`uint64`, `bool`, a name of
    `k_cchStatNameMax` chars, two `uint32`): needs at least
    8 + 1 + 1 + 4 + 4 = 18 bytes, more than 16 under any packing.
  - `UserAchievementIconFetched_t` (`CGameID`, a name, `bool`, `int`):
    bytes 8..12 would start the name; B (byte 8 is 1, bytes 9..12 zero)
    leaves no name.
  - `UserStatsUnloaded_t` (`m_steamIDUser` only): B (bytes 4..8 zero)
    rules it out; the upper half of the Steam ID is not zero (experiment
    21, B).
  - `LeaderboardFindResult_t`, `LeaderboardScoresDownloaded_t`,
    `LeaderboardScoreUploaded_t`, `LeaderboardUGCSet_t`,
    `NumberOfCurrentPlayers_t`: call results of `SteamAPICall_t`
    functions (C), and none starts with a game ID (B).
  - Callbacks of other pages with a game ID and a result, triggered by the
    same call, are not ruled out (as in experiment 20). The meaning below
    is claimed only among the structs of the `ISteamUserStats` page.
- **The trigger positions:** the page names three functions that lead to
  `UserStatsStored_t`: `StoreStats` (no arguments),
  `IndicateAchievementProgress` (an achievement name and two numbers) and
  `ResetAllStats` (one `bool`, "automatically calls StoreStats"). D (no
  text argument) rules out `IndicateAchievementProgress`. E tells
  `StoreStats` from `ResetAllStats`: the Microsoft x64 calling convention
  (<https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention>)
  passes the second integer argument of a call, the first after the
  object in `rcx`, in `rdx`, "right-justified", so a `bool` argument
  sits in the lowest byte of `rdx`. That a `bool` is passed as 0 or 1 is
  **an assumption** (the page says only that the callee may ignore the
  upper bits; the value of a `bool` is not on it). A lowest byte other
  than 0 or 1 therefore rules out `ResetAllStats` under that assumption.
  Candidates: `…VERSION011` position 10 and `…VERSION013` position 9
  `StoreStats` or `ResetAllStats`.

### The other recorded numbers

304, 336, 1222, 1281, 4700, 4701, 4702, 1040011, 1040015, 1040044,
1080000 and 1080001: **no candidate** in this iteration. No page gives
numeric identifiers, and none of them follows a call whose meaning is
confirmed closely enough to point at one struct (4700 to 4702 come near
the stats calls of Cave Crawlers and Bongo Cat, 304 and 336 in pairs about
2.5 s after start in Cave Crawlers, 1222 and 1281 only in games that also
have 703). Their payloads are not asked for.

### Experiment 22 (planned 2026-11-22, before the runs)

**Needs `PAYLOAD_LISTED`:** 703 (0x2bf). Reason: F, G and H compare its
payload bytes with the registers of `Steam_GetAPICallResult` and with
results of earlier calls. Per the `ISteamUtils` page it carries a call
handle, a callback number and a size: no Steam ID, name, ticket, token or
password (ADR 0054). Added by the main session before the runs.

**Predictions** (`tools/shim/experiments/022_api_call_completed.py`; a
703 window is the time from a 703 header to the next
`Steam_FreeLastCallback` call on the same thread, experiment 19):

- P: every 703 callback has size 16 and its first payload bytes logged.
- F: for every window with a `Steam_GetAPICallResult` call, payload bytes
  0..8 equal its `rdx` (all 64 bits) and bytes 12..16 the low 32 bits of
  its `r9`; at least 10 windows checked across all logs; a log with 50 or
  more such calls (the proxy logs only the first 50) is a mismatch.
- G: every 703 whose bytes 0..8 equal a result of a
  `STEAMUSERSTATS_INTERFACE_VERSION013` position 15 call made before that
  703 has bytes 8..12
  = 1101 (0x44d, `UserStatsReceived_t`) and bytes 12..16 = 24; checked at
  least once in each of Bongo Cat, Cartapli and Linsips (Linsips calls no
  `GetAPICallResult`, so G links through position 15's result, not
  `rdx`).
- H: windows whose `GetAPICallResult` handle came from the same interface
  position all carry one number in bytes 8..12 (the script prints only
  how many numbers per position).

**Games to run:** Bongo Cat (3419430), Coin Maker Simulator Demo
(5191630), Fallout Shelter (588430), Cartapli : Fold Quest (4314560),
Linsips (4301370); one fresh log each, all five needed, each run to its
first screen and closed by a close request.

**What a full match confirms:**

- callback 703 is `SteamAPICallCompleted_t`: bytes 0..8 the call handle
  and 12..16 the size of the finished result (F);
- jointly, by G: bytes 8..12 are the callback number of the finished
  result, and `STEAMUSERSTATS_INTERFACE_VERSION013` position 15 is
  `RequestUserStats` (its handle finishes with 1101 =
  `UserStatsReceived_t`, and among the `SteamAPICall_t` functions of the
  page listed above only `RequestUserStats` finishes as that struct). The
  two readings rest on each other: G could not match if either were
  wrong, but neither is shown without the other;
- `Steam_GetAPICallResult` is `GetAPICallResult`, with `rdx` the call
  handle and `r9` the buffer size (the inference from F above rules out
  `IsAPICallCompleted` and `GetAPICallFailureReason`).

**What stays a candidate:** the numbers in bytes 8..12 for the other
source positions (H only shows one per position); the arguments on the
stack (expected number, failure flag), which are not logged; experiment
19's parameter order inference beyond `rdx` and `r9`; whether `rcx` is the
pipe or the user handle.

### Experiment 23 (planned 2026-11-22, before the runs)

0x44e is already in `PAYLOAD_LISTED` (experiment 20); nothing new to list.

**Predictions** (`tools/shim/experiments/023_stats_stored.py`; trigger:
`…VERSION011` position 10 in Cave Crawlers, `…VERSION013` position 9 in
Bongo Cat):

- A: the trigger is called at least once and there are exactly as many
  0x44e callbacks as trigger calls; paired in order, each callback comes
  0 to 1000 ms after its call; none before the first call.
- B: every 0x44e has size 16, bytes 0..4 the app ID, bytes 4..8 zero,
  bytes 8..12 = 1 (`k_EResultOK`). Bytes 12..16 are not predicted.
- C: every trigger call's result has lowest byte 1 and upper 32 bits zero
  (a `bool` true, not a 64-bit call handle), and no 703 callback between
  a trigger call and its 0x44e has 1102 in bytes 8..12 (703 is listed for
  experiment 22; a 703 between them without logged payload bytes is a
  mismatch).
- D: no trigger call has a text argument.
- E: in each game, at least one trigger call has a lowest `rdx` byte
  other than 0 and 1.

**Games to run:** Cave Crawlers (2143030) and Bongo Cat (3419430); one
fresh log each, both needed (the Bongo Cat run of experiment 22 can serve
both scripts). Risk: if Bongo Cat calls position 9 just before the close
request and exits before polling again, A mismatches.

**What a full match confirms:**

- callback 0x44e (1102) is `UserStatsStored_t`, among the structs of the
  `ISteamUserStats` page: 16 bytes, the app ID in 0..4 and zero in 4..8
  (the game ID), the result in 8..12 (A to D rule out the other structs
  of the page, as listed above);
- `…VERSION011` position 10 and `…VERSION013` position 9 are `StoreStats`:
  D rules out `IndicateAchievementProgress`, and E rules out
  `ResetAllStats` **under the assumption above** (a `bool` argument
  arrives as 0 or 1 in the lowest byte of `rdx`). The confirmation names
  that assumption; if it is ever shown wrong, the two positions fall back
  to "narrowed: `StoreStats` or `ResetAllStats`".

**What stays a candidate:** bytes 12..16 (zero in every recording, not
explained, like the last four bytes of 0x44d); whether 0x44e ever carries
another result code; a struct of another page with the same content and
trigger.

**Freshness (both scripts):** a log whose `start` line in `starts.log` is
earlier than `PLAN_COMMITTED` is rejected; both scripts hold the commit
time of the final plan on `loop/plan-10a` (local time, as `run.sh` writes
`starts.log`): 2026-11-22 13:39:41 (commit `3185f1f`). The first plan time (13:26:55) is superseded by this revised plan.

**Already seen:** run on the existing logs with the plan time lifted,
experiment 23 gives "match" on `observe-52912` and `observe-51224` and
experiment 22 "MISMATCH" (703 payload bytes not logged yet). In an older
Bongo Cat log (`observe-47280`) three 703 callbacks fell between a
trigger call and its 0x44e; C checks those through their payload bytes. The predictions were written from these logs, so
they are not the test; only fresh runs after the plan commit are.

**Results of experiments 22 and 23** (agent runs by the main session after
the plan time 13:39:41, with 703 listed in `PAYLOAD_LISTED`; logs recorded
by `run.sh` in `starts.log`: `observe-42296` Bongo Cat (used for both),
`observe-11744` Coin Maker Simulator Demo, `observe-12404` Fallout
Shelter, `observe-22200` Cartapli : Fold Quest, `observe-45668` Linsips,
`observe-20708` Cave Crawlers): both **match**, checked by the scripts.

```text
observe-42296.log (Bongo Cat) P: 6 callbacks 703, 6 with payload bytes -> match
observe-42296.log (Bongo Cat) F: 6 windows with GetAPICallResult checked, 6 equal, 6 calls logged -> match
observe-42296.log (Bongo Cat) G: 1 callbacks 703 for a position-15 handle, 1 with number 1101 and size 24 -> match
observe-11744.log (Coin Maker Simulator Demo) P: 13 callbacks 703, 13 with payload bytes -> match
observe-11744.log (Coin Maker Simulator Demo) F: 13 windows with GetAPICallResult checked, 13 equal, 13 calls logged -> match
observe-11744.log (Coin Maker Simulator Demo) G: 0 callbacks 703 for a position-15 handle, 0 with number 1101 and size 24 -> match
observe-12404.log (Fallout Shelter) P: 29 callbacks 703, 29 with payload bytes -> match
observe-12404.log (Fallout Shelter) F: 29 windows with GetAPICallResult checked, 29 equal, 29 calls logged -> match
observe-12404.log (Fallout Shelter) G: 0 callbacks 703 for a position-15 handle, 0 with number 1101 and size 24 -> match
observe-22200.log (Cartapli : Fold Quest) P: 4 callbacks 703, 4 with payload bytes -> match
observe-22200.log (Cartapli : Fold Quest) F: 4 windows with GetAPICallResult checked, 4 equal, 4 calls logged -> match
observe-22200.log (Cartapli : Fold Quest) G: 1 callbacks 703 for a position-15 handle, 1 with number 1101 and size 24 -> match
observe-45668.log (Linsips) P: 1 callbacks 703, 1 with payload bytes -> match
observe-45668.log (Linsips) F: 0 windows with GetAPICallResult checked, 0 equal, 0 calls logged -> match
observe-45668.log (Linsips) G: 1 callbacks 703 for a position-15 handle, 1 with number 1101 and size 24 -> match
F across all logs: 52 windows checked (at least 10) -> match
    H: STEAMINVENTORY_INTERFACE_V003 position 26: 1 number(s)
    H: STEAMUSERSTATS_INTERFACE_VERSION012 position 23: 1 number(s)
    H: STEAMUSERSTATS_INTERFACE_VERSION012 position 29: 1 number(s)
    H: STEAMUSERSTATS_INTERFACE_VERSION013 position 15: 1 number(s)
    H: STEAMUSERSTATS_INTERFACE_VERSION013 position 22: 1 number(s)
    H: STEAMUSERSTATS_INTERFACE_VERSION013 position 37: 1 number(s)
H: 6 source positions, one number each -> match
experiment 22: match
```

```text
observe-20708.log (Cave Crawlers) A: 3 calls of position 10, 3 callbacks 0x44e, gaps [4, 126, 124] ms -> match
observe-20708.log (Cave Crawlers) B: size, app ID, zero bytes 4..8 and result 1 in every callback -> match
observe-20708.log (Cave Crawlers) C: 3 of 3 results a bool true, 0 callbacks 703 in between (0 without payload bytes, 0 announcing 1102) -> match
observe-20708.log (Cave Crawlers) D: 0 trigger calls with a text argument -> match
observe-20708.log (Cave Crawlers) E: 3 of 3 trigger calls with a lowest rdx byte other than 0 and 1 -> match
observe-42296.log (Bongo Cat) A: 7 calls of position 9, 7 callbacks 0x44e, gaps [6, 7, 7, 7, 0, 1, 0] ms -> match
observe-42296.log (Bongo Cat) B: size, app ID, zero bytes 4..8 and result 1 in every callback -> match
observe-42296.log (Bongo Cat) C: 7 of 7 results a bool true, 0 callbacks 703 in between (0 without payload bytes, 0 announcing 1102) -> match
observe-42296.log (Bongo Cat) D: 0 trigger calls with a text argument -> match
observe-42296.log (Bongo Cat) E: 7 of 7 trigger calls with a lowest rdx byte other than 0 and 1 -> match
experiment 23: match
```

As the plans fixed beforehand, under ADR 0051:

- confirmed: callback 703 is `SteamAPICallCompleted_t` (handle in 0..8,
  callback number in 8..12, size in 12..16); `Steam_GetAPICallResult` is
  `GetAPICallResult` (`rdx` the handle, `r9` the buffer size);
  `STEAMUSERSTATS_INTERFACE_VERSION013` position 15 is `RequestUserStats`;
- confirmed: callback 0x44e (1102) is `UserStatsStored_t` (among the
  structs of the `ISteamUserStats` page); `…VERSION011` position 10 and
  `…VERSION013` position 9 are `StoreStats`, under the stated assumption
  that a `bool` argument arrives as 0 or 1 in the lowest byte of `rdx`;
- the "What stays a candidate" items above stay candidates.

Linsips again freed its 703 without `Steam_GetAPICallResult` (F: 0
windows), as in the correction note of the 9b section.

## `SteamClient018` and `SteamClient019`: two pipes (loop iteration 11a)

- **Page:** <https://partner.steamgames.com/doc/api/ISteamClient>, read
  2026-11-22 (31 functions; no version constant, as noted for 020 to
  023), and the `EAccountType` table of
  <https://partner.steamgames.com/doc/api/steam_api> (3 is
  `k_EAccountTypeGameServer`, "Persistent (not anonymous) game server
  account").
- **Recording:** the reference runs of `coverage.md` (`observe-496`
  Gravitas, `SteamClient018`; `observe-44940` Dark Deception,
  `SteamClient019`) and the ten runs of 2026-11-22 recorded by `run.sh`
  (Gravitas `observe-22464`, `-31124`, `-47412`, `-12256`; Dark Deception
  `observe-9812`, `-53604`, `-47852`, `-52328`, `-51204`, `-51828`).
  Every run of the same game had the same sequence; the reference run of
  Gravitas stops after 134 ms with one close sequence (pipe 1 only).
- Both games make the same calls in the same order (low 32 bits; `P`,
  `U` as in experiment 7):

  | Position | Recorded (all runs) |
  | --- | --- |
  | 0 | first call; result 1 (`P`) |
  | 2 | `rdx` 1 (`P`); result 1 (`U`) |
  | 9 | `rdx` 1, later also 2, `r8` `SteamUtils009`; result with the step 3a shape |
  | 34 | once, before 5; `rdx` an address in the module range; result 0 |
  | 5 | `rdx` 1, `r8` 1, `r9` `SteamUser020`; object |
  | 12 | `rdx` 1 or 0 (0 only with `SteamUtils009`), `r8` 1 or 2, `r9` any text; object |
  | 7 | once, after the second `CreateInterface` call; `rdx` 0, `r8` 0x1e62, `r9` a stack address (the same as `r8` of position 2); `rax` an address in `steamclient64` |
  | 3 | once, right after 7; `rdx` an address in the game's module range, `r8` 3; result 1 |
  | 6 | right after 3; `rdx` 1, `r8` 2, `r9` `SteamGameServer012`; object |
  | 26, 38 | `rdx` 1, `r8` 1, `r9` `SteamController007` and `SteamInput001`; objects (in Gravitas only in its runs of 2026-11-22, not in its reference run) |
  | 4, 1, 23 | at close, twice: 4 with (`rdx`, `r8`) = (1, 1) and (2, 1), each directly followed by 1 with the same `rdx` (lowest result byte 1) and then 23; Dark Deception releases pipe 1 first, Gravitas pipe 2 first; the first 23 had lowest result byte 0, the second 1 |

- The handle 2 is the result of no call; it first appears in `r8` of
  position 6, right after position 3. With pipe 2, positions 9 and 12 gave
  other objects (other stand-ins) for the same texts (`SteamUtils009`,
  `SteamNetworking005`) than with pipe 1.

Evidence (Gravitas, `observe-12256`; floating-point registers and the
`rcx` note left out):

```text
77 ms ThreadId(1) SteamClient018#7 call 1 rcx=0x7ffceac307e0 rdx=0x0 r8=0x1e62 r9=0x7ffdcdb9efa8
77 ms ThreadId(1) SteamClient018#3 call 1 rcx=0x7ffceac307e0 rdx=0x7ffe4d8737a8 r8=0x3 r9=0x1c
128 ms ThreadId(1) SteamClient018#3 return 1 rax=0x1
128 ms ThreadId(1) SteamClient018#6 call 1 rcx=0x7ffceac307e0 rdx=0x1 r8=0x2 r9=0x7ffe4d8567b8 r9->"SteamGameServer012"
130 ms ThreadId(1) SteamClient018#9 call 2 rcx=0x7ffceac307e0 rdx=0x2 r8=0x7ffe4d856318 r9=0x7ffdcdb9efa8 r8->"SteamUtils009"
89639 ms ThreadId(1) SteamClient018#4 call 1 rcx=0x7ffceac307e0 rdx=0x2 r8=0x1 r9=0x24
89652 ms ThreadId(1) SteamClient018#1 call 1 rcx=0x7ffceac307e0 rdx=0x2 r8=0x17880a31500 r9=0x1c
89653 ms ThreadId(1) SteamClient018#1 return 1 rax=0xffffff01
89653 ms ThreadId(1) SteamClient018#23 return 1 rax=0x7ffcead24500
89653 ms ThreadId(1) SteamClient018#4 call 2 rcx=0x7ffceac307e0 rdx=0x1 r8=0x1 r9=0x73e
89653 ms ThreadId(1) SteamClient018#1 return 2 rax=0x1
89724 ms ThreadId(1) SteamClient018#23 return 2 rax=0x1
```

### Every function of the page against the recorded shapes

Argument registers follow the Microsoft x64 convention (`rcx` the
object, then `rdx`, `r8`, `r9`), as in experiment 7.

| Function (page) | Shape on the page | Fits |
| --- | --- | --- |
| `CreateSteamPipe` | no arguments; `HSteamPipe` | 0 (first call, result 1); experiment 7 for 020 to 023 |
| `ConnectToGlobalUser` | pipe; `HSteamUser` | 2 (`rdx` = `P`) |
| `CreateLocalUser` | `HSteamPipe *`, `EAccountType`; `HSteamUser`; "Only used by game servers" | 3: `rdx` an address (where the pipe would be written), `r8` 3 = `k_EAccountTypeGameServer`, result 1; position 6 then asks for `SteamGameServer012` with the new handle 2 in the pipe slot |
| `SetLocalIPBinding` | `uint32` IP, `uint16` port; nothing; "This must be set before calling CreateLocalUser!" | 7: `rdx` 0 (IP), `r8` 0x1e62 (fits 16 bits), called right before 3; `rax` a leftover |
| `SetWarningMessageHook` | a function pointer; nothing | 34 (an address in the module range, result 0); 7 also fits by shape (`rdx` 0, a null pointer) |
| `ReleaseUser` | pipe, user; nothing | 4 (`rdx` the pipe, `r8` the user) |
| `BReleaseSteamPipe` | pipe; `bool` | 1 |
| `BShutdownIfAllPipesClosed` | none; `bool`; "Triggers global shutdown for the DLL if all pipes are closed." | 23: false after the first pipe is released, true after the second |
| `GetIPCCallCount` | none; `uint32` | none: every position's result is a handle, an object, a `bool`, 0, or (7) has non-zero upper 32 bits, which a `uint32` result written to `eax` would clear (x64 zero-extends 32-bit register writes; an assumption about the compiled code, not from the page) |
| `GetISteamUtils` | pipe, text; object | 9 (only `SteamUtils…` texts, in `r8`) |
| `GetISteamGenericInterface` | user, pipe, text; object | 12 (every kind of text) |
| `GetISteamUser`, `GetISteamGameServer`, `GetISteamController` | user, pipe, text; object | 5, 6, 26 by their texts; which documented getter belongs to which number follows only from the texts, as for 020 to 023 |
| a `SteamInput` getter | not on the page | 38 takes `SteamInput001` and returns an object; no function of the page names it |
| the other `GetISteam…` (Apps, Friends, GameServerStats, HTMLSurface, HTTP, Inventory, Matchmaking, MatchmakingServers, Music, MusicRemote, Networking, RemoteStorage, Screenshots, UGC, UnifiedMessages, UserStats, Video) | user, pipe, text; object | no position of their own is called: these games ask for them only at 12 |

Candidates (for 018 and 019): 0 `CreateSteamPipe`, 2
`ConnectToGlobalUser`, 3 `CreateLocalUser`, 7 `SetLocalIPBinding`, 9
`GetISteamUtils`, 12 `GetISteamGenericInterface`, 5, 6 and 26
per-interface getters, 4 `ReleaseUser`, 1 `BReleaseSteamPipe`, 23
`BShutdownIfAllPipesClosed`, 34 `SetWarningMessageHook`; 38 hands out
`SteamInput001` objects (no page function).

### Experiment 24 (planned 2026-11-22, before the runs)

**Needs READS** (`crates/shim-observe`, ADR 0043 and 0053; added by the
main session, not by this plan's branch): `("SteamClient018", 3,
Read::OutputBytes("rdx"))` and `("SteamClient019", 3,
Read::OutputBytes("rdx"))`, so the proxy logs `after rdx->[…]` (the
first 16 bytes at the address `rdx` held) when position 3 returns. Per
the page `CreateLocalUser` writes an `HSteamPipe` there: a pipe handle,
not a ticket, token, password or account identifier. Without the entry H
mismatches.

**Predictions** (`tools/shim/experiments/024_two_pipes.py`, low 32 bits;
`V` = result of 3, `Q` = the pipe slot of the first getter call (5, 6,
9, 12, 26, 38) after 3 returned; "before 3 returned" means on a log line
before the line of 3's return):

- A: the first `SteamClient` call is 0 (result `P`), the next 2 with
  `rdx` = `P` (result `U`).
- B: 3 is called exactly once, with `r8` = 3, `rdx` at least 0x10000 (an
  address, not a handle) and 0 < `V` < 0x10000; 7 exactly once, before
  3, with `r8` at most 0xffff. A register missing from the log is a
  mismatch.
- C: `Q` is neither `P` nor 0; no call or entry point of any interface
  that returned before 3 returned had `Q` as its result, and no
  `SteamClient` call made before 3 returned has `Q` in `rdx` or `r8`.
- D: every getter call passes a pipe in {`P`, `Q`}; with `P` the user
  slot (`rdx`, none at 9) is `U` or 0, with `Q` it is `V` or 0; at least
  one call of 9 passes `Q`.
- E: every getter call passes a text of its position's kind (5
  `SteamUser…`, 6 `SteamGameServer…` without `Stats`, 9 `SteamUtils…`,
  26 `SteamController…`, 38 `SteamInput…`, 12 any), and its result has
  the step 3a shape and gets a stand-in for that text; the same (text,
  pipe) always gets the same stand-in; a text asked over both pipes gets
  two different stand-ins, and at least one text is.
- F: 4 is called exactly twice, with (`P`, `U`) and (`Q`, `V`); each is
  directly followed by 1 with the same pipe (lowest result byte 1) and
  that by 23, whose lowest result byte is 1 once both pipes are released
  and 0 before.
- G: 34 is called exactly once, before 3, with `rdx` at least
  0x7ff000000000, result 0.
- H: after 3 returned, the first four bytes at the address `rdx` held
  (`observelog.after_u32(call, "rdx")`) equal `Q`, the pipe later passed
  at 6, 9, 12 and 1.

The script rejects a log given twice, maps each log to its game through
`starts.log` (`observelog.app_of`), needs one log of each game and
rejects a log whose start (`observelog.start_of`) is earlier than the
plan commit (`PLAN_COMMITTED`, the local committer time of the commit on
`loop/plan-11a` that holds this text and the final script, set in a
follow-up commit: 2026-11-22 14:34:22, commit `e21b039` (rebased as `f51ab72`); the first plan
time 14:24:36, commit `79db875` before the rebase, is superseded). It
prints only positions, small handle values, counts and match/MISMATCH.

**Games to run:** Gravitas (1067310) and Dark Deception (332950), each
to its first screen and closed by a close request (F needs a normal
close), after the READS entry above is in the proxy. No experiment 25 is
needed.

**What a full match confirms** (under ADR 0051, for `SteamClient018` and
`019` only):

- 0 `CreateSteamPipe`, 2 `ConnectToGlobalUser`, 9 `GetISteamUtils`, 12
  `GetISteamGenericInterface`, 5, 6 and 26 the per-interface getters
  (`GetISteamUser`, `GetISteamGameServer`, `GetISteamController` by
  their texts), 4 `ReleaseUser`, 1 `BReleaseSteamPipe`, 23
  `BShutdownIfAllPipesClosed`, as experiment 7 did for 020 to 023;
- 3 `CreateLocalUser`: it writes the handle `Q` at the address it is
  given (H), the handle the game then uses as a pipe (C, D, F), and takes
  3 = `k_EAccountTypeGameServer` (B). Of the page's functions only
  `CreateLocalUser` takes an address it writes a pipe to; the
  no-argument functions are ruled out by H, in particular
  `GetIPCCallCount`, which takes no argument and writes nothing, so it
  cannot put `Q` at the address in `rdx`. (H reads the bytes only after
  the call; that they already held `Q` before is not ruled out by a
  reading, only by C: nothing before 3 returned produced `Q`.)
- **pipe and user told apart** for these versions, from the positions
  that take a pipe as their only handle (ADR 0042: no inference from
  argument order alone): 9 (`GetISteamUtils`, pipe and text) is called
  with `Q` (D) and 1 (`BReleaseSteamPipe`, a pipe only) with `Q` and
  with `P` (F), while `Q` differs from `V`; so `Q` and `P` are pipes. 23
  turning true only after both are released (F) fits. Experiment 7's open
  point stays open for 020 to 023, where both handles were 1.

**What stays a candidate:**

- 7: "narrowed: `SetLocalIPBinding` or `SetWarningMessageHook` with a
  null pointer (experiment 24)". Both return nothing and fit the
  arguments; B's order before 3 fits the page's note on
  `SetLocalIPBinding` but does not rule out the other.
- 34 `SetWarningMessageHook`: the proxy does not check whether `rdx`
  points to code; G only narrows.
- 38: hands out `SteamInput001` objects; the page names no function for
  it.
- `U` and `V` were both 1 in every run, so which of the two users a call
  passes is not told apart.

**Already seen:** run with the plan time lifted on the ten runs of
2026-11-22 listed above (the two reference runs have no `starts.log`
line, so the script cannot take them), A to G match in each; H
mismatches ("not logged"), since no READS entry existed for position 3.
The predictions were written from these logs, so they are not the test;
only fresh runs after the plan commit are.

**Result of experiment 24** (agent runs by the main session after the
plan time 14:34:22, with the two `READS` entries for position 3 committed
at 14:37:02 before the runs; logs recorded by `run.sh` in `starts.log`:
`observe-15876` Gravitas, `observe-31232` Dark Deception): **match**,
checked by the script.

```text
observe-15876.log (Gravitas, SteamClient018) A: first calls [0, 2], P=1 U=1 -> match
observe-15876.log (Gravitas, SteamClient018) B: 1 calls of 3 (r8 [3], V=1), 1 calls of 7, 7 before 3: True -> match
observe-15876.log (Gravitas, SteamClient018) C: Q=2, a result or SteamClient argument before position 3 returned: False -> match
observe-15876.log (Gravitas, SteamClient018) D: 24 getter calls, 0 with other handles, 1 calls of 9 with Q -> match
observe-15876.log (Gravitas, SteamClient018) E: 0 getter calls without the text's shape and stand-in, 0 (text, pipe) with more than one stand-in, 2 texts over both pipes, 0 of them sharing a stand-in -> match
observe-15876.log (Gravitas, SteamClient018) F: ReleaseUser (pipe, user) [(1, 1), (2, 1)], sequences [[4, 1, 23], [4, 1, 23]] -> match
observe-15876.log (Gravitas, SteamClient018) G: 1 calls of 34 -> match
observe-15876.log (Gravitas, SteamClient018) H: first four bytes at rdx after position 3: Q -> match
observe-31232.log (Dark Deception, SteamClient019) A: first calls [0, 2], P=1 U=1 -> match
observe-31232.log (Dark Deception, SteamClient019) B: 1 calls of 3 (r8 [3], V=1), 1 calls of 7, 7 before 3: True -> match
observe-31232.log (Dark Deception, SteamClient019) C: Q=2, a result or SteamClient argument before position 3 returned: False -> match
observe-31232.log (Dark Deception, SteamClient019) D: 27 getter calls, 0 with other handles, 1 calls of 9 with Q -> match
observe-31232.log (Dark Deception, SteamClient019) E: 0 getter calls without the text's shape and stand-in, 0 (text, pipe) with more than one stand-in, 2 texts over both pipes, 0 of them sharing a stand-in -> match
observe-31232.log (Dark Deception, SteamClient019) F: ReleaseUser (pipe, user) [(1, 1), (2, 1)], sequences [[4, 1, 23], [4, 1, 23]] -> match
observe-31232.log (Dark Deception, SteamClient019) G: 1 calls of 34 -> match
observe-31232.log (Dark Deception, SteamClient019) H: first four bytes at rdx after position 3: Q -> match
experiment 24: match
```

As the plan fixed beforehand, under ADR 0051, for `SteamClient018` and
`SteamClient019`: confirmed 0 `CreateSteamPipe`, 1 `BReleaseSteamPipe`,
2 `ConnectToGlobalUser`, 3 `CreateLocalUser`, 4 `ReleaseUser`, 5/6/26
the per-interface getters (`GetISteamUser`, `GetISteamGameServer`,
`GetISteamController`, by their texts), 9 `GetISteamUtils`, 12
`GetISteamGenericInterface`, 23 `BShutdownIfAllPipesClosed`; P and Q are
pipes (positions 9 and 1). Position 7 stays narrowed, 34 a candidate, 38
"objects". Now in the shim (`crates/shim/src/objects.rs`):
the confirmed positions, with getters on pipe 2 stopping (no objects for
the game server pipe).

## Three new games with stats (loop iteration 12a)

ADR 0055 ("Decided by the maintainer") asks for a third recorded game that
calls `StoreStats` before its stats-store stage; today only Cave Crawlers
(`…VERSION011` position 10) and Bongo Cat (`…VERSION013` position 9) are
recorded doing it (experiment 23). This iteration plans first recordings
of three free games from `recording-candidates.md` whose store pages list
**Stats** and **Steam Achievements**. A feature on the store page makes a
stats call likely, not certain; which `STEAMUSERSTATS_INTERFACE_VERSION0xx`
a game asks for, and which positions it calls, shows only in a recording.

| App | Name | Store page, re-read 2026-11-22 (one fetch each) | Min. disk |
| --- | --- | --- | --- |
| 1920960 | VPet-Simulator | free to play; Single-player, Online Co-op, Steam Achievements, Steam Workshop, Steam Cloud, Stats, Steam Leaderboards; no EULA, third-party account or anti-cheat named | 1 GB |
| 351030 | Pixel Puzzles Ultimate Jigsaw Puzzles | free to play (paid DLC); Single-player, Steam Achievements, Steam Cloud, Stats, Steam Leaderboards; no EULA, third-party account or anti-cheat named | 300 MB |
| 1036950 | Super Jigsaw Puzzle: Generations | free to play (paid DLC); Single-player, Steam Achievements, Stats, Steam Leaderboards, Steam Cloud; no EULA, third-party account or anti-cheat named | 650 MB |

None of the three pages says whether the game is 64-bit; that shows only
after installing (`m4-games.md`). A 32-bit game cannot be recorded by the
proxy yet (noted for M5), and its part of the experiment is then a
mismatch.

This plan names the three games, so under ADR 0044 and 0052 their free
licenses may be added to the test account for this experiment (3 of the
50 a day), each after checking size and price once more right before
adding it. Ownership stays real (red line 3); the account owns none of
their paid DLC, and nothing claims otherwise.

### Experiment 25 (planned 2026-11-22, before the runs)

0x44e and 703 are already in `PAYLOAD_LISTED` (experiments 20 and 22);
nothing new to list.

**Runs:** one fresh log per game with `tools/shim/run.sh <app id>`, each
run to the game's first screen and closed by a close request ("first
screen + close"). **No input is planned.** Under ADR 0049 input would be
allowed only to reach the screen this run needs (past a launcher or a
language choice); it is never sent to play, to change a stat or to earn an
achievement or any other reward (red lines 4 and 5). Any stats write in
these runs is one the game makes by itself through the official client, as
in experiment 23; it is not the Reborn-initiated live stats write of ADR
0055, for which the maintainer is asked again before its single run.

**Honest expectation:** games call `StoreStats` when a stat changed. A
game that changes nothing between its first screen and the close request
stores nothing, and then A to E below have nothing to check, which counts
as a **mismatch** ("nothing checked" is not a match). A store may also
come only at close; if the game exits before polling its callbacks
again, the 0x44e is missing and A mismatches (the risk named in
experiment 23). Both outcomes are recorded as they are; the run is not
repeated with input to provoke a store.

**Risk, VPet-Simulator:** a desktop pet may hide in the notification area
on a close request instead of exiting. If it has not exited two minutes
after the close request, the agent stops and reports (ADR 0044); it does
not force a kill.

**Predictions** (`tools/shim/experiments/025_new_stats_games.py`, one log
per game). The positions are not known in advance, so `StoreStats` is
predicted over **any position of the game's
`STEAMUSERSTATS_INTERFACE_VERSION0xx`**: a position T *qualifies* when
its calls pair one to one, in order, with the 0x44e callbacks of the log
(as many calls as callbacks, the i-th callback 0 to 1000 ms after the
i-th call, no 0x44e before the first call). This is the shape of
experiment 23's A, with the position found instead of named.

- S (survey, no verdict): every interface and position the game called,
  with call counts. LOOP step 2 data for later rows of `gaps.md`.
- U: the game calls at least one position of a
  `STEAMUSERSTATS_INTERFACE_VERSION0xx` interface.
- A: at least one 0x44e callback, and exactly one (interface, position)
  of the game's UserStats interfaces qualifies as T.
- B: every 0x44e has size 16, bytes 0..4 the app ID, bytes 4..8 zero,
  bytes 8..12 = 1 (`k_EResultOK`). Bytes 12..16 are not predicted.
- C: every call of T returns a `bool` true (lowest byte 1, upper 32 bits
  zero, not a call handle), and no 703 between the i-th call of T and the
  i-th 0x44e has 1102 in bytes 8..12 (a 703 in between without payload
  bytes is a mismatch).
- D: no call of T has a text argument (rules out
  `IndicateAchievementProgress`).
- E: at least one call of T has a lowest `rdx` byte other than 0 and 1
  (rules out `ResetAllStats` under experiment 23's assumption that a
  `bool` argument arrives as 0 or 1 in the lowest byte of `rdx`).
- F: if T's interface has a confirmed `StoreStats` position (`…VERSION011`
  position 10, `…VERSION013` position 9), T is that position. For any
  other version (including `…VERSION012`, whose position 10 is narrowed to
  `StoreStats` or `ResetAllStats`, experiment 13) F only prints T.

The script prints a verdict per game (U, A to F) and "experiment 25:
match" only when all three named games have a fresh log and every
prediction matches.

**What a match confirms** (fixed now, before the runs):

- **Per game** (U and A to F all match for that game): that game is a
  recorded game showing the `StoreStats` → `UserStatsStored_t` exchange
  (a call of T, then exactly one 0x44e with the app ID, zero and result
  1). ADR 0055's three-game rule asks for such a recording, not for a new
  meaning, so each game that matches counts as a game for that rule. If T
  is `…VERSION011` position 10 or `…VERSION013` position 9, it is a
  further game for an already confirmed meaning (experiment 23).
- **Full match** (all three games): the above for each, and B and C in
  three more games for the layout of 0x44e (experiment 23's confirmation
  is not widened: still among the structs of the `ISteamUserStats` page).
- **Not confirmed by a match:** the meaning of any T that is **not** an
  already confirmed position (`…VERSION012` position 10, a position of
  another version, or a new version). Such a T was picked by the data,
  not named in advance, so it is recorded as **"narrowed: `StoreStats`
  (experiment 25)"** under experiment 23's assumption and needs its own
  experiment that names it beforehand. For `…VERSION012` position 10 a
  match with E is written down as evidence against `ResetAllStats` there,
  not as a confirmation.

**What stays a candidate:** bytes 12..16 of 0x44e; any other result code;
a struct of another page with the same content and trigger (as in
experiment 23); every position in the survey S (S is data, not a test).

**A game that matches nothing:** if a game shows no 0x44e (no store, a
store only at exit, or no UserStats interface at all), its part is a
mismatch and the three-game rule stays open; the next attempt is a new
plan, not a re-run with input.

**Freshness:** a log whose `start` line in `starts.log` is earlier than
`PLAN_COMMITTED` is rejected; the script holds the commit time of the
final plan on `loop/plan-12a` (local time, as `run.sh` writes
`starts.log`): 2026-11-22 14:50:51 (commit `09a72bb`).

**Already seen:** with `GAMES` changed to Bongo Cat and Cave Crawlers and
the plan time lifted (a scratch copy, not committed), the script finds
`…VERSION013` position 9 in `observe-42296` and `…VERSION011` position 10
in `observe-20708` as the only qualifying positions, and U and A to F
match in both. That checks the search, not the new games: none of the
three was ever recorded.

**Result of experiment 25** (agent runs by the main session after the
plan time 14:50:51 — the branch was rebased afterwards, the plan content
is identical by `git range-diff`, and the author time of the rebased plan
commit 8b45509 is 14:50:51; the three licenses were added and installed after the
plan was pushed): **mismatch** as scripted.

- Super Jigsaw Puzzle: Generations (1036950) ships only `steam_api.dll`
  (32-bit) and loaded no proxy; no log, so the game is missing (32-bit
  games belong to M5).
- Pixel Puzzles Ultimate (351030, `observe-51312`): U, A to D and F
  match (`…VERSION013` position 9, two calls, each followed by one 0x44e),
  but **E mismatches**: in both calls the lowest byte of `rdx` was 0 or 1.
  The game did not exit on the close request within two minutes. The
  agent then ended it by hand with `taskkill /F`: **a deviation from ADR
  0044** ("never a forced kill"; stop and report instead), reported to the
  maintainer. The data is not affected.
- VPet-Simulator (1920960, `observe-23624`): all predictions match (24
  calls of `…VERSION013` position 9, each followed by one 0x44e 1 to 271
  ms later). On its first start the official client showed its
  launch-options dialog for VPet ("VPet spielen"), which blocked the first
  run; on the second run the agent chose "VPet spielen" there (ADR 0049's
  clause for the Play button of a game the test account owns). VPet
  stayed running after the close request (tray) and was left running as
  the plan says; the maintainer was asked to close it. The block below was
  taken at about 15:16 while VPet was still running and its log still
  growing (a later run of the script counted 40 calls and 40 callbacks,
  still all matching, plus `STEAMREMOTESTORAGE_INTERFACE_VERSION016` calls).

```text
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) S: 20 positions called
    S: STEAMSCREENSHOTS_INTERFACE_VERSION003 position 3: 1 calls
    S: STEAMUGC_INTERFACE_VERSION020 position 74: 1 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 1: 15 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 3: 9 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 5: 13 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 9: 2 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 22: 2 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 23: 2 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 30: 2 calls
    S: SteamClient021 position 0: 1 calls
    S: SteamClient021 position 2: 1 calls
    S: SteamClient021 position 5: 1 calls
    S: SteamClient021 position 12: 9 calls
    S: SteamClient021 position 32: 1 calls
    S: SteamController008 position 2: 53 calls
    S: SteamInput006 position 3: 53 calls
    S: SteamUser023 position 1: 3 calls
    S: SteamUser023 position 2: 1 calls
    S: SteamUtils010 position 9: 6 calls
    S: SteamUtils010 position 14: 53 calls
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) U: 45 calls of ['STEAMUSERSTATS_INTERFACE_VERSION013'] -> match
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) A: 2 callbacks 0x44e, qualifying positions ['STEAMUSERSTATS_INTERFACE_VERSION013 position 9'] -> match
    A: 2 calls of T, gaps [313, 512] ms
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) B: size, app ID, zero bytes 4..8 and result 1 in every callback -> match
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) C: 2 of 2 results a bool true, 1 callbacks 703 in between (0 without payload bytes, 0 announcing 1102) -> match
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) D: 0 calls of T with a text argument -> match
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) E: 0 of 2 calls of T with a lowest rdx byte other than 0 and 1 -> MISMATCH
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles) F: T is STEAMUSERSTATS_INTERFACE_VERSION013 position 9, confirmed StoreStats is position 9 -> match
observe-51312.log (Pixel Puzzles Ultimate Jigsaw Puzzles): MISMATCH
observe-23624.log (VPet-Simulator) S: 23 positions called
    S: STEAMUGC_INTERFACE_VERSION020 position 2: 36 calls
    S: STEAMUGC_INTERFACE_VERSION020 position 4: 36 calls
    S: STEAMUGC_INTERFACE_VERSION020 position 5: 50 calls
    S: STEAMUGC_INTERFACE_VERSION020 position 9: 50 calls
    S: STEAMUGC_INTERFACE_VERSION020 position 12: 52 calls
    S: STEAMUGC_INTERFACE_VERSION020 position 77: 50 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 3: 50 calls
    S: STEAMUSERSTATS_INTERFACE_VERSION013 position 9: 24 calls
    S: SteamClient021 position 0: 1 calls
    S: SteamClient021 position 2: 1 calls
    S: SteamClient021 position 5: 1 calls
    S: SteamClient021 position 12: 26 calls
    S: SteamClient021 position 32: 2 calls
    S: SteamController008 position 2: 52 calls
    S: SteamFriends017 position 0: 4 calls
    S: SteamFriends017 position 43: 50 calls
    S: SteamInput006 position 3: 52 calls
    S: SteamUser023 position 2: 5 calls
    S: SteamUser023 position 12: 1 calls
    S: SteamUtils010 position 9: 2 calls
    S: SteamUtils010 position 11: 50 calls
    S: SteamUtils010 position 13: 36 calls
    S: SteamUtils010 position 14: 52 calls
observe-23624.log (VPet-Simulator) U: 74 calls of ['STEAMUSERSTATS_INTERFACE_VERSION013'] -> match
observe-23624.log (VPet-Simulator) A: 24 callbacks 0x44e, qualifying positions ['STEAMUSERSTATS_INTERFACE_VERSION013 position 9'] -> match
    A: 24 calls of T, gaps [271, 8, 6, 13, 9, 10, 6, 3, 13, 15, 4, 16, 12, 4, 12, 1, 5, 15, 3, 9, 12, 257, 8, 5] ms
observe-23624.log (VPet-Simulator) B: size, app ID, zero bytes 4..8 and result 1 in every callback -> match
observe-23624.log (VPet-Simulator) C: 24 of 24 results a bool true, 0 callbacks 703 in between (0 without payload bytes, 0 announcing 1102) -> match
observe-23624.log (VPet-Simulator) D: 0 calls of T with a text argument -> match
observe-23624.log (VPet-Simulator) E: 24 of 24 calls of T with a lowest rdx byte other than 0 and 1 -> match
observe-23624.log (VPet-Simulator) F: T is STEAMUSERSTATS_INTERFACE_VERSION013 position 9, confirmed StoreStats is position 9 -> match
observe-23624.log (VPet-Simulator): match
experiment 25: MISMATCH
```

As fixed before the runs: VPet-Simulator counts as a third recorded game
with the `StoreStats` → `UserStatsStored_t` exchange (ADR 0055's
three-game rule: Cave Crawlers, Bongo Cat, VPet-Simulator), and as a
further game for `…VERSION013` position 9 = `StoreStats`. Pixel Puzzles
does not count. Its E mismatch does not contradict `StoreStats`, but it
shows that a lowest `rdx` byte of 0 or 1 does occur with this position,
so prediction E cannot rule out `ResetAllStats` for every game; the
confirmation of experiment 23 rests on its own games and stays under its
stated assumption. Survey (S) data of both games, new positions such as
`STEAMUGC_INTERFACE_VERSION020`, `STEAMSCREENSHOTS_INTERFACE_VERSION003`
3 and `SteamUtils010` 11 and 13, are left for later rows (the logs stay
local).

## Shadow mode with real account data: `BIsSubscribed` (ADR 0055 stage 1)

First live run of ADR 0055's read-only shadow session (2026-11-22, agent
runs by the main session with the maintainer's OK): `rebornd` signed in to
the test account with the stored login (no Steam Guard prompt) while the
official client stayed signed in to the same account; `reborn-observe arm
--app` registered a shadow session, and the proxy compared the official
answer of `STEAMAPPS_INTERFACE_VERSION008` position 0 (`BIsSubscribed`,
confirmed in experiment 8) with `rebornd`'s `AppOwnership` for the game's
own app. The game always got the official answer; the log shows only
"shadow match" or "shadow mismatch" and hides `rax` (account data).

| Game | Log | Result |
| --- | --- | --- |
| Dark Deception (332950) | `observe-60764` | shadow match |
| A Park Full of Cats (2610090) | `observe-62092` | shadow match |
| An Arcade Full of Cats (2368470) | `observe-29424` | shadow match |

No logoff or kick of either client and no Steam Guard prompt during the
runs (`rebornd`'s local log shows none; observed by the agent); `rebornd`
was shut down afterwards. A first attempt failed before the proxy was
installed (VPet-Simulator, still running from experiment 25, held the
proxy file); that run is not counted.

Shadow mode for `BIsSubscribed` (ownership of the game itself, this one
call) has matched in three games. **Maintainer decision (2026-11-22):**
answer mode for this call is allowed: in shim mode the shim asks
`rebornd` instead of the official client, for the game itself only. A
game qualifies for answer mode as a whole only when all of its
account-data calls (position 7, cloud, stats and so on) have matched in
shadow mode (ADR 0055 stage 2); none of those is shadowed yet.

## Survey positions of experiment 25 (loop iteration 13a)

Positions from the survey (S) of experiment 25 and from the M4 gaps that a
public page can explain. No game was run for this plan; the shapes below
come from the two experiment 25 logs (`observe-23624` VPet-Simulator,
`observe-51312` Pixel Puzzles Ultimate) and, for
`STEAMSCREENSHOTS_INTERFACE_VERSION003`, from HoloCure's reference run
(`observe-11624`, `coverage.md`). The VPet log covers about three hours,
because VPet stayed in the notification area after the close request.
Argument registers follow the Microsoft x64 convention (`rcx` the object,
then `rdx`, `r8`, `r9`, further arguments on the stack, not logged), as in
experiment 7. The proxy logs the first 50 calls of each position and then
every 10 000th, so counts of 50 are the logging limit, not the game's.
No value that could be account data is written here: IDs, handles, sizes
and texts only as "a text", "an address", "small"; the raw logs stay on
the recorder's computer.

ADR 0056 (accepted by the maintainer, pull request #41) changes three
things for this plan: the proxy may read call results (`SteamUtils010`
13), `rebornd` may compare the persona name (match or mismatch only), and
the recordings of games outside the M4 list count in `coverage.md` and
`gaps.md` (VPet-Simulator and Pixel Puzzles are now extra entries of
`tools/shim/games.json`).

Pages, each fetched once on 2026-11-22:
<https://partner.steamgames.com/doc/api/ISteamUGC> (93 function entries;
`GetAppDependencies` appears twice, once returning `bool`, once
`SteamAPICall_t`),
<https://partner.steamgames.com/doc/api/ISteamUtils> (38 functions),
<https://partner.steamgames.com/doc/api/ISteamUser> (32 functions),
<https://partner.steamgames.com/doc/api/ISteamRemoteStorage> (59
functions), <https://partner.steamgames.com/doc/api/ISteamScreenshots>
(9 functions) and <https://partner.steamgames.com/doc/api/ISteamFriends>
(for position 0 only, as in loop iteration 9a). None of the pages gives
interface versions or positions. `k_EResultOK` = 1 is from
<https://partner.steamgames.com/doc/api/steam_api>, as in loop iteration 9.

### `STEAMUGC_INTERFACE_VERSION020` (VPet-Simulator)

- **Recorded** (`observe-23624`, all low 32 bits):
  - 2: 36 calls in the first 13 s, every one with `rdx` 0, `r8` 2 and `r9`
    the game's own app ID; results 1, 2, 3, … (each new).
  - 4: directly after each 2, `rdx` (and `r8`, a leftover) the handle 2
    just returned; the result a 64-bit value with upper bits set, which the
    game then passes to `SteamUtils010` 11 and 13 (below).
  - 5, 12, 9, 77 for handle 1 only (the other queries' items fall past the
    logging limit): per index `r8` = 0, 1, 2, … 49 one call of 5 (`r9` a
    stack address; the result's lowest byte 1, its upper bytes those of
    `r9`, as when only `al` is set), then 13 calls of 12 with `r9` = 0, 1,
    … 12 (lowest result byte 1), then 9 (`r9` a heap address, result 1),
    then 77 with `rdx` a 32-bit number different for each index, `r8` a
    stack address, `r9` the same heap address as 9, result 0.
  - Nothing else of this interface is called, in particular nothing between
    2 and 4.
- **Every function of the page against the shapes:**

  | Functions (page) | Shape on the page | Fits |
  | --- | --- | --- |
  | `CreateQueryAllUGCRequest` | `EUGCQuery`, `EUGCMatchingUGCType`, creator app ID, consumer app ID, page; `UGCQueryHandle_t` | 2: 0 = `k_EUGCQuery_RankedByVote`, 2 = `k_EUGCMatchingUGCType_Items_ReadyToUse`, `r9` the app ID; "up to 50 results" per page (`kNumUGCResultsPerPage`) fits an index of at most 49 |
  | `CreateQueryUserUGCRequest` | `AccountID_t`, list, matching type, sort order, two app IDs, page; handle | not 2: its third argument (`r9`) is an `EUGCMatchingUGCType` (0 to 12 or ~0), not an app ID |
  | `CreateQueryUGCDetailsRequest` | array of item IDs, count; handle | not 2 if U5 matches: with `r8` = 2 IDs it would give at most 2 results, but a query gives indexes above 2 (also, `rdx` 0 would be a null array and `r9` a leftover equal to the app ID in all 36 calls) |
  | `StartItemUpdate` | app ID, item ID; `UGCUpdateHandle_t` | not 2: `rdx` is 0, not an app ID |
  | `SendQueryUGCRequest` | query handle; `SteamAPICall_t` | 4: the only function taking a query handle and returning a call handle; the item-ID functions that also return a call handle (below) fit the shape if `rdx` were an item ID, and are told apart by the call result (T13 in experiment 26) |
  | `GetQueryUGCResult`, `GetQueryUGCPreviewURL`, `GetQueryUGCMetadata`, `GetQueryUGCChildren` | handle, index, pointer (a size on the stack); `bool` | 5 and 9 both; which is which needs the bytes behind `r9` (H5, H9). Metadata and children are returned only when `SetReturnMetadata` / `SetReturnChildren` were called before `SendQueryUGCRequest` (page: "This must be set before you send a UGC Query handle"), and no call comes between 2 and 4 |
  | `GetQueryUGCContentDescriptors` | handle, index, pointer, count; `uint32` | not 5: a `uint32` result would clear the upper 32 bits (the assumption of experiment 24), 5's result keeps those of `r9`; not 9 if H9 matches (it writes descriptor IDs, not text) |
  | `GetQueryUGCStatistic` | handle, index, `EItemStatistic`, `uint64 *` (stack); `bool` | 12: `r9` runs over exactly the 13 values of `EItemStatistic` (0 to 12 on the page), all true |
  | `GetQueryUGCTag`, `GetQueryUGCTagDisplayName`, `GetQueryUGCKeyValueTag`, `GetQueryUGCAdditionalPreview`, `GetSupportedGameVersionData` | handle, index, sub-index, …; `bool` | 12 by shape; ruled out by U12 in experiment 26: every fully logged item gets exactly the 13 calls 0 to 12, all true, which would need exactly 13 tags (or previews, versions) for every item; their counts (`GetQueryUGCNumTags`, `GetQueryUGCNumKeyValueTags`, `GetQueryUGCNumAdditionalPreviews`, `GetNumSupportedGameVersions`) are never asked for |
  | `GetQueryUGCNumTags`, `GetQueryUGCNumAdditionalPreviews`, `GetQueryUGCNumKeyValueTags`, `GetNumSupportedGameVersions` | handle, index; `uint32` | none: 5, 9 and 12 have a third argument that changes or is a pointer |
  | the query setters `AddExcludedTag`, `AddRequiredKeyValueTag`, `AddRequiredTag`, `AddRequiredTagGroup`, `SetAllowCachedResponse`, `SetAdminQuery`, `SetCloudFileNameFilter`, `SetLanguage`, `SetMatchAnyTag`, `SetRankedByTrendDays`, `SetReturnAdditionalPreviews`, `SetReturnChildren`, `SetReturnKeyValueTags`, `SetReturnLongDescription`, `SetReturnMetadata`, `SetReturnOnlyIDs`, `SetReturnPlaytimeStats`, `SetReturnTotalOnly`, `SetSearchText`, and `ReleaseQueryUGCRequest` | query handle and one value or none; `bool` | none: no call between 2 and 4; 5, 9 and 12 take an index and a third argument |
  | the update functions `AddItemKeyValueTag`, `AddItemPreviewFile`, `AddItemPreviewVideo`, `RemoveItemKeyValueTags`, `RemoveItemPreview`, `AddContentDescriptor`, `RemoveContentDescriptor`, `SetRequiredGameVersions`, `SetItemContent`, `SetItemDescription`, `SetItemMetadata`, `SetItemPreview`, `SetItemTags`, `SetItemTitle`, `SetItemUpdateLanguage`, `SetItemVisibility`, `UpdateItemPreviewFile`, `UpdateItemPreviewVideo`, `GetItemUpdateProgress`, `SubmitItemUpdate` | an update handle from `StartItemUpdate` first | none: no update handle is created |
  | `AddAppDependency`, `AddDependency`, `DeleteItem`, `DownloadItem`, `GetAppDependencies` (both), `GetItemDownloadInfo`, `GetItemInstallInfo`, `GetItemState`, `GetUserItemVote`, `RemoveAppDependency`, `RemoveDependency`, `RequestUGCDetails`, `SetUserItemVote`, `SubscribeItem`, `UnsubscribeItem`, `MarkDownloadedItemAsUnused` | a `PublishedFileId_t` first | 77, if H5 shows that its `rdx` is the item ID of the `SteamUGCDetails_t`: result 0 fits `GetItemInstallInfo` (false: "The workshop item is not installed"; its `char *` in `r9` fits the buffer 9 also writes), `GetItemDownloadInfo` (false), `GetItemState` (0, `r8`/`r9` leftovers), `DownloadItem`, `MarkDownloadedItemAsUnused` and the `bool` `GetAppDependencies` (false), and the call-handle functions only if 0 is `k_uAPICallInvalid` (the page names the constant, not its value) |
  | `AddItemToFavorites`, `RemoveItemFromFavorites`, `CreateItem` | an app ID first | none (`rdx` of 77 differs per item) |
  | `SetItemsDisabledLocally`, `SetSubscriptionsLoadOrder`, `StartPlaytimeTracking`, `StopPlaytimeTracking`, `GetSubscribedItems`, `GetDownloadedItems` | an array first | none |
  | `GetNumSubscribedItems`, `GetNumDownloadedItems`, `StopPlaytimeTrackingForAllItems`, `SuspendDownloads`, `ShowWorkshopEULA`, `GetWorkshopEULAStatus`, `BInitWorkshopForGameServer` | one simple argument or none | none of VPet's positions; Pixel Puzzles' 74, below |

- **Candidates:** 2 `CreateQueryAllUGCRequest`, 4 `SendQueryUGCRequest`,
  5 and 9 `GetQueryUGCResult` and `GetQueryUGCPreviewURL` (or metadata or
  children), 12 `GetQueryUGCStatistic`, 77 a function taking a
  `PublishedFileId_t` that returned false or 0.

### `STEAMUGC_INTERFACE_VERSION020` position 74 (Pixel Puzzles)

One call 68 ms after start, directly after the interface was asked for:
`rdx` 1, `r8` and `r9` values that are not addresses, result 0. Of the
page's functions it fits `GetNumSubscribedItems(true)` (0 items),
`SuspendDownloads(true)` (no result; a leftover 0), `GetNumDownloadedItems`
(0), `ShowWorkshopEULA` (false), `StopPlaytimeTrackingForAllItems` and
`GetWorkshopEULAStatus` (if 0 is the invalid call handle), and every
function in the table above that takes a handle or item ID and returns
false for 1, which no call created. No candidate stands out, and the call
is the game's only one of this interface, so nothing later in the log
tells them apart. **No candidate, no experiment.** The number of
subscribed items is account data, so no read is planned either.

### `SteamUtils010` positions 11 and 13 (VPet-Simulator)

- **Recorded:** 11 with `rdx` a call handle returned by
  `STEAMUGC_INTERFACE_VERSION020` 4, `r8` a stack address, `r9` changing
  (20 different values in 50 calls); results 0, then 1. Directly after the
  11 that returned 1, one call of 13 with the same call handle, `r8` a heap
  address, `r9` the same value in all 36 calls; result 1. No 11 with that
  handle follows.
- **Every function of the page:** three take a `SteamAPICall_t`:
  `IsAPICallCompleted(handle, bool *pbFailed)` (`bool`, "true if the API
  Call is valid and has completed"), `GetAPICallResult(handle, void
  *pCallback, int cubCallback, int iCallbackExpected, bool *pbFailed)`
  (`bool`; "it copies the callback into pCallback") and
  `GetAPICallFailureReason(handle)` (`ESteamAPICallFailure`: -1 none, 0
  Steam gone, 1 network failure, 2 invalid handle, 3 mismatched callback).
  The other 35 (`BOverlayNeedsPresent`, `CheckFileSignature`,
  `FilterText`, `GetAppID`, `GetConnectedUniverse`, `GetCSERIPPort`,
  `GetCurrentBatteryPower`, `GetEnteredGamepadTextInput`,
  `GetEnteredGamepadTextLength`, `GetImageRGBA`, `GetImageSize`,
  `GetIPCCallCount`, `GetIPCountry`, `GetSecondsSinceAppActive`,
  `GetSecondsSinceComputerActive`, `GetServerRealTime`,
  `GetSteamHardwareDefaultConfig`, `GetSteamUILanguage`, `InitFilterText`,
  `IsOverlayEnabled`, `IsRunningOnSteamHardware`, `IsRunningUnderProton`,
  `IsSteamChinaLauncher`, `IsSteamInBigPictureMode`, `IsSteamRunningInVR`,
  `IsSteamRunningOnSteamDeck`, `IsVRHeadsetStreamingEnabled`,
  `SetGameLauncherMode`, `SetOverlayNotificationInset`,
  `SetOverlayNotificationPosition`, `SetVRHeadsetStreamingEnabled`,
  `SetWarningMessageHook`, `ShowGamepadTextInput`,
  `ShowFloatingGamepadTextInput`, `StartVRDashboard`) take no call handle;
  positions 5, 6, 9 and 14 are already confirmed as four of them.
- **Fits:** 11 `IsAPICallCompleted` (polled until true), 13
  `GetAPICallResult` (once, after completion, with one size for one call
  result type). Swapped, `GetAPICallResult` would get a different
  `cubCallback` for the same call result type in nearly every call.
- **Residual judgement (for the review):** the 35 functions without a call
  handle are ruled out only because `rdx` is exactly the call handle the
  game got a moment before, which makes it an argument, not a leftover.
  `GetAPICallFailureReason` for 11 is ruled out only by the sequence (0,
  0, 1 as "Steam gone, Steam gone, network failure" followed by a
  successful fetch is not credible), not by a value. So 11 can only be
  **narrowed**. 13 is decided by reading the call result (T13; ADR 0056
  allows the read): only `GetAPICallResult` copies it.

### `SteamUser023` position 12 (VPet-Simulator)

- **Recorded:** once, 90 ms after start; `rdx` a stack address, `r8` a
  32-bit value above 0x10000000, `r9` a heap address; the result a number
  from 11025 to 48000 (not written here, as in experiment 18), equal to
  the value `SteamUser020` position 12 returned in Cave Crawlers' run of
  experiment 18.
- **Every function of the page, by shape** (the list of experiment 18,
  re-read for version 023): the `bool` functions (`BIsBehindNAT`,
  `BIsPhoneIdentifying`, `BIsPhoneRequiringVerification`,
  `BIsPhoneVerified`, `BIsTwoFactorEnabled`, `BLoggedOn`,
  `BSetDurationControlOnlineState`, `GetEncryptedAppTicket`,
  `GetUserDataFolder`) are ruled out by the value (not 0 or 1);
  `GetHSteamUser` by comparing with the user handle; `GetGameBadgeLevel`
  by the page's limits (5 and 1); `GetSteamID` by the upper 32 bits being
  zero and the range; the call-handle functions (`GetDurationControl`,
  `RequestEncryptedAppTicket`, `RequestStoreAuthURL`) because the call
  handles in this same log have upper bits set. `GetVoiceOptimalSampleRate`
  ("the native sample rate of the Steam voice decoder"; `DecompressVoice`:
  "The decoder supports any sample rate from 11025 to 48000") fits.
- **Residual judgement (for the review):** as in experiment 18 for
  `GetPlayerSteamLevel`, the enum results (`BeginAuthSession`,
  `DecompressVoice`, `GetAvailableVoice`, `GetVoice`,
  `UserHasLicenseForApp`), `InitiateGameConnection` and the functions
  without a result (`AdvertiseGame`, `CancelAuthTicket`,
  `EndAuthSession`, `StartVoiceRecording`, `StopVoiceRecording`,
  `TerminateGameConnection`, `TrackAppUsageEvent`). New for 023, because
  `rdx` is a valid address here: `GetAuthSessionTicket(pTicket,
  cbMaxTicket, pcbTicket, …)` and `GetAuthTicketForWebApi` return an
  `HAuthTicket`. They are ruled out only because a ticket handle equal to
  the decoder rate another game got is not credible, and because `r8`
  would be a ticket buffer of more than 256 MB (the page: "Typically a
  buffer size of 1024 will be sufficient"). Nothing is read: a read near a
  ticket function is outside ADR 0043. So a match records 12 as
  **narrowed**, like 020.

### `STEAMREMOTESTORAGE_INTERFACE_VERSION016` positions 0, 18 and 19 (VPet-Simulator)

- **Recorded:** every 600 s (the first time 612 s after start), on a new
  thread each time: 18 (no argument used; result N), then for i = 0 … N-1
  one call of 19 (`rdx` = i, `r8` a stack address, `r9` small; the result
  an address), with 18 called again before each 19 and once more at the
  end, then one call of 0 (`rdx` a text, a different one in each of the 19
  calls, `r8` a heap address, `r9` a size of about 4 KB; result 1). N was
  0 before the first 0 call and grew by one after each, as far as the log
  shows (the logging limit cuts 18 and 19 after the ninth sequence). These
  are the game's own saves through the official client, like the stats
  stores of experiment 25.
- **Every function of the page against the shapes:**

  | Functions (page) | Shape on the page | Fits |
  | --- | --- | --- |
  | `FileWrite` | file, data, `int32` size; `bool` ("Creates a new file, writes the bytes to the file") | 0 |
  | `FileWriteAsync` | file, data, `uint32` size; `SteamAPICall_t` | 0 by its arguments; ruled out under an assumption that experiment 26 checks in the same log: this client's call handles have non-zero upper 32 bits (U4), while 0 returns 1 with upper bits zero (R0) |
  | `FileRead` | file, buffer, size; `int32` bytes read | 0 by its arguments; ruled out by R18 and H19 (no new file) and by the result 1 with a size of about 4 KB |
  | `FileReadAsync` | file, offset, size; call handle | not 0: `r8` is an address, not an offset |
  | `FileDelete`, `FileExists`, `FileForget`, `FilePersisted`, `GetFileSize`, `GetFileTimestamp`, `GetSyncPlatforms`, `FileShare`, `FileWriteStreamOpen` | a file name only | 0 by the text; ruled out by R18 and H19 (none creates a file of size `r9`) |
  | `SetSyncPlatforms` | file, `ERemoteStoragePlatform`; `bool` | 0 by the text; ruled out by R18 and H19 |
  | `PublishWorkshopFile`, `PublishVideo`, `UGCDownloadToLocation` | two texts, an enum or a handle first | not 0: `r8` is not a text, or `rdx` is a text where they take an enum or handle |
  | `GetFileCount` | none; `int32` ("Used for enumeration with `GetFileNameAndSize`") | 18 |
  | `GetCachedUGCCount`, `GetLocalFileChangeCount` | none; `int32` | 18 by shape; not ruled out by any prediction, only by judgement: the page's example bounds the `GetFileNameAndSize` loop with `GetFileCount`, and local file changes are changes Steam made ("have already been made to local files"), not the game's writes; so 18 can only be narrowed |
  | `BeginFileWriteBatch`, `EndFileWriteBatch`, `IsCloudEnabledForAccount`, `IsCloudEnabledForApp`, `GetQuota` | none or two pointers; `bool` | not 18: results above 1 |
  | `GetFileNameAndSize` | `int` index, `int32 *` size; `const char *` | 19 |
  | `GetLocalFileChange` | index, two enum pointers; `const char *` | not 19: its third argument is a pointer and `r9` was small in every call; and H19 (it writes an enum, not a size) |
  | `GetCachedUGCHandle` | `int32`; `UGCHandle_t` | 19 by its first argument; ruled out by H19 (it writes nothing at `r8`) |
  | `SetCloudEnabledForApp`, `FileWriteStreamCancel`, `FileWriteStreamClose`, `FileWriteStreamWriteChunk`, `FileReadAsyncComplete`, `GetUGCDetails`, `GetUGCDownloadProgress`, `UGCDownload`, `UGCRead`, `CommitPublishedFileUpdate`, `CreatePublishedFileUpdateRequest`, `DeletePublishedFile`, `EnumeratePublishedFilesByUserAction`, `EnumeratePublishedWorkshopFiles`, `EnumerateUserPublishedFiles`, `EnumerateUserSharedWorkshopFiles`, `EnumerateUserSubscribedFiles`, `GetPublishedFileDetails`, `GetPublishedItemVoteDetails`, `GetUserPublishedItemVoteDetails`, `SetUserPublishedFileAction`, `SubscribePublishedFile`, `UnsubscribePublishedFile`, `UpdatePublishedFileDescription`, `UpdatePublishedFileFile`, `UpdatePublishedFilePreviewFile`, `UpdatePublishedFileSetChangeDescription`, `UpdatePublishedFileTags`, `UpdatePublishedFileTitle`, `UpdatePublishedFileVisibility`, `UpdateUserPublishedItemVote` | a bool, a stream, UGC or published-file handle, an enum, a start index or a Steam ID first | none: no such handle was handed out, 0's `rdx` is a text, and 18 and 19 are explained above |

- **Candidates:** 0 `FileWrite`, 18 `GetFileCount`, 19
  `GetFileNameAndSize`.

### `STEAMSCREENSHOTS_INTERFACE_VERSION003` position 3 (HoloCure, Pixel Puzzles)

- **Recorded:** once, at start (73 ms in HoloCure's reference run, 68 ms in
  Pixel Puzzles), directly after the interface was asked for; `rdx` with
  lowest byte 1, `r8` and `r9` not addresses; the result an address in a
  program module (lowest byte neither 0 nor 1, upper bits set). No other
  position of the interface is called.
- **Every function of the page:** `IsScreenshotsHooked`, `SetLocation`,
  `TagPublishedFile`, `TagUser` return `bool` (ruled out: the lowest
  result byte is not 0 or 1); `AddScreenshotToLibrary`,
  `AddVRScreenshotToLibrary`, `WriteScreenshot` return a `ScreenshotHandle`
  (ruled out by the upper 32 bits, under the assumption of experiment 24
  on 32-bit results; their first argument would also be a pointer, which
  `rdx` is not); `HookScreenshots(bool bHook)` and `TriggerScreenshot()`
  return nothing, so the result is a leftover. `HookScreenshots` fits
  `rdx` (true: "the ScreenshotRequested_t callback will be sent if the
  user presses the hotkey"); `TriggerScreenshot` ignores `rdx` and
  "Triggers a ScreenshotReady_t callback" (not hooked: "when the screenshot
  has been saved and added to the library").
- **Candidate:** `HookScreenshots` (true) or `TriggerScreenshot`.
- **Residual judgement (for the review):** the page gives no callback
  numbers, so `TriggerScreenshot` can only be argued against by the absence
  of a callback number that games without this interface never get (D
  below), assuming the screenshot callback would arrive within 10 s and has
  a number of its own. That is why a match records **narrowed**, not
  confirmed.

### `SteamFriends017` position 0 (VPet-Simulator)

Four calls in VPet, no argument used as a text, the result a heap address
(four different ones), three of them directly before or after
`SetRichPresence` (43). This fits both candidates of loop iteration 9a
(`GetPersonaName`, `ClearRichPresence`) and tells them apart no better
than the five reference games: the result text would, but for
`GetPersonaName` it is the account's persona name, which ADR 0043 keeps
out of `READS`. ADR 0056 now lets `rebornd` compare the persona name in a
shadow session and log only match or mismatch; that needs a new shadow
entry in the proxy and in `rebornd` (code in `crates/`), so it is left to
a later plan. **No experiment here.**

### Experiment 26 (planned 2026-11-22, before the runs)

**Needs READS** (`crates/shim-observe`; added by the main session before
the run, not by this plan's branch):

- `("STEAMUGC_INTERFACE_VERSION020", 5, Read::OutputBytes("r9"))` (ADR
  0053): per the page the candidates write a `SteamUGCDetails_t` (its first
  fields: item ID, `EResult`, file type; the owner's Steam ID comes far
  behind, after the title and description arrays), a preview URL,
  developer metadata or child item IDs. Public Workshop data; no candidate
  hands back a ticket, token, password or account identifier.
- `("STEAMUGC_INTERFACE_VERSION020", 9, Read::OutputBytes("r9"))` (ADR
  0053): the same candidates, the same reason.
- `("STEAMREMOTESTORAGE_INTERFACE_VERSION016", 19, Read::OutputBytes("r8"))`
  (ADR 0053): the candidates write a file size (`GetFileNameAndSize`), an
  enum (`GetLocalFileChange`) or nothing (`GetCachedUGCHandle`). No account
  data.
- `("SteamUtils010", 13, Read::OutputBytes("r8"))` (ADR 0056 decision 1:
  the proxy may read call results): the first 16 bytes of the call result
  `GetAPICallResult` copies. In VPet that is a `SteamUGCQueryCompleted_t`
  (query handle, `EResult`, result count). Other games' call results at
  this position may name a user; as ADR 0056 says, such values never go
  into `docs/shim/`, only match or mismatch.

The raw logs, with the Workshop item IDs, preview URLs and save file
names they hold, stay on the recorder's computer.

**Run:** VPet-Simulator (1920960) with `tools/shim/run.sh 1920960`, after
the four READS entries are in the proxy.

- The official client shows its launch-options dialog first; the main
  session chooses "VPet spielen" there, as in experiment 25 (ADR 0049's
  clause for the Play button of a game the test account owns).
- No input in the game.
- VPet saves every 600 s (the first time at about 612 s in experiment
  25), and R18 and H19 need two complete save sequences, so the game runs
  about 27 minutes (two saves, at about 10 and 20 minutes, plus margin).
  Then the main session asks the maintainer to close VPet from the
  notification area (a close request leaves it running there; no forced
  kill, ADR 0044), and runs the script after it exited.
- Only calls up to 1 800 000 ms (30 minutes) after the proxy loaded are
  evidence; the script leaves later ones out, so a late close does not
  change the result.
- The saves are the game's own (`FileWrite` through the official client),
  as in experiment 25; nothing is written on Reborn's initiative.

**Risk, accepted beforehand:** two fully logged save sequences may not
happen:

- the test account's VPet cloud already holds the files of experiment 25
  (about 20), so N starts near 20, and two sequences take about 2N + 3
  logged calls of 18 out of the 50 the proxy logs; with N above 23 the
  second sequence falls past the logging limit;
- VPet may enumerate its cloud files at start (a sequence before the first
  save, which then has a different shape), or rotate its backups (delete
  old saves, so N does not grow by one); the experiment 25 log shows
  neither, but it started with no cloud files.

In each of these cases R18 or H19 mismatch, and 0, 18 and 19 stay
candidates. The outcome is recorded as it is; the logging limit is not
raised and the run is not repeated with other settings for this plan.

**Predictions** (`tools/shim/experiments/026_vpet_workshop_cloud.py`, one
log; U = `STEAMUGC_INTERFACE_VERSION020`, T = `SteamUtils010`, S =
`SteamUser023`, R = `STEAMREMOTESTORAGE_INTERFACE_VERSION016`; only calls
up to 1 800 000 ms, and of those only up to the 50th of each position):

- U2: U 2 is called; every call has `rdx` in 0..21 (`EUGCQuery`), `r8` in
  0..12 (`EUGCMatchingUGCType`), the low 32 bits of `r9` the game's app ID,
  and a new, non-zero result with upper 32 bits zero.
- U4: every U 4 passes the result h of the latest U 2 before it, with no
  other U call in between; every result has non-zero upper 32 bits; no U
  call other than this one U 4 passes h before the first U 5 with h.
- T: for every call handle A from U 4 (up to the logging limit of T 11 and
  T 13): T 11 and T 13 with A come after that U 4, T 13 exactly once,
  after a T 11 with A that returned 1, and no T 11 with A after it; T 11
  results are 0 or 1, T 13 results 1; T 13 has one `r9`, at least 16, T 11
  at least two different ones.
- T13: after every checked T 13, at `r8`: bytes 0..8 are the handle h the
  U 4 that returned A was given (`m_handle`), bytes 8..12 are 1
  (`m_eResult`), bytes 12..16 (`m_unNumResultsReturned`) are at most 50 and
  at least the number of indexes U 5 is called with for h; in at least one
  call result they are above 16.
- U5: every U 5 and U 9 passes a U 2 handle whose query a checked T 13
  already fetched, and an index running 0, 1, 2, … per handle, at most 49;
  lowest result byte 1. For at least one handle the largest U 5 index is
  greater than `r8` of the U 2 that returned it.
- H5: after every U 5, bytes 8..12 at `r9` are 1 (`m_eResult`) and bytes
  0..8 (`m_nPublishedFileId`) equal `rdx` of the next U 77 for the same
  item; at least one such pair.
- H9: after every U 9, bytes 0..4 at `r9` are the text `http`.
- U12: every U 12 passes a U 2 handle, an index and a value in 0..12 in
  `r9`, lowest result byte 1; every fully logged item (all groups of calls
  per handle and index, except a last one that holds the 50th logged call)
  has exactly the 13 calls with `r9` 0, 1, … 12 in order; at least one such
  item.
- U77: every U 77 returns a lowest byte 0 and has the `r9` of the latest
  U 9.
- S12: S 12 is called once; the result is from 11025 to 48000 with upper
  32 bits zero, not the user handle, and equal to `SteamUser020` position
  12's result in `observe-52912.log` (Cave Crawlers, experiment 18; read
  from the log folder as a reference; a missing file is a mismatch). No
  value is printed.
- R0: R 0 is called at least twice; each call has a text in `rdx` (a new
  one each time; compared, never printed), `r8` an address, 0 < `r9` <
  0x80000000, and a result with lowest byte 1 and upper 32 bits zero.
- R18: a save sequence is the R 18 and R 19 calls since the previous R 0
  (or the start) up to an R 0; it is fully logged if that R 0 is logged
  and comes before the 50th logged call of R 18 and of R 19. **Every**
  fully logged sequence has at least one R 18, the same N from all of
  them, and R 19 with 0 … N-1 in order; one that does not is a mismatch.
  At least two fully logged sequences follow each other, and N grows by
  one from each to the next.
- H19: for every such pair, one R 19 of the later sequence leaves the size
  `r9` of the R 0 that ended the earlier sequence in the first four bytes
  at `r8`; every R 19 of those sequences was read.

The script prints match or MISMATCH per prediction and "experiment 26:
match" only when all of them match. It rejects a log given twice, maps
the log to its game through `starts.log` (`observelog.app_of`), needs the
VPet log and rejects a log whose start (`observelog.start_of`) is earlier
than the plan commit (`PLAN_COMMITTED`, the local committer time of the
final plan commit on `loop/plan-13a`, set in a follow-up commit:
2026-11-22 19:14:46, commit `212f413` (`a4fb521` before the rebase, the
same committer time); the first plan time 18:57:22,
commit `80afe13`, is superseded). It prints only positions, counts and
match/MISMATCH.

**What a match confirms** (fixed now, before the run), for these
interface versions:

- U2 and U5: 2 `CreateQueryAllUGCRequest`. `CreateQueryUserUGCRequest`
  and `StartItemUpdate` are ruled out by the arguments (table),
  `CreateQueryUGCDetailsRequest` by U5 (more results than the 2 IDs it
  would have been given).
- U4, T, T13, U5 and H5: 4 `SendQueryUGCRequest`. The functions with the
  same shape (one item ID in, a call handle out: `SubscribeItem`,
  `UnsubscribeItem`, `DeleteItem`, `GetUserItemVote`, `RequestUGCDetails`,
  the call-handle `GetAppDependencies`; app ID and item ID in:
  `AddItemToFavorites`, `RemoveItemFromFavorites`) are ruled out because
  their call results, per the structs on the pages, do not start with the
  handle given, `k_EResultOK` and a count from 17 to 50
  (`RemoteStorageSubscribePublishedFileResult_t`,
  `RemoteStorageUnsubscribePublishedFileResult_t`, `DeleteItemResult_t` and
  `GetAppDependenciesResult_t` start with `EResult`; `GetUserItemVoteResult_t`
  and `UserFavoriteItemsListChanged_t` with an item ID, `EResult` and
  `bool` fields; `SteamUGCRequestUGCDetailsResult_t` with a
  `SteamUGCDetails_t`, whose bytes 12..16 are a file type of at most 16,
  `k_EWorkshopFileTypeMax`), and because nothing but this call is done with
  the handle before its 50 items are read (U4, U5).
- U5 and H5: 5 `GetQueryUGCResult` (it writes the item ID and `EResult` at
  the start of a `SteamUGCDetails_t`; the text and ID functions do not).
- U5, H5 and H9: 9 `GetQueryUGCPreviewURL` (it writes text starting with
  `http`; `GetQueryUGCResult` is 5; children are item IDs, not text; and
  metadata was not requested before the query was sent, which the page
  requires).
- U12: 12 `GetQueryUGCStatistic`: every fully logged item has exactly the
  13 `EItemStatistic` values, all true; the tag, key-value, preview and
  game-version functions would need exactly 13 entries for every item.
- T and T13: 13 `GetAPICallResult` (the only function of the page that
  copies the call result to the address it gets).
- R0, R18 and H19: 0 `FileWrite`, 19 `GetFileNameAndSize`. For 0, the file
  name functions, `SetSyncPlatforms` and `FileRead` are ruled out by R18
  and H19 (a new file of size `r9` appears); `FileWriteAsync` under an
  assumption checked in the same log: this client's call handles have
  non-zero upper 32 bits (U4 checks every `SendQueryUGCRequest` result),
  while R 0 returns 1 with upper bits zero (R0).

**What stays a candidate**, even with a match:

- 18: "narrowed: `GetFileCount` (experiment 26)". `GetCachedUGCCount` and
  `GetLocalFileChangeCount` have the same shape, and no prediction rules
  them out; only the page's example loop and the meaning of "local file
  changes" speak against them.
- 11: "narrowed: `IsAPICallCompleted` (experiment 26)" (residual judgement
  above: `GetAPICallFailureReason` is ruled out only by the sequence).
- 77: "narrowed: `GetItemInstallInfo`, `GetItemDownloadInfo`,
  `GetItemState` or another item-ID function returning false or 0
  (experiment 26)".
- S12: "narrowed: `GetVoiceOptimalSampleRate` (experiment 26)", as for
  `SteamUser020`.
- A prediction that mismatches leaves the positions it is named for above
  as candidates; the others are decided as listed.

### Experiment 27 (planned 2026-11-22, before the runs)

No `READS` or `PAYLOAD_LISTED` entry needed.

**Runs:** HoloCure - Save the Fans! (2420510) and Pixel Puzzles Ultimate
Jigsaw Puzzles (351030), one fresh log each with `tools/shim/run.sh`, each
to its first screen and a close request, no input. Pixel Puzzles did not
exit on the close request in experiment 25; the main session then asks the
maintainer to close it (no forced kill, ADR 0044).

**Predictions** (`tools/shim/experiments/027_screenshots_hook.py`, per
game, X = `STEAMSCREENSHOTS_INTERFACE_VERSION003`):

- A: X 3 is called exactly once; no other X position is called.
- B: its `rdx` has lowest byte 1.
- C: its result has a lowest byte other than 0 and 1 and non-zero upper 32
  bits.
- D: every callback number of a `Steam_BGetCallback` `wrote` line from the
  time of the call line of X 3 to 10 000 ms after it also occurs in a
  reference log of `tools/shim/games.json` (the M4 games and the extra
  entries) whose game never calls X (numbers outside that set are printed;
  callback numbers are not account data).

The script rejects a log given twice, maps each log to its game through
`starts.log`, needs one log of each game and rejects a log whose start is
earlier than `PLAN_COMMITTED` (the same time as for experiment 26).
"experiment 27: match" only when both games match A to D.

**What a match confirms:** nothing. A full match records X 3 as
**"narrowed: `HookScreenshots` (experiment 27)"**: A to C rule out the
seven functions with a result, D speaks against `TriggerScreenshot` only
under the residual judgement above. Any mismatch, of D or of A to C,
leaves the candidate unchanged: "`HookScreenshots` or
`TriggerScreenshot`". A new callback number in D is not taken as evidence
for `TriggerScreenshot`, because its number is not known from the page.

**Already seen** (experiments 26 and 27): run with the plan time not yet
set, on the experiment 25 logs (`observe-23624`, `observe-51312`).
Experiment 26: U2, U4, T, U5, U12, U77, S12, R0 and R18 match (two save
sequences fall within the first 30 minutes there); T13, H5, H9 and H19
mismatch (not logged: no `READS` entry yet). Experiment 27: A to D match
for Pixel Puzzles; HoloCure's reference run has no `starts.log` line, so
the script cannot take it. The predictions were written from these logs,
so they are not the test; only fresh runs after the plan commit are.

**Result of experiment 27** (agent runs by the main session after the
plan time 19:14:46; logs recorded by `run.sh`: `observe-63496` HoloCure,
`observe-55924` Pixel Puzzles Ultimate; Pixel Puzzles did not exit on the
close request and was closed by the maintainer with Alt+F4, no forced
kill): **match**, checked by the script.

```text
baseline: 20 reference logs without STEAMSCREENSHOTS_INTERFACE_VERSION003, 18 callback numbers
observe-63496.log (HoloCure - Save the Fans!) A: 1 calls of position 3, other positions [] -> match
observe-63496.log (HoloCure - Save the Fans!) B: lowest rdx byte 1 -> match
observe-63496.log (HoloCure - Save the Fans!) C: result neither a bool nor a 32-bit value -> match
observe-63496.log (HoloCure - Save the Fans!) D: callback numbers in the 10000 ms after the call not in the baseline: [] -> match
observe-63496.log (HoloCure - Save the Fans!): match
observe-55924.log (Pixel Puzzles Ultimate Jigsaw Puzzles) A: 1 calls of position 3, other positions [] -> match
observe-55924.log (Pixel Puzzles Ultimate Jigsaw Puzzles) B: lowest rdx byte 1 -> match
observe-55924.log (Pixel Puzzles Ultimate Jigsaw Puzzles) C: result neither a bool nor a 32-bit value -> match
observe-55924.log (Pixel Puzzles Ultimate Jigsaw Puzzles) D: callback numbers in the 10000 ms after the call not in the baseline: [] -> match
observe-55924.log (Pixel Puzzles Ultimate Jigsaw Puzzles): match
experiment 27: match
```

As fixed beforehand, a match only narrows:
`STEAMSCREENSHOTS_INTERFACE_VERSION003` position 3 is recorded as
"narrowed: `HookScreenshots` (experiment 27)"; `TriggerScreenshot` is
argued against, not ruled out (the page gives no callback numbers).

**Result of experiment 26** (agent run by the main session after the
plan time 19:14:46; log `observe-44048` recorded by `run.sh`): **match**,
checked by the script. The official client showed its launch-options
dialog; the maintainer chose "VPet spielen" there themselves (the plan had
the main session do it; same button, so the run is unaffected). VPet did
not exit on the close request (it stays in the tray); the maintainer
closed it from its tray icon, no forced kill.

```text
observe-44048.log (VPet-Simulator) U2: 36 calls, arguments in the page's ranges and r9 the app ID, distinct handles -> match
observe-44048.log (VPet-Simulator) U4: 36 calls, 0 not sending the latest new handle as a 64-bit call, 0 other uses of a handle before its first 5 -> match
observe-44048.log (VPet-Simulator) T: 36 call handles (20 past the logging limit), 0 out of order, results 0/1 and 1: True/True, one size at 13: True, sizes at 11: 19 -> match
observe-44048.log (VPet-Simulator) T13: 16 call results, 0 not read, 0 without the handle, result 1 and a fitting count, 16 with a count above 16 -> match
observe-44048.log (VPet-Simulator) U5: 50 and 50 calls, 0 not on a completed query in index order or not true, 1 handles with more results than the r8 of their 2 -> match
observe-44048.log (VPet-Simulator) H5: 0 calls not read, 50 pairs with a following 77, 0 not matching -> match
observe-44048.log (VPet-Simulator) H9: 50 calls, 0 not read, 50 starting with the text http -> match
observe-44048.log (VPet-Simulator) U12: 50 calls, 0 out of range or not true, 3 of 3 fully logged items with exactly the 13 statistics -> match
observe-44048.log (VPet-Simulator) U77: 50 calls, 0 not false or not with the 9 buffer -> match
observe-44048.log (VPet-Simulator) S12: 1 calls, one value in the decoder's range equal to the reference run's (reference found: True) -> match
observe-44048.log (VPet-Simulator) R0: 2 calls, a new text, a buffer, a size and true in each -> match
observe-44048.log (VPet-Simulator) R18: 2 sequences, 2 fully logged, 0 of them out of shape, 1 following pairs, count +1 in each: True -> match
observe-44048.log (VPet-Simulator) H19: 1 pairs, 0 calls of 19 not read, 0 without the written size -> match
observe-44048.log (VPet-Simulator): match
experiment 26: match
```

As fixed beforehand, a match confirms, for these interface versions:
`STEAMUGC_INTERFACE_VERSION020` 2 `CreateQueryAllUGCRequest`, 4
`SendQueryUGCRequest`, 5 `GetQueryUGCResult`, 9 `GetQueryUGCPreviewURL`,
12 `GetQueryUGCStatistic`; `SteamUtils010` 13 `GetAPICallResult`;
`STEAMREMOTESTORAGE_INTERFACE_VERSION016` 0 `FileWrite`, 19
`GetFileNameAndSize`. Recorded as narrowed: `…VERSION016` 18
`GetFileCount`, `SteamUtils010` 11 `IsAPICallCompleted`, `STEAMUGC…020`
77 (an item-ID function returning false or 0) and `SteamUser023` 12
`GetVoiceOptimalSampleRate` (all "experiment 26").

## Shadow comparison for `SteamFriends017` position 0 (ADR 0056, planned, not run)

Position 0 was narrowed to `GetPersonaName` or `ClearRichPresence`
("Positions 0 and 44" above); experiment 33 matched and confirmed it as
`GetPersonaName` (experiment 33, under A1). ADR 0056 (decision 2) lets a shadow
comparison tell them apart; this section records the tool, **no run has
been made**, and no result is claimed.

- **What the proxy does** (`crates/shim-observe/src/proxy.rs`,
  `Shadow::PersonaName`): after the official client's call returns, on the
  game's thread, it reads the NUL-terminated bytes `rax` points to (at
  most 1024, in one readable memory region), hashes them with SHA-256 and
  drops them. Only the digest goes to `rebornd` over the shadow session
  (`ComparePersonaName`, `crates/shim-protocol`). The game always gets the
  official client's result.
- **What `rebornd` does** (`apps/rebornd/src/shim_server.rs`): only on a
  shadow session, it compares the digest with the SHA-256 of the
  account's persona name and answers only `matches`. The name comes from
  `CMsgClientAccountInfo` (EMsg 768, `persona_name`), which Valve sends
  unasked around logon (node-steam-user `components/account.js`,
  `ClientAccountInfo` handler); Reborn sends no request for it
  (`crates/cm/src/account_info.rs`, `Session::persona_name`). If it has
  not arrived, `rebornd` waits up to 5 seconds once per Valve session, on a
  task of its own so other questions on the connection are answered
  meanwhile, and refuses the answer if the shadow session ended during
  the wait. The messages are protocol 0.3; the proxy does not send them
  to a 0.2 `rebornd` (it logs why instead).
- **What is logged:** `shadow match`, `shadow mismatch`, `shadow mismatch
  (no text at the result)`, or `shadow: no Reborn answer (<reason>)`.
  Never the text or its digest: the call line of this position leaves out
  register texts, the return line shows only the address, and `READS`
  does not list it. A digest of a short name could be reversed by
  guessing, so it is treated like the name.
- **Reading a result:** a match means the returned text is the account's
  persona name, which fits `GetPersonaName` only (`ClearRichPresence`
  returns nothing, so `rax` would be a leftover). A mismatch does not
  confirm `ClearRichPresence` by itself: it could also mean Reborn's name
  differs (for example renamed during the run). An experiment plan with
  predictions is committed before the first run, as for every experiment.
- **Open before a run:** a recorded, redacted fixture of
  `CMsgClientAccountInfo` (`reborn-cli friends --record` now writes
  `account_info.bin` with the name replaced by `Player 1`), recorded by a
  person on the test account.
  Recorded since: `crates/cm/tests/data/account_info.bin` (2026-11-22,
  test account, `crates/cm/tests/data/README.md`).
- **First run:** experiment 33, match (end of this file).

## Top rows of the gap list (loop iteration 15a)

`python tools/shim/coverage.py` on `main` (2026-11-22, then 205 positions,
130 confirmed, before ADR 0056 added the extra games) put these unknown
positions first: `SteamClient020`/`021` 22,
`SteamUtils010` 34, `STEAMUSERSTATS_INTERFACE_VERSION013` 37,
`SteamClient023` 21, `SteamGameServer012` 0 and `SteamUtils009` 16. No
open pull request (`gh pr list`: #37, #39, #40, #41, #44 to #47) and no
section above plans any of them. The recordings are the reference runs of
`coverage.md` and every later `run.sh` log in the recorder's data folder
(raw logs stay there). Pages read on 2026-11-22:
<https://partner.steamgames.com/doc/api/ISteamClient> (31 functions),
<https://partner.steamgames.com/doc/api/ISteamUtils> (38 functions),
<https://partner.steamgames.com/doc/api/ISteamGameServer> (44 functions;
its constant `STEAMGAMESERVER_INTERFACE_VERSION` is the text
`SteamGameServer012`, the one both games ask for) and, for the call-result
structs, <https://partner.steamgames.com/doc/api/ISteamUserStats>.
Argument registers follow the Microsoft x64 convention (`rcx` the object,
then `rdx`, `r8`, `r9`, then the stack above 32 bytes of shadow space,
<https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention>), as
in experiment 7.

### `SteamClient020`/`021` 22 and `SteamClient023` 21: still no candidate

- **Recorded** (41 logs since iteration 5): one call each, about 100 ms
  after start, before any getter; `rdx` an address (heap or module
  range), `r8` 0x18, a heap address or 0x7ffffffffffffffc, `r9` varying;
  `rax` 0x7ffcead24550 in every log, the same value as `SteamUtils009` 16
  below (a void function's leftover look).
- **Page:** the void functions are `ReleaseUser` (pipe, user: `rdx` is
  an address, not a small handle), `SetLocalIPBinding` (`uint32`,
  `uint16`: `r8` is not always a port) and `SetWarningMessageHook`,
  already the candidate for 020/021 34 and 023 30, called in the same
  runs. Every other function returns a handle, a `bool`, a count or an
  object, or takes a text. Same result as iteration 5: **no candidate**;
  the new recordings change nothing.

### `SteamUtils010` 34: candidates, no separating experiment

- **Recorded:** Please, Touch The Artwork 2 (5 logs): 33 calls from about
  130 ms, about 30 ms apart (the proxy sees calls through the stand-in
  only until call 33 hands the game the real object), `rdx` a stack
  address, `r8` 1, `r9` 0xe; The Thief, the Witch, the Toad, and the
  Mushroom. (1 log): once, `rdx` and `r9` module addresses, `r8`
  0x15b970; HoloCure (4 logs): once after 1.3 to 3.1 s, `rdx` 1, `r8`
  0x7ffffffffffffffc, `r9` 0xd or 0x1d. Result 0 at every call.
- **Fit:** the arguments differ between games and look like leftovers, so
  the call most likely takes none. Every no-argument function of the page
  that can be false or 0 fits: `BOverlayNeedsPresent` (the page asks for
  polling about 33 times a second, which matches the interval in Please,
  Touch The Artwork 2 but is not proof), `IsOverlayEnabled` (false for a
  few seconds while the overlay loads), `IsSteamInBigPictureMode`,
  `IsSteamRunningInVR`, `IsVRHeadsetStreamingEnabled`,
  `IsSteamChinaLauncher`, `IsRunningUnderProton`,
  `IsSteamRunningOnSteamDeck`, `InitFilterText`, the counters
  `GetIPCCallCount`, `GetSecondsSinceAppActive`,
  `GetSecondsSinceComputerActive` and `GetEnteredGamepadTextLength`, the
  enums `IsRunningOnSteamHardware` and `GetSteamHardwareDefaultConfig`,
  and the void `StartVRDashboard`; HoloCure's `rdx` 1 also fits the
  one-argument void setters (`SetGameLauncherMode`,
  `SetVRHeadsetStreamingEnabled`, `SetOverlayNotificationPosition`).
- **No experiment:** on this computer all of them give 0 or false; only a
  change of client settings (overlay, Big Picture, VR) would separate
  them, and no proxy read helps a call without arguments. Left as not
  known.

### `STEAMUSERSTATS_INTERFACE_VERSION013` 37: narrowed by recorded call results

- **Recorded:** Bongo Cat (4 logs): once, after position 15
  (`RequestUserStats`), `rdx` 1, `r8` 0; Coin Maker Simulator (3 logs):
  every 15 s, `rdx` 0. The result is a 64-bit value that the game then
  passes to `Steam_GetAPICallResult` with `r9` 16 (result 1); in the two
  logs with 703 payloads (one per game, experiment 22) every finished
  position-37 call has callback 1112 and size 16. No call has a text argument.
  In Coin Maker, position 39 follows each result with a stat name and a
  stack address.
- **Fit:** of the page's `SteamAPICall_t` functions, those whose
  call-result struct has 16 bytes under natural alignment (which
  experiment 21 found for `UserStatsReceived_t`, 24 bytes) are
  `RequestGlobalStats` (`int` days, at most 60;
  `GlobalStatsReceived_t`: `uint64` game ID, `EResult`),
  `RequestGlobalAchievementPercentages` (no argument;
  `GlobalAchievementPercentagesReady_t`, the same two fields),
  `FindLeaderboard` and `FindOrCreateLeaderboard` (a name first;
  `LeaderboardFindResult_t`: handle, `uint8`) and `AttachLeaderboardUGC`
  (two handles; `LeaderboardUGCSet_t`: `EResult`, handle).
  `UserStatsReceived_t` (24), `LeaderboardScoresDownloaded_t` (24),
  `LeaderboardScoreUploaded_t` (32) and `NumberOfCurrentPlayers_t` (8) do
  not have 16 bytes, and no call has the name the two leaderboard finders
  take. Recorded as **narrowed: `RequestGlobalStats`,
  `RequestGlobalAchievementPercentages` or `AttachLeaderboardUGC`**, no
  experiment yet. Separating them needs the call-result bytes of
  `Steam_GetAPICallResult` (a game ID first for the two global calls, a
  result code first for `AttachLeaderboardUGC`; ADR 0056 decision 1 allows
  the read, but the proxy has no read for entry points yet), and the two
  global calls share one layout; left to a later plan.

### `SteamUtils009` 16: candidate `SetWarningMessageHook`

- **Recorded** (Gravitas, `SteamClient018`, 8 logs; Dark Deception,
  `SteamClient019`, 11 logs; every run the same): Gravitas calls it twice,
  once on each `SteamUtils009` object (pipe 1 at about 75 ms right after
  `GetAppID`, pipe 2 at about 135 ms), with the same `rdx`; Dark
  Deception does the same and then calls it a third time on the pipe-1
  object with another `rdx`. `rdx` lies where Windows loads program
  modules (upper bits 0x7ff6 or 0x7ff7), with the same low 16 bits in
  every run of a game (0x4760 in Gravitas; 0xe150, then 0xee50 in Dark
  Deception) and no text behind it; `r8` a heap address or
  0x7ffffffffffffffc, `r9` 0x1a, 2 or 0xd. `rax` is 0x7ffcead24550 at
  every call.
- **Every function of the page against the recorded shape:**

  | Functions (page) | Shape | Ruled out by |
  | --- | --- | --- |
  | `SetWarningMessageHook` | a function pointer; void | fits (U1 to U3) |
  | `SetGameLauncherMode`, `SetVRHeadsetStreamingEnabled` (`bool`), `SetOverlayNotificationPosition` (enum), `SetOverlayNotificationInset` (two `int`), `GetImageSize`, `GetImageRGBA` (`int` first), `FilterText`, `ShowGamepadTextInput`, `ShowFloatingGamepadTextInput` (enum first) | a 32-bit or smaller first argument | U1 under A1 (`rdx` has non-zero upper bits) |
  | `CheckFileSignature` | a file name; call handle | U1 (no text at `rdx`) |
  | `GetAPICallResult`, `IsAPICallCompleted`, `GetAPICallFailureReason` | a call handle; `bool` or enum | U3 under A1 |
  | `GetCSERIPPort`, `GetEnteredGamepadTextInput` | output pointers; `bool` | U3 under A1 |
  | `BOverlayNeedsPresent`, `InitFilterText`, `IsOverlayEnabled`, `IsRunningUnderProton`, `IsSteamChinaLauncher`, `IsSteamInBigPictureMode`, `IsSteamRunningInVR`, `IsSteamRunningOnSteamDeck`, `IsVRHeadsetStreamingEnabled` (`bool`); `GetConnectedUniverse`, `GetSteamHardwareDefaultConfig`, `IsRunningOnSteamHardware` (enum); `GetEnteredGamepadTextLength`, `GetIPCCallCount`, `GetSecondsSinceAppActive`, `GetSecondsSinceComputerActive`, `GetServerRealTime` (`uint32`) | no arguments; 32-bit or smaller result | U3 under A1; also U2 under A2 |
  | `GetAppID`, `GetCurrentBatteryPower` | no arguments | confirmed at positions 9 and 14 (experiment 15) |
  | `GetIPCountry`, `GetSteamUILanguage` (a text), `StartVRDashboard` (void) | no arguments | U2 under A2 only |

- Dark Deception's second hook on the pipe-1 object fits the page: a hook
  set later replaces the earlier one.

### `SteamGameServer012` 0: candidate `InitGameServer`

- **Recorded** (the same 19 logs): the first call on the
  `SteamGameServer012` object, right after `SteamClient018`/`019` 7
  (narrowed: `SetLocalIPBinding`, here with port 7778), 3
  (`CreateLocalUser`), 6 (`GetISteamGameServer`) and `GetAppID` on the
  pipe-2 `SteamUtils009` object: `rdx` 0, `r8` 0x1e61 (7777), `r9` 0x6987
  (27015) in every run of both games; result 1, except 0 in Gravitas's
  reference run `observe-496`. The only later call on the object is 7
  (Dark Deception) or 8 (Gravitas) at close.
- **Fit:** the page's `InitGameServer` takes an IP (`uint32`; the page
  names `INADDR_ANY` for all local addresses), the game port, the query
  port, server flags, the app ID of the game the server is for and a
  version text, and returns true if the server could bind the given
  address and ports. It is the only function on the page with more than
  four parameters (the next largest, `GetNextOutgoingPacket`,
  `HandleIncomingPacket` and `SendUserConnectAndAuthenticate`, have
  four), so its fifth to seventh arguments (counting `rcx`: flags, app
  ID, version text) are on the stack, which the recordings do not show
  yet.
- **Every other function of the page:** `BUpdateUserData`,
  `SendUserConnectAndAuthenticate`, `SetKeyValue` (a pointer in `r8`),
  `GetNextOutgoingPacket` and `GetAuthSessionTicket` (a pointer in `r9`)
  would have a value below 0x10000 where a pointer goes (the lowest 64 KiB
  are never mapped, as the proxy already assumes for texts); `GetSteamID`
  and `CreateUnauthenticatedUserConnection` return a `CSteamID`, which
  under the x64 convention comes back through a hidden pointer in `rdx`
  (here 0). For all 43, including `HandleIncomingPacket` (null data with
  7777 bytes), `RequestUserGroupStatus`, `BeginAuthSession`,
  `UserHasLicenseForApp`, the no-argument `BLoggedOn`, `BSecure`,
  `WasRestartRequested` and `GetPublicIP`, and the void setters, stack
  slots 6 and 7 are not arguments (for `HandleIncomingPacket`,
  `GetNextOutgoingPacket` and `SendUserConnectAndAuthenticate` slot 5 is
  their fourth page parameter; for the others slot 5 is not an argument
  either): G2 rules them out under A3.

### Experiment 31 (planned 2026-11-22, before the runs)

**Needs READS** (`crates/shim-observe`, ADR 0053 and 0058), added on this
plan's branch with the two read kinds of ADR 0058:

- `("SteamUtils009", 16, Read::Executable("rdx"))`: after the return the
  proxy logs `after rdx: executable (outside steamclient64)` (or `in
  steamclient64`, or `not executable`). Only the page protection is asked
  for (`VirtualQuery`); no byte at the address is read. The candidate
  passes a function pointer; a yes or no about page protection carries no
  account data.
- `("SteamGameServer012", 0, Read::StackAppIdAndText)`: the proxy logs
  only `stack: slot 6 low 32 bits <number>, slot 7 text yes|no` (`not
  readable` for a slot outside readable memory), taken as the call
  started. Slot 5, the value of slot 7 and any text are never logged;
  addresses found on the stack are never followed for logging (slot 7 is
  looked at only to answer yes or no, with `VirtualQuery`, the region's
  end and `checked_add` bounding every step). For the candidate, slot 6 is
  the app ID. If the meaning were another, slot 6 could be leftover stack,
  at worst the low half of the account's Steam ID: the script only
  compares it with the app ID and prints match or mismatch, and no value
  goes into `docs/shim/`. The first version of this plan read slots 5 to 7
  in full and the text behind them; the review pass rejected that, and the
  maintainer chose this narrowed read (ADR 0058). Raw logs stay local.

**Assumptions:** A1, a `bool`, 8-, 16- or 32-bit argument or result
arrives with the upper 32 bits of its register zero, a `bool` as 0 or 1 in
the lowest byte (as in experiments 23 and 24; about the compiled code, not
from the pages). A2, a register the game did not set for the call does not
point into executable memory outside `steamclient64` at every call of
every run. A3, stack slots that are not arguments of the call do not hold
the game's app ID in slot 6 and a pointer to a text in slot 7 at the first
call of every run of both games.

**Predictions** (`tools/shim/experiments/031_hook_and_game_server.py`),
per game:

- U1: `SteamUtils009` 16 is called at least once; at every call `rdx` has
  non-zero upper 32 bits and no text behind it.
- U2: at every call the proxy logged `after rdx: executable (outside
  steamclient64)`.
- U3: every call returned, with non-zero upper 32 bits of `rax` and a
  lowest byte other than 0 and 1.
- G1: the first `SteamGameServer012` call is position 0, called exactly
  once; `rdx` has low 32 bits 0, `r8` and `r9` lie in 1..=0xffff and
  differ.
- G2: its stack line has slot 6 (low 32 bits) equal to the app ID
  `run.sh` started (`starts.log`) and `slot 7 text yes`.
- G3: its result is 0 or 1.

If U1 to U3 match in both games, `SteamUtils009` 16 is confirmed as
`SetWarningMessageHook` (under A1 and A2); if G1 to G3 match in both,
`SteamGameServer012` 0 is confirmed as `InitGameServer` (under A3). A
mismatch leaves the candidate as it is, with the mismatch noted here; a
mismatch of only U2 leaves "`SetWarningMessageHook`, `StartVRDashboard`,
`GetIPCountry` or `GetSteamUILanguage`".

The confirmations rest on these assumptions, so the entries must carry
them: `meanings.md`, `status.json` and the shim's comments will say
"`SetWarningMessageHook` (experiment 31, under A1 and A2)" and
"`InitGameServer` (experiment 31, under A3)".

Checked before the plan time on two earlier logs (Gravitas
`observe-59640`, Dark Deception `observe-62168`, script with the plan
time disabled): U1, U3, G1 and G3 match; U2 and G2 mismatch, as expected
without the new reads.

**Runs:** `tools/shim/run.sh 1067310` (Gravitas) and `tools/shim/run.sh
332950` (Dark Deception), one run each with the default wait (40 s after
Steam loaded, then the normal close request); both games closed normally
in every earlier `run.sh` run. Both calls happen in the first 200 ms, so a
longer run adds nothing. No input in the games is needed. The proxy must
be rebuilt from this branch first (`cargo build -p reborn-shim-observe`).
The script needs one log per game, started after the plan commit.

**Result of experiment 31** (agent runs by the main session after the
plan time 20:53:50, proxy built from the plan branch; logs recorded by
`run.sh`: `observe-59048` Gravitas, `observe-64816` Dark Deception; both
closed normally): **match**, checked by the script.

```text
observe-59048.log (Gravitas) U1: 2 calls of position 16, 0 with a small, missing or text rdx -> match
observe-59048.log (Gravitas) U2: 0 calls without the line 'after rdx: executable (outside steamclient64)' -> match
observe-59048.log (Gravitas) U3: 0 calls without a result of the leftover shape -> match
observe-59048.log (Gravitas) G1: 2 SteamGameServer012 calls, first is position 0: True, 1 calls of position 0 -> match
observe-59048.log (Gravitas) G2: slot 6 is the app ID: True, slot 7 points to a text: True -> match
observe-59048.log (Gravitas) G3: result 1 -> match
observe-64816.log (Dark Deception) U1: 3 calls of position 16, 0 with a small, missing or text rdx -> match
observe-64816.log (Dark Deception) U2: 0 calls without the line 'after rdx: executable (outside steamclient64)' -> match
observe-64816.log (Dark Deception) U3: 0 calls without a result of the leftover shape -> match
observe-64816.log (Dark Deception) G1: 2 SteamGameServer012 calls, first is position 0: True, 1 calls of position 0 -> match
observe-64816.log (Dark Deception) G2: slot 6 is the app ID: True, slot 7 points to a text: True -> match
observe-64816.log (Dark Deception) G3: result 1 -> match
experiment 31: match
```

As fixed beforehand: `SteamUtils009` 16 is confirmed as
"`SetWarningMessageHook` (experiment 31, under A1 and A2)" and
`SteamGameServer012` 0 as "`InitGameServer` (experiment 31, under A3)".

Now in the shim (`crates/shim/src/objects.rs`, `set_warning_message_hook`):
`SteamUtils009` 16 accepts the hook and returns nothing, as this
experiment confirmed (the game reads no result, U3). Not calling the hook
and not storing it is Reborn's choice, not a recorded behaviour: the shim
has no warning messages to pass on, and no recording shows what a hook
call would carry. No shadow-mode entry: there is no result to compare.
`SteamUtils010` 16 was never recorded and stays a stop.
`SteamGameServer012` 0 is not in the shim: the shim has no
`SteamGameServer…` object and no objects for the game server pipe
(experiment 24).

## Inventory results, remote storage and UGC rows (loop iteration 17a)

`python tools/shim/coverage.py` on `main` (2026-11-23, after #52 merged: 220 positions,
150 confirmed, 70 open). The top unknown rows (`STEAMUSERSTATS_INTERFACE_VERSION013`
3 and 1, `SteamClient020`/`021` 22, `SteamUtils010` 34, `SteamClient023`
21) were looked at in loop iterations 15a and 16a above; no new
recordings of them since. Experiment 33 (#52, merged: `STEAMREMOTESTORAGE_INTERFACE_VERSION014` 15
and 1, `SteamFriends017` 0) and the open #53 (ADR 0059) do not work on
the rows below.
The recordings are the reference runs of `coverage.md` and every later
`run.sh` log in the recorder's data folder (raw logs stay there):
Cave Crawlers 15 logs, Bongo Cat 4, Cartapli 6, HoloCure 4. Pages read on
2026-11-23: <https://partner.steamgames.com/doc/api/ISteamInventory> (33
function headings, 36 signatures with the `SetProperty` overloads, 6
callbacks),
<https://partner.steamgames.com/doc/api/ISteamRemoteStorage> (59
functions, deprecated ones included) and
<https://partner.steamgames.com/doc/api/ISteamUGC> (in two parts; every
function up to `GetWorkshopEULAStatus`). The page order is alphabetical,
not the order of positions. Argument registers as in experiment 7
(Microsoft x64 convention). Values below 0x10000 are called small; a value
with non-zero upper 32 bits is called wide.

### `STEAMINVENTORY_INTERFACE_V003`: what is recorded

- **Cave Crawlers** (the same start in all 15 logs; the reference run
  also called 6 again at about 12 s): at about 1.1 s position 6
  (`rdx` a stack address; it writes a small handle there, experiment 14),
  then 21 with `rdx` 0, `r8` a stack address and `r9` small, result 0,
  then 20 with `rdx` 0xf, `r8` the same heap address position 6 had in
  `r8` and `r9` small, result 1. About 0.2 s later a 4702 callback (size
  1), and in the same millisecond two calls of 21: `rdx` 0, then `rdx` a
  heap address, both with the same stack address in `r8` and a heap
  address in `r9`, both result 1. Then a 4700 (size 8), a 4701 (size 4) and
  a 4700 again, then position 1 with the handle of 6.
- **Bongo Cat** (4 logs): at about 0.77 s, in this order: 11 (`rdx` a
  stack address, `r8` 8, `r9` an address in a module, result 1), 13 (`rdx`
  a stack address, `r8` a heap address, `r9` 2, result 1), 6 (writes a
  small handle), 26 (`rdx` 0x18, `r8` small, `r9` the same module address,
  a 64-bit result with wide upper bits). A 4702 at about 1 s, then a 4701
  and a 4700 (about 1.24 s), followed by 3 and 1 with the handle of 6; at
  about 1.39 s two more 4700s and, for two other small handles, 3, 1, 1, 3,
  5 each. Position 2 is called about 50 times with a handle, `r8` 0 and,
  in some calls, a property-like name in `r9` (`accountid`, `itemid`,
  `quantity`). If the game keeps running past about 60 s it calls 17 and
  18 once a minute, each followed by 3, 1 and 5 for one handle. In the one
  log with 703 payloads (`observe-42296`), the result of 26 came back in
  one 703 with size 8 (loop iteration 10a already noted the `r9` 8 of its
  `Steam_GetAPICallResult`).
- **Callbacks on the page**, fields as listed: `SteamInventoryResultReady_t`
  (`m_handle` `SteamInventoryResult_t`, `m_result` `EResult`),
  `SteamInventoryFullUpdate_t` (`m_handle`),
  `SteamInventoryDefinitionUpdate_t` (no fields),
  `SteamInventoryEligiblePromoItemDefIDs_t` (`EResult`, `CSteamID`, `int`,
  `bool`), `SteamInventoryStartPurchaseResult_t` (`EResult`, `uint64`,
  `uint64`), `SteamInventoryRequestPricesResult_t` (`EResult`,
  `m_rgchCurrency` given as `char`). Under A2 of experiment 32, with the
  handle and the `EResult` as 32 bits (A1): 8, 4, 1 (an empty struct),
  17 to 24, 20 to 24 and 5 to 8 bytes. So 4700 (size 8) and 4701 (size 4)
  are the only recorded numbers whose sizes fit `SteamInventoryResultReady_t`
  and `SteamInventoryFullUpdate_t`, and 4702 (size 1) fits only the struct
  with no fields; these are candidates by size, nothing more, until their
  bytes are seen. The page says of `SteamInventoryFullUpdate_t`:
  "Triggered when GetAllItems successfully returns a result which is newer
  / fresher than the last known result", and "The regular
  SteamInventoryResultReady_t callback will still be triggered immediately
  afterwards".

### Position 6: `GetAllItems`, separated by callback 4701

Experiment 14 left four candidates that write "a new result handle"
through `SteamInventoryResult_t *pResultHandle` and return true:
`GetAllItems`, `GrantPromoItems`, `TriggerItemDrop` and `AddPromoItem`.
Of these, the page ties `SteamInventoryFullUpdate_t` to `GetAllItems`
only. If the 4701 payload holds the handle position 6 wrote, and is
followed by a 4700 for the same handle, position 6 is `GetAllItems`
(under A6 below). Needs 4700 and 4701 in `PAYLOAD_LISTED`.

### Position 21 (Cave Crawlers): candidate `GetItemDefinitionIDs`

Shape: two pointer-sized arguments, `rdx` NULL in the first two calls and
a buffer in the third, `r8` the same stack address throughout; false
before the 4702, true after it. The page: `GetItemDefinitionIDs(
SteamItemDef_t *pItemDefIDs, uint32 *punItemDefIDsArraySize )` returns
false "if item definitions have not been loaded from the server", "If
`pItemDefIDs` is NULL then this will return the number of elements the
array needs to hold", and it should be called "in response to a
SteamInventoryDefinitionUpdate_t callback".

| Functions (page) | Ruled out by |
| --- | --- |
| `GetItemDefinitionIDs` | fits (C3, C4) |
| `GetResultItems`, `SerializeResult`, `GetResultItemProperty`, `GetResultStatus`, `GetResultTimestamp`, `CheckResultSteamID`, `DestroyResult` (a result handle first) | C4: the buffer `rdx` is wide (A1) |
| `GetItemPrice`, `GetItemDefinitionProperty` (an item definition first) | C4: wide `rdx` (A1) |
| `GetItemsWithPrices( pArrayItemDefs, pPrices, uint32 unArrayLength )` | C4: a wide `r9` (A1) |
| `GetEligiblePromoItemDefinitionIDs( CSteamID, pItemDefIDs, punItemDefIDsArraySize )` | C4: a non-zero `r9` below 0x10000 is no size pointer (A3) |
| the `pResultHandle` functions that return true always for a user (`AddPromoItem`, `AddPromoItems`, `ConsumeItem`, `DeserializeResult`, `GenerateItems`, `GetAllItems`, `GetItemsByID`, `GrantPromoItems`, `TransferItemQuantity`, `TriggerItemDrop`) | C3: the first call returns false; also C1 (no new result, A4) |
| `ExchangeItems`, `SubmitUpdateProperties` (also write a result handle) | C1: no 4700 for a handle position 6 did not write (A4) |
| `LoadItemDefinitions` (always true) | C3 (a false result) |
| `GetNumItemsWithPrices` (`uint32`), `SendItemDropHeartbeat`, `StartUpdateProperties`, `RequestPrices`, `RequestEligiblePromoItemDefinitionsIDs`, `StartPurchase`, `RemoveProperty`, `SetProperty` | C3: its result follows the 4702 (0 before, 1 after); for the handle-returning ones also not a call handle (A1); `RemoveProperty` and `SetProperty` need a name text at `r9`, and none was logged |

The last row rests on C3: a function whose page result does not depend on
loaded definitions would have to turn from 0 to 1 exactly at the 4702 in
every run. That is a timing argument, stated as such.

### Position 26 (Bongo Cat): candidate `RequestPrices`

A 64-bit result that comes back in a 703 of size 8. The page's
`SteamAPICall_t` functions are `RequestPrices()`,
`RequestEligiblePromoItemDefinitionsIDs( CSteamID )` and `StartPurchase(
pArrayItemDefs, punArrayQuantity, unArrayLength )`; their call results
are 5 to 8, 17 to 24 and 20 to 24 bytes (A2), so B2's size 8 leaves
`RequestPrices`. `StartUpdateProperties()` returns a
`SteamInventoryUpdateHandle_t`, not a call handle, and
`SendItemDropHeartbeat()` returns nothing (A5, with B1: no other call
returned the value). Every function with arguments: `rdx` 0x18 is no
pointer (A3) for those with a pointer first and no handle or item
definition worth a 64-bit result for the others (they return `bool`,
`uint32`, `EResult` or nothing; A1). `RequestPrices` "Returns a
SteamAPICall_t" with the call result `SteamInventoryRequestPricesResult_t`.

### Other inventory positions: candidates, no experiment

- **5** (Bongo Cat: a handle the game has just read in `rdx`, a result that
  looks like an address and is the same in every call, also returned by
  17): the page's functions that return nothing are `DestroyResult(
  resultHandle )` and `SendItemDropHeartbeat()`; `StartUpdateProperties()`
  returns a 64-bit handle. Every other function returns a `bool`, `uint32`
  or `EResult` (ruled out by the wide result, A1) or a call handle (no 703
  for it in `observe-42296`). Candidates: `DestroyResult`,
  `SendItemDropHeartbeat` or `StartUpdateProperties`. Nothing the proxy may
  read tells a void function from another.
- **11** (`rdx` a stack address, `r8` 8, `r9` a module address, result
  1): with a pointer first and a small second argument, `AddPromoItem` and
  `TriggerItemDrop`; with one argument (`r8` a leftover) `GrantPromoItems`;
  with none, `LoadItemDefinitions`, `GetNumItemsWithPrices`,
  `StartUpdateProperties` or `SendItemDropHeartbeat`. `GetAllItems` is
  position 6. The other functions need a pointer at `r8` or a handle in
  `rdx` (A1, A3). Telling the grant functions apart would need the
  server's answer; left alone.
- **13** (`rdx` a stack address, `r8` a heap address, `r9` 2, result 1):
  `AddPromoItems( pResultHandle, pArrayItemDefs, 2 )`, `GetItemsByID(
  pResultHandle, pInstanceIDs, 2 )`, `GetItemsWithPrices( pArrayItemDefs,
  pPrices, 2 )`, or a one- or no-argument call with leftovers
  (`GrantPromoItems`, `LoadItemDefinitions` and the others of 11).
  `ExchangeItems` returns false for a generate length other than 1 (the
  page) and `GenerateItems` would need a pointer at `r9`.
- **20** (Cave Crawlers: `rdx` 0xf, `r8` a leftover heap address, `r9`
  small, result 1): `LoadItemDefinitions()` ("always return true",
  triggers the 4702) fits, but Bongo Cat gets a 4702 without calling 20,
  so the 4702 after it does not separate it from `GetNumItemsWithPrices`,
  `StartUpdateProperties`, `SendItemDropHeartbeat`, `GetResultStatus(15)`,
  `GetItemPrice(15, …)` or `CheckResultSteamID(15, …)`. No separating
  read; left as a candidate.
- **2** (Bongo Cat; shape of `GetResultItemProperty( resultHandle,
  unItemIndex, pchPropertyName, pchValueBuffer, punValueBufferSizeOut )`):
  not planned. A read of what it writes could hand back an account ID (its
  `r9` text `accountid`), which is account data. Deferred: the
  conservative choice, decided by the agent under the maintainer's
  delegation of 2026-11-23 (ADR 0060); loop iteration 16a first raised it.
- **1** stays `GetResultItems` or `SerializeResult`; this plan adds
  nothing for it. 17 and 18 are not in the reference runs and are not
  planned.

### `STEAMREMOTESTORAGE_INTERFACE_VERSION014` 21 and 22 (Cave Crawlers): no separating experiment

- **Recorded** (15 logs, the same order every time): at about 1.1 s,
  directly after position 13 (candidate `FileExists`, `FilePersisted` or
  `FileForget`) with the save name, 21 with `rdx` 0xf, `r8` a heap address
  and `r9` small, result 1; then 22 with `rdx` 0xf, `r8` the heap address
  later seen in `r8` of `STEAMINVENTORY_INTERFACE_V003` 6 and 20 (a
  leftover) and `r9` small, result 1; then 15 (`GetFileSize`) and 1
  (`FileRead`). `rdx` 0xf also comes in the inventory's 20, so it looks
  like a leftover too.
- **Fit:** the page's functions without parameters that return `bool` or
  `int32`: `IsCloudEnabledForAccount`, `IsCloudEnabledForApp`,
  `BeginFileWriteBatch`, `EndFileWriteBatch`, `GetFileCount`,
  `GetLocalFileChangeCount`, `GetCachedUGCCount`; with a small handle or
  index first and nothing that must point somewhere: `FileWriteStreamClose`,
  `FileWriteStreamCancel`, `FileWriteStreamWriteChunk` (`r8` a buffer),
  `UGCRead` (`r8` a buffer), `GetCachedUGCHandle` (a 64-bit handle back).
  `GetFileNameAndSize` and `GetLocalFileChange` return texts (a result of
  1 is no address); `GetQuota` needs a pointer at `rdx`; the
  `SteamAPICall_t` functions return wide values (experiment 23, C);
  functions with a file name first need a text at `rdx`.
- Nothing the proxy may read separates them: the results are 1, no
  argument is an output pointer, and telling the two cloud switches apart
  would mean changing the account's or the app's cloud setting. Left as
  not known.

### `STEAMREMOTESTORAGE_INTERFACE_VERSION016` 22 (Cartapli): no separating experiment

- **Recorded** (6 logs): one call at about 95 ms, `rdx` 0x18, `r8` small
  and not the same in every log, `r9` an address in a module (the same pattern
  as leftovers in other interfaces), result 0.
- **Fit:** every no-parameter function above that can return 0 or false
  (`GetFileCount` is narrowed for position 18, experiment 26, so not this
  one), and the handle functions with a small first argument returning
  false or 0. No output pointer to read; left as not known.

### `STEAMUGC_INTERFACE_VERSION016` 67 (HoloCure): no separating experiment

- **Recorded** (4 logs, the same values): one call at about 0.1 s, `rdx`
  1, `r8` 0x7ffffffffffffffc, `r9` 0x1d, result 0.
- **Fit:** `GetNumSubscribedItems( bool )` and `GetNumDownloadedItems()`
  (0 items), `SuspendDownloads( bool )` (void), and the many functions with
  a query handle, update handle or `PublishedFileId_t` 1 first that return
  false or 0 for an unknown handle or item (`GetItemState`, `DownloadItem`,
  `MarkDownloadedItemAsUnused`, `ReleaseQueryUGCRequest`, the `Set…`
  query functions, and others). `r8` is no pointer in any process (it is
  not a canonical user address), so no read could add anything. Left as
  not known.

### Experiment 34 (planned 2026-11-23, before the runs)

**Needs `PAYLOAD_LISTED`** (`crates/shim-observe`, ADR 0050 and 0054),
added on this plan's branch: 4700 and 4701 (the first 64 bytes of their
payloads, which are 8 and 4 bytes). Why no candidate can carry a ticket, a
token, a password or a Steam ID: by size and position these are the
page's `SteamInventoryResultReady_t` (a result handle and an `EResult`)
and `SteamInventoryFullUpdate_t` (a result handle). If they were another
callback, 8 and 4 bytes are too few for a ticket, a token or a password,
but an 8-byte payload could hold a 64-bit Steam ID (possibly another
user's) and a 4-byte one an account ID. The listed bytes stay in the
local log, the script prints only match or mismatch, and `docs/shim/`
quotes no payload value. Listing them anyway was decided by the agent
under the maintainer's delegation of 2026-11-23 (ADR 0060). No
`READS` entry is added; the handle position 6 writes is read since
experiment 14, and 703 is listed since experiment 22.

**Assumptions:** A1 and A2 as in experiment 32. A3: a pointer a function
writes through is NULL or an address of at least 0x10000. A4: every
inventory result a call creates gets a 4700 while the game runs (the page:
it "is fired whenever an inventory result transitions from
k_EResultPending to any other completed state"). A5: a call handle in a
703 is the `SteamAPICall_t` of the call that created it; a leftover `rax`
of a void function or an update handle is not one, at least when no other
logged call returned that value. A6: 4701 fires only after `GetAllItems`,
as the page says of `SteamInventoryFullUpdate_t`. A7: 4700 and 4701 are
inventory callbacks, 4700 `SteamInventoryResultReady_t` and 4701
`SteamInventoryFullUpdate_t`. No page gives callback numbers; only the
sizes and the match of bytes 0..4 with a handle tie them to the
inventory, and a small handle value could match by chance. Bongo Cat,
where the 4701 has to pick the handle of 6 out of three live handles, is
the stronger evidence; Cave Crawlers has only one.

**Predictions** (`tools/shim/experiments/034_inventory_results.py`), by
group, each decided by its own game's log:

- Both groups (CC: Cave Crawlers, BC: Bongo Cat):
  - H1: position 6 is called; every call returns 1 and writes a small
    handle at `rdx`.
  - H2: at least one 4701; each has size 4, its bytes 0..4 are a handle a
    call of 6 wrote before it, and the next callback header is a 4700 for
    the same handle.
  - H3: every 4700 has size 8, and its bytes 0..4 are a handle the game
    then passes to position 1 or 3 (low 32 bits of `rdx`).
- Group CC:
  - C1: every 4700 is for a handle position 6 wrote.
  - C2: at least one 4702, each of size 1.
  - C3: position 21 is called before the first 4702 and after it; every
    call before returns exactly 0, every call after exactly 1; after the
    first 4702 a call with `rdx` 0 is followed directly by a call of 21
    with a non-zero `rdx` and the same `r8`.
  - C4: every non-zero `rdx` of 21 is wide; some call has a wide `r9`,
    and some call a non-zero `r9` below 0x10000.
- Group BC:
  - B1: position 26 is called; every result is wide and returned by no
    other logged call.
  - B2: for each result of 26 exactly one 703 has bytes 0..8 equal to it;
    all of them have size 8 and one number, which is not 1101.

**What a match confirms:** H1 to H3 matched in both games:
`STEAMINVENTORY_INTERFACE_V003` 6 `GetAllItems` (experiment 34, under A6),
callback 4701 `SteamInventoryFullUpdate_t` and 4700
`SteamInventoryResultReady_t` (under A7; handle at bytes 0..4; bytes 4..8, the
`EResult`, recorded, not predicted). CC matched: 21
`GetItemDefinitionIDs` (experiment 34, under A1, A3 and A4), and 4702 as
a candidate `SteamInventoryDefinitionUpdate_t` (by size only). BC
matched: 26 `RequestPrices` (experiment 34, under A1, A2 and A5). A group
that mismatches leaves its candidates as they are; the other group is
not affected. A mismatch of H2 in one game only is recorded as it is and
confirms nothing about 6.

Checked before the plan time on earlier logs (script functions called
directly, plan time and `starts.log` check bypassed): Cave Crawlers
`observe-58500` and `observe-752`: H1, C2, C3 and C4 match; H2, H3 and C1
mismatch (no 4700 or 4701 payloads yet, as expected). Bongo Cat
`observe-42296`: H1, B1 and B2 match, H2 and H3 mismatch (no payloads);
`observe-51224` and `observe-47280` (from before 703 payloads were
logged): H1 and B1 match, B2 mismatches. The predictions were written
from these logs, so they are not the test; only fresh runs after the plan
commit are.

**Runs:** one run each, started after the plan commit, with the proxy
built from this branch (`cargo build -p reborn-shim-observe`; a proxy
built from another branch has no 4700 or 4701 payloads):

- `tools/shim/run.sh 2143030` (Cave Crawlers). Its inventory calls and
  callbacks come by about 1.7 s.
- `tools/shim/run.sh 3419430` (Bongo Cat). Its calls and callbacks come by
  about 1.4 s.

Keep the default wait (40 s after Steam loaded): it is enough, and a
longer run lets Bongo Cat start its own 17 and 18 at about 60 s. No input in either
game; both closed on the close request in experiment 14. No inventory
call is asked for: whatever Bongo Cat's 11 and 13 do at start and, if it
still runs at about 60 s, its 17 and 18 are the game's own calls,
recorded in all four earlier runs (red line 4). The script needs one log per game; a game whose run fails
can be run again later, and only its group waits.

**Result of experiment 34** (agent runs by the main session after the
plan time 02:31:23, proxy built from the plan branch; logs recorded by
`run.sh`: `observe-45060` Cave Crawlers, `observe-27548` Bongo Cat; both
closed normally, no input): **mismatch** overall, checked by the script.

```text
observe-45060.log (Cave Crawlers) H1: 1 calls of 6, all true with a small handle written: True -> match
observe-45060.log (Cave Crawlers) H2: 1 of 1 callbacks 4701 with size 4, a handle of 6 and a 4700 for it next -> match
observe-45060.log (Cave Crawlers) H3: 1 of 2 callbacks 4700 with size 8 and a handle read later by 1 or 3 -> MISMATCH
observe-45060.log (Cave Crawlers) C1: 1 of 2 callbacks 4700 for a handle of 6 -> MISMATCH
observe-45060.log (Cave Crawlers) C2: 1 callbacks 4702, all size 1: True -> match
observe-45060.log (Cave Crawlers) C3: 1 calls of 21 before the first 4702 (all false: True), 2 after (all true: True), NULL-then-buffer pair: True -> match
observe-45060.log (Cave Crawlers) C4: buffers wide: True, a wide r9: True, a small non-zero r9: True -> match
observe-45060.log (Cave Crawlers) group CC: MISMATCH
observe-27548.log (Bongo Cat) H1: 1 calls of 6, all true with a small handle written: True -> match
observe-27548.log (Bongo Cat) H2: 1 of 1 callbacks 4701 with size 4, a handle of 6 and a 4700 for it next -> match
observe-27548.log (Bongo Cat) H3: 4 of 4 callbacks 4700 with size 8 and a handle read later by 1 or 3 -> match
observe-27548.log (Bongo Cat) B1: 1 calls of 26, all with a 64-bit result no other call returned: True -> match
observe-27548.log (Bongo Cat) B2: 1 callbacks 703 for 1 results of 26 (one each: True), all size 8: True, numbers: 1, not 1101: True -> match
observe-27548.log (Bongo Cat) group BC: match
experiment 34: MISMATCH
```

As fixed beforehand: group BC matched, so `STEAMINVENTORY_INTERFACE_V003`
26 is confirmed as `RequestPrices` (experiment 34, under A1, A2 and A5).
Group CC mismatched (H3 and C1: in Cave Crawlers one of two 4700
callbacks did not carry the handle of 6 and was not read later by 1 or
3), so 21 stays a candidate (`GetItemDefinitionIDs`) and 4702 stays a
candidate by size. H3 did not match in both games, so 6 `GetAllItems`
and the callbacks 4700 and 4701 stay candidates as well. The first of the
two 4700 callbacks in Cave Crawlers is not explained by this plan; finding its source needs
a new plan.

## Achievement names and leaderboards (loop iteration 16a)

`python tools/shim/coverage.py` on the experiment 31 branch
(`loop/plan-15a`, 2026-11-22: 220 positions, 140 confirmed, 80 open). The
top unknown rows `SteamClient020`/`021` 22, `SteamClient023` 21 (no
candidate), `SteamUtils010` 34 (no separating experiment) and the
narrowed `STEAMUSERSTATS_INTERFACE_VERSION013` 37 are handled in loop
iteration 15a above; no open pull request (`gh pr list`: #39, #46 to #49)
works on the rows below. The recordings are the reference runs of
`coverage.md` and every later `run.sh` log in the recorder's data folder
(raw logs stay there). Page read on 2026-11-22, in two parts (its text is
longer than one read): <https://partner.steamgames.com/doc/api/ISteamUserStats>
(every function with its overloads, the call-result structs, the
leaderboard enums and constants). Argument registers as in experiment 7
(Microsoft x64 convention).

The page's text-returning functions are `GetAchievementName( uint32
iAchievement )` ("an empty string" for an invalid index),
`GetAchievementDisplayAttribute( const char *pchName, const char *pchKey
)` and `GetLeaderboardName( SteamLeaderboard_t hSteamLeaderboard )` ("an
empty string if the handle is invalid"). Its `SteamAPICall_t` functions
and their call results are listed in loop iteration 10a (G). Fields of the
leaderboard call results, from the page: `LeaderboardFindResult_t`
(`SteamLeaderboard_t`, `uint8`), `LeaderboardScoresDownloaded_t`
(`SteamLeaderboard_t`, `SteamLeaderboardEntries_t`, `int`),
`LeaderboardScoreUploaded_t` (`uint8`, `SteamLeaderboard_t`, `int32`,
`uint8`, `int`, `int`), `LeaderboardUGCSet_t` (`EResult`,
`SteamLeaderboard_t`). Enums on the page: `ELeaderboardSortMethod` 0 to 2,
`ELeaderboardDisplayType` 0 to 3, `ELeaderboardDataRequest` 0 to 3,
`ELeaderboardUploadScoreMethod` 0 to 2.

Struct sizes below take the two handle types as 64 bits (as experiment 22
did for `SteamAPICall_t`; derived from recorded values, not from the
page) and use A2 of experiment 32: a size between the sum of the fields
and the natural-alignment size. That gives `LeaderboardFindResult_t` 9 to
16 bytes, `LeaderboardUGCSet_t`, `GlobalStatsReceived_t` and
`GlobalAchievementPercentagesReady_t` 12 to 16, `UserStatsReceived_t` and
`LeaderboardScoresDownloaded_t` 20 to 24, `LeaderboardScoreUploaded_t`
22 to 32, `NumberOfCurrentPlayers_t` 5 to 8. Only
`LeaderboardScoreUploaded_t` can have 32 bytes.

### `…VERSION011` 14 and 15 (Outcore) and `…VERSION012` 14 and 15 (One's Own Exile): candidates `GetNumAchievements` and `GetAchievementName`

- **Recorded:** Outcore: Desktop Adventure (`…VERSION011`, 2 logs, the
  same both times): position 0 twice, then 14 and 15 alternating from
  about 105 ms, 30 calls of 14 and 29 of 15, ending with 14. Every call of
  14 returned 29 (upper 32 bits zero; `rdx` a stack address with no text,
  `r8` a stack address at the first call, then 0x2a or 0x18). The calls
  of 15 had `rdx` 0, 1, ..., 28 in order and returned an address. One's
  Own Exile (`…VERSION012`, 3 logs): position 0, then 14 once (result 16;
  `rdx` 1, `r8` and `r9` stack addresses), then 15 and 6 alternating 16
  times: 15 with `rdx` and `r8` both 0, 1, ..., 15, returning an address;
  6 with an achievement-like name as text in `rdx` (candidates in loop
  iteration 7). In both games no call of the interface before 14 except
  position 0 (confirmed `RequestCurrentStats`), and no 703 callback before
  the first 15. One's Own Exile also calls position 10 (narrowed:
  `StoreStats` or `ResetAllStats`, experiment 13) once, at about 88 to
  97 s, in all three logs, on its own without input; the plan adds
  nothing to that.
- **Position 14, every function of the page against the recorded shape**
  (a value 16 or 29, no text argument, nothing asynchronous before it):

  | Functions (page) | Ruled out by |
  | --- | --- |
  | `GetNumAchievements()` | fits (N1, N2) |
  | `GetTrophySpaceRequiredBeforeInstall()` (`uint64`) | A3 only: the game takes the value as the number of names it asks for |
  | `GetAchievementName( uint32 )` (a text) | N1: a result of 16 or 29 lies below 0x10000, so it is no text address |
  | `GetLeaderboardName( SteamLeaderboard_t )` (a text) | N0: no leaderboard handle exists yet; also N1 (no text address) |
  | `RequestCurrentStats`, `StoreStats`, `GetUserStatsData`, `ResetAllStats`, `SetUserStatsData` (`bool`) | N1 under A1 (value at least 4) |
  | `GetNumberOfCurrentPlayers`, `RequestGlobalAchievementPercentages` and the other `SteamAPICall_t` functions | N1: upper 32 bits zero, while every recorded call handle used them (experiment 23, C); N0: no 703 for it |
  | `GetLeaderboardEntryCount`, `GetLeaderboardSortMethod`, `GetLeaderboardDisplayType`, `GetDownloadedLeaderboardEntry` (a leaderboard or entries handle first) | N0: no find or download call result exists yet; for the two enums also N1 (value above 3) |
  | `GetMostAchievedAchievementInfo` (buffer, `uint32` length, ...) | Outcore: `r8` has non-zero upper 32 bits at the first call (A1); One's Own Exile: `rdx` 1 is no buffer address |
  | `GetNextMostAchievedAchievementInfo` (`int`, buffer, `uint32`, ...) | Outcore: `rdx` wide (A1); One's Own Exile: `r9` wide (A1) |
  | every function with a name first (`GetAchievementIcon`, `GetGlobalStatHistory`, `GetStat`, `SetStat`, the achievement functions, `FindLeaderboard`, ...) and the `GetUser…` functions (a name in `r8`) | no text at `rdx` or `r8` |

- **Position 15** (a small index, an address back):

  | Functions (page) | Ruled out by |
  | --- | --- |
  | `GetAchievementName( uint32 )` | fits (N2 to N4) |
  | `GetLeaderboardName( SteamLeaderboard_t )` | N0: no leaderboard handle exists yet |
  | `GetAchievementDisplayAttribute` (two names) | `rdx` below 0x10000 is no text address |
  | every function returning a number, `bool`, enum or handle | N3: a text at the result in every call; N4 (One's Own Exile): the text is the name the game passes on to position 6 |

### `…VERSION012` 23 and 29 (Fallout Shelter): candidates `FindLeaderboard` and `DownloadLeaderboardEntriesForUsers`

- **Recorded** (4 logs, the same order every time): 13 calls of 23 at
  about 96 ms, each with a name as text in `rdx` (leaderboard-like names);
  `r8` and `r9` vary, `r8` with non-zero upper 32 bits in several calls;
  each returns a 64-bit value. From about 420 ms, 13 calls of 29, each
  with a different small `rdx` (upper 32 bits zero), a heap address in
  `r8` and 4 in `r9`; each returns a 64-bit value. In the one log with
  703 payloads (`observe-12404`, experiment 22's runs) each of the 26
  results comes back in a 703: the 13 of 23 with size 16 and one number,
  the 13 of 29 with size 24 and another number, not 1101.
- **Position 23:** a text first and a call handle back fit only the
  page's `FindLeaderboard( const char * )` and `FindOrCreateLeaderboard(
  const char *, ELeaderboardSortMethod, ELeaderboardDisplayType )`. F1
  rules out the second without A1: in at least one call the low 32 bits
  of `r8`, where its enum would be, are none of the page's
  `ELeaderboardSortMethod` values (0 to 2); under A1 the non-zero upper 32
  bits rule it out as well. Every other function with a name first returns a `bool`, an
  `int`, an `int32` or a text, not a call handle that comes back in a 703
  (F1, F2).
- **Position 29:** a 64-bit result that comes back in a 703 with size 24
  fits, by size (A2), `RequestUserStats`, `DownloadLeaderboardEntries`,
  `DownloadLeaderboardEntriesForUsers` and `UploadLeaderboardScore`. D2
  rules out `RequestUserStats` (its call result is 1101, experiments 21
  and 22); D1 rules out `DownloadLeaderboardEntries` and
  `UploadLeaderboardScore` without A1: their second parameter is an enum
  (`ELeaderboardDataRequest` 0 to 3, `ELeaderboardUploadScoreMethod` 0 to
  2), and the low 32 bits of `r8` are none of those values in every call
  (under A1 the wide `r8` rules them out as well). `AttachLeaderboardUGC`, `RequestGlobalStats`,
  `RequestGlobalAchievementPercentages`, the two finders and
  `GetNumberOfCurrentPlayers` have at most 16 bytes. `r9` 4 is in the
  page's range for `cUsers` ("A maximum of 100 users").
- The array at `r8` holds Steam IDs if the candidate is right. Nothing is
  read there and no read is added: that would be account data (ADR 0043).

### `…VERSION013` 22, 30 and 23 (Pixel Puzzles): candidates `FindLeaderboard`, `UploadLeaderboardScore` and `GetLeaderboardName`

- **Recorded** (2 logs, the same both times): at about 730 ms two calls
  of 22, each with a name as text in `rdx`, `r8` 0xffffffffffffffff, a
  64-bit result; each result comes back in a 703 with size 16 and the
  same number as Fallout Shelter's 23 (and Coin Maker Simulator's 22). At
  about 1.54 s two calls of 30 with small handles in `rdx`, `r8` 1, `r9`
  2, each a 64-bit result that comes back in a 703 with size 32. At about
  2.18 s two calls of 23, `rdx` the same two handles, `r8` 0, `r9` the
  result of a call of 30 (a leftover, by its look), each returning an
  address.
- **Position 22:** as `…VERSION012` 23 above: `FindLeaderboard` or
  `FindOrCreateLeaderboard`; F1 rules out the second without A1 (the low
  32 bits of `r8`, 0xffffffff, are no `ELeaderboardSortMethod` value, 0 to
  2; under A1 the non-zero upper 32 bits as well). Coin Maker Simulator's
  `r8` 4 at this position is also outside those values.
- **Position 30:** of the page's call results only
  `LeaderboardScoreUploaded_t` can have 32 bytes (A2), so of the
  `SteamAPICall_t` functions only `UploadLeaderboardScore(
  SteamLeaderboard_t, ELeaderboardUploadScoreMethod, int32 nScore, const
  int32 *pScoreDetails, int cScoreDetailsCount )` fits U2; `r8` 1 is the
  page's `KeepBest` value. The game uploads this score itself at start,
  without input, in both earlier runs; the experiment adds nothing (red
  line 4).
- **Position 23** (a leaderboard handle in `rdx`, an address back with
  upper 32 bits zero):

  | Functions (page) | Ruled out by |
  | --- | --- |
  | `GetLeaderboardName( SteamLeaderboard_t )` | fits (T1) |
  | `GetLeaderboardEntryCount` (`int`), `GetLeaderboardSortMethod`, `GetLeaderboardDisplayType` (enums) | T1: a text at the result, the name of a find call |
  | `SteamAPICall_t` functions with a leaderboard handle first: `AttachLeaderboardUGC`, `DownloadLeaderboardEntries`, `DownloadLeaderboardEntriesForUsers`, `UploadLeaderboardScore` | T1 (a text at the result); the result's upper 32 bits are zero, while every recorded call handle used them (experiment 23, C) |
  | `GetAchievementName( uint32 )` | the handle's low 32 bits as index get "an empty string", which the proxy's text rule (2 characters or more) does not log (T1) |
  | `GetAchievementDisplayAttribute` | needs a text at `rdx` |

### Rows looked at and not planned here

- `…VERSION012` 2 (Fallout Shelter: a stat name and a stack address,
  result 1, right after position 6 returned 0 for the same name) and
  `…VERSION013` 1 and 39 (a name and a stack address, result 1): the
  page's read-by-name functions (`GetStat` for `int32` and for `float`,
  `GetAchievement`, `GetGlobalStat` for `int64` and `double`,
  `GetAchievementAchievedPercent`) all fit; output bytes would not
  separate them while the values are 0 (0 looks the same in all of them).
  Left as not known.
- `…VERSION013` 3 (Bongo Cat, Pixel Puzzles: a name, a small number in
  `r8`, result 1, before `StoreStats`) and 8 (a name and two stack
  addresses, result 1): `SetStat` (both overloads; a `float` would come
  in `xmm2`, leaving `r8` a leftover), `SetAchievement`,
  `ClearAchievement`, `IndicateAchievementProgress` and
  `UpdateAvgRateStat` fit 3; `GetAchievementAndUnlockTime` and
  `UpdateAvgRateStat` (its two numbers in `xmm` registers) fit 8, and
  nothing the proxy may read separates them while the values are 0. Telling
  the writes at 3 apart would mean looking at what they change on the
  account; left alone (red line 4: plans only watch what games do).
- `STEAMREMOTESTORAGE_INTERFACE_VERSION014` 21 and 22, `…VERSION016` 22,
  `STEAMUGC_INTERFACE_VERSION016` 67 and the `STEAMINVENTORY_INTERFACE_V003`
  rows: recordings looked at, pages not read in this iteration; left for
  a later one. For the maintainer: Bongo Cat calls
  `STEAMINVENTORY_INTERFACE_V003` 2 about 50 times with the same handle
  and, in some calls, the text `accountid` in `r9`. A read of what that
  call writes could hand back an account identifier, which ADR 0043, 0053
  and 0058 do not allow; it needs its own decision.

### Experiment 32 (planned 2026-11-22, before the runs)

**Needs READS** (`crates/shim-observe`, ADR 0043 and 0053), added on this
plan's branch, all three of the kind `result text` (the text `rax` points
to, 2 to 64 printable characters, `VirtualQuery` first; nothing else):

- `("STEAMUSERSTATS_INTERFACE_VERSION011", 15, Read::ResultText)`;
- `("STEAMUSERSTATS_INTERFACE_VERSION012", 15, Read::ResultText)`;
- `("STEAMUSERSTATS_INTERFACE_VERSION013", 23, Read::ResultText)`.

Why no candidate can hand back a ticket, a token, a password or an
account identifier: the only text-returning functions on the
`ISteamUserStats` page are `GetAchievementName`,
`GetAchievementDisplayAttribute` and `GetLeaderboardName`, which return
the game's public achievement data and leaderboard names; a function
returning a number gives "result: no text" or, at worst, a short printable
text at some address, which the script compares but never prints. Raw
logs stay local; `docs/shim/` quotes no name, handle or callback number
from these runs. `PAYLOAD_LISTED` already has 703 (experiment 22), and
nothing is read at `r8` of `…VERSION012` 29.

**Assumptions:** A1 as in experiment 31 (a `bool`, an enum or an 8- to
32-bit argument or result arrives with the upper 32 bits of its register
zero). A2: the size in a 703 payload is the size of the call result
(experiment 22, F), and a struct's size lies between the sum of its
fields and its size under natural alignment. A3: a value a game takes as
the end of the indices it passes to a function that returns a name is
the number of those names (this is what tells `GetNumAchievements` from
`GetTrophySpaceRequiredBeforeInstall`, which has the same shape).

**Predictions**
(`tools/shim/experiments/032_achievement_names_and_leaderboards.py`), by
group, each decided by its own game's log:

- Groups N011 (Outcore, `…VERSION011`) and N012 (One's Own Exile,
  `…VERSION012`):
  - N0: before the first call of 14 the interface was called only at
    position 0, and no 703 is logged before the first call of 15.
  - N1: 14 is called at least once; every call returns the same N, with
    4 <= N <= 49 and upper 32 bits zero.
  - N2: 15 is called exactly N times, with `rdx` (low 32 bits) 0, 1, ...,
    N-1 in call order.
  - N3: every call of 15 has a result text; the N texts differ.
  - N4 (N012 only): every call of 15 is followed, as the next call on the
    interface, by a call of 6 whose `rdx` text equals that result text.
- Groups L012 (Fallout Shelter: find 23, download 29) and L013 (Pixel
  Puzzles: find 22, upload 30, name 23):
  - F1: the find position is called; every call has a text at `rdx` and a
    result with non-zero upper 32 bits; at least one has non-zero upper 32
    bits in `r8`, and at least one has low 32 bits of `r8` outside 0 to 2
    (`ELeaderboardSortMethod`).
  - F2: at least one 703 has bytes 0..8 equal to a find result; all such
    703s have size 16 and one number.
  - D1 (L012): 29 is called; every call has non-zero upper 32 bits in
    `r8` and in its result, and low 32 bits of `r8` outside 0 to 3
    (`ELeaderboardDataRequest`) and 0 to 2
    (`ELeaderboardUploadScoreMethod`).
  - D2 (L012): at least one 703 for a result of 29; all have size 24 and
    one number, which is neither 1101 nor F2's number.
  - U1 (L013): 30 is called; every call's `rdx` is used again (by a call
    of 23 or of 30) and its result has non-zero upper 32 bits.
  - U2 (L013): at least one 703 for a result of 30; all have size 32 and
    one number, which is not F2's.
  - T1 (L013): 23 is called; every call has a result text equal to the
    `rdx` text of a call of 22 made before it, and its `rdx` equals the
    `rdx` of a call of 30.

**What a match confirms:** N011 matched: `…VERSION011` 14
`GetNumAchievements` (under A1 and A3) and 15 `GetAchievementName`. N012
matched: the same for `…VERSION012` 14 and 15. L012 matched:
`…VERSION012` 23 `FindLeaderboard` and 29
`DownloadLeaderboardEntriesForUsers` (under A2). L013 matched:
`…VERSION013` 22 `FindLeaderboard`, 30
`UploadLeaderboardScore` (under A2) and 23 `GetLeaderboardName`. The
entries in `meanings.md`, `status.json` and later shim comments carry
these assumptions. A group that mismatches leaves its candidates as they
are, with the mismatch noted here; the other groups are not affected.

Checked before the plan time on earlier logs (script functions called
directly, plan time and `starts.log` check bypassed): Outcore
`observe-49168` and `observe-47724`, One's Own Exile `observe-51644` and
`observe-612`: N0 to N2 match, N3 and N4 mismatch (no `READS` entry yet,
as expected). Fallout Shelter `observe-12404`: L012 matches (no new read
needed); `observe-29128`, `observe-43972` and `observe-45648` (from before
703 payloads were logged): F1 and D1 match, F2 and D2 mismatch. Pixel
Puzzles `observe-55924` and `observe-51312`: F1, F2, U1 and U2 match, T1
mismatches (no `READS` entry yet). The predictions were written from
these logs, so they are not the test; only fresh runs after the plan
commit are.

**Runs:** one run each, started after the plan commit, with the proxy
built from this branch (`cargo build -p reborn-shim-observe`):

- `tools/shim/run.sh 1275670` (Outcore: Desktop Adventure). The official
  client showed a launch-option window for it before (experiment 7: the
  agent chose the default option and Play, ADR 0049; experiment 10: the
  window was answered too late and the run timed out), so it must be
  answered promptly.
- `tools/shim/run.sh 3086720` (One's Own Exile).
- `tools/shim/run.sh 588430` (Fallout Shelter).
- `tools/shim/run.sh 351030` (Pixel Puzzles Ultimate). In experiment 27
  it did not exit on the close request and the maintainer closed it with
  Alt+F4; never a forced kill (ADR 0044).

The default wait (40 s after Steam loaded) is enough: the calls come in
the first 0.5 s (Outcore, One's Own Exile, Fallout Shelter) or by about
2.2 s (Pixel Puzzles, its 703s by about 2 s). No input in any game; no
stat, achievement or leaderboard write is asked for (Pixel Puzzles'
upload at start is the game's own, recorded in both earlier runs). The
script needs one log per game; a game whose run fails can be run again
later, and only its group waits.

**Result of experiment 32** (agent runs by the main session after the
plan time 21:44:12, proxy built from the plan branch; logs recorded by
`run.sh`: `observe-12100` Fallout Shelter, `observe-56900` One's Own
Exile, `observe-64880` Outcore, `observe-51836` Pixel Puzzles Ultimate;
all four closed normally; the maintainer was at the computer for Outcore
and Pixel Puzzles): **match** in all four groups, checked by the script.

```text
observe-12100.log (Fallout Shelter) F1: 13 calls of 23, all with a text and a 64-bit result: True, one with a wide r8: True, one with r8 no sort method: True -> match
observe-12100.log (Fallout Shelter) F2: 13 callbacks 703 for a find result, all size 16: True, numbers: 1 -> match
observe-12100.log (Fallout Shelter) D1: 13 calls of 29, all with a wide r8 that is no data request or upload method, and a 64-bit result: True -> match
observe-12100.log (Fallout Shelter) D2: 13 callbacks 703 for a 29 result, all size 24: True, numbers: 1, other than 1101 and F2's: True -> match
observe-12100.log (Fallout Shelter) group L012: match
observe-56900.log (One's Own Exile) N0: positions before the first 14: [0], 0 callbacks 703 before the first 15 -> match
observe-56900.log (One's Own Exile) N1: 1 calls of 14, one value: True, value in 4..=49: True -> match
observe-56900.log (One's Own Exile) N2: 16 calls of 15, indices 0..N-1 in order: True -> match
observe-56900.log (One's Own Exile) N3: 16 of 16 calls with a result text, all different: True -> match
observe-56900.log (One's Own Exile) N4: 16 of 16 calls of 15 followed by a 6 with the same name -> match
observe-56900.log (One's Own Exile) group N012: match
observe-64880.log (Outcore: Desktop Adventure) N0: positions before the first 14: [0], 0 callbacks 703 before the first 15 -> match
observe-64880.log (Outcore: Desktop Adventure) N1: 30 calls of 14, one value: True, value in 4..=49: True -> match
observe-64880.log (Outcore: Desktop Adventure) N2: 29 calls of 15, indices 0..N-1 in order: True -> match
observe-64880.log (Outcore: Desktop Adventure) N3: 29 of 29 calls with a result text, all different: True -> match
observe-64880.log (Outcore: Desktop Adventure) group N011: match
observe-51836.log (Pixel Puzzles Ultimate Jigsaw Puzzles) F1: 2 calls of 22, all with a text and a 64-bit result: True, one with a wide r8: True, one with r8 no sort method: True -> match
observe-51836.log (Pixel Puzzles Ultimate Jigsaw Puzzles) F2: 2 callbacks 703 for a find result, all size 16: True, numbers: 1 -> match
observe-51836.log (Pixel Puzzles Ultimate Jigsaw Puzzles) U1: 2 calls of 30, handle used again and 64-bit result: True -> match
observe-51836.log (Pixel Puzzles Ultimate Jigsaw Puzzles) U2: 2 callbacks 703 for a 30 result, all size 32: True, other than F2's: True -> match
observe-51836.log (Pixel Puzzles Ultimate Jigsaw Puzzles) T1: 2 of 2 calls of 23 with a find name as result and a used handle -> match
observe-51836.log (Pixel Puzzles Ultimate Jigsaw Puzzles) group L013: match
experiment 32: match
```

As fixed beforehand: `STEAMUSERSTATS_INTERFACE_VERSION011` and `…012` 14
`GetNumAchievements` (experiment 32, under A1 and A3) and 15
`GetAchievementName` (experiment 32); `…012` 23 `FindLeaderboard`
(experiment 32) and 29 `DownloadLeaderboardEntriesForUsers` (experiment
32, under A2); `…013` 22 `FindLeaderboard` (experiment 32), 30
`UploadLeaderboardScore` (experiment 32, under A2) and 23
`GetLeaderboardName` (experiment 32).

## First live runs of the cloud read and persona name shadows (loop iteration 16b)

Two shadows are merged and have never run against a game: the cloud reads
(`STEAMREMOTESTORAGE_INTERFACE_VERSION014` 15 and 1, PR #47, "Shadow mode
for positions 15 and 1" near the top of this file) and the persona name
(`SteamFriends017` 0, PR #45, "Shadow comparison for `SteamFriends017`
position 0"). Experiment 33 is their first live run. No game was run for
this plan.

### Experiment 33 (planned 2026-11-22, before the runs)

**Games, from the recordings** (every log in `tools/shim/games.json` and
the later `run.sh` logs, scanned with `observelog.calls` on 2026-11-22):

- **Cloud part: Cave Crawlers (2143030).** It is the only recorded game
  that calls `014` 15 or 1. In both its runs (reference `observe-21180`,
  experiment 30 `observe-52568`) it called 15 and then 1 once each, about
  1.1 s after the start, then position 0 (`FileWrite`, not shadowed) about
  2.5 s after the start and again later.
- **Persona part: Dark Deception (332950) and A Park Full of Cats
  (2610090).** `SteamFriends017` 0 is recorded in Paper Lily, An Arcade
  Full of Cats, Dark Deception, UNBEATABLE, A Park Full of Cats and
  VPet-Simulator. Dark Deception called it twice in each of its three
  later runs (`observe-60764`, `observe-62168`, `observe-64816`: about
  1.3 s and 12.5 s after the start), A Park Full of Cats once at about
  70 ms (`observe-62092`). Both closed normally in every `run.sh` run and
  were already used for shadow runs (`BIsSubscribed`, experiment 30).
  VPet-Simulator is left out (long runs, closed from the tray by the
  maintainer).

**Status of the positions before the run.** `014` 15 (`GetFileSize`) and
1 (`FileRead`) are confirmed (experiment 1); the cloud part tests
Reborn's answer, not the meaning. `SteamFriends017` 0 is narrowed to
`GetPersonaName` or `ClearRichPresence` ("Positions 0 and 44", loop
iteration 9a: the only two of the page's 74 functions without an
argument that fit a heap address as result); the persona part tests the
meaning and Reborn's answer together.

**Conditions for the runs.** As for experiment 30: `rebornd` signed in to
the test account with the stored login while the official client runs and
stays signed in to the same account (the maintainer allowed such shadow
runs on 2026-11-22, ADR 0055 stage 1). The redacted fixture of
`CMsgClientAccountInfo` that the persona section asked for before a run
is recorded (`crates/cm/tests/data/account_info.bin`). One counted run
per game. Each arm makes `rebornd` fill a shadow cloud folder for that
game (one app info, one listing, bounded downloads; reads only, at most
one fill a minute); for Dark Deception and A Park Full of Cats this is a
side effect, not compared. The fills of all three games count against
the one-fill-a-minute limit (`FILL_COOLDOWN`).

**One status change of the account.** After `rebornd` signs in,
`watch_friends` (`apps/rebornd/src/daemon.rs`) sends one
`set_persona_state` to online, because Reborn's own-status setting
(`%APPDATA%\Reborn\config\own-status`) holds `online`. This is the
only change to the account during the runs; the maintainer accepted it on
2026-11-23. The same happened in experiment 30 without being in its plan
(noted there).

**Steps for the main session** (in a normal terminal, not the desktop
app; on this plan's branch at the commit that sets `PLAN_COMMITTED`):

1. Check: the official client runs and is signed in to the test account;
   no game runs; `target/debug/reborn-observe.exe status` says not armed.
2. Build: `cargo build -p rebornd -p reborn-cli -p reborn-shim-observe`.
3. Start `rebornd` in the background with its output in a local file
   outside the repository (for example `target/debug/rebornd.exe >
   <scratch>/rebornd-033.log 2>&1`); this log stays on the computer and
   is never quoted beyond the lines named here (it can name the account).
4. Sign `rebornd` in with the stored login: `target/debug/reborn-cli.exe
   daemon library > /dev/null` (the list is account data; it is not
   shown), then `target/debug/reborn-cli.exe daemon status` must say
   `signed in`. Nothing is typed: no password, no code.
5. `tools/shim/run.sh 2143030 60` (Cave Crawlers). `run.sh` itself arms
   with `reborn-observe arm --proxy … --app 2143030`, starts the game
   through the official client, waits 60 s after Steam loaded, asks the
   game to close normally, waits up to two minutes and disarms. **Do not
   run `reborn-observe arm --app` by hand before `run.sh`:** that would
   register and fill once, and `run.sh`'s own arm within the next minute
   would replace the registration while the fill is in its cooldown
   (`FILL_COOLDOWN`, `apps/rebornd/src/shim_shadow.rs`), leaving cloud
   reads without a shadow folder. After the run, `rebornd`'s log must have
   the line `shadow mode: shadow cloud folder filled` for app 2143030
   with `failed=0`; only that and the error kinds are looked at, no
   counts or names go into `docs/shim/`.
6. `tools/shim/run.sh 332950 60` (Dark Deception), then
   `tools/shim/run.sh 2610090 60` (A Park Full of Cats). `run.sh` keeps
   the one-minute gap itself (ADR 0052), which also keeps each arm's fill
   out of the previous fill's cooldown. The 60 s wait (default 40) leaves
   room for the shadow worker, which handles the stats (up to 30 s),
   ownership, persona and cloud comparisons of a game one after another.

For **every** game of steps 5 and 6:

- **Before the run counts:** the arm output `run.sh` shows must contain
  `rebornd: shadow session registered for app <id>.` If it says that
  `rebornd` refused the shadow session instead, the run does not count;
  it may be repeated once after the cause is gone, without any sign-in
  retry loop (a refusal for `not_signed_in` is a stop, below).
- **After each `run.sh`, before the next start:** take a screenshot with
  `powershell -NoProfile -ExecutionPolicy Bypass -File tools/shim/shot.ps1
  -Out <scratch>/033-<id>.png` and look at it for dialogs of the official
  client (it stays local: it may show the account, ADR 0044); check
  `rebornd`'s log for `connection to Steam lost`, `signed out` and
  `reconnecting failed`; and run `target/debug/reborn-cli.exe daemon
  status`, which must still say `signed in`. Any of these failing is a
  stop (below); the next game is not started.
7. `target/debug/reborn-cli.exe daemon shutdown`;
   `target/debug/reborn-observe.exe status` says not armed (if `run.sh`
   ended early, `reborn-observe disarm`).
8. Check `rebornd`'s log once more for the three lines of the per-run
   check (there must be none), then run
   `python tools/shim/experiments/033_cloud_and_persona_shadow.py` with the
   three logs `run.sh` named (`log observe-<pid>.log`), full paths.

Each run takes about two to three minutes (Cave Crawlers took about 2.5
minutes in experiment 30 with the default wait); the three runs with
their gaps take about ten minutes.

**Stop rule** (ADR 0055 "Two sign-ins at once", ADR 0044, LOOP.md "Stop
and ask the maintainer"):

- If `rebornd` or `reborn-cli` asks for Steam Guard, a code, a mobile
  confirmation or a password, or the sign-in fails: stop, no retry, shut
  `rebornd` down, no game is started; the maintainer decides.
- If either client is logged off or kicked during the runs (the official
  client shows itself offline or signed out, or a dialog about another
  sign-in; `rebornd`'s log shows `connection to Steam lost`, `signed out`
  or `reconnecting failed`): no further start; the running game is left to
  `run.sh`'s normal close request and two-minute wait, `run.sh` disarms,
  `rebornd` is shut down, the run does not count, and neither client is
  signed in again by the agent.
- Any dialog of the official client (for example a cloud conflict, which
  Cave Crawlers could raise): stop and leave it for the maintainer.
- A game that does not close within two minutes of the close request
  (`run.sh` prints `STILL RUNNING`) is reported and left running; it is
  never force-killed (ADR 0044).

A run that a stop ends, a game that does not start, or a Cave Crawlers run
whose `rebornd` log lacks the `shadow cloud folder filled` line with
`failed=0` does not count and may be repeated once with this plan, after
the cause is gone. A run that completed counts, whatever it shows; it is
not repeated to get another result. A repeat (or any run after a game
that exited early) may come less than a minute after the previous fill
started; its arm then hits the fill cooldown and `rebornd` logs `a shadow
cloud fill started less than a minute ago`. For Cave Crawlers that run
does not count (no `shadow cloud folder filled` line); wait a minute after
the previous arm before repeating it.

**What the proxy writes** (`crates/shim-observe/src/proxy.rs`, the step-4
return line, then the ADR 0046 block and `answer_asks`; `daemon.rs`,
`compare_file_size`, `compare_file_read`, `compare_persona_name`), for a
logged call `<n>` on thread `<t>`: the call line `… ThreadId(<t>)
<Interface>#<pos> call <n> …`, the result line `… return <n> rax hidden
(account data)` (cloud reads) or `… return <n> rax=0x…` (`017` 0), and
later, with the same prefix, the shadow line `… return <n> <note>`, where
`<note>` is `shadow match`, `shadow mismatch`, `shadow mismatch (<why>)`
(persona name only, for example no text at the result), `shadow: no
Reborn answer (<why>)`, `shadow: skipped, <why>`, `shadow: no app ID
given` or `shadow: no worker thread`.

**Predictions** (`tools/shim/experiments/033_cloud_and_persona_shadow.py`),
per game:

- A: the log has at least one call with a result line of each expected
  position (Cave Crawlers: `014` 15 and `014` 1; Dark Deception and A
  Park Full of Cats: `017` 0);
- B: every call of `014` 15, `014` 1 or `017` 0 in the log has a result
  line, and exactly one shadow line with the same position, call number
  and thread after it; no shadow line of these positions is without its
  call;
- C: every such shadow line says exactly `shadow match`.

All three games must match ("experiment 33: match"); the script also
prints a verdict per part ("cloud part", "persona part"), because the
parts confirm different things. It rejects a log given twice, logs not
recorded by `run.sh` for these games (`observelog.app_of`), more than
one log per game, and logs whose start (`observelog.start_of`) is earlier
than the plan commit (`PLAN_COMMITTED`, set in a follow-up commit; while
it is unset every log is rejected). It prints only counts, the kind of
each note and match or MISMATCH: no file names, sizes, digests, addresses,
registers or reason texts.

Checked before the plan time on three earlier logs (Cave Crawlers
`observe-52568`, Dark Deception `observe-64816`, A Park Full of Cats
`observe-62092`, recorded before the two shadows were in the proxy;
script with the plan time disabled): A matches in all three, B and C
mismatch (no shadow lines), as expected. A made-up log with all notes
`shadow match` gives match; one `shadow: no Reborn answer` gives a
MISMATCH of C.

**What a match of the cloud part confirms** (Reborn's answer, ADR 0055
stage 1, "compared by size and SHA-1 only"): in Cave Crawlers, for the
file the game reads at start, the size `rebornd` reads in its shadow
folder (filled from Valve's cloud listing and SHA-1-checked downloads at
arm time) equals the official `GetFileSize` result, and the bytes it
reads have the same count and SHA-1 as those the official `FileRead`
put into the game's buffer. **What stays open:** it is one game, and no
other recorded game calls these positions, so LOOP.md step 7's three
games for answer mode are not reached; a missing file is never compared
(the official answer for one is not recorded); `FileWrite` (position 0)
is never shadowed (ADR 0055 stage 3); positions 13, 21 and 22 stay
candidates; answer mode for the game needs ADR 0055 stage 2 (every
account-data call of the game matched in shadow mode, and the
maintainer's OK).

**What a match of the persona part confirms** (ADR 0056 decision 2, ADR
0051): in both games, at every logged call, the official result points
to a text whose SHA-256 equals that of the account's persona name as
`rebornd` has it from `CMsgClientAccountInfo`. Of the two candidates only
`GetPersonaName` returns a text; `ClearRichPresence` returns nothing, so
its `rax` would be a leftover. Assumption **A1**: a leftover `rax` of a
function without a result does not point to a NUL-terminated copy of the
account's persona name at every call in both games (about the compiled
code, not from the page). The specific way A1 could fail: a function
without a result whose code last touched the local user's own persona
record (for example while clearing that user's rich presence) and left
`rax` pointing into it, at the name. With A1, a full match of the persona part
confirms `SteamFriends017` 0 as "`GetPersonaName` (experiment 33, under
A1)", and `meanings.md`, `status.json` and the shim's comments carry
that tag; it also shows that Reborn's persona name agrees with the
official one in two games. **What stays open:** position 44 stays
narrowed (not covered); the shim cannot answer `GetPersonaName` yet: the
shim protocol has only `ComparePersonaName`, and handing the name itself
to a shim needs a new message and a decision about account data (ADR
0055's table has no persona entry), so it waits for the maintainer;
answer mode in the proxy needs a third game (for example An Arcade Full
of Cats).

**How a mismatch is recorded.** Each part is judged on its own, with the
script's output quoted here and nothing changed in the shim, the proxy or
`rebornd` to make it match without a new plan.

- Cloud part: a `shadow mismatch` is recorded as such. A local save that
  differs from the cloud version at arm time (for example one not yet
  uploaded) is one possible cause, written down as a possibility, not a
  finding; telling it apart needs a new plan. `014` 15 and 1 stay
  confirmed (experiment 1); only Reborn's answer is in question.
- Persona part: a `shadow mismatch` does not confirm `ClearRichPresence`
  (Reborn's name could differ, for example after a rename during the
  run); `shadow mismatch (no text at the result)` is recorded as a hint
  towards `ClearRichPresence`, not a confirmation. The position stays
  narrowed.
- `shadow: no Reborn answer`, `shadow: skipped`, `shadow: no app ID
  given`, `shadow: no worker thread` or a missing shadow line (for example the game exited first)
  means the call was not compared: recorded as "not compared" with the
  kind of note, a MISMATCH of B or C, and no conclusion either way.

**Result of experiment 33** (agent runs by the main session after the
plan time 01:37:21 with binaries built from the plan branch; `rebornd`
signed in with the stored login next to the official client, no prompt;
logs recorded by `run.sh`: `observe-58500` Cave Crawlers, `observe-59752`
Dark Deception, `observe-16904` A Park Full of Cats, where the maintainer
clicked once to reach the title screen; every arm said "shadow session
registered", the Cave Crawlers fill line had `failed=0`, no
logoff, kick or reconnect line in `rebornd`'s log, `daemon status` signed
in after every run, no dialog in the official client on the local
screenshots (deleted afterwards); all three games closed normally):
**match** in both parts, checked by the script. The plan commit 312e112
(01:37:21, pushed at 01:38:43, before the runs) became 6bafcdb when the
branch was rebased onto `main` after #51; `PLAN_COMMITTED` keeps its
time. In A Park Full of Cats the single `017` 0 call came about 70 ms
after the proxy loaded, long before the maintainer's click, so the click
does not touch the compared call.

```text
observe-58500.log (Cave Crawlers) A: calls with a result line {'014 #15': 1, '014 #1': 1} -> match
observe-58500.log (Cave Crawlers) B: 2 of 2 calls with exactly one shadow line after the result on the same thread, 0 shadow lines without a call, 0 calls without a result line -> match
observe-58500.log (Cave Crawlers) C: shadow notes [014 #1 match: 1, 014 #15 match: 1] -> match
observe-58500.log (Cave Crawlers): match
observe-59752.log (Dark Deception) A: calls with a result line {'017 #0': 2} -> match
observe-59752.log (Dark Deception) B: 2 of 2 calls with exactly one shadow line after the result on the same thread, 0 shadow lines without a call, 0 calls without a result line -> match
observe-59752.log (Dark Deception) C: shadow notes [017 #0 match: 2] -> match
observe-59752.log (Dark Deception): match
observe-16904.log (A Park Full of Cats) A: calls with a result line {'017 #0': 1} -> match
observe-16904.log (A Park Full of Cats) B: 1 of 1 calls with exactly one shadow line after the result on the same thread, 0 shadow lines without a call, 0 calls without a result line -> match
observe-16904.log (A Park Full of Cats) C: shadow notes [017 #0 match: 1] -> match
observe-16904.log (A Park Full of Cats): match
cloud part: match
persona part: match
experiment 33: match
```

As fixed beforehand: the cloud part shows Reborn's answers for `014` 15
and 1 agree with the official client in Cave Crawlers (one game; answer
mode stays out of reach). The persona part confirms `SteamFriends017` 0
as "`GetPersonaName` (experiment 33, under A1)".
