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
reference run). Under ADR 0051, positions 6 and 3 of
`STEAMINVENTORY_INTERFACE_V003` are confirmed as `GetAllItems` and
`GetResultTimestamp`; position 1 stays a candidate (`GetResultItems`).
Not in the shim yet: it has no inventory object, and an honest answer
needs the account's inventory from Valve's servers.
