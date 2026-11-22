//! The battery level for `GetCurrentBatteryPower` (`docs/shim/meanings.md`,
//! experiment 10): 255 on AC power, otherwise 0 to 100 percent, as Valve's
//! public `ISteamUtils` page describes the result.
//!
//! On Windows it comes from `GetSystemPowerStatus` as Microsoft documents
//! it: `ACLineStatus` 1 means AC power, `BatteryLifePercent` is 0 to 100 or
//! 255 when unknown.

/// 255 on AC power or when unknown, otherwise the battery level in percent.
#[cfg(windows)]
pub fn battery_power() -> u8 {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    // SAFETY: an all-zero SYSTEM_POWER_STATUS is valid; the call fills it.
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: `status` is a valid, writable SYSTEM_POWER_STATUS.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return 255;
    }
    from_status(status.ACLineStatus, status.BatteryLifePercent)
}

/// The shim's Linux side is not decided yet (ADR 0048).
#[cfg(not(windows))]
pub fn battery_power() -> u8 {
    crate::not_implemented("GetCurrentBatteryPower outside Windows")
}

/// The answer from Windows' AC line status and battery percentage.
#[cfg_attr(not(windows), allow(dead_code))]
fn from_status(ac_line: u8, percent: u8) -> u8 {
    if ac_line == 1 || percent > 100 {
        255
    } else {
        percent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_power_and_unknown_give_255() {
        assert_eq!(from_status(1, 80), 255, "on AC power");
        assert_eq!(from_status(0, 42), 42, "on battery");
        assert_eq!(from_status(0, 255), 255, "battery level unknown");
        assert_eq!(from_status(255, 255), 255, "status unknown");
    }
}
