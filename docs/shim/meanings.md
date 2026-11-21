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
