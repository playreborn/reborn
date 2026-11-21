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

### Position 9: `GetAppID`, experiment 3 matched (waiting for the maintainer's OK)

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

Under ADR 0044 an experiment the agent ran is marked confirmed only
after the maintainer's OK; until then position 9 stays at "matched".

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
