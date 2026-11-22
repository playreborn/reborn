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
    (ADR 0043). No experiment.

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
follow-up commit: 2026-11-22 14:34:22, commit `e21b039`; the first plan
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
"objects". Not in the shim yet: it has no 018/019 objects.
