# 0032: Downloading Steam Cloud files

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0031 step 2: download an app's cloud files into a folder of the
user's choice, never into a game's save folder. `Cloud.EnumerateUserFiles`
already returns a download `url` per file; Valve's `ICloudService`
documentation and ValvePython/steamctl (MIT) download with a plain GET
on it. Whether the body can be compressed is unknown
(`docs/protocol/cloud.md`). File names and URLs come from the network:
names are untrusted, and a URL gives access to the user's file.

## Problem

How Reborn downloads cloud files without writing outside the chosen
folder, without overwriting anything, and without leaking the links.

## Alternatives considered

1. **`Cloud.ClientFileDownload` per file:** what the official client may
   use, but no allowed reference shows how; not recorded yet.
2. **GET on the `url` from the file list:** documented by Valve and used
   by steamctl.

## Decision

Option 2.

- `reborn-cm`: `CloudFile` gets `download_url: Option<CloudUrl>`.
  `CloudUrl` prints `<redacted>` in `Debug`; the raw value only comes out
  through `expose()`. Links are never logged and never stored.
- `reborn-content`: new module `cloud` with `cloud_path(name)` and
  `download_cloud_file(client, folder, file)`:
  - The name goes through `install_path` (no `..`, no absolute paths, no
    unsafe characters or device names). A leading root token such as
    `%WinAppDataLocal%` becomes a folder of that name.
  - No path component below the folder may be a symlink.
  - The body may not be larger than the size Steam listed; the download
    stops otherwise.
  - The body must match Steam's SHA-1, or nothing is written.
  - It is written to `<name>.reborn-part` first and renamed afterwards.
  - An existing file with the same SHA-1 is left alone; one with other
    content is never overwritten (AGENTS.md: the user decides).
  - Errors from the HTTP client are stripped of the URL.
- `reborn-cli cloud <app_id> --download <folder>` downloads all files of
  the app one after the other and reports each one.
- Nothing is uploaded or deleted; no daemon or GUI part yet.

## Consequences

- Writes files: needs human review (AGENTS.md section 7).
- One GET per file, in sequence (red line 8).
- A compressed body would fail the SHA-1 check and be reported, which
  answers the open question without risk.
