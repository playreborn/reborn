# 0013: Own content engine, steamroom and SteamKit2 as references

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

M1 needs depot downloads: manifests, chunk decryption and decompression,
verification and installation. Rust crates for this exist:

- **steamroom** 0.3 (MIT/Apache, since April 2026): manifests, chunks, CDN,
  but also a complete second CM client with its own protobufs, duplicating
  steam-vent. Its manifest section magics differ from SteamKit2's and it
  silently skips unknown sections.
- **steamdepot** (LGPL-2.1-only): incompatible with our MIT/Apache crates.

## Decision

`reborn-content` implements the content format itself:

- protobufs from `steam-vent-proto-steam` (same source as `reborn-cm`),
- RustCrypto `aes`/`cbc`/`ecb`, `lzma-rs`, `zstd`, `zip`,
- behaviour taken from **SteamKit2** (read only, LGPL) where sources
  disagree, and from **steamroom** (may be adapted with attribution) where
  it fills gaps.

CM requests for content (depot keys, manifest request codes, CDN servers)
go into `reborn-cm` on top of steam-vent; `reborn-content` stays free of
networking to Steam's CM.

Only the three chunk containers SteamKit2 accepts (VSZa, VZa, PK) are
supported; everything else is an error. Every chunk is verified by size and
zero-seeded Adler-32 before it is written.

## Stages of M1

1. **Offline core** (done): depot key type, AES, chunk containers,
   checksum, manifest parser with encrypted names, safe install paths.
2. **Steam side** (done): depot list from PICS app info, depot decryption key,
   manifest request code, CDN server list, manifest and chunk download.
3. **Install** (done, accepted by the official client): parallel chunk download into a staging directory, file
   assembly, SHA-1 check per file, commit, `appmanifest_*.acf`;
   `reborn-cli install <appid>`. First live target: Spacewar (app 480,
   small and in every account's base package).
4. **Update and verify**: manifest diff, reuse of unchanged chunks,
   resuming, verifying an existing install.

## Consequences

- More code of our own, but no second CM stack and no hidden divergences.
- `zstd` builds the C zstd library (needs a C compiler; MSVC on Windows,
  cc on Linux CI).
