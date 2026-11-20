# Protocol knowledge

What we know about the Steam protocol, **with sources**. Nothing goes into
this folder without a source. If something is unknown, write "unknown"
instead of guessing.

## Entry format

One file per topic (e.g. `auth.md`, `pics.md`, `content.md`). Each behaviour
is documented like this:

```markdown
### Begin QR login

- **What:** ...
- **Message / method:** ...
- **Source:** <project> `<path/to/file>` (commit or tag), lines X–Y
- **Fixture:** `fixtures/cm/auth/qr-begin.json`
- **Verified:** yes / no (date, Reborn commit)
```

## Allowed sources

- Reference implementations: SteamKit2, node-steam-user, ValvePython/steam,
  DepotDownloader, steam-vent. Read them, cite them, but only copy code from
  MIT/Apache/BSD sources (see ADR 0003).
- Protobuf definitions: SteamDatabase/Protobufs (see ADR 0011).
- Our own recorded traffic (see `fixtures/README.md`).

## Planned topics

| File | Milestone |
| --- | --- |
| `connection.md` (CM discovery, WebSocket transport, framing, heartbeat) | M0 |
| [`auth.md`](auth.md) (IAuthenticationService, tokens, logon) | M0, written |
| [`licenses-pics.md`](licenses-pics.md) (license list, PICS, access tokens) | M0, written |
| [`content.md`](content.md) (depot keys, manifest request codes, CDN, chunks) | M1, written |
| `social.md` (friends, persona state, chat) | M3 |
| [`tickets.md`](tickets.md) (ownership and encrypted app tickets) | M4, written |
| `cloud.md` | M4 |
| `steamclient-interfaces.md` (shim) | M4 |
