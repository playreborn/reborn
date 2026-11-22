# Licenses and PICS

How Reborn builds the library (`reborn-cm`, `src/session.rs`, `src/pics.rs`,
`src/library.rs`).

### License list

- **What:** Steam sends the account's licenses (packages) unasked right
  after logon. Each license has a `package_id` and an `access_token`.
- **Message / method:** `CMsgClientLicenseList` (`k_EMsgClientLicenseList`).
  It can arrive before the client subscribes; Reborn also checks steam-vent's
  unprocessed messages.
- **Source:** node-steam-user `components/apps.js` (`ClientLicenseList`
  handler, `this.licenses`); message in `steam-vent-proto-steam`
  `steammessages_clientserver`.
- **Verified:** yes (2026-11-19, live with a test account: 2 licenses, 197 apps with names and types)

### Base package and expired licenses

- **What:** Package 0 is granted to every account (free demos, SDKs,
  dedicated servers, Dota 2, TF2, ...); its apps are not shown in the
  library. Expired licenses (`ELicenseFlags::Expired = 0x08`) are skipped.
- **Source:** node-steam-user `components/apps.js` (`getOwnedPackages`:
  "sub 0 is covered by NoCost", expired filter); SteamKit2
  `Resources/SteamLanguage/enums.steamd` (`ELicenseFlags`).
- **Verified:** yes for package 0 (2026-11-19, live: a new test account
  with one added free game lists exactly that game after filtering).
  Expired licenses not yet seen live.

### Ownership for the shim (ADR 0055)

- **What:** `Session::licensed_app_ids` answers "does the account own
  this app" for the shim. Unlike the library list it **keeps package 0**:
  an app granted through the base package is owned, it is only not
  listed. Expired licenses are dropped, as above. Only the package step
  below runs (no access tokens, app infos or playtime). `rebornd` loads it
  on the first shim-mode launch of a Valve session and keeps it for that
  session; a new session after a reconnect or sign-in loads it again.
- **Source:** node-steam-user `components/apps.js`: `ownsApp` looks the
  app up in `getOwnedApps`, the `appids` of every package from
  `getOwnedPackages`; that keeps package 0 unless the caller asks to
  exclude free licenses ("sub 0 is covered by NoCost") and always drops
  expired ones ("If expired, filter it out, regardless of the filter
  options"). SteamKit2 `ELicenseFlags`. Checked 2026-11-22.
- **Verified:** the requests are the library's (live 2026-11-19); the
  ownership use itself is not yet observed against the official client
  (ADR 0055 stage 1, shadow mode).

### Package info

- **What:** Which apps a package contains.
- **Message / method:** `CMsgClientPICSProductInfoRequest` with `packages`
  (`packageid` + license `access_token`); answer
  `CMsgClientPICSProductInfoResponse`, possibly split over several messages
  (`response_pending`, collected with steam-vent `job_multi`).
- **Buffer:** little-endian `u32` format version (`1`), then binary
  KeyValues; root key is the package ID, apps are under `appids`.
- **Source:** SteamKit2 `Steam/Handlers/SteamApps/Callbacks.cs`
  (`PICSProductInfo`, format check and `TryReadAsBinary`); node-steam-user
  `components/apps.js` (`packageinfo.appids`). Binary KeyValues: SteamKit2
  `Types/KeyValue.cs` (`TryReadAsBinaryCore`).
- **Verified:** yes (2026-11-19, live with a test account: 2 licenses, 197 apps with names and types)

### App access tokens

- **What:** Tokens needed to see the full info of some apps.
- **Message / method:** `CMsgClientPICSAccessTokenRequest` (`appids`), answer
  `CMsgClientPICSAccessTokenResponse` (`app_access_tokens`).
- **Source:** node-steam-user `components/apps.js` (`getProductAccessToken`).
- **Verified:** yes (2026-11-19, live with a test account: 2 licenses, 197 apps with names and types)

### App info

- **What:** Name and type of apps.
- **Message / method:** `CMsgClientPICSProductInfoRequest` with `apps`
  (`appid` + access token).
- **Buffer:** text KeyValues, root `appinfo`, fields `common/name` and
  `common/type`; may end with NUL bytes. Large infos can be offered over HTTP
  instead (`http_host`, `http_min_size`); Reborn does not fetch those yet and
  shows such apps without a name.
- **Source:** node-steam-user `components/apps.js` (`VDF.parse(...).appinfo`);
  steam-vent `examples/product_info.rs` (trims `\0`); SteamKit2
  `Callbacks.cs` (HTTP fallback via `http_host`).
- **Verified:** yes (2026-11-19, live with a test account: 2 licenses, 197 apps with names and types)

### Playtime

- **What:** Playtime per app, merged into the library.
- **Message / method:** `Player.GetOwnedGames#1`. Not used as the source of
  the library: live test (2026-11-19) showed a free game that was added but
  never played was missing from its answer.
- **Source:** steam-vent `examples/access_token.rs`.
- **Verified:** yes for the request itself (live, 2026-11-19); playtime values not yet.
