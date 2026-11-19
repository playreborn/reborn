# reborn-formats

Parser and writer for Steam's text KeyValues files (VDF/ACF), part of
[Reborn](https://github.com/playreborn/reborn), a native, open-source client
for your Steam library.

- `vdf`: parse and write text KeyValues. Order and duplicate keys are
  preserved, lookups are case-insensitive, errors report line and column.
  The writer reproduces Steam's own files byte for byte
  (`appmanifest_*.acf`, `libraryfolders.vdf`, `loginusers.vdf`,
  `config.vdf`, `localconfig.vdf`).
- `AppManifest`: typed reader for `steamapps/appmanifest_<appid>.acf`.
- `LibraryFolders`: typed reader for `steamapps/libraryfolders.vdf`
  (current and legacy layout).

```rust
use reborn_formats::{AppManifest, LibraryFolders, vdf};

let text = std::fs::read_to_string("appmanifest_228980.acf")?;
let manifest = AppManifest::parse(&text)?;
println!("{} installed: {}", manifest.app_id, manifest.is_fully_installed());

// Generic, lossless editing:
let mut document = vdf::parse(&text)?;
// ... change something ...
let written = vdf::write(&document);
```

Planned: binary KeyValues (`appinfo.vdf`) and depot manifests.

Not affiliated with Valve Corporation. Steam is a trademark of Valve Corporation.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
