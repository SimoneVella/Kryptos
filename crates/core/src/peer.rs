//! Identifies the process on the other side of a local connection and checks
//! its code signature. Used to make sure that only the genuine Kryptos native
//! host talks to the app, and only when a genuine browser launched it.
//!
//! On macOS the checks are backed by the kernel's code-signing enforcement
//! (`SecCodeCopyGuestWithAttributes` validates the *running* code, not just the
//! file on disk). On Windows `WinVerifyTrust` checks Authenticode signatures on
//! the executable file. On Linux they are path-based and therefore weaker.

use std::path::{Path, PathBuf};

/// PID of the process connected to a Unix-domain socket.
#[cfg(unix)]
pub fn socket_peer_pid(fd: std::os::fd::RawFd) -> Option<i32> {
    #[cfg(target_os = "macos")]
    unsafe {
        const SOL_LOCAL: libc::c_int = 0;
        const LOCAL_PEERPID: libc::c_int = 2;
        let mut pid: libc::pid_t = 0;
        let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
        let r = libc::getsockopt(fd, SOL_LOCAL, LOCAL_PEERPID, (&mut pid as *mut libc::pid_t).cast(), &mut len);
        (r == 0 && pid > 0).then_some(pid)
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        let mut cred: libc::ucred = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let r = libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len);
        (r == 0 && cred.pid > 0).then_some(cred.pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    {
        let _ = fd;
        None
    }
}

/// True when `pid` belongs to the exact executable file at `expected`.
pub fn same_exe(pid: i32, expected: &Path) -> bool {
    let (Some(actual), Ok(expected)) = (exe_path(pid).and_then(|p| p.canonicalize().ok()), expected.canonicalize()) else {
        return false;
    };
    actual == expected
}

/// Parent process ID of the current process.
pub fn parent_pid() -> Option<i32> {
    #[cfg(unix)]
    {
        let ppid = unsafe { libc::getppid() };
        (ppid > 0).then_some(ppid)
    }
    #[cfg(windows)]
    {
        win::parent_pid()
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Executable path of a running process.
pub fn exe_path(pid: i32) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    unsafe {
        extern "C" {
            fn proc_pidpath(pid: libc::c_int, buf: *mut libc::c_void, size: u32) -> libc::c_int;
        }
        let mut buf = vec![0u8; 4096];
        let n = proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32);
        if n <= 0 {
            return None;
        }
        buf.truncate(n as usize);
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(buf)))
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }
    #[cfg(windows)]
    {
        win::exe_path(pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android", windows)))]
    {
        let _ = pid;
        None
    }
}

#[cfg(windows)]
mod win {
    use core::ffi::c_void;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::io::RawHandle;
    use std::path::{Path, PathBuf};

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const MAX_PATH: usize = 260;

    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; MAX_PATH],
    }

    #[repr(C)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    struct WinTrustFileInfo {
        cb_struct: u32,
        pcwsz_file_path: *const u16,
        h_file: *mut c_void,
        pg_known_subject: *const Guid,
    }

    #[repr(C)]
    struct WinTrustData {
        cb_struct: u32,
        p_policy_callback_data: *mut c_void,
        p_sip_client_data: *mut c_void,
        dw_ui_choice: u32,
        fdw_revocation_checks: u32,
        dw_union_choice: u32,
        p_file: *mut WinTrustFileInfo,
        dw_state_action: u32,
        h_wvt_state_data: *mut c_void,
        pwsz_url_reference: *const u16,
        dw_prov_flags: u32,
        dw_ui_context: u32,
        p_signature_settings: *mut c_void,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut c_void;
        fn CloseHandle(hObject: *mut c_void) -> i32;
        fn QueryFullProcessImageNameW(hProcess: *mut c_void, dwFlags: u32, lpExeName: *mut u16, lpdwSize: *mut u32) -> i32;
        fn GetCurrentProcessId() -> u32;
        fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> *mut c_void;
        fn Process32FirstW(hSnapshot: *mut c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(hSnapshot: *mut c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn GetNamedPipeClientProcessId(Pipe: *mut c_void, ClientProcessId: *mut u32) -> i32;
        fn GetNamedPipeServerProcessId(Pipe: *mut c_void, ServerProcessId: *mut u32) -> i32;
    }

    #[link(name = "wintrust")]
    extern "system" {
        fn WinVerifyTrust(hwnd: isize, pgActionID: *const Guid, pWVTData: *mut WinTrustData) -> i32;
    }

    pub fn exe_path(pid: i32) -> Option<PathBuf> {
        let Ok(pid) = u32::try_from(pid) else { return None };
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 4096];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok == 0 || len == 0 {
                return None;
            }
            Some(PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize])))
        }
    }

    pub fn parent_pid() -> Option<i32> {
        unsafe {
            let me = GetCurrentProcessId();
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || snap as isize == -1 {
                return None;
            }
            let mut entry: ProcessEntry32W = std::mem::zeroed();
            entry.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
            let mut out = None;
            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    if entry.th32_process_id == me {
                        out = i32::try_from(entry.th32_parent_process_id).ok().filter(|&p| p > 0);
                        break;
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
            out
        }
    }

    pub fn pipe_client_pid(handle: RawHandle) -> Option<i32> {
        let mut pid = 0u32;
        let ok = unsafe { GetNamedPipeClientProcessId(handle, &mut pid) };
        (ok != 0 && pid > 0).then(|| i32::try_from(pid).ok()).flatten()
    }

    pub fn pipe_server_pid(handle: RawHandle) -> Option<i32> {
        let mut pid = 0u32;
        let ok = unsafe { GetNamedPipeServerProcessId(handle, &mut pid) };
        (ok != 0 && pid > 0).then(|| i32::try_from(pid).ok()).flatten()
    }

    /// True if the executable at `path` has a valid Windows Authenticode signature
    /// (offline cache-only revocation check so no network request is ever triggered).
    pub fn valid_authenticode(path: &Path) -> bool {
        // WINTRUST_ACTION_GENERIC_VERIFY_V2 {00AAC56B-CD44-11d0-8CC2-00C04FC295EE}
        const ACTION: Guid = Guid {
            data1: 0x00AA_C56B,
            data2: 0xCD44,
            data3: 0x11d0,
            data4: [0x8C, 0xC2, 0x00, 0xC0, 0x4F, 0xC2, 0x95, 0xEE],
        };
        const WTD_UI_NONE: u32 = 2;
        const WTD_REVOKE_NONE: u32 = 0;
        const WTD_CHOICE_FILE: u32 = 1;
        const WTD_STATEACTION_VERIFY: u32 = 1;
        const WTD_STATEACTION_CLOSE: u32 = 2;
        const WTD_CACHE_ONLY_URL_RETRIEVAL: u32 = 0x0000_0080;

        let path_w: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut file = WinTrustFileInfo {
            cb_struct: std::mem::size_of::<WinTrustFileInfo>() as u32,
            pcwsz_file_path: path_w.as_ptr(),
            h_file: std::ptr::null_mut(),
            pg_known_subject: std::ptr::null(),
        };
        let mut data = WinTrustData {
            cb_struct: std::mem::size_of::<WinTrustData>() as u32,
            p_policy_callback_data: std::ptr::null_mut(),
            p_sip_client_data: std::ptr::null_mut(),
            dw_ui_choice: WTD_UI_NONE,
            fdw_revocation_checks: WTD_REVOKE_NONE,
            dw_union_choice: WTD_CHOICE_FILE,
            p_file: &mut file,
            dw_state_action: WTD_STATEACTION_VERIFY,
            h_wvt_state_data: std::ptr::null_mut(),
            pwsz_url_reference: std::ptr::null(),
            dw_prov_flags: WTD_CACHE_ONLY_URL_RETRIEVAL,
            dw_ui_context: 0,
            p_signature_settings: std::ptr::null_mut(),
        };
        let status = unsafe { WinVerifyTrust(-1, &ACTION, &mut data) };
        data.dw_state_action = WTD_STATEACTION_CLOSE;
        unsafe { WinVerifyTrust(-1, &ACTION, &mut data) };
        status == 0
    }
}

#[cfg(windows)]
pub use win::{pipe_client_pid, pipe_server_pid, valid_authenticode};

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use security_framework::os::macos::code_signing::{Flags, GuestAttributes, SecCode, SecRequirement};

    #[link(name = "Security", kind = "framework")]
    extern "C" {
        fn SecCodeCopySigningInformation(code: *const std::ffi::c_void, flags: u32, info: *mut CFDictionaryRef) -> i32;
        static kSecCodeInfoTeamIdentifier: CFStringRef;
    }
    const K_SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;

    fn code_for_pid(pid: i32) -> Option<SecCode> {
        let mut attrs = GuestAttributes::new();
        attrs.set_pid(pid);
        SecCode::copy_guest_with_attribues(None, &attrs, Flags::NONE).ok()
    }

    /// True if the running process `pid` has a valid signature satisfying `requirement`
    /// (code-signing requirement language, e.g. `anchor apple generic and ...`).
    pub fn satisfies(pid: i32, requirement: &str) -> bool {
        let (Some(code), Ok(req)) = (code_for_pid(pid), requirement.parse::<SecRequirement>()) else {
            return false;
        };
        code.check_validity(Flags::NONE, &req).is_ok()
    }

    /// True if the running process has an intact signature (ad-hoc allowed).
    pub fn valid_signature(pid: i32) -> bool {
        satisfies(pid, "always")
    }

    /// Apple Developer Team ID this process is signed with, if any.
    pub fn own_team_id() -> Option<String> {
        let code = SecCode::for_self(Flags::NONE).ok()?;
        unsafe {
            let mut info: CFDictionaryRef = std::ptr::null();
            if SecCodeCopySigningInformation(code.as_CFTypeRef(), K_SEC_CS_SIGNING_INFORMATION, &mut info) != 0 || info.is_null() {
                return None;
            }
            let dict: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(info);
            let key = CFString::wrap_under_get_rule(kSecCodeInfoTeamIdentifier);
            dict.find(&key).and_then(|v| v.downcast::<CFString>()).map(|s| s.to_string())
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::{own_team_id, satisfies, valid_signature};

/// Browsers allowed to launch the native host, by Apple Developer Team ID.
/// Chrome's ID was read from the installed app (`codesign -dv`); add others the same way.
#[cfg(target_os = "macos")]
pub const TRUSTED_BROWSER_TEAMS: &[(&str, &str)] = &[
    ("Google Chrome", "EQHXZ8M8AV"),
    ("Firefox", "43AQ936H96"),
    ("Microsoft Edge", "UBF8T346G9"),
    ("Brave", "KL8N8XSYF4"),
    ("Arc", "S6N382Y83G"),
    ("Vivaldi", "4XF3XNRN6Y"),
];

/// Requirement matching any trusted browser: a Developer ID (or Apple) signature
/// whose leaf certificate belongs to one of the teams above.
#[cfg(target_os = "macos")]
pub fn trusted_browser_requirement() -> String {
    let teams: Vec<String> =
        TRUSTED_BROWSER_TEAMS.iter().map(|(_, t)| format!("certificate leaf[subject.OU] = \"{t}\"")).collect();
    format!("anchor apple generic and ({})", teams.join(" or "))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn identifies_own_process() {
        let me = std::process::id() as i32;
        assert_eq!(exe_path(me).unwrap(), std::env::current_exe().unwrap().canonicalize().unwrap());
        // Test binaries are ad-hoc signed by the linker on Apple Silicon.
        assert!(valid_signature(me));
        assert!(!satisfies(me, &trusted_browser_requirement()));
    }

    /// `KRYPTOS_BROWSER_PID=$(pgrep -x "Google Chrome") cargo test -p kryptos-core -- --ignored`
    #[test]
    #[ignore]
    fn real_browser_is_trusted() {
        let pid: i32 = std::env::var("KRYPTOS_BROWSER_PID").unwrap().parse().unwrap();
        assert!(satisfies(pid, &trusted_browser_requirement()));
    }

    #[test]
    fn socket_peer_is_us() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        use std::os::fd::AsRawFd;
        assert_eq!(socket_peer_pid(a.as_raw_fd()), Some(std::process::id() as i32));
    }
}
