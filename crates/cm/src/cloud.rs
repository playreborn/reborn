//! Steam Cloud, read only (ADR 0031): which files an app has in the
//! account's cloud. Nothing is downloaded, uploaded or deleted here yet.
//!
//! `Cloud.EnumerateUserApps` is not used: Steam answered Fail for it,
//! also for an account with cloud files (live 2026-11-20).
//!
//! Methods and fields follow ValvePython/steamctl
//! `steamctl/commands/cloud/gcmds.py` (MIT) and Valve's public
//! `ICloudService` documentation. Details: `docs/protocol/cloud.md`.

use steam_vent::ConnectionTrait;
use steam_vent_proto_common::protobuf::Message;
use steam_vent_proto_steam::steammessages_cloud_steamclient::{
    CCloud_EnumerateUserFiles_Request, CCloud_EnumerateUserFiles_Response, CCloud_UserFile,
};

use crate::{Error, Session};

/// Files per page; Valve's documented default for `count`.
const PAGE_SIZE: u32 = 500;

/// Upper bound on pages for one app, so a server that keeps answering
/// with too few files cannot keep Reborn asking (red line 8).
const MAX_PAGES: u32 = 20;

/// A file in an app's Steam Cloud.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudFile {
    /// Name as stored by Steam. Auto-Cloud files start with a root token
    /// such as `%WinAppDataLocal%`. Untrusted: never use it as a path
    /// without checking.
    pub name: String,
    /// Size in bytes.
    pub size: u32,
    /// Last change, seconds since the Unix epoch.
    pub timestamp: u64,
    /// SHA-1 of the content, 40 hex digits.
    pub sha1: String,
    /// Platforms the file syncs to (e.g. `all`).
    pub platforms: Vec<String>,
}

impl Session {
    /// The files of an app in the account's Steam Cloud.
    pub async fn cloud_files(&self, app_id: u32) -> Result<Vec<CloudFile>, Error> {
        let response = self.cloud_files_response(app_id).await?;
        Ok(cloud_files_from(&response))
    }

    /// Steam's answer for an app, all pages in one, protobuf-encoded and
    /// redacted for fixtures: download URLs, UGC IDs and the creator's
    /// SteamID removed; the own SteamID and account ID in file names
    /// replaced by `{64BitSteamID}` and `{Steam3AccountID}`.
    pub async fn record_cloud_files(&self, app_id: u32) -> Result<Vec<u8>, Error> {
        let mut files = self.cloud_files_response(app_id).await?;
        redact_files(&mut files, self.steam_id());
        files
            .write_to_bytes()
            .map_err(|_| Error::IncompleteResponse("cloud files"))
    }

    /// All pages of `Cloud.EnumerateUserFiles`, merged into one answer.
    async fn cloud_files_response(
        &self,
        app_id: u32,
    ) -> Result<CCloud_EnumerateUserFiles_Response, Error> {
        let mut all = CCloud_EnumerateUserFiles_Response::default();
        for _ in 0..MAX_PAGES {
            let start = u32::try_from(all.files.len()).unwrap_or(u32::MAX);
            let request = CCloud_EnumerateUserFiles_Request {
                appid: Some(app_id),
                extended_details: Some(true),
                count: Some(PAGE_SIZE),
                start_index: Some(start),
                ..Default::default()
            };
            let page: CCloud_EnumerateUserFiles_Response =
                self.connection.service_method(request).await?;
            let total = page.total_files.unwrap_or(0);
            let empty = page.files.is_empty();
            all.total_files = Some(total);
            all.files.extend(page.files);
            if empty || all.files.len() >= total as usize {
                break;
            }
        }
        Ok(all)
    }
}

/// Files from a `Cloud.EnumerateUserFiles` answer, sorted by name. Entries
/// without a name are skipped.
pub fn cloud_files_from(response: &CCloud_EnumerateUserFiles_Response) -> Vec<CloudFile> {
    let mut files: Vec<CloudFile> = response
        .files
        .iter()
        .filter_map(|file: &CCloud_UserFile| {
            Some(CloudFile {
                name: file.filename.clone().filter(|name| !name.is_empty())?,
                size: file.file_size.unwrap_or(0),
                timestamp: file.timestamp.unwrap_or(0),
                sha1: file.file_sha.clone().unwrap_or_default(),
                platforms: file.platforms_to_sync.clone(),
            })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// Removes what points at the account or its files (see
/// [`Session::record_cloud_files`]).
fn redact_files(response: &mut CCloud_EnumerateUserFiles_Response, steam_id: u64) {
    let account_id = (steam_id & 0xFFFF_FFFF).to_string();
    let steam_id = steam_id.to_string();
    for file in &mut response.files {
        file.url = None;
        file.ugcid = None;
        file.steamid_creator = None;
        if let Some(name) = &mut file.filename {
            *name = name
                .replace(&steam_id, "{64BitSteamID}")
                .replace(&account_id, "{Steam3AccountID}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEAM_ID: u64 = 76_561_197_960_389_184;

    fn file(name: &str) -> CCloud_UserFile {
        CCloud_UserFile {
            appid: Some(480),
            ugcid: Some(123_456_789),
            filename: Some(name.to_owned()),
            timestamp: Some(1_795_133_048),
            file_size: Some(42),
            url: Some("https://example.invalid/ugc/123/abc/".to_owned()),
            steamid_creator: Some(STEAM_ID),
            platforms_to_sync: vec!["all".to_owned()],
            file_sha: Some("da39a3ee5e6b4b0d3255bfef95601890afd80709".to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn reads_files_sorted_by_name() {
        let response = CCloud_EnumerateUserFiles_Response {
            files: vec![file("b.sav"), file(""), file("%WinAppDataLocal%Game/a.sav")],
            total_files: Some(3),
            ..Default::default()
        };
        let files = cloud_files_from(&response);
        assert_eq!(files.len(), 2, "nameless entry skipped");
        assert_eq!(files[0].name, "%WinAppDataLocal%Game/a.sav");
        assert_eq!(
            files[1],
            CloudFile {
                name: "b.sav".to_owned(),
                size: 42,
                timestamp: 1_795_133_048,
                sha1: "da39a3ee5e6b4b0d3255bfef95601890afd80709".to_owned(),
                platforms: vec!["all".to_owned()],
            }
        );
    }

    /// Recorded with the first test account (see `tests/data/README.md`).
    const RECORDED: &[u8] = include_bytes!("../tests/data/cloud_files.bin");

    #[test]
    fn replays_recorded_cloud_files() {
        let response = CCloud_EnumerateUserFiles_Response::parse_from_bytes(RECORDED).unwrap();
        assert_eq!(response.total_files, Some(2));
        let files = cloud_files_from(&response);
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["BackUp_MainPlayerData.json", "MainPlayerData.json"]);
        for file in &files {
            assert_eq!(file.size, 44_873);
            assert_eq!(file.timestamp, 1_795_209_106);
            assert_eq!(file.sha1, "8fd5b59589120ebdf6928e9b778a40662b1660e8");
            assert_eq!(file.platforms, ["All"]);
        }
        for file in &response.files {
            assert_eq!(file.url, None, "redacted");
            assert_eq!(file.ugcid, None, "redacted");
            assert_eq!(file.steamid_creator, None, "redacted");
        }
    }

    #[test]
    fn redaction_removes_links_and_ids() {
        let account_id = STEAM_ID & 0xFFFF_FFFF;
        let mut response = CCloud_EnumerateUserFiles_Response {
            files: vec![
                file(&format!("saves/{STEAM_ID}/slot1.sav")),
                file(&format!("%WinAppDataLocal%Game/{account_id}/config.ini")),
            ],
            total_files: Some(2),
            ..Default::default()
        };
        redact_files(&mut response, STEAM_ID);
        let names: Vec<&str> = response
            .files
            .iter()
            .map(|f| f.filename.as_deref().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "saves/{64BitSteamID}/slot1.sav",
                "%WinAppDataLocal%Game/{Steam3AccountID}/config.ini"
            ]
        );
        for file in &response.files {
            assert_eq!(file.url, None);
            assert_eq!(file.ugcid, None);
            assert_eq!(file.steamid_creator, None);
            assert!(file.file_sha.is_some(), "kept");
        }
    }
}
