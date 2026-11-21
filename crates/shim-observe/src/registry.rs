//! The one registry value an observation session changes (ADR 0039):
//! `HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll64`.
//!
//! Uses `RegGetValueW` and `RegSetKeyValueW` as Microsoft documents them.
//! Nothing else in the registry is read or written.
#![allow(unsafe_code)]

use std::io;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegGetValueW, RegSetKeyValueW,
};

use crate::session::ValueStore;

/// The key below `HKEY_CURRENT_USER` (`docs/shim/loading.md`).
const KEY: &str = r"Software\Valve\Steam\ActiveProcess";
/// The value the game reads for the 64-bit DLL.
const VALUE: &str = "SteamClientDll64";

/// A string value below `HKEY_CURRENT_USER`. Outside tests, only
/// [`RegistryStore::steam_client_dll64`] exists.
#[derive(Debug, Clone)]
pub struct RegistryStore {
    key: String,
    value: &'static str,
}

impl RegistryStore {
    /// `HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll64`.
    pub fn steam_client_dll64() -> Self {
        Self {
            key: KEY.to_owned(),
            value: VALUE,
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

impl ValueStore for RegistryStore {
    fn get(&self) -> io::Result<Option<String>> {
        let key = wide(&self.key);
        let value = wide(self.value);
        // Ask for the size first, then read; retry if it grew in between.
        for _ in 0..3 {
            let mut bytes: u32 = 0;
            // SAFETY: `key` and `value` are NUL-terminated UTF-16 strings that
            // live for the call; a null data pointer with a valid size pointer
            // asks only for the size (RegGetValueW documentation).
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut bytes,
                )
            };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if status != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(status as i32));
            }
            let mut buffer = vec![0u16; (bytes as usize).div_ceil(2)];
            let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
            // SAFETY: as above; `buffer` is writable for `size` bytes, and
            // RegGetValueW writes at most `size` bytes and updates `size`.
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            if status == ERROR_MORE_DATA {
                continue;
            }
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if status != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(status as i32));
            }
            buffer.truncate((size as usize) / 2);
            while buffer.last() == Some(&0) {
                buffer.pop();
            }
            return String::from_utf16(&buffer)
                .map(Some)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "value is not UTF-16"));
        }
        Err(io::Error::other(
            "the value kept changing while it was read",
        ))
    }

    fn set(&self, text: &str) -> io::Result<()> {
        let key = wide(&self.key);
        let value = wide(self.value);
        let data = wide(text);
        let bytes = u32::try_from(data.len() * 2)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path too long"))?;
        // SAFETY: `key`, `value` and `data` are NUL-terminated UTF-16 strings
        // that live for the call; `bytes` is the size of `data` in bytes,
        // including the terminating NUL, as REG_SZ requires.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                bytes,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::Registry::RegDeleteKeyW;

    use super::*;

    /// Writes and reads a value in a throwaway key of the current user,
    /// never Valve's, and deletes the key afterwards.
    #[test]
    fn writes_and_reads_a_string_value() -> io::Result<()> {
        let key = format!(r"Software\RebornObserveTest{}", std::process::id());
        let store = RegistryStore {
            key: key.clone(),
            value: VALUE,
        };
        let result = (|| {
            assert_eq!(store.get()?, None, "the key does not exist yet");
            let path = r"C:\Users\x\AppData\Local\Reborn\data\shim-observe\steamclient64.dll";
            store.set(path)?;
            assert_eq!(store.get()?.as_deref(), Some(path));
            store.set("short")?;
            assert_eq!(store.get()?.as_deref(), Some("short"));
            Ok(())
        })();
        let wide_key = wide(&key);
        // SAFETY: `wide_key` is a NUL-terminated UTF-16 string that lives for
        // the call; the key has no subkeys, as RegDeleteKeyW requires.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide_key.as_ptr()) };
        result
    }
}
