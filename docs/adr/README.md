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
| 0012 | Shim approach (vtables, versions, IPC) | Planned (M4) |
| [0013](0013-own-content-engine.md) | Own content engine, steamroom and SteamKit2 as references | Accepted |
| [0014](0014-sign-in-through-the-daemon.md) | Sign-in through the daemon | Accepted |
| [0015](0015-compatibility-database.md) | Compatibility database | Proposed |
| [0016](0016-launching-games.md) | Launching games | Accepted (Proton: see 0017) |
| [0017](0017-windows-games-through-proton.md) | Windows games on Linux through Proton | Accepted |
| [0018](0018-uninstalling-apps.md) | Uninstalling apps | Accepted |
| [0019](0019-settings.md) | Settings for the library folder and the Proton build | Accepted |
