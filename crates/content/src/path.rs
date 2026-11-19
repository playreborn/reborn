//! Turning untrusted manifest file names into safe relative install paths.
//!
//! A manifest comes from the network. A name like `..\..\Windows\evil.dll`
//! or `C:\evil` must never lead to a write outside the install directory.
//! [`install_path`] only accepts plain relative paths and rejects everything
//! that could escape, alias or behave differently between Windows and Linux.

use std::path::PathBuf;

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
