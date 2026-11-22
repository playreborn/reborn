//! The account's library, built the way the Steam client does it:
//! licenses (sent after logon) → packages → apps via PICS (see [`crate::pics`]).
//!
//! `Player.GetOwnedGames#1` is only used to add playtime. It is not the
//! source of the library because it leaves out free games that were added
//! but never played (observed live with a test account, 2026-11-19).

use std::collections::{BTreeSet, HashMap};

use steam_vent::ConnectionTrait;
use steam_vent_proto_steam::steammessages_player_steamclient::{
    CPlayer_GetOwnedGames_Request, cplayer_get_owned_games_response::Game,
};
use tracing::warn;

use crate::pics::{self, AppInfo, License};
use crate::{Error, Session};

/// An app (game, software, tool, DLC, ...) the account owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedApp {
    /// The app ID.
    pub app_id: u32,
    /// Display name. Empty if Steam did not send one.
    pub name: String,
    /// App type in lowercase as Steam reports it, e.g. `game`, `application`,
    /// `tool`, `dlc`, `demo`, `config`. Empty if unknown.
    pub app_type: String,
    /// Total playtime in minutes, all platforms. `0` if never played or unknown.
    pub playtime_minutes: u32,
    /// Last time played as Unix timestamp in seconds, if known.
    pub last_played: Option<u32>,
}

impl OwnedApp {
    /// Whether this is something the user would expect in a library list by
    /// default: games, software and demos (not DLC, tools or config entries).
    pub fn is_listed_by_default(&self) -> bool {
        matches!(self.app_type.as_str(), "game" | "application" | "demo")
    }
}

/// Package 0 is the base license every Steam account gets. It holds free
/// demos, SDKs, dedicated servers and free games (Dota 2, TF2, ...) that the
/// official client does not list in the library unless the user adds them.
/// node-steam-user `components/apps.js` treats it as a free license
/// ("sub 0 is covered by NoCost"); seen live with a new test account.
const BASE_PACKAGE_ID: u32 = 0;

/// `ELicenseFlags::Expired` (SteamKit2 `Resources/SteamLanguage/enums.steamd`).
/// node-steam-user drops expired licenses (e.g. ended free weekends) in
/// `getOwnedPackages`.
const LICENSE_FLAG_EXPIRED: u32 = 0x08;

/// The licenses whose apps belong in the library.
fn library_licenses(licenses: &[License]) -> Vec<License> {
    licenses
        .iter()
        .filter(|license| license.package_id != BASE_PACKAGE_ID)
        .filter(|license| license.flags & LICENSE_FLAG_EXPIRED == 0)
        .copied()
        .collect()
}

/// The licenses that grant ownership: every active license, the base
/// package included. node-steam-user `components/apps.js`: `ownsApp`
/// uses `getOwnedApps` over `getOwnedPackages`, which keeps package 0
/// unless free licenses are excluded on request ("sub 0 is covered by
/// NoCost") and always drops expired licenses.
fn ownership_licenses(licenses: &[License]) -> Vec<License> {
    licenses
        .iter()
        .filter(|license| license.flags & LICENSE_FLAG_EXPIRED == 0)
        .copied()
        .collect()
}

/// Playtime of one app from `GetOwnedGames`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Playtime {
    minutes: u32,
    last_played: Option<u32>,
}

impl Session {
    /// Lists the apps of the account's library, sorted by app ID: every app
    /// from its active licenses, except the base package every account has.
    pub async fn owned_apps(&self) -> Result<Vec<OwnedApp>, Error> {
        let licenses = library_licenses(&self.licenses);
        let app_ids: Vec<u32> = pics::apps_in_packages(&self.connection, &licenses)
            .await?
            .into_iter()
            .collect();
        let tokens = pics::app_access_tokens(&self.connection, &app_ids).await?;
        let mut infos = pics::app_infos(&self.connection, &app_ids, &tokens).await?;
        let playtimes = self.playtimes().await.unwrap_or_else(|error| {
            warn!(%error, "could not load playtimes");
            HashMap::new()
        });

        Ok(app_ids
            .into_iter()
            .map(|app_id| {
                let AppInfo { name, app_type } = infos.remove(&app_id).unwrap_or_default();
                let playtime = playtimes.get(&app_id);
                OwnedApp {
                    app_id,
                    name: name.unwrap_or_default(),
                    app_type: app_type.unwrap_or_default(),
                    playtime_minutes: playtime.map_or(0, |p| p.minutes),
                    last_played: playtime.and_then(|p| p.last_played),
                }
            })
            .collect())
    }

    /// The app IDs the account's active licenses grant, the base package
    /// included (ADR 0055, for ownership answers to the shim). Only the
    /// package step of [`Session::owned_apps`]: the license list sent
    /// after logon, then the packages' app lists via PICS
    /// (`docs/protocol/licenses-pics.md`); no access tokens, app infos or
    /// playtimes are requested.
    pub async fn licensed_app_ids(&self) -> Result<BTreeSet<u32>, Error> {
        pics::apps_in_packages(&self.connection, &ownership_licenses(&self.licenses)).await
    }

    /// Playtime per app via `Player.GetOwnedGames#1`, the request used in
    /// steam-vent's `examples/access_token.rs` (v0.5.0).
    async fn playtimes(&self) -> Result<HashMap<u32, Playtime>, Error> {
        let request = CPlayer_GetOwnedGames_Request {
            steamid: Some(self.steam_id()),
            include_played_free_games: Some(true),
            include_free_sub: Some(true),
            ..CPlayer_GetOwnedGames_Request::default()
        };
        let response = self.connection.service_method(request).await?;
        Ok(response
            .games
            .into_iter()
            .filter_map(playtime_from)
            .collect())
    }
}

fn playtime_from(game: Game) -> Option<(u32, Playtime)> {
    let app_id = u32::try_from(game.appid?).ok()?;
    let minutes = game
        .playtime_forever
        .and_then(|minutes| u32::try_from(minutes).ok())
        .unwrap_or(0);
    let last_played = game.rtime_last_played.filter(|&time| time > 0);
    Some((
        app_id,
        Playtime {
            minutes,
            last_played,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(appid: Option<i32>) -> Game {
        Game {
            appid,
            playtime_forever: Some(125),
            rtime_last_played: Some(1_700_000_000),
            ..Game::default()
        }
    }

    #[test]
    fn converts_playtimes() {
        assert_eq!(
            playtime_from(game(Some(730))),
            Some((
                730,
                Playtime {
                    minutes: 125,
                    last_played: Some(1_700_000_000)
                }
            ))
        );
        assert_eq!(playtime_from(game(None)), None);
        assert_eq!(playtime_from(game(Some(-1))), None);

        let mut never = game(Some(1));
        never.rtime_last_played = Some(0);
        never.playtime_forever = None;
        assert_eq!(
            playtime_from(never),
            Some((
                1,
                Playtime {
                    minutes: 0,
                    last_played: None
                }
            ))
        );
    }

    #[test]
    fn library_skips_the_base_package_and_expired_licenses() {
        let license = |package_id, flags| License {
            package_id,
            access_token: 0,
            flags,
        };
        let licenses = [
            license(0, 0),
            license(100, 0),
            license(200, LICENSE_FLAG_EXPIRED),
            license(300, 0x01 | 0x40),
        ];
        let kept: Vec<_> = library_licenses(&licenses)
            .iter()
            .map(|l| l.package_id)
            .collect();
        assert_eq!(kept, [100, 300]);
    }

    #[test]
    fn ownership_keeps_the_base_package_and_skips_expired_licenses() {
        let license = |package_id, flags| License {
            package_id,
            access_token: 0,
            flags,
        };
        let licenses = [
            license(0, 0),
            license(100, 0),
            license(200, LICENSE_FLAG_EXPIRED),
            license(300, 0x01 | 0x40),
        ];
        let kept: Vec<_> = ownership_licenses(&licenses)
            .iter()
            .map(|l| l.package_id)
            .collect();
        assert_eq!(kept, [0, 100, 300]);
    }

    #[test]
    fn default_listing_shows_games_software_and_demos() {
        let app = |app_type: &str| OwnedApp {
            app_id: 1,
            name: String::new(),
            app_type: app_type.to_owned(),
            playtime_minutes: 0,
            last_played: None,
        };
        for listed in ["game", "application", "demo"] {
            assert!(app(listed).is_listed_by_default(), "{listed}");
        }
        for hidden in ["dlc", "tool", "config", ""] {
            assert!(!app(hidden).is_listed_by_default(), "{hidden}");
        }
    }
}
