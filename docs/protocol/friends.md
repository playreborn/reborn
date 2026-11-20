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
- **Pitfall:** steam-vent hands the next message of a kind to a pending
  one-time wait (`one`) before any subscription (`on`), see its
  `src/connection/filter.rs`. An unused `one` for the friends list therefore
  swallowed the first incremental change after logon (live, 2026-11-20).
  Reborn waits for the full list with a subscription instead.
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

### Own online status

- **What:** The client sets the account's status (online, away, ...).
- **Message:** `CMsgClientChangeStatus` (EMsg 716) with `persona_state`
  (field 1, an `EPersonaState` value). Reborn sends nothing else;
  `player_name` would rename the profile (ADR 0021).
- **Source:** node-steam-user `components/friends.js` (`setPersona` sends
  `ClientChangeStatus` with `persona_state` and the optional `player_name`,
  no response expected); SteamKit2 `enums.steamd` (`EPersonaState`).
- **Verified:** yes (2026-11-20, test account): a friend saw Online, Away
  and Offline in the friends list as chosen in Reborn. While the official
  client ran with the same account, its status won; with it closed,
  Reborn's status applied. The profile page shows only online/offline/in
  game; the detailed status appears in the friends list.

### Friend requests

- **What:** Send a friend request or accept one (same message), decline
  one. Afterwards Steam reports the change with an incremental friends list
  (`bincremental` set): the new relationship, or `None` if it ended.
- **Messages:** `CMsgClientAddFriend` (EMsg 791) with `steamid_to_add`,
  sent as a job; answer `CMsgClientAddFriendResponse` with `eresult`,
  `steam_id_added`, `persona_name_added`. `CMsgClientRemoveFriend`
  (EMsg 714) with `friendid`, no answer.
- **Input:** SteamID64; profile URL `…/profiles/<SteamID64>`; friend code
  read as the account ID of an individual public account: SteamID64 =
  (1 << 56) | (1 << 52) | (1 << 32) | code = 76561197960265728 + code
  (SteamKit2 `Types/SteamID.cs`: universe, account type, instance, account
  ID).
- **Source:** node-steam-user `components/friends.js` (`addFriend` sends
  `ClientAddFriend` with `steamid_to_add` and reads `eresult` and
  `persona_name_added` from the answer, JSDoc "Send (or accept) a friend
  invitation"; `removeFriend` sends `ClientRemoveFriend` with `friendid`,
  JSDoc "... (or decline an invitation)"; the `ClientFriendsList` handler
  treats `bincremental` lists as changes and `None` as removal).
- **Refusals:** a new test account without purchases got `eresult` 24
  (`InsufficientPrivilege`, steam-vent `src/eresult.rs`) when sending a
  request (fixture `add_friend_refused.bin`). Steam is known to limit
  accounts that have not spent money; accepting requests stays possible.
  The official FAQ ("Limited User Accounts") could not be read
  automatically, so the exact rule is not quoted here.
- **Verified:** the refusal live (2026-11-20); friend code conversion live
  (2026-11-20: Steam's own "Add a Friend" page resolved the test account's
  friend code to the profile whose SteamID64 is 76561197960265728 + code).
  Declining live (2026-11-20: the request was gone afterwards, but see
  below). Accepting
  live (2026-11-20, second test account: after Accept, Steam sent an
  incremental friends list within a second and the GUI showed the friend
  without reloading). The answer to an accepted request and the incremental
  list use the same messages as the recorded fixtures; no separate fixture.

### Declining: open questions (ADR 0023)

- After `CMsgClientRemoveFriend` on a pending request, Steam sent an
  incremental list with a relationship of Blocked, Ignored or
  IgnoredFriend for the sender (Reborn then showed all three as "Blocked";
  the exact value was not logged). The sender could not send a new request
  afterwards. The test account's block list on the Steam website did not
  contain the sender, and after a new login the full list had no entry for
  them.
- Not verified: the exact value, how long the sender is held back, and
  whether the official client's "Ignore" does the same. Declining is
  disabled until this is compared.
