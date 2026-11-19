//! Checks that Steam's text KeyValues files parse and round-trip, without
//! printing anything from their contents. The output is safe to share in
//! bug reports.
//!
//! ```sh
//! cargo run -p reborn-formats --example check -- <file>...
//! ```
//!
//! For each file it reports whether parsing succeeded, whether writing the
//! parsed document back reproduces the file byte for byte, and, for
//! `appmanifest_*.acf` and `libraryfolders.vdf`, whether the typed reader
//! accepts it.

use std::path::Path;
use std::process::ExitCode;

use reborn_formats::{AppManifest, LibraryFolders, vdf};

fn main() -> ExitCode {
    let paths: Vec<_> = std::env::args_os().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: check <file>...");
        return ExitCode::FAILURE;
    }

    let mut failures = 0;
    for path in &paths {
        let path = Path::new(path);
        match check(path) {
            Ok(summary) => println!("ok     {}  {summary}", path.display()),
            Err(message) => {
                failures += 1;
                println!("FAILED {}  {message}", path.display());
            }
        }
    }

    println!("{} checked, {failures} failed", paths.len());
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn check(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    let document = vdf::parse(&text).map_err(|e| format!("parse error: {e}"))?;

    let written = vdf::write(&document);
    let round_trip = if written == text {
        "byte-identical"
    } else if vdf::parse(&written).as_ref() == Ok(&document) {
        "equal after rewrite (formatting differs)"
    } else {
        return Err("rewritten document differs".to_owned());
    };

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let typed = if name.starts_with("appmanifest_") {
        AppManifest::from_vdf(&document)
            .map(|_| ", AppManifest ok")
            .map_err(|e| format!("AppManifest error: {e}"))?
    } else if name.eq_ignore_ascii_case("libraryfolders.vdf") {
        LibraryFolders::from_vdf(&document)
            .map(|_| ", LibraryFolders ok")
            .map_err(|e| format!("LibraryFolders error: {e}"))?
    } else {
        ""
    };

    Ok(format!("{round_trip}{typed}"))
}
