# Test data

| File | What | Origin |
| --- | --- | --- |
| `appmanifest_228980.acf`, `libraryfolders.vdf`, `libraryfolders_legacy.vdf` | Samples in the exact layout Steam writes | Synthetic: every value that could identify a user is made up |
| `appinfo_5191630.vdf` | App info of Coin Maker Simulator Demo: `ufs` with one `savefiles` rule and a Linux override with `addpath` | Written by `reborn-cli appinfo` on 2026-11-20 (first test account). App info is public data; searched for account names, SteamIDs and account IDs: none. Checked by hand |
| `appinfo_1511340.vdf` | App info of Defender of Kuiper Belt Demo: recursive rule with `{64BitSteamID}`, `copied_from_parent`, `appidredirect`, overrides with `pathtransforms` | Same as above |
| `appinfo_2143030.vdf` | App info of Cave Crawlers: `ufs` with only `quota` and `maxnumfiles` | Same as above |
