# Test data for reborn-cm

| File | What | Recorded |
| --- | --- | --- |
| `friends_list.bin`, `persona_state_0.bin` | `CMsgClientFriendsList` and the `CMsgClientPersonaState` answer to our friend data request, protobuf-encoded, written by `reborn-cli friends --record` | 2026-11-20, test account with one (offline) friend, Reborn commit after `dfe018e`; redacted by the recorder: SteamID replaced by a dev-universe placeholder, name by `Friend 1`, last logoff by a fixed time, all other fields dropped. Checked by hand: no real ID or name |
| `add_friend_refused.bin` | `CMsgClientAddFriendResponse` to a friend request sent from the test account, written by `reborn-cli friends add --record` | 2026-11-20, test account (new, no purchases); Steam answered EResult 24 (insufficient privilege). Redacted: `steam_id_added` is a dev-universe placeholder, no name. Checked by hand |
| `chat_history.bin`, `chat_incoming_typing.bin`, `chat_incoming_message.bin` | `CFriendMessages_GetRecentMessages_Response` with three messages, and two `CFriendMessages_IncomingMessage_Notification` (a typing notice, then a message), written by `reborn-cli chat --record` | 2026-11-20, second test account chatting with the maintainer's main account. Redacted: texts replaced by `Message n` (the typing notice's empty text became `Message 3`; the recorder now keeps empty texts empty), the user as account 1, the friend as account 2 / dev-universe SteamID; message times kept. Checked by hand: no real text or ID |
| `appinfo_3419430.vdf` | App info of app 3419430 (Bongo Cat, free), text VDF as written by `reborn-cli appinfo` | 2026-11-20, test account, Reborn commit after `1af3fb1`; unchanged (public store data, no account data) |

App info is public: it describes the app, not the account that asked for
it. Still check every new file by hand before committing it (see
`fixtures/README.md`).
