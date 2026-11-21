# Architecture Decision Records

Important technical decisions are recorded here, one file per decision,
using [`0000-template.md`](0000-template.md). An ADR is never rewritten after
it is accepted; a new ADR supersedes it instead.

| No. | Title | Status |
| --- | --- | --- |
| [0001](0001-interop-and-red-lines.md) | Interoperability through reverse engineering, and red lines | Accepted |
| [0002](0002-daemon-frontend-split.md) | Daemon / frontend split | Accepted |
| [0003](0003-license-split.md) | License split GPL / MIT-Apache | Accepted |
| [0004](0004-project-name.md) | Project name | Accepted |
| 0005 | Steam file formats as source of truth for installs | Planned |
| 0006 | CM transport: WebSocket first | Planned |
| [0007](0007-steam-vent-as-cm-base.md) | steam-vent as the base of `reborn-cm` | Accepted |
| [0008](0008-ipc-format.md) | IPC between rebornd and its frontends | Accepted (Windows pipe access and secrets: see 0014) |
| [0009](0009-iced-gui.md) | Iced as GUI framework | Accepted |
| [0010](0010-credential-storage.md) | Credential storage | Accepted |
| 0011 | Origin of protobuf definitions | Planned (M0) |
| [0012](0012-shim-approach.md) | Shim approach: clean room, observation, daemon first | Accepted |
| [0013](0013-own-content-engine.md) | Own content engine, steamroom and SteamKit2 as references | Accepted |
| [0014](0014-sign-in-through-the-daemon.md) | Sign-in through the daemon | Accepted |
| [0015](0015-compatibility-database.md) | Compatibility database | Proposed |
| [0016](0016-launching-games.md) | Launching games | Accepted (Proton: see 0017) |
| [0017](0017-windows-games-through-proton.md) | Windows games on Linux through Proton | Accepted |
| [0018](0018-uninstalling-apps.md) | Uninstalling apps | Accepted |
| [0019](0019-settings.md) | Settings for the library folder and the Proton build | Accepted |
| [0020](0020-friends-and-presence.md) | Friends and presence, read-only first | Accepted |
| [0021](0021-own-online-status.md) | The user's own online status | Accepted |
| [0022](0022-friend-requests.md) | Friend requests: send, accept, decline | Accepted (decline: see 0023) |
| [0023](0023-decline-disabled.md) | Declining friend requests disabled until verified | Accepted |
| [0024](0024-friend-chat.md) | One-to-one chat with friends | Accepted (typing, read markers: see 0026) |
| [0025](0025-reconnecting.md) | Reconnecting after a lost connection | Accepted (sign-in limit: see 0028) |
| [0026](0026-typing-and-read-markers.md) | Typing notices, read markers and unread counts | Accepted |
| [0027](0027-app-tickets.md) | App tickets from Valve | Accepted |
| [0028](0028-trying-several-servers.md) | Trying several Steam servers when signing in | Accepted (context corrected by 0036) |
| [0029](0029-reading-achievements.md) | Reading achievements | Accepted |
| [0030](0030-achievements-in-the-gui.md) | Achievements in the daemon and the GUI | Accepted |
| [0031](0031-steam-cloud-step-by-step.md) | Steam Cloud, step by step | Accepted |
| [0032](0032-downloading-cloud-files.md) | Downloading Steam Cloud files | Accepted |
| [0033](0033-mapping-cloud-files.md) | Mapping Steam Cloud files to local paths | Accepted |
| [0034](0034-restoring-missing-saves.md) | Restoring missing saves from Steam Cloud | Accepted |
| [0035](0035-restoring-saves-on-launch.md) | Restoring missing saves when a game starts | Accepted |
| [0036](0036-servers-are-already-shuffled.md) | Steam servers are already chosen at random | Accepted |
| [0037](0037-preferring-port-443.md) | Preferring Steam servers on port 443 | Accepted |
| [0038](0038-shim-observation-tool.md) | The shim observation tool | Accepted |
| [0039](0039-loading-the-observation-proxy.md) | Loading the observation proxy | Accepted |
| [0040](0040-observing-returned-objects.md) | Observing calls on returned objects (stage 3) | Accepted |
| [0041](0041-stand-ins-for-nested-objects.md) | Stand-ins for objects returned through stand-ins (step 3c) | Accepted |
| [0042](0042-meanings-and-callbacks.md) | How recorded calls get a meaning, and observing callbacks (stage 4) | Accepted |
| [0043](0043-reading-results-for-experiments.md) | Reading result texts and output bytes for planned experiments (step 4c) | Accepted |
| [0044](0044-agent-run-observation-runs.md) | Observation runs carried out by an AI agent | Accepted |
| [0045](0045-higher-limits-for-agent-runs.md) | Higher limits for agent-run observation runs | Accepted |
| [0046](0046-shadow-and-answer-mode.md) | Shadow and answer mode, the first step from proxy to shim (stage 5) | Accepted |
| [0047](0047-shim-daemon-connection.md) | How the shim talks to rebornd | Accepted |
| [0048](0048-loading-the-shim.md) | How games find the shim when Reborn starts them | Accepted |
