//! CM requests needed to download depot content.
//!
//! - Depot decryption key: `CMsgClientGetDepotDecryptionKey` with depot and
//!   app ID; the answer carries an EResult and the 32-byte AES key.
//! - Manifest request code: `ContentServerDirectory.GetManifestRequestCode#1`;
//!   required in manifest URLs (`/depot/<depot>/manifest/<gid>/5/<code>`).
//! - Content servers: `ContentServerDirectory.GetServersForSteamPipe#1` with
//!   the session's cell ID.
//!
//! Sources: SteamKit2 `Steam/WebAPI/ContentServerDirectoryService.cs` (server
//! mapping: `vhost`, `https_support`), `Steam/CDN/Client.cs` (URL uses the
//! vhost); DepotDownloader `DepotDownloader/CDNClientPool.cs` (only types
//! `SteamCache` and `CDN`, `allowed_app_ids`, ordered by weighted load);
//! steamroom `src/client/mod.rs` (the three requests).

use std::fmt;

use steam_vent::ConnectionTrait;
use steam_vent_proto_steam::steammessages_clientserver_2::{
    CMsgClientGetDepotDecryptionKey, CMsgClientGetDepotDecryptionKeyResponse,
};
use steam_vent_proto_steam::steammessages_contentsystem_steamclient::{
    CContentServerDirectory_GetManifestRequestCode_Request,
    CContentServerDirectory_GetServersForSteamPipe_Request, CContentServerDirectory_ServerInfo,
};

use crate::{Error, Session};

/// EResult `OK`.
const ERESULT_OK: i32 = 1;

/// The AES key of a depot as returned by Steam. `Debug` does not print it.
#[derive(Clone, PartialEq, Eq)]
pub struct DepotDecryptionKey(Vec<u8>);

impl DepotDecryptionKey {
    /// The raw key bytes (32 bytes for AES-256).
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for DepotDecryptionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DepotDecryptionKey(<redacted>)")
    }
}

/// A CDN or cache server that serves depot content.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentServer {
    /// Host name to use in URLs (Steam's `vhost`, falling back to `host`).
    pub host: String,
    /// Whether to use HTTPS. Reborn uses HTTPS whenever the server supports
    /// it ("mandatory" or "optional"), unlike SteamKit2, which only does so
    /// when it is mandatory.
    pub https: bool,
    /// Server type, `CDN` or `SteamCache`.
    pub server_type: String,
    /// Steam's load estimate; lower is better.
    pub weighted_load: f32,
    /// If not empty, the server only serves these apps.
    pub allowed_app_ids: Vec<u32>,
}

impl ContentServer {
    /// Whether this server may serve content of `app_id`.
    pub fn serves_app(&self, app_id: u32) -> bool {
        self.allowed_app_ids.is_empty() || self.allowed_app_ids.contains(&app_id)
    }
}

impl Session {
    /// The AES key of a depot. Steam refuses it (`AccessDenied`) if the
    /// account does not own the depot.
    pub async fn depot_key(&self, app_id: u32, depot_id: u32) -> Result<DepotDecryptionKey, Error> {
        let request = CMsgClientGetDepotDecryptionKey {
            depot_id: Some(depot_id),
            app_id: Some(app_id),
            ..CMsgClientGetDepotDecryptionKey::default()
        };
        let response: CMsgClientGetDepotDecryptionKeyResponse =
            self.connection.job(request).await?;
        let code = response.eresult.unwrap_or(0);
        if code != ERESULT_OK {
            return Err(Error::Refused {
                what: "depot key",
                code,
            });
        }
        response
            .depot_encryption_key
            .filter(|key| !key.is_empty())
            .map(DepotDecryptionKey)
            .ok_or(Error::IncompleteResponse("depot key"))
    }

    /// The request code that has to be part of a manifest URL. Codes are
    /// short-lived; fetch one right before downloading the manifest.
    pub async fn manifest_request_code(
        &self,
        app_id: u32,
        depot_id: u32,
        manifest_id: u64,
        branch: &str,
    ) -> Result<u64, Error> {
        let request = CContentServerDirectory_GetManifestRequestCode_Request {
            app_id: Some(app_id),
            depot_id: Some(depot_id),
            manifest_id: Some(manifest_id),
            app_branch: Some(branch.to_owned()),
            ..CContentServerDirectory_GetManifestRequestCode_Request::default()
        };
        let response = self.connection.service_method(request).await?;
        response
            .manifest_request_code
            .filter(|&code| code != 0)
            .ok_or(Error::IncompleteResponse("manifest request code"))
    }

    /// Content servers for this session's region, best first.
    pub async fn content_servers(&self) -> Result<Vec<ContentServer>, Error> {
        let request = CContentServerDirectory_GetServersForSteamPipe_Request {
            cell_id: Some(self.connection.cell_id()),
            max_servers: Some(20),
            ..CContentServerDirectory_GetServersForSteamPipe_Request::default()
        };
        let response = self.connection.service_method(request).await?;
        Ok(usable_servers(response.servers))
    }
}

/// Keeps CDN and cache servers that need no proxy and are not China-only,
/// sorted by weighted load.
fn usable_servers(servers: Vec<CContentServerDirectory_ServerInfo>) -> Vec<ContentServer> {
    let mut usable: Vec<ContentServer> = servers
        .into_iter()
        .filter(|server| !server.use_as_proxy.unwrap_or(false))
        .filter(|server| !server.steam_china_only.unwrap_or(false))
        .filter_map(|server| {
            let server_type = server.type_.clone().unwrap_or_default();
            if server_type != "CDN" && server_type != "SteamCache" {
                return None;
            }
            let host = server
                .vhost
                .clone()
                .filter(|vhost| !vhost.is_empty())
                .or_else(|| server.host.clone())
                .filter(|host| !host.is_empty())?;
            let https = matches!(
                server.https_support.as_deref(),
                Some("mandatory" | "optional")
            );
            Some(ContentServer {
                host,
                https,
                server_type,
                weighted_load: server.weighted_load.unwrap_or(f32::MAX),
                allowed_app_ids: server.allowed_app_ids,
            })
        })
        .collect();
    usable.sort_by(|a, b| a.weighted_load.total_cmp(&b.weighted_load));
    usable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(
        server_type: &str,
        host: &str,
        vhost: &str,
        https: &str,
        load: f32,
    ) -> CContentServerDirectory_ServerInfo {
        CContentServerDirectory_ServerInfo {
            type_: Some(server_type.to_owned()),
            host: Some(host.to_owned()),
            vhost: Some(vhost.to_owned()),
            https_support: Some(https.to_owned()),
            weighted_load: Some(load),
            ..CContentServerDirectory_ServerInfo::default()
        }
    }

    #[test]
    fn keeps_cdn_and_cache_servers_sorted_by_load() {
        let mut proxy = server("CDN", "proxy", "proxy", "mandatory", 1.0);
        proxy.use_as_proxy = Some(true);
        let mut china = server("CDN", "cn", "cn", "mandatory", 1.0);
        china.steam_china_only = Some(true);

        let servers = usable_servers(vec![
            server("CDN", "1.2.3.4", "cdn.example", "optional", 50.0),
            server("SteamCache", "cache", "", "none", 10.0),
            server("OpenCache", "other", "other", "mandatory", 1.0),
            proxy,
            china,
        ]);
        let hosts: Vec<_> = servers.iter().map(|s| (s.host.as_str(), s.https)).collect();
        assert_eq!(hosts, [("cache", false), ("cdn.example", true)]);
    }

    #[test]
    fn respects_allowed_app_ids() {
        let open = ContentServer {
            host: "a".to_owned(),
            https: true,
            server_type: "CDN".to_owned(),
            weighted_load: 1.0,
            allowed_app_ids: Vec::new(),
        };
        let limited = ContentServer {
            allowed_app_ids: vec![730],
            ..open.clone()
        };
        assert!(open.serves_app(480));
        assert!(limited.serves_app(730));
        assert!(!limited.serves_app(480));
    }

    #[test]
    fn depot_key_debug_is_redacted() {
        let key = DepotDecryptionKey(vec![0xAB; 32]);
        assert_eq!(format!("{key:?}"), "DepotDecryptionKey(<redacted>)");
    }
}
