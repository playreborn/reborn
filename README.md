# Reborn

**Reborn: a native, open-source client for your Steam library.**

Written from scratch in Rust. Fast, small, no embedded browser, no bloat.
Your account, your licenses, your games, on Valve's servers, with a client
you can read, build and change yourself.

> [!WARNING]
> **Alpha.** Installing, updating and playing work on Windows and Linux, but
> expect rough edges. If you can, try it with a secondary Steam account first.

> Reborn is not affiliated with, endorsed by or sponsored by Valve Corporation.
> Steam is a trademark of Valve Corporation.

## What works (0.1)

- Sign in with password and Steam Guard (code or approval in the mobile app);
  the login is kept in your system's keyring.
- Your library, with install, update, verify and resume, using Reborn's own
  download engine and the Steam library format.
- Play: native games start directly; on Linux, Windows-only games run
  through an installed Proton build.
- A desktop app, a background service and a command line tool.

Most games still need the **official Steam client running in the
background**, because they talk to it through Steamworks. See the
[changelog](CHANGELOG.md) for all known limitations.

## What comes next

- QR-code sign-in, settings, game details.
- Friends, presence and chat.
- Run Steamworks games **without the official Steam client** through Reborn's
  own `steamclient` replacement.

## Install

Download the archive for your system from
[Releases](https://github.com/playreborn/reborn/releases), unpack it and
start `reborn` (`reborn.exe` on Windows). Keep the three programs in the same
folder: the app starts the background service `rebornd` from there.

On Linux you need a desktop with a keyring that offers the Secret Service
API (for example GNOME Keyring or KWallet), and Vulkan or OpenGL drivers.

## What Reborn will never do

Bypass DRM or Steam Guard, fabricate game ownership, unlock achievements,
farm cards, run trading bots, or ship Valve code or assets. See
[AGENTS.md § 2](AGENTS.md#2-red-lines-never-cross-these).

## Roadmap

| Milestone | Goal | Status |
| --- | --- | --- |
| M0 | CLI: login and list owned games | done |
| M1 | CLI: download and verify a game with our own content engine | done |
| M2 | Daemon + Iced GUI → **0.1** | done |
| M3 | Friends and chat → 0.2 | |
| M4 | `steamclient` replacement for singleplayer games → 0.3 | |
| M5 | Multiplayer (lobbies, Steam networking) | |
| M6 | Workshop, store browsing, polish → **1.0** | |

The full plan (German) is in [`docs/plan.md`](docs/plan.md).

## Building

Requires a current stable Rust toolchain ([rustup](https://rustup.rs)).
On Windows you also need the Visual Studio C++ Build Tools.

```sh
cargo build --workspace
cargo run -p reborn-cli -- --help
cargo run -p rebornd      # background daemon
cargo run -p reborn       # desktop app (starts rebornd if needed)
```

The desktop app starts the `rebornd` binary next to it, and `cargo run -p
reborn` does not rebuild that binary. After changing daemon or engine code,
run `cargo build --workspace` (and stop a running daemon with
`cargo run -p reborn-cli -- daemon shutdown`) so the app does not start an
old daemon.

## Contributing

Contributions are very welcome, including compatibility reports and
translations that need no programming at all. AI-assisted contributions are
welcome too, as long as you have run and reviewed every line. Start with
[CONTRIBUTING.md](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md).

## License

- Applications and core crates (`rebornd`, `reborn-cli`, `reborn-core`,
  `reborn-platform`, `reborn-ipc`): [GPL-3.0-or-later](LICENSE-GPL).
- Protocol and format crates (`reborn-proto`, `reborn-formats`, `reborn-cm`,
  `reborn-content`): [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE), at your option.

Each crate's `Cargo.toml` states its license.
