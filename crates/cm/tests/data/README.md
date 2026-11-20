# Test data for reborn-cm

| File | What | Recorded |
| --- | --- | --- |
| `friends_list.bin`, `persona_state_0.bin` | `CMsgClientFriendsList` and the `CMsgClientPersonaState` answer to our friend data request, protobuf-encoded, written by `reborn-cli friends --record` | 2026-11-20, test account with one (offline) friend, Reborn commit after `dfe018e`; redacted by the recorder: SteamID replaced by a dev-universe placeholder, name by `Friend 1`, last logoff by a fixed time, all other fields dropped. Checked by hand: no real ID or name |
| `add_friend_refused.bin` | `CMsgClientAddFriendResponse` to a friend request sent from the test account, written by `reborn-cli friends add --record` | 2026-11-20, test account (new, no purchases); Steam answered EResult 24 (insufficient privilege). Redacted: `steam_id_added` is a dev-universe placeholder, no name. Checked by hand |
| `appinfo_3419430.vdf` | App info of app 3419430 (Bongo Cat, free), text VDF as written by `reborn-cli appinfo` | 2026-11-20, test account, Reborn commit after `1af3fb1`; unchanged (public store data, no account data) |

App info is public: it describes the app, not the account that asked for
it. Still check every new file by hand before committing it (see
`fixtures/README.md`).
