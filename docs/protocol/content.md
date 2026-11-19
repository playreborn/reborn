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
  `LauncherPath`, updates of existing installs (stage 4).
- **Verified:** yes (2026-11-19, live: Spacewar installed by Reborn into the
  official client's library while it was closed; the client then listed it as
  installed without downloading it again and "verify integrity of game files"
  reported all files as valid). Offline: 7 install tests.
