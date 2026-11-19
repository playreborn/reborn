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
| 0008 | IPC format frontend ↔ daemon | Planned (M2) |
| 0009 | Iced as GUI framework | Planned (M2) |
| [0010](0010-credential-storage.md) | Credential storage | Accepted |
| 0011 | Origin of protobuf definitions | Planned (M0) |
| 0012 | Shim approach (vtables, versions, IPC) | Planned (M4) |
| [0013](0013-own-content-engine.md) | Own content engine, steamroom and SteamKit2 as references | Accepted |
