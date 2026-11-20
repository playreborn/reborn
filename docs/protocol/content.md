# Content (depots, manifests, chunks)

Implemented in `reborn-cm` (`src/apps.rs`, `src/content.rs`) and
`reborn-content` (`src/manifest.rs`, `src/chunk.rs`, `src/cdn.rs`).
See ADR 0013.

### Depots of an app

- **What:** Depot list with platform config and manifest IDs per branch.
- **Message / method:** PICS app info (`CMsgClientPICSProductInfoRequest`),
  section `appinfo/depots`; `branches/<branch>/buildid`; `config/installdir`.
  Manifests: `manifests/<branch>/gid` (newer) or `manifests/<branch> = gid`
  (older). Shared depots use `depotfromapp`.
- **Selection:** `config/oslist` contains the OS, `osarch` matches,
  `language` matches (default `english`), not `lowviolence`.
- **Source:** DepotDownloader `DepotDownloader/ContentDownloader.cs`;
  steamroom `src/types/kv_de.rs`.
- **Verified:** yes (2026-11-19, live: Spacewar app 480, depot 481)

### Depot decryption key

- **What:** The AES-256 key of a depot; refused if the depot is not owned.
- **Message / method:** `CMsgClientGetDepotDecryptionKey` (`depot_id`,
  `app_id`) → `CMsgClientGetDepotDecryptionKeyResponse` (`eresult`,
  `depot_encryption_key`).
- **Source:** steamroom `src/client/mod.rs` (`get_depot_decryption_key`);
  message in `steam-vent-proto-steam` `steammessages_clientserver_2`.
- **Verified:** yes (2026-11-19, live: Spacewar app 480, depot 481)

### Manifest request code

- **What:** Short-lived code required in manifest URLs.
- **Message / method:** `ContentServerDirectory.GetManifestRequestCode#1`
  (`app_id`, `depot_id`, `manifest_id`, `app_branch`).
- **Source:** steamroom `src/client/mod.rs`; SteamKit2 `Steam/CDN/Client.cs`
  (URL with request code).
- **Verified:** yes (2026-11-19, live: Spacewar app 480, depot 481)

### Content servers

- **What:** CDN and cache hosts for the client's cell.
- **Message / method:** `ContentServerDirectory.GetServersForSteamPipe#1`
  (`cell_id`). Use `vhost` in URLs; Reborn uses HTTPS whenever
  `https_support` is `mandatory` or `optional` (SteamKit2: only when
  mandatory). Keep types `SteamCache` and `CDN`, respect `allowed_app_ids`,
  skip proxies and China-only servers, order by `weighted_load`.
- **Source:** SteamKit2 `Steam/WebAPI/ContentServerDirectoryService.cs`,
  `Steam/CDN/Client.cs` (`BuildCommand` uses the vhost);
  DepotDownloader `DepotDownloader/CDNClientPool.cs`.
- **Verified:** yes (2026-11-19, live: Spacewar app 480, depot 481)

### Manifest download and format

- **What:** `GET /depot/<depot>/manifest/<gid>/5/<request code>`; a ZIP with
  one entry containing sections `magic u32, length u32, protobuf`:
  payload `0x71F617D0`, metadata `0x1F4812BE`, signature `0x1B81B817`,
  end `0x32C415AB`. Encrypted file names: Base64 of `ECB(IV) || CBC(name\0)`.
- **Source:** SteamKit2 `Types/DepotManifest.cs`, `Steam/CDN/Client.cs`;
  steamroom-client `src/manifest.rs` (ZIP container).
- **Verified:** yes (2026-11-19, live: depot 481 manifest with encrypted names, served by an Akamai CDN host over HTTPS without CDN auth token; offline tests with synthetic data)

### Chunk download and processing

- **What:** `GET /depot/<depot>/chunk/<sha1 hex>`; AES-256 `ECB(IV) ||
  CBC/PKCS#7` with the depot key; then VSZa (Zstd), VZa (LZMA) or PK (ZIP);
  check size and zero-seeded Adler-32.
- **Source:** SteamKit2 `Steam/CDN/DepotChunk.cs`, `Util/VZstdUtil.cs`,
  `Util/VZipUtil.cs`; steamroom `src/depot/chunk.rs`.
- **Verified:** yes (2026-11-19, live: first chunk of depot 481 downloaded,
  decrypted, decompressed and checksum-verified; all three containers
  offline with synthetic data)

### Install and appmanifest

- **What:** Chunks are downloaded in parallel (8 at a time, 5 attempts on
  rotating servers) into `steamapps/downloading/<app>`, every file with a
  SHA-1 in the manifest is hashed, then files are moved to
  `steamapps/common/<installdir>`. `appmanifest_<app>.acf` is written last
  (temporary file + rename) with the official client's key order and values
  (`StateFlags` 4, `Universe` 1, `DownloadType` 1, `UpdateResult` 0,
  `InstalledDepots/<depot>/{manifest,size}`).
- **Source:** key order and values compared with 18 appmanifests written by
  the official client (2026-11-19, keys and non-personal values only);
  staging folder name as used by the official client (`downloading`).
- **Not yet:** install scripts (`installscript.vdf`, e.g. redistributables),
  `LauncherPath`.
- **Verified:** yes (2026-11-19, live: Spacewar installed by Reborn into the
  official client's library while it was closed; the client then listed it as
  installed without downloading it again and "verify integrity of game files"
  reported all files as valid). Offline: 7 install tests.

### Update, resume and verify

- **What:** The installed manifest ID comes from `InstalledDepots` in the
  existing appmanifest; that manifest is downloaded like the new one. For
  every chunk of a changed file, Reborn uses the first source that has it
  with the right size and Adler-32: the staging file (resume), the installed
  file found via the chunk ID in the previous manifest or at the same
  offset (reuse), then the CDN. Files with the same SHA-1 in both manifests
  and the right size are left alone; `verify` hashes them instead. A file
  that reused local data and fails its SHA-1 is downloaded completely once
  more. Files and empty directories of the previous manifest that the new
  one lacks are removed. The appmanifest is updated in place, keeping
  Steam's own fields.
- **Source:** Reborn's own design; chunk IDs are content hashes (SHA-1) in
  the manifest format (SteamKit2 `Types/DepotManifest.cs`), which makes
  them usable as reuse keys.
- **Verified:** offline (update, resume, verify/repair, trust trade-off
  tests). Live (2026-11-19, Spacewar): second `install` reports up to date,
  `verify` keeps all 8 files, and a file with appended bytes is repaired from
  its own still-valid chunk without downloading. A real version update has
  not been seen live yet.

### File permissions on Unix

- **What:** Every installed file gets an x bit wherever it has an r bit
  (`0644` becomes `0755`, `0664` becomes `0775`), regardless of the
  manifest's executable flag. Files kept unchanged from an earlier install
  are fixed as well.
- **Why:** Some depots flag no file as executable. Example: the Linux depot
  3419433 of app 3419430 (Bongo Cat) has flags `0x0000` on all files,
  including `BongoCat.x86_64` and `launch.sh`. Installed only by the flags,
  the game cannot start (`Permission denied`).
- **Source:** observed behaviour of the official Steam client on Linux
  (Debian 13 VM, 2026-11-20): right after installing app 3419430, all 189
  files were `rwxrwxr-x` (umask 002), before the first start.
  DepotDownloader only marks flagged files (`ContentDownloader.cs`, around
  the `EDepotFileFlag.Executable` check), so it is not followed here.
- **Verified:** observation of the official client as above; Reborn's
  behaviour is covered by a Unix-only test. Live (2026-11-20, same VM):
  after Verify, Reborn's install of app 3419430 starts natively through
  `launch.sh`, and the in-game Steam Inventory works.
