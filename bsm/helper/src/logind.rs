//! logind access through libsystemd's `sd-bus`.
//!
//! The helper must stay single threaded so it can `fork` with PAM state in
//! the child. `sd-bus` is thread free and synchronous, which is why it is
//! used here instead of a D-Bus crate with its own executor thread. Calls
//! block, so the helper runs them in a short-lived forked child.

use std::ffi::{CStr, CString, c_char, c_int, c_void};

#[repr(C)]
struct SdBusError {
    name: *const c_char,
    message: *const c_char,
    need_free: c_int,
}

#[link(name = "systemd")]
unsafe extern "C" {
    fn sd_bus_open_system(ret: *mut *mut c_void) -> c_int;
    fn sd_bus_flush_close_unref(bus: *mut c_void) -> *mut c_void;
    fn sd_bus_call_method(
        bus: *mut c_void,
        destination: *const c_char,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
        error: *mut SdBusError,
        reply: *mut *mut c_void,
        types: *const c_char,
        ...
    ) -> c_int;
    fn sd_bus_error_free(e: *mut SdBusError);
    fn sd_bus_path_encode(prefix: *const c_char, external_id: *const c_char, ret: *mut *mut c_char) -> c_int;
}

/// A logind session id is short and alphanumeric (`3`, `c1`). Anything else is
/// not passed to the bus.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Call `org.freedesktop.login1.Session.SetLockedHint(locked)` on the system bus.
/// Runs as root in the helper's child; logind accepts root for any session.
pub fn set_locked_hint(logind_session: &str, locked: bool) -> Result<(), String> {
    if !valid_session_id(logind_session) {
        return Err(format!("not a logind session id: {logind_session:?}"));
    }
    let id = CString::new(logind_session).map_err(|e| e.to_string())?;
    let prefix = CString::new("/org/freedesktop/login1/session").unwrap();
    let mut path: *mut c_char = std::ptr::null_mut();
    let r = unsafe { sd_bus_path_encode(prefix.as_ptr(), id.as_ptr(), &mut path) };
    if r < 0 || path.is_null() {
        return Err(format!("sd_bus_path_encode failed: {}", errno_text(r)));
    }
    let mut bus: *mut c_void = std::ptr::null_mut();
    let r = unsafe { sd_bus_open_system(&mut bus) };
    if r < 0 {
        unsafe { libc::free(path as *mut c_void) };
        return Err(format!("cannot connect to the system bus: {}", errno_text(r)));
    }
    let dest = CString::new("org.freedesktop.login1").unwrap();
    let iface = CString::new("org.freedesktop.login1.Session").unwrap();
    let member = CString::new("SetLockedHint").unwrap();
    let types = CString::new("b").unwrap();
    let mut error = SdBusError { name: std::ptr::null(), message: std::ptr::null(), need_free: 0 };
    let r = unsafe {
        sd_bus_call_method(
            bus,
            dest.as_ptr(),
            path,
            iface.as_ptr(),
            member.as_ptr(),
            &mut error,
            std::ptr::null_mut(),
            types.as_ptr(),
            locked as c_int,
        )
    };
    let result = if r < 0 {
        let detail = if error.message.is_null() {
            errno_text(r)
        } else {
            unsafe { CStr::from_ptr(error.message) }.to_string_lossy().into_owned()
        };
        Err(format!("SetLockedHint failed: {detail}"))
    } else {
        Ok(())
    };
    unsafe {
        sd_bus_error_free(&mut error);
        sd_bus_flush_close_unref(bus);
        libc::free(path as *mut c_void);
    }
    result
}

fn errno_text(r: c_int) -> String {
    std::io::Error::from_raw_os_error(-r).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_restricted_to_alphanumerics() {
        assert!(valid_session_id("3"));
        assert!(valid_session_id("c12"));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id("../x"));
        assert!(!valid_session_id("a b"));
        assert!(!valid_session_id(&"1".repeat(33)));
    }

    #[test]
    fn invalid_id_is_refused_before_touching_the_bus() {
        assert!(set_locked_hint("a/b", true).is_err());
    }
}
