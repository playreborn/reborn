# Friends and presence

Read-only part of M3 (ADR 0020). Each section names its sources.

### Friends list

- **What:** Steam sends the account's relationships after logon: one
  entry per SteamID with its relationship. A full list has `bincremental`
  unset; later changes come as incremental lists. An entry with
  relationship `None` means the relationship ended.
- **Message:** `CMsgClientFriendsList` (EMsg 767), fields `bincremental`,
  `friends[].ulfriendid`, `friends[].efriendrelationship`.
- **Values:** `EFriendRelationship`: None 0, Blocked 1, RequestRecipient 2,
  Friend 3, RequestInitiator 4, Ignored 5, IgnoredFriend 6.
- **SteamIDs:** bits 52-55 of the 64-bit ID are the account type; only
  individual accounts (type 1) are friends, clans (type 7) are skipped.
- **Source:** node-steam-user `components/friends.js` (`ClientFriendsList`
  handler: `bincremental`, `ulfriendid`, `efriendrelationship`, clans told
  apart by SteamID type); SteamKit2 `enums.steamd` (`EFriendRelationship`)
  and `Types/SteamID.cs` (bit layout).
- **Arrival:** it comes in the same multi-message as the logon response, so
  it is usually among the messages steam-vent kept unprocessed (live log,
  2026-11-20).
- **Verified:** yes (2026-11-20, test account with one friend; fixture
  `crates/cm/tests/data/friends_list.bin`).

### Persona data

- **What:** The client asks for names and status of friends; Steam answers
  with persona states and keeps sending updates while the session lasts.
- **Messages:** request `CMsgClientRequestFriendData` (EMsg 815) with
  `persona_state_requested` and `friends` (SteamID64s); answers and
  updates `CMsgClientPersonaState` (EMsg 766) with `friends[]`: `friendid`,
  `persona_state`, `player_name`, `game_played_app_id`, `gameid`,
  `game_name`, `last_logoff`.
- **Flags:** `EClientPersonaStateFlag` Status 1 | PlayerName 2 | Presence 16
  | LastSeen 64 | GameExtraInfo 256 = 339.
- **Values:** `EPersonaState`: Offline 0, Online 1, Busy 2, Away 3,
  Snooze 4, LookingToTrade 5, LookingToPlay 6, Invisible 7.
- **Offline friends:** Steam leaves `persona_state` out for an offline
  friend even when the status was requested; a missing optional protobuf
  field means its default, 0 = Offline. Reborn reads it that way only if
  `status_flags` has the Status bit (fixture `persona_state_0.bin`).
- **Source:** node-steam-user `components/friends.js` (`getPersonas` sends
  `ClientRequestFriendData` with `friends` and `persona_state_requested`
  built from `EClientPersonaStateFlag`; the `ClientFriendsList` handler
  requests personas for all friends after a full list; the
  `ClientPersonaState` handler reads `friendid` and merges missing fields
  with known values); SteamKit2 `enums.steamd` (`EClientPersonaStateFlag`,
  `EPersonaState`).
- **Verified:** yes (2026-11-20, test account; name and offline status of
  one friend; fixture `crates/cm/tests/data/persona_state_0.bin`). Online
  and in-game states not seen live yet.
