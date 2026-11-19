//! The current OS user.

/// The string SID (`S-1-5-21-...`) of the user running this process, read
/// from the process token. `rebornd` limits its named pipe to this SID
/// (ADR 0014).
#[cfg(windows)]
pub fn windows_sid() -> std::io::Result<String> {
    windows::current_sid()
}

/// Windows API calls. Needs human review (AGENTS.md § 7).
///
/// Steps, as documented by Microsoft: `OpenProcessToken` with
/// `TOKEN_QUERY`, `GetTokenInformation(TokenUser)` (first for the size,
/// then for the data), `ConvertSidToStringSidW`, then `LocalFree` for the
/// string and `CloseHandle` for the token.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use std::io;
    use std::ptr;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// Closes the token handle when dropped.
    struct Token(HANDLE);

    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful OpenProcessToken and
            // is closed exactly once, here.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// Frees a string allocated by Windows with `LocalAlloc` when dropped.
    struct LocalString(*mut u16);

    impl Drop for LocalString {
        fn drop(&mut self) {
            // SAFETY: the pointer came from ConvertSidToStringSidW, which
            // documents that it must be freed with LocalFree, once.
            unsafe {
                LocalFree(self.0.cast());
            }
        }
    }

    pub fn current_sid() -> io::Result<String> {
        let mut handle: HANDLE = ptr::null_mut();
        // SAFETY: GetCurrentProcess returns a pseudo handle that needs no
        // closing; `handle` is a valid place for the output handle.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Token(handle);

        let mut needed = 0u32;
        // SAFETY: a null buffer of length 0 is allowed; the call fails and
        // writes the required size to `needed`.
        unsafe {
            GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        // u64 elements keep the buffer aligned for TOKEN_USER (which holds
        // a pointer).
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        let length = u32::try_from(buffer.len() * 8).map_err(io::Error::other)?;
        // SAFETY: `buffer` is writable, 8-byte aligned and `length` bytes long.
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: on success the buffer starts with a TOKEN_USER; its SID
        // pointer points into `buffer`, which lives until the end of this
        // function.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };

        let mut wide: *mut u16 = ptr::null_mut();
        // SAFETY: `sid` is a valid SID (see above); `wide` is a valid place
        // for the output pointer.
        if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let wide = LocalString(wide);
        let mut len = 0;
        // SAFETY: ConvertSidToStringSidW returns a NUL-terminated UTF-16
        // string; reading up to and including the NUL stays inside it.
        while unsafe { *wide.0.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the first `len` elements were just read and are initialized.
        let units = unsafe { std::slice::from_raw_parts(wide.0, len) };
        Ok(String::from_utf16_lossy(units))
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn reads_a_user_sid() {
        let sid = super::windows_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        assert!(
            sid.chars()
                .all(|c| c.is_ascii_digit() || c == '-' || c == 'S')
        );
    }
}
