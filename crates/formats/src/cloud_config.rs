//! The Steam Cloud settings of an app (`ufs` in the app info), ADR 0033.
//!
//! Layout as recorded from app info (`docs/protocol/cloud.md`): `ufs` holds
//! `quota`, `maxnumfiles`, `savefiles` (Auto-Cloud rules) and
//! `rootoverrides`. The entries of `savefiles` and `rootoverrides` are
//! objects keyed `0`, `1`, ...; other keys next to them (such as
//! `copied_from_parent`) are skipped.

use crate::vdf::{Object, Value};

/// The `ufs` section of an app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudConfig {
    /// Bytes the app may keep in the cloud per user.
    pub quota: Option<u64>,
    /// Files the app may keep in the cloud per user.
    pub max_files: Option<u32>,
    /// Auto-Cloud rules; empty for apps that save through the Steam
    /// interface themselves.
    pub save_files: Vec<SaveFileRule>,
    /// How roots change on other systems.
    pub root_overrides: Vec<RootOverride>,
}

/// One Auto-Cloud rule: files below `root`/`path` matching `pattern`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveFileRule {
    /// Root name, e.g. `WinAppDataLocalLow` or `gameinstall`.
    pub root: String,
    /// Folder below the root; may contain `{64BitSteamID}` or
    /// `{Steam3AccountID}`. Untrusted.
    pub path: String,
    /// File mask with `*` as wildcard.
    pub pattern: String,
    /// Whether subfolders are searched too.
    pub recursive: bool,
    /// Systems the rule is limited to (e.g. `Windows`); empty means all.
    pub platforms: Vec<String>,
}

/// A root that is replaced on some system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootOverride {
    /// The root that is replaced.
    pub root: String,
    /// The system, e.g. `Linux` or `MacOS`.
    pub os: String,
    /// The comparison, e.g. `=`.
    pub os_compare: String,
    /// The root used instead.
    pub use_instead: String,
    /// `addpath`, if set. Whether it replaces or extends the path is not
    /// known yet (ADR 0033).
    pub add_path: Option<String>,
    /// `pathtransforms`: find and replace pairs for the path.
    pub path_transforms: Vec<(String, String)>,
}

/// Reads `appinfo` → `ufs` from an app info document. `None` if the app
/// has no `ufs` section (no Steam Cloud).
pub fn cloud_config(document: &Object) -> Option<CloudConfig> {
    let ufs = document
        .get_object("appinfo")
        .unwrap_or(document)
        .get_object("ufs")?;
    Some(CloudConfig {
        quota: ufs.get_str("quota").and_then(|value| value.parse().ok()),
        max_files: ufs
            .get_str("maxnumfiles")
            .and_then(|value| value.parse().ok()),
        save_files: entries(ufs.get_object("savefiles"))
            .map(|rule| SaveFileRule {
                root: text(rule, "root"),
                path: text(rule, "path"),
                pattern: text(rule, "pattern"),
                recursive: rule.get_str("recursive") == Some("1"),
                platforms: rule
                    .get_object("platforms")
                    .map(|platforms| {
                        platforms
                            .iter()
                            .filter_map(|(_, value)| value.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect(),
        root_overrides: entries(ufs.get_object("rootoverrides"))
            .map(|entry| RootOverride {
                root: text(entry, "root"),
                os: text(entry, "os"),
                os_compare: text(entry, "oscompare"),
                use_instead: text(entry, "useinstead"),
                add_path: entry.get_str("addpath").map(str::to_owned),
                path_transforms: entries(entry.get_object("pathtransforms"))
                    .map(|transform| (text(transform, "find"), text(transform, "replace")))
                    .collect(),
            })
            .collect(),
    })
}

/// The object values of a list section; other values are skipped.
fn entries(section: Option<&Object>) -> impl Iterator<Item = &Object> {
    section
        .into_iter()
        .flat_map(|section| section.iter())
        .filter_map(|(_, value)| match value {
            Value::Object(object) => Some(object),
            Value::String(_) => None,
        })
}

fn text(object: &Object, key: &str) -> String {
    object.get_str(key).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdf;

    /// Recorded app info (see `tests/data/README.md`).
    const COIN_MAKER: &str = include_str!("../tests/data/appinfo_5191630.vdf");
    const KUIPER_BELT: &str = include_str!("../tests/data/appinfo_1511340.vdf");
    const CAVE_CRAWLERS: &str = include_str!("../tests/data/appinfo_2143030.vdf");

    #[test]
    fn reads_a_rule_with_an_addpath_override() {
        let config = cloud_config(&vdf::parse(COIN_MAKER).unwrap()).unwrap();
        assert_eq!(config.quota, Some(15_000_000));
        assert_eq!(config.max_files, Some(10));
        assert_eq!(
            config.save_files,
            vec![SaveFileRule {
                root: "WinAppDataLocalLow".into(),
                path: "Studio312/Coin Maker Simulator Demo/".into(),
                pattern: "save_slot_demo_*.json".into(),
                recursive: false,
                platforms: Vec::new(),
            }]
        );
        assert_eq!(
            config.root_overrides,
            vec![RootOverride {
                root: "WinAppDataLocalLow".into(),
                os: "Linux".into(),
                os_compare: "=".into(),
                use_instead: "LinuxHome".into(),
                add_path: Some(".config/unity3d/Studio312/Coin Maker Simulator Demo/".into()),
                path_transforms: Vec::new(),
            }]
        );
    }

    #[test]
    fn reads_path_transforms_and_skips_other_keys() {
        let config = cloud_config(&vdf::parse(KUIPER_BELT).unwrap()).unwrap();
        // `copied_from_parent` sits next to the rule and is not one.
        assert_eq!(config.save_files.len(), 1);
        let rule = &config.save_files[0];
        assert_eq!(
            rule.path,
            "Cascus Harmony Games/Defender of Kuiper Belt/{64BitSteamID}/"
        );
        assert!(rule.recursive);
        assert_eq!(config.root_overrides.len(), 2);
        let linux = &config.root_overrides[1];
        assert_eq!(linux.os, "Linux");
        assert_eq!(linux.use_instead, "LinuxHome");
        assert_eq!(linux.add_path, None);
        assert_eq!(
            linux.path_transforms,
            vec![(
                "Cascus Harmony Games/Defender of Kuiper Belt".to_owned(),
                ".config/unity3d/Cascus Harmony Games/Defender of Kuiper Belt".to_owned()
            )]
        );
    }

    #[test]
    fn apps_saving_through_the_interface_have_no_rules() {
        let config = cloud_config(&vdf::parse(CAVE_CRAWLERS).unwrap()).unwrap();
        assert_eq!(config.quota, Some(1_000_000));
        assert_eq!(config.max_files, Some(3));
        assert!(config.save_files.is_empty());
        assert!(config.root_overrides.is_empty());
    }

    #[test]
    fn no_ufs_means_no_cloud() {
        let document = vdf::parse("\"appinfo\"\n{\n\t\"appid\"\t\t\"1\"\n}\n").unwrap();
        assert_eq!(cloud_config(&document), None);
    }

    #[test]
    fn reads_platforms() {
        let text = r#""appinfo"
{
	"ufs"
	{
		"savefiles"
		{
			"0"
			{
				"root"		"gameinstall"
				"path"		"SavesDir"
				"pattern"		"*.sav"
				"platforms"
				{
					"1"		"Windows"
				}
			}
		}
	}
}
"#;
        let config = cloud_config(&vdf::parse(text).unwrap()).unwrap();
        assert_eq!(config.save_files[0].platforms, ["Windows"]);
        assert_eq!(config.quota, None);
    }
}
