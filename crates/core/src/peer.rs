//! Identifies the process on the other side of a local connection and checks
//! its code signature. Used to make sure that only the genuine Kryptos native
//! host talks to the app, and only when a genuine browser launched it.
//!
//! On macOS the checks are backed by the kernel's code-signing enforcement
//! (`SecCodeCopyGuestWithAttributes` validates the *running* code, not just the
//! file on disk). On Linux they are path-based and therefore weaker.

use std::path::PathBuf;

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
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    {
        let _ = pid;
        None
    }
}

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
