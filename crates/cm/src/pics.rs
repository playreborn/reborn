//! PICS (Product Info and Change System): which apps a package contains
//! and the metadata (name, type) of apps.
//!
//! Sources:
//! - Package info buffers start with a little-endian `u32` format version
//!   (only `1` is known), followed by binary KeyValues whose root key is the
//!   package ID and which contain an `appids` object.
//!   SteamKit2 `Steam/Handlers/SteamApps/Callbacks.cs` (`PICSProductInfo`),
//!   node-steam-user `components/apps.js` (`packageinfo.appids`).
//! - App info buffers are text KeyValues with the root key `appinfo`
//!   (`appinfo/common/name`, `appinfo/common/type`), possibly followed by NUL
//!   bytes. node-steam-user `components/apps.js` (`VDF.parse(...).appinfo`),
//!   steam-vent `examples/product_info.rs` (trims `\0`).
//! - App access tokens come from `CMsgClientPICSAccessTokenRequest`;
//!   package access tokens come from the license list.
//!   node-steam-user `components/apps.js` (`getProductAccessToken`, `inclTokens`).

use std::collections::{BTreeSet, HashMap};

use futures_util::TryStreamExt;
use reborn_formats::{binary, vdf};
use steam_vent::{Connection, ConnectionTrait};
use steam_vent_proto_steam::steammessages_clientserver_appinfo::{
    CMsgClientPICSAccessTokenRequest, CMsgClientPICSAccessTokenResponse,
    CMsgClientPICSProductInfoRequest, CMsgClientPICSProductInfoResponse,
    cmsg_client_picsproduct_info_request as request,
};
use tracing::{debug, warn};

use crate::Error;

/// How many packages or apps are requested per PICS call.
const CHUNK_SIZE: usize = 100;

/// A license of the account, as sent by Steam after logon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct License {
    pub(crate) package_id: u32,
    pub(crate) access_token: u64,
    /// `ELicenseFlags` bit set.
    pub(crate) flags: u32,
}

/// Name and type of an app, from its PICS app info.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AppInfo {
    pub(crate) name: Option<String>,
    pub(crate) app_type: Option<String>,
}

/// Why a PICS buffer could not be read. Only logged, never fatal.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PicsError {
    #[error("package info is shorter than its 4-byte header")]
    TooShort,
    #[error("unknown package info format {0}")]
    UnknownFormat(u32),
    #[error("package info has no package object")]
    NoPackage,
    #[error(transparent)]
    Binary(#[from] binary::BinaryError),
    #[error("app info is not UTF-8")]
    NotUtf8,
    #[error(transparent)]
    Text(#[from] vdf::ParseError),
}

/// All app IDs contained in the given licensed packages.
pub(crate) async fn apps_in_packages(
    connection: &Connection,
    licenses: &[License],
) -> Result<BTreeSet<u32>, Error> {
    let mut apps = BTreeSet::new();
    for chunk in licenses.chunks(CHUNK_SIZE) {
        let request = CMsgClientPICSProductInfoRequest {
            packages: chunk
                .iter()
                .map(|license| request::PackageInfo {
                    packageid: Some(license.package_id),
                    access_token: Some(license.access_token),
                    ..request::PackageInfo::default()
                })
                .collect(),
            meta_data_only: Some(false),
            ..CMsgClientPICSProductInfoRequest::default()
        };
        for response in product_info(connection, request).await? {
            for package in response.packages {
                let package_id = package.packageid.unwrap_or_default();
                let Some(buffer) = package.buffer else {
                    warn!(package_id, "package info without data");
                    continue;
                };
                match package_app_ids(&buffer) {
                    Ok(ids) => apps.extend(ids),
                    Err(error) => warn!(package_id, %error, "could not read package info"),
                }
            }
        }
    }
    debug!(
        packages = licenses.len(),
        apps = apps.len(),
        "resolved licensed apps"
    );
    Ok(apps)
}

/// Access tokens for apps that need one to reveal their full info.
pub(crate) async fn app_access_tokens(
    connection: &Connection,
    app_ids: &[u32],
) -> Result<HashMap<u32, u64>, Error> {
    let mut tokens = HashMap::new();
    for chunk in app_ids.chunks(CHUNK_SIZE) {
        let request = CMsgClientPICSAccessTokenRequest {
            appids: chunk.to_vec(),
            ..CMsgClientPICSAccessTokenRequest::default()
        };
        let response: CMsgClientPICSAccessTokenResponse = connection.job(request).await?;
        for token in response.app_access_tokens {
            if let (Some(app_id), Some(access_token)) = (token.appid, token.access_token) {
                tokens.insert(app_id, access_token);
            }
        }
    }
    Ok(tokens)
}

/// Name and type for each app. Apps Steam does not answer for are missing.
pub(crate) async fn app_infos(
    connection: &Connection,
    app_ids: &[u32],
    tokens: &HashMap<u32, u64>,
) -> Result<HashMap<u32, AppInfo>, Error> {
    let mut infos = HashMap::new();
    for chunk in app_ids.chunks(CHUNK_SIZE) {
        let request = CMsgClientPICSProductInfoRequest {
            apps: chunk
                .iter()
                .map(|&app_id| request::AppInfo {
                    appid: Some(app_id),
                    access_token: tokens.get(&app_id).copied(),
                    ..request::AppInfo::default()
                })
                .collect(),
            meta_data_only: Some(false),
            ..CMsgClientPICSProductInfoRequest::default()
        };
        for response in product_info(connection, request).await? {
            for app in response.apps {
                let Some(app_id) = app.appid else { continue };
                let info = match app.buffer.as_deref().map(parse_app_info) {
                    Some(Ok(info)) => info,
                    Some(Err(error)) => {
                        warn!(app_id, %error, "could not read app info");
                        AppInfo::default()
                    }
                    // Large app infos may only be offered over HTTP; not needed for names yet.
                    None => AppInfo::default(),
                };
                infos.insert(app_id, info);
            }
        }
    }
    Ok(infos)
}

async fn product_info(
    connection: &Connection,
    request: CMsgClientPICSProductInfoRequest,
) -> Result<Vec<CMsgClientPICSProductInfoResponse>, Error> {
    Ok(connection
        .job_multi::<_, CMsgClientPICSProductInfoResponse>(request)
        .try_collect()
        .await?)
}

/// App IDs listed in a PICS package info buffer.
pub(crate) fn package_app_ids(buffer: &[u8]) -> Result<Vec<u32>, PicsError> {
    let (header, body) = buffer.split_first_chunk::<4>().ok_or(PicsError::TooShort)?;
    let format = u32::from_le_bytes(*header);
    if format != 1 {
        return Err(PicsError::UnknownFormat(format));
    }
    let document = binary::parse(body)?;
    let Some((_, vdf::Value::Object(package))) = document.iter().next() else {
        return Err(PicsError::NoPackage);
    };
    Ok(package
        .get_object("appids")
        .map(|ids| {
            ids.iter()
                .filter_map(|(_, value)| value.as_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default())
}

/// A PICS app info buffer as KeyValues (root key `appinfo`).
pub(crate) fn parse_app_document(buffer: &[u8]) -> Result<vdf::Object, PicsError> {
    let text = std::str::from_utf8(buffer).map_err(|_| PicsError::NotUtf8)?;
    Ok(vdf::parse(text.trim_end_matches('\0'))?)
}

/// The full app info document of one app, using its access token if any.
pub(crate) async fn app_document(
    connection: &Connection,
    app_id: u32,
) -> Result<vdf::Object, Error> {
    let tokens = app_access_tokens(connection, &[app_id]).await?;
    let request = CMsgClientPICSProductInfoRequest {
        apps: vec![request::AppInfo {
            appid: Some(app_id),
            access_token: tokens.get(&app_id).copied(),
            ..request::AppInfo::default()
        }],
        meta_data_only: Some(false),
        ..CMsgClientPICSProductInfoRequest::default()
    };
    for response in product_info(connection, request).await? {
        for app in response.apps {
            if app.appid != Some(app_id) {
                continue;
            }
            let Some(buffer) = app.buffer.filter(|buffer| !buffer.is_empty()) else {
                // Steam may offer large app infos only over HTTP (http_host).
                return Err(Error::AppInfoUnavailable(app_id));
            };
            return parse_app_document(&buffer)
                .map_err(|error| Error::AppInfo(app_id, error.to_string()));
        }
    }
    Err(Error::AppInfoUnavailable(app_id))
}

/// Name and type from a PICS app info buffer.
pub(crate) fn parse_app_info(buffer: &[u8]) -> Result<AppInfo, PicsError> {
    let document = parse_app_document(buffer)?;
    let common = document
        .get_object("appinfo")
        .and_then(|info| info.get_object("common"));
    Ok(AppInfo {
        name: common.and_then(|c| c.get_str("name")).map(str::to_owned),
        app_type: common
            .and_then(|c| c.get_str("type"))
            .map(str::to_ascii_lowercase),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Package info as Steam sends it: format header, then binary KeyValues.
    fn package_buffer(format: u32, app_ids: &[u32]) -> Vec<u8> {
        let mut bytes = format.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"\x00999\x00"); // object "999"
        bytes.extend_from_slice(b"\x02packageid\x00");
        bytes.extend_from_slice(&999i32.to_le_bytes());
        bytes.extend_from_slice(b"\x00appids\x00"); // object "appids"
        for (index, app_id) in app_ids.iter().enumerate() {
            bytes.push(2);
            bytes.extend_from_slice(index.to_string().as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(&app_id.to_le_bytes());
        }
        bytes.extend_from_slice(&[8, 8, 8]);
        bytes
    }

    #[test]
    fn reads_app_ids_from_package_info() {
        assert_eq!(
            package_app_ids(&package_buffer(1, &[730, 228980])).unwrap(),
            [730, 228980]
        );
        assert_eq!(
            package_app_ids(&package_buffer(1, &[])).unwrap(),
            Vec::<u32>::new()
        );
    }

    #[test]
    fn rejects_unknown_package_formats() {
        assert!(matches!(
            package_app_ids(&package_buffer(2, &[1])),
            Err(PicsError::UnknownFormat(2))
        ));
        assert!(matches!(package_app_ids(&[1, 0]), Err(PicsError::TooShort)));
    }

    #[test]
    fn reads_name_and_type_from_app_info() {
        let buffer = b"\"appinfo\"\n{\n\t\"appid\"\t\t\"440\"\n\t\"common\"\n\t{\n\t\t\"name\"\t\t\"Team Fortress 2\"\n\t\t\"type\"\t\t\"Game\"\n\t}\n}\n\0\0";
        assert_eq!(
            parse_app_info(buffer).unwrap(),
            AppInfo {
                name: Some("Team Fortress 2".to_owned()),
                app_type: Some("game".to_owned()),
            }
        );
    }

    #[test]
    fn app_info_without_common_section_has_no_name() {
        let info = parse_app_info(b"\"appinfo\" { \"appid\" \"1\" }").unwrap();
        assert_eq!(info, AppInfo::default());
    }
}
