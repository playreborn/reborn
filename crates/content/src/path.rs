//! Turning untrusted manifest file names into safe relative install paths.
//!
//! A manifest comes from the network. A name like `..\..\Windows\evil.dll`
//! or `C:\evil` must never lead to a write outside the install directory.
//! [`install_path`] only accepts plain relative paths and rejects everything
//! that could escape, alias or behave differently between Windows and Linux.

use std::path::{Path, PathBuf};

/// Why a name was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PathError {
    /// The name is empty.
    #[error("empty path")]
    Empty,
    /// The name starts with a separator, so it is absolute.
    #[error("absolute path `{0}`")]
    Absolute(String),
    /// A component is empty, `.` or `..`.
    #[error("path `{0}` contains an empty, `.` or `..` component")]
    Traversal(String),
    /// A component contains a character that is unsafe in file names:
    /// `:` (drive letters, NTFS streams), `<>"|?*`, or control characters.
    #[error("path `{0}` contains an unsafe character")]
    UnsafeCharacter(String),
    /// A component ends with `.` or a space, which Windows silently strips.
    #[error("path `{0}` has a component ending with `.` or a space")]
    TrailingDotOrSpace(String),
    /// A component is a reserved Windows device name such as `CON` or `NUL`.
    #[error("path `{0}` contains a reserved device name")]
    ReservedName(String),
    /// A symlink target points outside the install directory.
    #[error("link target `{0}` points outside the install directory")]
    EscapesInstallDir(String),
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Converts a manifest name (`\` or `/` separated) into a relative path, or
/// rejects it. The result never contains `..`, never is absolute and has the
/// same meaning on Windows and Linux.
pub fn install_path(name: &str) -> Result<PathBuf, PathError> {
    if name.is_empty() {
        return Err(PathError::Empty);
    }
    if name.starts_with(['\\', '/']) {
        return Err(PathError::Absolute(name.to_owned()));
    }
    let mut path = PathBuf::new();
    for component in name.split(['\\', '/']) {
        check_component(name, component)?;
        path.push(component);
    }
    Ok(path)
}

/// Checks the target of a symlink from a manifest. `link` is the link's
/// own path as returned by [`install_path`]; `target` is untrusted.
///
/// Only relative targets are accepted, and following them from the link's
/// directory must never leave the install directory (`..` is allowed as
/// long as it stays inside). Returns the target as a relative path for
/// creating the link.
pub fn link_target(link: &Path, target: &str) -> Result<PathBuf, PathError> {
    if target.is_empty() {
        return Err(PathError::Empty);
    }
    if target.starts_with(['\\', '/']) || target.contains(':') {
        return Err(PathError::Absolute(target.to_owned()));
    }
    // Depth of the link's directory below the install root.
    let mut depth = link.components().count().saturating_sub(1);
    let mut path = PathBuf::new();
    for component in target.split(['\\', '/']) {
        match component {
            "" => return Err(PathError::Traversal(target.to_owned())),
            "." => {}
            ".." => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| PathError::EscapesInstallDir(target.to_owned()))?;
                path.push("..");
            }
            normal => {
                check_component(target, normal)?;
                depth += 1;
                path.push(normal);
            }
        }
    }
    Ok(path)
}

fn check_component(name: &str, component: &str) -> Result<(), PathError> {
    if component.is_empty() || component == "." || component == ".." {
        return Err(PathError::Traversal(name.to_owned()));
    }
    if component
        .chars()
        .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(PathError::UnsafeCharacter(name.to_owned()));
    }
    if component.ends_with(['.', ' ']) {
        return Err(PathError::TrailingDotOrSpace(name.to_owned()));
    }
    let stem = component.split('.').next().unwrap_or(component);
    if RESERVED
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return Err(PathError::ReservedName(name.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn accepts_normal_relative_paths() {
        assert_eq!(install_path("game.exe").unwrap(), Path::new("game.exe"));
        assert_eq!(
            install_path("data\\maps\\level 1.bsp").unwrap(),
            Path::new("data").join("maps").join("level 1.bsp")
        );
        assert_eq!(
            install_path("bin/linux64/lib.so").unwrap(),
            Path::new("bin").join("linux64").join("lib.so")
        );
        assert!(install_path(".hidden\\file").is_ok());
        assert!(install_path("Ünïcödé\\ファイル.txt").is_ok());
        assert!(install_path("console.log").is_ok());
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        for name in [
            "..\\evil.dll",
            "a\\..\\..\\b",
            "a\\.\\b",
            "a\\\\b",
            "a\\",
            "\\Windows\\x",
            "/etc/passwd",
        ] {
            assert!(install_path(name).is_err(), "{name}");
        }
        assert_eq!(install_path(""), Err(PathError::Empty));
    }

    #[test]
    fn accepts_link_targets_inside_the_install_dir() {
        let link = install_path("bin\\lib\\libfoo.so").unwrap();
        assert_eq!(
            link_target(&link, "libfoo.so.1").unwrap(),
            Path::new("libfoo.so.1")
        );
        assert_eq!(
            link_target(&link, "../../data/x").unwrap(),
            Path::new("..").join("..").join("data").join("x")
        );
        assert_eq!(
            link_target(&link, "./a/../b").unwrap(),
            Path::new("a").join("..").join("b")
        );
    }

    #[test]
    fn rejects_link_targets_that_escape() {
        let link = install_path("bin\\run.sh").unwrap();
        for target in [
            "../../etc/passwd",
            "../..",
            "/usr/bin/x",
            "C:\\x",
            "\\\\server\\share",
            "a//b",
            "",
        ] {
            assert!(link_target(&link, target).is_err(), "{target}");
        }
        assert!(matches!(
            link_target(&link, "../../x"),
            Err(PathError::EscapesInstallDir(_))
        ));
        let top = install_path("run.sh").unwrap();
        assert!(link_target(&top, "../x").is_err());
    }

    #[test]
    fn rejects_windows_specific_tricks() {
        assert!(matches!(
            install_path("C:\\evil"),
            Err(PathError::UnsafeCharacter(_))
        ));
        assert!(matches!(
            install_path("file.txt:stream"),
            Err(PathError::UnsafeCharacter(_))
        ));
        assert!(matches!(
            install_path("a\u{0}b"),
            Err(PathError::UnsafeCharacter(_))
        ));
        assert!(matches!(
            install_path("dir.\\x"),
            Err(PathError::TrailingDotOrSpace(_))
        ));
        assert!(matches!(
            install_path("x\\file "),
            Err(PathError::TrailingDotOrSpace(_))
        ));
        assert!(matches!(
            install_path("NUL"),
            Err(PathError::ReservedName(_))
        ));
        assert!(matches!(
            install_path("dir\\com1.txt"),
            Err(PathError::ReservedName(_))
        ));
    }
}
