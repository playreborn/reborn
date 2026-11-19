# Reborn

**Reborn: a native, open-source client for your Steam library.**

Written from scratch in Rust. Fast, small, no embedded browser, no bloat.
Your account, your licenses, your games, on Valve's servers, with a client
you can read, build and change yourself.

> [!WARNING]
> **Pre-alpha.** Nothing works yet. This repository currently contains the
> project skeleton and the plan. Do not use it with an account you care about.

> Reborn is not affiliated with, endorsed by or sponsored by Valve Corporation.
> Steam is a trademark of Valve Corporation.

## What Reborn will do

- Sign in with your Steam account (QR code or password + Steam Guard).
- Show your library, install, update, verify and uninstall games with its own
  download engine, compatible with existing Steam library folders.
- Launch games, including Windows games on Linux through Proton.
- Friends, presence and chat.
- Later: run Steamworks games **without the official Steam client** by
  providing its own `steamclient` replacement.

## What Reborn will never do

Bypass DRM or Steam Guard, fabricate game ownership, unlock achievements,
farm cards, run trading bots, or ship Valve code or assets. See
[AGENTS.md § 2](AGENTS.md#2-red-lines-never-cross-these).

## Roadmap

| Milestone | Goal |
| --- | --- |
| M0 | CLI: login and list owned games |
| M1 | CLI: download and verify a game with our own content engine |
| M2 | Daemon + Iced GUI → **0.1** |
| M3 | Friends and chat → 0.2 |
| M4 | `steamclient` replacement for singleplayer games → 0.3 |
| M5 | Multiplayer (lobbies, Steam networking) |
| M6 | Workshop, store browsing, polish → **1.0** |

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
