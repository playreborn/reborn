# Steam Cloud

Files Steam keeps per account and app, mostly save games (ADR 0031).
This file is also the spike the plan asks for before Cloud work
(`docs/plan.md`, 7.5): what is known, from where, and what is not.

All methods are service methods of the `Cloud` service; the request and
response types are in steam-vent-proto-steam
`steammessages_cloud_steamclient.rs` (generated from Valve's protobufs).

## Implemented

### List an app's files

- **What:** The files of one app.
- **Method:** `Cloud.EnumerateUserFiles#1` with `appid`,
  `extended_details` true, `count`, `start_index`. Answer: `total_files`
  and `files` (`appid`, `ugcid`, `filename`, `timestamp`, `file_size`,
  `url`, `steamid_creator`, `flags`, `platforms_to_sync`, `file_sha`).
- **Paging:** start at index 0 and ask again while fewer than
  `total_files` arrived. `count` defaults to 500. Reborn stops on an empty
  page and after 20 pages (steamctl loops forever if the server keeps
  sending too few files).
- **Names:** Auto-Cloud files start with a root token such as
  `%WinAppDataLocal%` (steamctl rewrites a leading `%VAR%` before saving).
  Names are untrusted and are never used as paths without checks.
- **Sensitive fields:** `url` is a direct download link to the file;
  `steamid_creator` is the account. Both are removed in fixtures, together
  with `ugcid` and the own SteamID or account ID inside file names.
- **Source:** steamctl `get_cloud_files` (method, `extended_details`,
  paging by `total_files`, `%VAR%` names); Valve's public `ICloudService`
  documentation, `EnumerateUserFiles` (fields, `count` default 500, paging
  by `start_index`).
- **Observed:** 2026-11-20, first test account: app 3419430 (Bongo Cat)
  answered OK with no files; app 2143030 (Cave Crawlers), after playing
  it in the official client, listed 2 files (`MainPlayerData.json`,
  `BackUp_MainPlayerData.json`, 43.8 KiB each) with plain names (no root
  token) and `platforms_to_sync` `All`. Both have the same SHA-1 (the
  backup holds the same content); `flags` is 0; `total_files` is 2.
- **Verified:** yes (2026-11-20, first test account, fixture
  `cloud_files.bin`; one page, so paging past 500 files is untested).

### Download a file

- **What:** The content of one file (ADR 0032).
- **How:** HTTP GET on the `url` from `EnumerateUserFiles`; the body is
  the file. Reborn reads at most the listed `file_size` bytes and keeps
  the body only if its SHA-1 equals `file_sha`.
- **Source:** Valve's public `ICloudService` documentation
  (`EnumerateUserFiles`: "make a GET request to the URL"); steamctl
  `cmd_cloud_download` writes the body as it is.
- **Unknown:** whether a body can be compressed; a compressed body would
  fail the SHA-1 check and be reported.
- **Verified:** yes (2026-11-20, first test account, app 2143030): both
  files arrived uncompressed and matched `file_sha`; a second run found
  them "already there"; a locally changed file was not overwritten. No
  fixture: the body is the user's file itself, and the listing that
  carries size and SHA-1 has one (`cloud_files.bin`).

### Map a file to a local path

- **What:** Where an Auto-Cloud file belongs on this computer (ADR 0033).
- **How:** split the name into the root token and the rest; check the
  rest like an install path; look up the root's folder; join. A root
  with an override for this system, a Windows root on Linux and names
  without a token are reported, not guessed.
- **Roots:** Valve's Steam Cloud documentation (Windows: Documents,
  `AppData\Local`, `AppData\LocalLow`, `AppData\Roaming`, `Saved Games`;
  Linux: home, XDG data folder); `LinuxXdgConfigHome` from recorded app
  info, with the XDG default.
- **Local files not in the cloud:** files below a rule's root and path
  (with `{64BitSteamID}` and `{Steam3AccountID}` filled in) that match its
  `*` pattern, recursive if the rule says so.
- **Verified:** partly (2026-11-20, first test account, Windows): app
  5191630 mapped to
  `AppData\LocalLow\Studio312\Coin Maker Simulator Demo\save_slot_demo_1.json`
  and reported "same"; the log files and `steam_autocloud.vdf` in that
  folder were not listed as local only; a hand-made
  `save_slot_demo_2.json` was listed as local only; app 2143030 reported
  both files as saved through the Steam interface. "Differs", "missing
  locally" and Linux are covered by unit tests only.

### Restore missing saves

- **What:** Download the cloud files that are missing locally into their
  mapped place (ADR 0034), like the official client does before a game
  starts (`AC Launch, down` in its log, see below). `rebornd` does it
  before every game start (ADR 0035).
- **When not:** any file differs, local saves match a rule but are not in
  the cloud, or a root folder does not exist. Then nothing is written.
- **Verified:** yes (2026-11-20, first test account, Windows, app
  5191630): with a hand-made local-only save it refused and wrote
  nothing; with an empty save folder it restored `save_slot_demo_1.json`,
  which then compared as "same"; after a local change it refused with
  "differs" and wrote nothing. Before a game start in the GUI
  (ADR 0035): with the save moved away, starting the game restored it
  and the game showed the save (2026-11-21).

### Cloud reads for the shim (`rebornd`, ADR 0055 stage 0)

- **What:** the shim's `CloudFileSize { name }` and `CloudRead { name,
  max }` (shim protocol 0.2) are answered from the run's cloud folder
  `<Reborn data>/cloud/<account ID>/<app ID>/` (`reborn_core::run_cloud`;
  account ID = the low 32 bits of the SteamID64, `friends.md`):
  `FileSize { size }` or `FileData { data }` (base64, at most `max`
  bytes from the start), each `None` for a missing file. No Valve
  request per call; nothing is written, created or uploaded.
- **Rules:** a connection without a session, a run not started in shim
  mode, an app whose app info has no `ufs` `quota`, and shadow sessions
  (their separate shadow folder comes with stage 1) get `NotAllowed`.
  Names are checked like install paths (no `..`, no absolute paths or
  drive letters, no unsafe characters or device names) and must have no
  root token (`%...%`) and at most 1024 bytes; no symlink (on Windows
  also no junction) is followed, neither below the Reborn data folder
  nor inside the run's folder; a folder is not a file. The last part is
  opened without following a link (Windows: `FILE_FLAG_OPEN_REPARSE_POINT`
  and a reparse-point check on the handle; Unix: device and inode must
  match the checked file), so a link swapped in after the check is
  refused. All of these get `NotAllowed`, as does a read error (ADR 0055
  names no other reason). A file larger than the app's quota (at most
  16 MiB) gets `TooLarge`.
- **Limits:** the threat is another process of the same user changing
  the folder during a read. A *folder* on the way swapped for a link
  between check and open is not caught (std has no handle-relative open);
  the launch side below accepts the same limit, writes by the game decide
  it again. Hard links are regular files and are not detected. Names
  whose last part ends in `.reborn-part` (any case) are refused: that is
  the part file a download writes first.
- **Requests:** none per call. The quota comes from the app info that the
  launch already fetches for restoring saves (ADR 0035): one fetch for
  both, under the restore's time limit. In shim mode, a launch whose app
  info cannot be fetched is refused, like one without the ownership set.
- **Logs:** only sizes and the refusal reason, never names or contents;
  `Debug` of the file data shows only its length.
- **Not yet:** writes by the game, and the shim sending these requests.
  Filling the folder at launch is below.
- **Source:** ADR 0055 ("Protocol messages", "What is cached, and
  where", "Cloud"); no Valve protocol is involved.
- **Verified:** unit tests in `crates/core` and `apps/rebornd` with
  temporary folders (traversal, absolute names, root tokens, streams and
  device names, folder links (junctions on Windows), file symlinks on
  systems that allow making them, a link swapped in before the open,
  other accounts and apps, quota, a
  200 000-byte answer over a served in-memory connection); no live run.

### Filling the run's cloud folder at a shim-mode launch (`rebornd`, ADR 0055 stage 0)

- **What:** before a game starts in shim mode, the run's cloud folder is
  brought up to date from Valve's cloud (`reborn_core::run_cloud_fill`),
  extending ADR 0035. For each listed file without a root token, with L
  the SHA-1 of the local file, C the cloud's and B the base:
  - L = C: nothing (the base is set to C);
  - no local file: downloaded;
  - L = B, C ≠ B (only the cloud changed): the local file is moved to
    `<Reborn data>/cloud-backup/<account ID>/<app ID>/<unix time>/local/`,
    then the cloud version is downloaded; a failed download moves it back;
  - C = B, L ≠ B (only the local file changed): kept, waits for uploads;
  - otherwise (both changed, or no base and L ≠ C): a **true conflict**.
    Nothing at all is written and the launch is answered with
    `Response::CloudConflict { app_id, files }` (IPC 1.13): name, SHA-1,
    size and time of both versions, and whether a base was known. The
    frontend asks the user and sends the launch again with
    `cloud_choice { choice, files }`: `local` or `cloud`, and the
    conflicts the user was shown (name and both SHA-1s). The answer
    resolves only conflicts it names exactly; if any current conflict is
    missing from it or its versions changed since, nothing is written and
    all current conflicts are asked again (AGENTS.md section 5: the user
    decides). `cloud`: as "only the cloud changed". `local`: the local
    file stays, the cloud version is downloaded to `.../<unix time>/cloud/`
    as the backup and becomes the base, so the next launch keeps the
    local file without asking again.
- **Base file:** `<Reborn data>/cloud/<account ID>/<app ID>.base.json`
  ("next to" the folder), JSON `{"version": 1, "files": {name: {"sha1",
  "size", "timestamp"}}}` with the cloud's SHA-1 (lowercase), size and
  time of the version last downloaded (or found equal). Written to a
  part file, then renamed. A damaged or unknown file counts as no base:
  then every differing file is a conflict the user decides, so nothing
  can be overwritten because of it.
- **Never:** upload, delete, or overwrite. Local files that are not in
  the cloud and base entries for files gone from the cloud are left
  alone. Downloads go through `download_cloud_file_into` (SHA-1 checked,
  never over an existing file).
- **Refusals (the game does not start in shim mode):** a true conflict
  without an answer that names it; the listing failed; a download failed
  (the other files are still handled and the base saved); a local file
  is a link, a folder or unreadable; two cloud names that meet in one
  path (compared part by part, case ignored, so `a\b` and `A/B` meet); a
  link on the way to the base file or the backup folder; a file in the
  place of a download's part file. The fill runs before shim mode is
  entered (`ActiveProcess`), so a refusal leaves nothing to undo.
  Skipped, not refused: names with a root token (Auto-Cloud files,
  restored into the game's save folders by ADR 0035), unsafe names
  (including a last part ending in `.reborn-part`), files above the
  run's limit, SHA-1s that are not 40 hex digits.
- **Requests:** the cloud is listed once per launch and shared by the
  ADR 0035 restore and the fill; only files that need it are downloaded.
  A launch answered with `CloudConflict` lists again when it is sent
  with the choice (the download links of the first listing are not
  kept). Downloads are bounded by the CDN client's timeouts, not by a
  limit around the whole fill, so a file moved aside is never left
  without its replacement or its way back.
- **Logs:** counts only (downloaded, replaced, kept, conflicts), never
  names or contents.
- **Limits:** as for reads, a folder swapped for a link between the
  check and the write is not caught (std has no handle-relative open);
  the threat is another process of the same user.
- **Source:** ADR 0055 ("Cloud", "What is cached, and where", decided by
  the maintainer 2026-11-22: a true conflict asks the user before the
  game starts); downloads as in "Download a file" above. The backup
  folder's place and the base file's format are Reborn's choice; the ADR
  names neither.
- **Verified:** unit tests in `crates/core` (temporary folders, a fake
  cloud: every L/C/B case, both choices, failed downloads, skipped and
  refused names, junctions on the way, damaged base) and `apps/rebornd`
  (conflict and failure answers); no live run.

## Known, not implemented

### Download through `Cloud.ClientFileDownload`

- `Cloud.ClientFileDownload#1` (`appid`, `filename`) answers with
  `url_host`, `url_path`, `use_https`, `request_headers`, `file_size`,
  `raw_file_size`, `encrypted`. **Unknown:** when the official client uses
  it instead of the `url`, and whether `raw_file_size` means compressed
  content.


### List apps with cloud files

- **Method:** `Cloud.EnumerateUserApps#1`, empty request; answer `apps`
  with `appid`, `totalcount`, `totalsize`.
- **Source:** ValvePython/steamctl `steamctl/commands/cloud/gcmds.py`
  (MIT), `cmd_cloud_list_apps`.
- **Observed:** 2026-11-20, first test account: Steam answered EResult 2
  (Fail) four times, before and after the account had cloud files. So
  Fail does not mean "no apps". **Unknown** why; Reborn does not use the
  method.

### Upload and delete

- Valve's documentation for the Web API variant: `BeginHTTPUpload`
  (`file_size`, `filename`, `file_sha`, `platforms_to_sync`,
  `upload_batch_id`) answers with `url_host`, `url_path`, `use_https`,
  `request_headers` for an HTTP PUT; `CommitHTTPUpload` afterwards,
  whether the PUT worked or not. `Delete` is permanent.
- The client variant (`Cloud.BeginAppUploadBatch`,
  `Cloud.ClientBeginFileUpload`, `Cloud.ClientCommitFileUpload`,
  `Cloud.CompleteAppUploadBatchBlocking`, `Cloud.ClientDeleteFile`) is only
  described by third-party tools so far; no allowed reference
  implementation shows it. **Not used until it is recorded and confirmed.**

### Sync around a game session

- Valve's Steam Cloud documentation: Steam syncs when a game starts and
  when it exits.
- `Cloud.SignalAppLaunchIntent#1` (`appid`, `client_id`, `machine_name`)
  answers with pending remote operations; `Cloud.SignalAppExitSyncDone#1`
  and `Cloud.GetAppFileChangelist#1` (`appid`, `synced_change_number`;
  answer with `current_change_number`, `files`, `path_prefixes`,
  `machine_names`) exist.
- **Observed** in the official client's `logs/cloud_log.txt`, 2026-11-20,
  Coin Maker Simulator Demo, first test account (only the order of
  steps is logged, not method names or message fields):
  - First use of the app by this account on the computer (`init`): the
    client found local files of a *previous user* in the Auto-Cloud
    folder and moved them into its own per-account folder
    (`userdata/<account>/<app>/ac/...`) before going on. Different
    accounts' saves are kept apart.
  - Launch (`AC Launch, down`): "sync from local change number to global
    change number", download, then watch the files.
  - Exit (`up, AC Exit`): evaluate each `savefiles` rule (logged with the
    same `root`, `path`, `pattern` as the app info), "Need to upload file
    ...", "Upload batch initiated" with an ID, a change number and the
    app build ID, then an HTTP upload per file to a Google Cloud Storage
    host, then "Upload complete, result OK".
  - The 20496-byte save went up as an HTTP body of 9019 bytes
    (`offset=0, length=9019`). **Unknown:** how the upload body is
    transformed (compression? which format?); `ClientBeginFileUpload`
    has both `file_size` and `raw_file_size`, which fits a compressed
    body. Downloads so far arrived raw (SHA-1 of the file).
- **Not used by Reborn** until the upload format and the exact messages
  are backed by a source.

### Auto-Cloud paths

- Valve's Steam Cloud documentation lists the roots and where they are,
  e.g. `WinMyDocuments` (`%USERPROFILE%\My Documents\`),
  `WinAppDataLocal`, `WinAppDataLocalLow`, `WinAppDataRoaming`,
  `WinSavedGames`, `MacHome`, `MacAppSupport`, `MacDocuments`,
  `LinuxHome`, `LinuxXdgDataHome`, the install folder and
  `SteamCloudDocuments`. Each entry has a subdirectory (may contain
  `{64BitSteamID}` or `{Steam3AccountID}`), a pattern, a recursive flag and
  an OS restriction; root overrides map roots between systems.
- The per-app configuration is in the app info (`ufs` section).
  Community app info dumps (e.g. updatemonitor.com) and the tool
  SteamCloudFileManager show `savefiles` entries (`root`, `path`,
  `pattern`, optional `recursive`, `platforms`) and `rootoverrides`
  (`root`, `os`, `oscompare`, `useinstead`, `addpath` or
  `pathtransforms`). Not a usual Reborn source; **unverified** until
  recorded.
- **Observed** 2026-11-20 in recorded app info: app 2143030 (Cave
  Crawlers) has `ufs` with only `quota` 1000000 and `maxnumfiles` 3; app
  3419430 (Bongo Cat) only `quota`, `maxnumfiles`, `hidecloudui`,
  `ignoreexternalfiles`; app 2158270 (Quatro!) the same four keys; app
  3393110 (AION 2) only `quota` and `maxnumfiles`. None has `savefiles`. Cave Crawlers' cloud
  files have names without a root token, so the game writes them itself
  through the Steam interface; such files only matter once the shim
  exists (ADR 0012).
- **Observed** 2026-11-20: app info of an app the account does not own
  can be read (app 1511340, Defender of Kuiper Belt Demo). Its `ufs`:
  `quota` 600000, `maxnumfiles` 50, `appidredirect` 1505520; `savefiles`
  with one entry (`root` `WinAppDataLocalLow`, `path`
  `Cascus Harmony Games/Defender of Kuiper Belt/{64BitSteamID}/`,
  `pattern` `*`, `recursive` `1`) and a key `copied_from_parent` `1`
  next to the entries; `rootoverrides` for `MacOS` (`useinstead`
  `MacHome`) and `Linux` (`useinstead` `LinuxHome`), each with
  `oscompare` `=` and `pathtransforms` (`find`, `replace`). So the key
  names from the dumps match. **Unknown:** whether `appidredirect` means
  the files are stored under app 1505520; how Steam names these files in
  the cloud.
- **Observed** 2026-11-20, app 5191630 (Coin Maker Simulator Demo):
  `savefiles` with one entry (`root` `WinAppDataLocalLow`, `path`
  `Studio312/Coin Maker Simulator Demo/`, `pattern`
  `save_slot_demo_*.json`, no `recursive`); `rootoverrides` for `Linux`
  with `useinstead` `LinuxHome` and `addpath`
  `.config/unity3d/Studio312/Coin Maker Simulator Demo/` instead of
  `pathtransforms`.
- **Observed** 2026-11-20, same app, after playing it in the official
  client: the cloud name is
  `%WinAppDataLocalLow%Studio312/Coin Maker Simulator Demo/save_slot_demo_1.json`,
  so root token, the `savefiles` `path` and the file name, with no
  separator after the token. The local file
  `AppData\LocalLow\Studio312\Coin Maker Simulator Demo\save_slot_demo_1.json`
  has the same size (20496 bytes) and time. The folder also holds
  `steam_autocloud.vdf` (Steam's marker, see Valve's documentation) and
  log files that do not match the pattern; neither is in the cloud.
  Fixture `cloud_files_autocloud.bin`.
