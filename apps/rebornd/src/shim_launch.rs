//! Shim mode around a game launch (ADR 0048).
//!
//! Off unless the environment variable `REBORN_SHIM_MODE` is `1` when
//! `rebornd` starts: ADR 0048 keeps shim mode for planned experiments
//! until the shim answers enough calls for games to run. The shim DLL is
//! `steamclient64.dll` in the `shim` folder of Reborn's data folder.

#[cfg(windows)]
use tracing::{info, warn};

/// Whether launches run in shim mode.
pub fn enabled() -> bool {
    std::env::var("REBORN_SHIM_MODE").as_deref() == Ok("1")
}

/// Points `ActiveProcess` at the shim right before a launch.
#[cfg(windows)]
pub fn enter() -> Result<(), String> {
    use reborn_platform::shim_mode::{RegistryValues, official_client_running};
    let (mode, shim) = paths().map_err(|error| error.to_string())?;
    mode.enter(
        &RegistryValues::active_process(),
        &shim,
        std::process::id(),
        official_client_running(),
    )
    .map_err(|error| error.to_string())?;
    info!(shim = %shim.display(), "shim mode: ActiveProcess points to the shim");
    Ok(())
}

/// Shim mode is decided for Windows only so far (ADR 0048).
#[cfg(not(windows))]
pub fn enter() -> Result<(), String> {
    Err("shim mode is not available on this system yet (ADR 0048)".to_owned())
}

/// Puts `ActiveProcess` back after a launch, and at daemon start after an
/// interrupted run. Logs instead of failing.
pub fn leave() {
    #[cfg(windows)]
    {
        use reborn_platform::shim_mode::RegistryValues;
        match paths() {
            Ok((mode, _)) => match mode.leave(&RegistryValues::active_process()) {
                Ok(true) => info!("shim mode: ActiveProcess restored"),
                Ok(false) => {}
                Err(error) => warn!(%error, "shim mode: restoring ActiveProcess failed"),
            },
            Err(error) => warn!(%error, "shim mode: no data folder"),
        }
    }
}

#[cfg(windows)]
fn paths() -> Result<
    (reborn_platform::shim_mode::ShimMode, std::path::PathBuf),
    reborn_platform::settings::SettingsError,
> {
    let dir = reborn_platform::settings::data_dir()?.join("shim");
    let shim = dir.join("steamclient64.dll");
    Ok((reborn_platform::shim_mode::ShimMode::new(dir), shim))
}
