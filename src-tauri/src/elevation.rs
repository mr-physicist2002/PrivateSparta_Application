//! Admin-rights detection and self-relaunch for TUN mode. TUN needs to
//! create a network adapter and rewrite the routing table, which Windows
//! only allows elevated — the UI explains this before asking.

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // SAFETY: standard token-query sequence; handles are closed on all paths.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Relaunch this executable elevated (UAC prompt) and report success. The
/// caller exits the current instance only if the new one actually started.
#[cfg(windows)]
pub fn relaunch_elevated() -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let to_wide = |s: &std::ffi::OsStr| -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    };
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let path = to_wide(exe.as_os_str());
    // SAFETY: null-terminated wide strings; ShellExecuteW copies them.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Per the docs, values > 32 mean success.
    result as isize > 32
}

#[cfg(not(windows))]
pub fn relaunch_elevated() -> bool {
    false
}
