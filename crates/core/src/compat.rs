//! The compatibility database: how well each app works with Reborn.
//!
//! The database lives in the repository's `compat/` directory, one TOML file
//! per app named `<app_id>.toml` (plan section 8.5, ADR 0015). Every file
//! holds one or more reports from people who actually played the app:
//!
//! ```toml
//! app_id = 12345
//! name = "Some Game"
//!
//! [[report]]
//! tier = 2
//! platform = "linux-proton"
//! reborn_version = "0.1.0"
//! proton_version = "Proton 9.0-4"
//! date = "2026-11-20"
//! notes = "Singleplayer works."
//! ```
//!
//! [`CompatDb::load_dir`] reads a whole directory, [`load_file`] a single
//! file. Both validate every file (tier range, known platform, date format,
//! app ID matching the file name) and refuse the whole input on the first
//! problem, so a broken contribution is caught in review and CI instead of
//! being silently skipped. The schema is documented in `compat/README.md`.
//!
//! This module only reads local files; it never contacts any server.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

/// File extension of database files.
const EXTENSION: &str = "toml";

/// How well an app works with Reborn (plan section 8.5).
///
/// Every tier includes everything below it. The numeric value is the tier
/// number used in the TOML files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// 0: does not start.
    DoesNotStart = 0,
    /// 1: starts, but only with the official client as fallback.
    NeedsOfficialClient = 1,
    /// 2: starts without the official client; singleplayer is playable.
    Singleplayer = 2,
    /// 3: additionally achievements and cloud saves work.
    AchievementsAndCloud = 3,
    /// 4: additionally multiplayer works.
    Multiplayer = 4,
    /// 5: no known differences to the official client.
    NoKnownDifferences = 5,
}

impl Tier {
    /// All tiers, lowest first.
    pub const ALL: [Tier; 6] = [
        Tier::DoesNotStart,
        Tier::NeedsOfficialClient,
        Tier::Singleplayer,
        Tier::AchievementsAndCloud,
        Tier::Multiplayer,
        Tier::NoKnownDifferences,
    ];

    /// The tier number (0 to 5).
    pub fn number(self) -> u8 {
        self as u8
    }

    /// The tier with this number, if it is in range.
    pub fn from_number(number: i64) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|tier| i64::from(tier.number()) == number)
    }

    /// A short English description, as in `compat/README.md`.
    pub fn description(self) -> &'static str {
        match self {
            Tier::DoesNotStart => "does not start",
            Tier::NeedsOfficialClient => "starts only with the official client as fallback",
            Tier::Singleplayer => "starts without the official client, singleplayer playable",
            Tier::AchievementsAndCloud => "additionally achievements and cloud work",
            Tier::Multiplayer => "additionally multiplayer works",
            Tier::NoKnownDifferences => "no known differences to the official client",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.number(), self.description())
    }
}

/// Where a report was tested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Platform {
    /// Windows, `"windows"`.
    Windows,
    /// A native Linux build of the app, `"linux-native"`.
    LinuxNative,
    /// The Windows build of the app on Linux through Proton, `"linux-proton"`.
    LinuxProton,
}

impl Platform {
    /// All platforms.
    pub const ALL: [Platform; 3] = [
        Platform::Windows,
        Platform::LinuxNative,
        Platform::LinuxProton,
    ];

    /// The spelling used in the TOML files.
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::LinuxNative => "linux-native",
            Platform::LinuxProton => "linux-proton",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The text is not one of the known platform names.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown platform `{0}`; expected windows, linux-native or linux-proton")]
pub struct UnknownPlatform(pub String);

impl FromStr for Platform {
    type Err = UnknownPlatform;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|platform| platform.as_str() == text)
            .ok_or_else(|| UnknownPlatform(text.to_owned()))
    }
}

/// A calendar date in `YYYY-MM-DD` form.
///
/// Ordering is chronological.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReportDate {
    /// Year, e.g. 2026.
    pub year: u16,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day of the month, 1 to 31 (checked against the month).
    pub day: u8,
}

impl ReportDate {
    /// Parses exactly `YYYY-MM-DD` and checks that the day exists.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let digits = |range: std::ops::Range<usize>| -> Option<u16> {
            let part = text.get(range)?;
            if !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let year = digits(0..4)?;
        let month = u8::try_from(digits(5..7)?).ok()?;
        let day = u8::try_from(digits(8..10)?).ok()?;
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (1..=days_in_month)
            .contains(&day)
            .then_some(Self { year, month, day })
    }
}

impl fmt::Display for ReportDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// One compatibility report: one person, one platform, one Reborn version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// How well the app worked.
    pub tier: Tier,
    /// Where it was tested.
    pub platform: Platform,
    /// Reborn version or commit that was tested.
    pub reborn_version: String,
    /// Proton version, only for [`Platform::LinuxProton`].
    pub proton_version: Option<String>,
    /// When it was tested.
    pub date: ReportDate,
    /// Free text: what works, what does not, workarounds. May be empty.
    pub notes: String,
}

/// All reports for one app, as stored in `compat/<app_id>.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppCompat {
    /// The app ID.
    pub app_id: u32,
    /// The app's name, for humans reading the file.
    pub name: String,
    /// The reports, in file order. Never empty.
    pub reports: Vec<Report>,
}

impl AppCompat {
    /// Parses and validates the text of a database file.
    ///
    /// This does not check the file name; [`load_file`] does.
    pub fn from_toml(text: &str) -> Result<Self, InvalidFile> {
        let raw: RawFile =
            toml::from_str(text).map_err(|error| InvalidFile::Toml(Box::new(error)))?;
        raw.validate()
    }

    /// The best report for `platform`: the highest tier, and among reports
    /// with the same tier the newest one. `None` without reports for it.
    pub fn best_report(&self, platform: Platform) -> Option<&Report> {
        self.reports
            .iter()
            .filter(|report| report.platform == platform)
            .max_by_key(|report| (report.tier, report.date))
    }

    /// The highest tier reported for `platform`.
    pub fn best_tier(&self, platform: Platform) -> Option<Tier> {
        self.best_report(platform).map(|report| report.tier)
    }

    /// The highest tier per platform, for every platform with a report.
    pub fn best_tiers(&self) -> BTreeMap<Platform, Tier> {
        Platform::ALL
            .into_iter()
            .filter_map(|platform| Some((platform, self.best_tier(platform)?)))
            .collect()
    }
}

/// The compatibility database: every app's reports, by app ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompatDb {
    apps: BTreeMap<u32, AppCompat>,
}

impl CompatDb {
    /// Loads every `*.toml` file in `dir` (not recursive).
    ///
    /// Files with another extension (`README.md`, `example.toml.sample`)
    /// and subdirectories are ignored. Any invalid `*.toml` file fails the
    /// whole load.
    pub fn load_dir(dir: &Path) -> Result<Self, CompatError> {
        let io_error = |source| CompatError::Io {
            path: dir.to_path_buf(),
            source,
        };
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            let is_file = entry.file_type().map_err(io_error)?.is_file();
            if is_file && path.extension().is_some_and(|ext| ext == EXTENSION) {
                paths.push(path);
            }
        }
        // Deterministic error reporting regardless of directory order.
        paths.sort();
        let mut db = Self::default();
        for path in paths {
            db.insert(load_file(&path)?);
        }
        Ok(db)
    }

    /// Adds or replaces the entry for one app.
    pub fn insert(&mut self, app: AppCompat) -> Option<AppCompat> {
        self.apps.insert(app.app_id, app)
    }

    /// The reports for an app, if there are any.
    pub fn get(&self, app_id: u32) -> Option<&AppCompat> {
        self.apps.get(&app_id)
    }

    /// The highest tier reported for an app on a platform.
    pub fn best_tier(&self, app_id: u32, platform: Platform) -> Option<Tier> {
        self.get(app_id)?.best_tier(platform)
    }

    /// All apps, by ascending app ID.
    pub fn apps(&self) -> impl Iterator<Item = &AppCompat> {
        self.apps.values()
    }

    /// Number of apps with reports.
    pub fn len(&self) -> usize {
        self.apps.len()
    }

    /// Returns `true` if the database has no apps.
    pub fn is_empty(&self) -> bool {
        self.apps.is_empty()
    }
}

/// Loads and validates one database file, which must be named
/// `<app_id>.toml` with the same app ID as inside the file.
pub fn load_file(path: &Path) -> Result<AppCompat, CompatError> {
    let file_app_id = app_id_from_file_name(path).ok_or_else(|| CompatError::FileName {
        path: path.to_path_buf(),
    })?;
    let text = std::fs::read_to_string(path).map_err(|source| CompatError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let app = AppCompat::from_toml(&text).map_err(|source| CompatError::Invalid {
        path: path.to_path_buf(),
        source,
    })?;
    if app.app_id != file_app_id {
        return Err(CompatError::AppIdMismatch {
            path: path.to_path_buf(),
            file_name: file_app_id,
            app_id: app.app_id,
        });
    }
    Ok(app)
}

/// The app ID from a file name `<app_id>.toml`. The number must be written
/// canonically (no sign, no leading zeros), so every app has exactly one
/// possible file name.
fn app_id_from_file_name(path: &Path) -> Option<u32> {
    if path.extension()? != EXTENSION {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let app_id: u32 = stem.parse().ok()?;
    (app_id.to_string() == stem).then_some(app_id)
}

/// Loading the database failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CompatError {
    /// A file or directory could not be read.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// A `*.toml` file is not named `<app_id>.toml`.
    #[error("{}: database files must be named <app_id>.toml", path.display())]
    FileName {
        /// The file.
        path: PathBuf,
    },
    /// The app ID inside the file differs from its file name.
    #[error("{}: file name says app {file_name}, but the file has app_id = {app_id}", path.display())]
    AppIdMismatch {
        /// The file.
        path: PathBuf,
        /// The app ID from the file name.
        file_name: u32,
        /// The app ID inside the file.
        app_id: u32,
    },
    /// The file's content is invalid.
    #[error("{}: {source}", path.display())]
    Invalid {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        source: InvalidFile,
    },
}

/// The content of a database file is invalid.
///
/// Report numbers in messages count from 1, in file order.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidFile {
    /// Not valid TOML, a missing or misspelled key, or a wrong value type.
    #[error("{0}")]
    Toml(Box<toml::de::Error>),
    /// The file has no `[[report]]` entries.
    #[error("the file has no [[report]] entries")]
    NoReports,
    /// A required text field is empty.
    #[error("{} must not be empty", field_location(.report, .field))]
    EmptyField {
        /// Index of the report (0-based), `None` for top-level fields.
        report: Option<usize>,
        /// The field name.
        field: &'static str,
    },
    /// The tier is not between 0 and 5.
    #[error("report {}: tier {tier} is out of range 0-5", .report + 1)]
    TierOutOfRange {
        /// Index of the report (0-based).
        report: usize,
        /// The tier found in the file.
        tier: i64,
    },
    /// The platform is not one of the known names.
    #[error("report {}: {source}", .report + 1)]
    UnknownPlatform {
        /// Index of the report (0-based).
        report: usize,
        /// The unknown name.
        source: UnknownPlatform,
    },
    /// The date is not a valid `YYYY-MM-DD` date.
    #[error("report {}: date `{date}` is not a valid YYYY-MM-DD date", .report + 1)]
    InvalidDate {
        /// Index of the report (0-based).
        report: usize,
        /// The date as found in the file.
        date: String,
    },
    /// `proton_version` is set on a report that is not for `linux-proton`.
    #[error("report {}: proton_version is only allowed for platform linux-proton, not {platform}", .report + 1)]
    ProtonVersionNotAllowed {
        /// Index of the report (0-based).
        report: usize,
        /// The report's platform.
        platform: Platform,
    },
}

fn field_location(report: &Option<usize>, field: &str) -> String {
    match report {
        Some(index) => format!("report {}: {field}", index + 1),
        None => field.to_owned(),
    }
}

/// The file exactly as written, before validation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    app_id: u32,
    name: String,
    #[serde(default)]
    report: Vec<RawReport>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReport {
    tier: i64,
    platform: String,
    reborn_version: String,
    #[serde(default)]
    proton_version: Option<String>,
    date: String,
    #[serde(default)]
    notes: String,
}

impl RawFile {
    fn validate(self) -> Result<AppCompat, InvalidFile> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(InvalidFile::EmptyField {
                report: None,
                field: "name",
            });
        }
        if self.report.is_empty() {
            return Err(InvalidFile::NoReports);
        }
        let reports = self
            .report
            .into_iter()
            .enumerate()
            .map(|(index, report)| report.validate(index))
            .collect::<Result<_, _>>()?;
        Ok(AppCompat {
            app_id: self.app_id,
            name: name.to_owned(),
            reports,
        })
    }
}

impl RawReport {
    fn validate(self, index: usize) -> Result<Report, InvalidFile> {
        let tier = Tier::from_number(self.tier).ok_or(InvalidFile::TierOutOfRange {
            report: index,
            tier: self.tier,
        })?;
        let platform: Platform =
            self.platform
                .parse()
                .map_err(|source| InvalidFile::UnknownPlatform {
                    report: index,
                    source,
                })?;
        let reborn_version = self.reborn_version.trim();
        if reborn_version.is_empty() {
            return Err(InvalidFile::EmptyField {
                report: Some(index),
                field: "reborn_version",
            });
        }
        let proton_version = match self.proton_version.as_deref().map(str::trim) {
            None => None,
            Some("") => {
                return Err(InvalidFile::EmptyField {
                    report: Some(index),
                    field: "proton_version",
                });
            }
            Some(_) if platform != Platform::LinuxProton => {
                return Err(InvalidFile::ProtonVersionNotAllowed {
                    report: index,
                    platform,
                });
            }
            Some(version) => Some(version.to_owned()),
        };
        let date = ReportDate::parse(&self.date).ok_or_else(|| InvalidFile::InvalidDate {
            report: index,
            date: self.date.clone(),
        })?;
        Ok(Report {
            tier,
            platform,
            reborn_version: reborn_version.to_owned(),
            proton_version,
            date,
            notes: self.notes.trim().to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example shipped in `compat/`, kept valid by this test.
    const EXAMPLE: &str = include_str!("../../../compat/example.toml.sample");

    /// Unique scratch directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-compat-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let dir = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn file(app_id: u32, reports: &str) -> String {
        format!("app_id = {app_id}\nname = \"Test App\"\n{reports}")
    }

    const WINDOWS_2: &str = "[[report]]\ntier = 2\nplatform = \"windows\"\nreborn_version = \"0.1.0\"\ndate = \"2026-11-01\"\n";

    fn invalid(text: &str) -> InvalidFile {
        AppCompat::from_toml(text).unwrap_err()
    }

    #[test]
    fn example_is_valid() {
        let app = AppCompat::from_toml(EXAMPLE).unwrap();
        assert!(!app.reports.is_empty());
        assert!(app.name.contains("Example"));
    }

    #[test]
    fn repository_database_is_valid() {
        // Every file contributed to compat/ must load; this runs in CI.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compat");
        CompatDb::load_dir(&dir).unwrap();
    }

    #[test]
    fn parses_all_fields() {
        let text = file(
            7,
            "[[report]]\ntier = 3\nplatform = \"linux-proton\"\nreborn_version = \" 0.1.0 \"\n\
             proton_version = \"Proton 9.0-4\"\ndate = \"2024-02-29\"\nnotes = \"fine\"\n",
        );
        let app = AppCompat::from_toml(&text).unwrap();
        assert_eq!(app.app_id, 7);
        assert_eq!(app.name, "Test App");
        assert_eq!(
            app.reports,
            vec![Report {
                tier: Tier::AchievementsAndCloud,
                platform: Platform::LinuxProton,
                reborn_version: "0.1.0".to_owned(),
                proton_version: Some("Proton 9.0-4".to_owned()),
                date: ReportDate {
                    year: 2024,
                    month: 2,
                    day: 29
                },
                notes: "fine".to_owned(),
            }]
        );
    }

    #[test]
    fn best_tier_per_platform() {
        let text = file(
            7,
            "[[report]]\ntier = 4\nplatform = \"windows\"\nreborn_version = \"0.1.0\"\ndate = \"2026-01-01\"\n\
             [[report]]\ntier = 1\nplatform = \"windows\"\nreborn_version = \"0.2.0\"\ndate = \"2026-03-01\"\n\
             [[report]]\ntier = 2\nplatform = \"linux-proton\"\nreborn_version = \"0.1.0\"\ndate = \"2026-01-01\"\n\
             [[report]]\ntier = 2\nplatform = \"linux-proton\"\nreborn_version = \"0.2.0\"\ndate = \"2026-02-01\"\n",
        );
        let app = AppCompat::from_toml(&text).unwrap();
        assert_eq!(app.best_tier(Platform::Windows), Some(Tier::Multiplayer));
        assert_eq!(app.best_tier(Platform::LinuxNative), None);
        // Same tier: the newer report wins.
        let proton = app.best_report(Platform::LinuxProton).unwrap();
        assert_eq!(proton.reborn_version, "0.2.0");
        assert_eq!(
            app.best_tiers(),
            BTreeMap::from([
                (Platform::Windows, Tier::Multiplayer),
                (Platform::LinuxProton, Tier::Singleplayer),
            ])
        );
    }

    #[test]
    fn loads_directory_and_ignores_other_files() {
        let dir = TempDir::new("dir");
        dir.write("10.toml", &file(10, WINDOWS_2));
        dir.write("20.toml", &file(20, WINDOWS_2));
        dir.write("README.md", "not a database file");
        dir.write("example.toml.sample", "not = [valid");
        std::fs::create_dir(dir.0.join("sub.toml")).unwrap();
        let db = CompatDb::load_dir(&dir.0).unwrap();
        assert_eq!(db.len(), 2);
        assert_eq!(
            db.apps().map(|app| app.app_id).collect::<Vec<_>>(),
            vec![10, 20]
        );
        assert_eq!(
            db.best_tier(10, Platform::Windows),
            Some(Tier::Singleplayer)
        );
        assert_eq!(db.best_tier(10, Platform::LinuxNative), None);
        assert_eq!(db.best_tier(30, Platform::Windows), None);
    }

    #[test]
    fn empty_directory_is_an_empty_database() {
        let dir = TempDir::new("empty");
        assert!(CompatDb::load_dir(&dir.0).unwrap().is_empty());
    }

    #[test]
    fn missing_directory_fails() {
        let dir = TempDir::new("missing");
        let error = CompatDb::load_dir(&dir.0.join("nope")).unwrap_err();
        assert!(matches!(error, CompatError::Io { .. }), "{error}");
    }

    #[test]
    fn one_invalid_file_fails_the_directory() {
        let dir = TempDir::new("invalid");
        dir.write("10.toml", &file(10, WINDOWS_2));
        dir.write("20.toml", &file(20, ""));
        let error = CompatDb::load_dir(&dir.0).unwrap_err();
        assert!(
            matches!(
                &error,
                CompatError::Invalid { path, source: InvalidFile::NoReports } if path.ends_with("20.toml")
            ),
            "{error}"
        );
    }

    #[test]
    fn app_id_must_match_file_name() {
        let dir = TempDir::new("mismatch");
        let path = dir.write("10.toml", &file(11, WINDOWS_2));
        let error = load_file(&path).unwrap_err();
        assert!(
            matches!(
                error,
                CompatError::AppIdMismatch {
                    file_name: 10,
                    app_id: 11,
                    ..
                }
            ),
            "{error}"
        );
    }

    #[test]
    fn file_name_must_be_a_canonical_app_id() {
        let dir = TempDir::new("names");
        for name in [
            "game.toml",
            "010.toml",
            "+10.toml",
            "-1.toml",
            "99999999999.toml",
        ] {
            let path = dir.write(name, &file(10, WINDOWS_2));
            let error = load_file(&path).unwrap_err();
            assert!(
                matches!(error, CompatError::FileName { .. }),
                "{name}: {error}"
            );
        }
        let path = dir.write("10.txt", &file(10, WINDOWS_2));
        assert!(matches!(
            load_file(&path),
            Err(CompatError::FileName { .. })
        ));
    }

    #[test]
    fn rejects_tier_out_of_range() {
        for tier in ["6", "-1", "100"] {
            let text = file(1, &WINDOWS_2.replace("tier = 2", &format!("tier = {tier}")));
            let error = invalid(&text);
            assert!(
                matches!(error, InvalidFile::TierOutOfRange { report: 0, tier: t } if t.to_string() == tier),
                "{tier}: {error}"
            );
        }
    }

    #[test]
    fn rejects_unknown_platform() {
        for platform in ["linux", "steamos", "Windows", ""] {
            let text = file(
                1,
                &WINDOWS_2.replace("\"windows\"", &format!("\"{platform}\"")),
            );
            let error = invalid(&text);
            assert!(
                matches!(error, InvalidFile::UnknownPlatform { report: 0, .. }),
                "{platform}: {error}"
            );
        }
    }

    #[test]
    fn rejects_invalid_dates() {
        for date in [
            "2026-13-01",
            "2026-02-29",
            "2026-04-31",
            "2026-00-10",
            "2026-1-01",
            "26-01-01",
            "2026/01/01",
            "",
        ] {
            let text = file(1, &WINDOWS_2.replace("2026-11-01", date));
            let error = invalid(&text);
            assert!(
                matches!(error, InvalidFile::InvalidDate { report: 0, .. }),
                "{date}: {error}"
            );
        }
    }

    #[test]
    fn rejects_proton_version_on_other_platforms() {
        let text = file(1, &format!("{WINDOWS_2}proton_version = \"9.0\"\n"));
        assert!(matches!(
            invalid(&text),
            InvalidFile::ProtonVersionNotAllowed {
                report: 0,
                platform: Platform::Windows
            }
        ));
    }

    #[test]
    fn rejects_empty_fields() {
        let text = "app_id = 1\nname = \"  \"\n".to_owned() + WINDOWS_2;
        assert_eq!(
            invalid(&text),
            InvalidFile::EmptyField {
                report: None,
                field: "name"
            }
        );
        let text = file(1, &WINDOWS_2.replace("\"0.1.0\"", "\"\""));
        assert_eq!(
            invalid(&text),
            InvalidFile::EmptyField {
                report: Some(0),
                field: "reborn_version"
            }
        );
    }

    #[test]
    fn rejects_bad_toml_and_unknown_keys() {
        for text in [
            "app_id = ".to_owned(),
            file(1, &WINDOWS_2.replace("tier", "level")),
            file(1, &format!("{WINDOWS_2}runtime = \"native\"\n")),
            WINDOWS_2.to_owned(),
            file(1, &WINDOWS_2.replace("\"2026-11-01\"", "2026-11-01")),
            "app_id = -5\nname = \"x\"\n".to_owned() + WINDOWS_2,
        ] {
            assert!(
                matches!(invalid(&text), InvalidFile::Toml(_)),
                "should fail as TOML: {text}"
            );
        }
    }

    #[test]
    fn report_numbers_in_messages_start_at_one() {
        let text = file(
            1,
            &format!("{WINDOWS_2}{}", WINDOWS_2.replace("tier = 2", "tier = 9")),
        );
        let error = invalid(&text);
        assert_eq!(error.to_string(), "report 2: tier 9 is out of range 0-5");
    }

    #[test]
    fn tier_and_platform_round_trip() {
        for tier in Tier::ALL {
            assert_eq!(Tier::from_number(i64::from(tier.number())), Some(tier));
        }
        for platform in Platform::ALL {
            assert_eq!(platform.as_str().parse::<Platform>(), Ok(platform));
        }
        assert_eq!(
            ReportDate::parse("2026-11-20").unwrap().to_string(),
            "2026-11-20"
        );
    }
}
