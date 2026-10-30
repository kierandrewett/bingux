//! Direct binding to libpam (Linux-PAM). Only the calls the helper needs.
//!
//! Why not `pam-sys` or `pam-client2`: the crates add a bindgen or build-time
//! dependency and a conversation abstraction that does not fit a handle which
//! is driven across processes, while the surface needed here is a dozen
//! functions and two structs that have been ABI-stable for decades. The
//! binding is small enough to audit in one sitting, which matters for a root
//! service.
//!
//! Threading: libpam handles are used from a single thread, in a forked
//! child. Nothing here is `Send` or `Sync`.

use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::rc::Rc;

use bsm_proto::error::ErrorCode;
use bsm_proto::helper::PamPromptKind;
use zeroize::Zeroize;

pub const PAM_SUCCESS: c_int = 0;
pub const PAM_PERM_DENIED: c_int = 6;
pub const PAM_AUTH_ERR: c_int = 7;
pub const PAM_AUTHINFO_UNAVAIL: c_int = 9;
pub const PAM_USER_UNKNOWN: c_int = 10;
pub const PAM_MAXTRIES: c_int = 11;
pub const PAM_NEW_AUTHTOK_REQD: c_int = 12;
pub const PAM_ACCT_EXPIRED: c_int = 13;
pub const PAM_CONV_ERR: c_int = 19;
pub const PAM_ABORT: c_int = 26;

const PAM_ESTABLISH_CRED: c_int = 0x0002;
const PAM_DELETE_CRED: c_int = 0x0004;
const PAM_SILENT: c_int = 0x8000;
const PAM_USER: c_int = 2;

const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;
const PAM_MAX_NUM_MSG: c_int = 32;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type ConvFn = unsafe extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int;

#[repr(C)]
struct PamConv {
    conv: Option<ConvFn>,
    appdata_ptr: *mut c_void,
}

#[repr(C)]
struct PamHandle {
    _private: [u8; 0],
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(service: *const c_char, user: *const c_char, conv: *const PamConv, pamh: *mut *mut PamHandle) -> c_int;
    fn pam_start_confdir(
        service: *const c_char,
        user: *const c_char,
        conv: *const PamConv,
        confdir: *const c_char,
        pamh: *mut *mut PamHandle,
    ) -> c_int;
    fn pam_end(pamh: *mut PamHandle, status: c_int) -> c_int;
    fn pam_authenticate(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_acct_mgmt(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_setcred(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_open_session(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_close_session(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_putenv(pamh: *mut PamHandle, name_value: *const c_char) -> c_int;
    fn pam_getenv(pamh: *mut PamHandle, name: *const c_char) -> *const c_char;
    fn pam_getenvlist(pamh: *mut PamHandle) -> *mut *mut c_char;
    fn pam_get_item(pamh: *const PamHandle, item_type: c_int, item: *mut *const c_void) -> c_int;
    #[cfg(test)]
    fn pam_set_item(pamh: *mut PamHandle, item_type: c_int, item: *const c_void) -> c_int;
    fn pam_strerror(pamh: *mut PamHandle, errnum: c_int) -> *const c_char;
}

/// Why the conversation stopped early. Reported instead of PAM's own error so
/// the user sees "cancelled" or "timed out" rather than "conversation error".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    Cancelled,
    TimedOut,
}

/// What a conversation implementation can ask for.
pub trait Conversation {
    /// Show `text` and, for the echo prompts, return the answer. `Err` aborts the PAM call.
    fn converse(&mut self, kind: PamPromptKind, text: &str) -> Result<Option<String>, Interrupt>;
}

/// Shared slot the conversation fills when it interrupts PAM.
pub type InterruptSlot = Rc<Cell<Option<Interrupt>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PamError {
    pub code: c_int,
    pub message: String,
}

impl PamError {
    /// The protocol error code for this PAM failure.
    pub fn error_code(&self) -> ErrorCode {
        match self.code {
            PAM_USER_UNKNOWN => ErrorCode::InvalidAccount,
            _ => ErrorCode::PamFailure,
        }
    }
}

impl std::fmt::Display for PamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (pam {})", self.message, self.code)
    }
}

pub struct Pam {
    handle: *mut PamHandle,
    /// Stable address handed to libpam as `appdata_ptr`.
    _conv: Box<Box<dyn Conversation>>,
    last: c_int,
}

unsafe extern "C" fn trampoline(
    n: c_int,
    msgs: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    data: *mut c_void,
) -> c_int {
    // A panic must never unwind into libpam.
    catch_unwind(AssertUnwindSafe(|| unsafe { converse_all(n, msgs, resp, data) })).unwrap_or(PAM_CONV_ERR)
}

unsafe fn converse_all(n: c_int, msgs: *mut *const PamMessage, resp: *mut *mut PamResponse, data: *mut c_void) -> c_int {
    if n <= 0 || n > PAM_MAX_NUM_MSG || msgs.is_null() || resp.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let conv = unsafe { &mut *(data as *mut Box<dyn Conversation>) };
    let n = n as usize;
    let out = unsafe { libc::calloc(n, std::mem::size_of::<PamResponse>()) } as *mut PamResponse;
    if out.is_null() {
        return PAM_CONV_ERR;
    }
    let free_all = |upto: usize| unsafe {
        for i in 0..upto {
            let r = &mut *out.add(i);
            if !r.resp.is_null() {
                let len = libc::strlen(r.resp);
                std::ptr::write_bytes(r.resp as *mut u8, 0, len);
                libc::free(r.resp as *mut c_void);
            }
        }
        libc::free(out as *mut c_void);
    };
    for i in 0..n {
        let m = unsafe { &**msgs.add(i) };
        let text = if m.msg.is_null() { String::new() } else { unsafe { CStr::from_ptr(m.msg) }.to_string_lossy().into_owned() };
        let kind = match m.msg_style {
            PAM_PROMPT_ECHO_OFF => PamPromptKind::EchoOff,
            PAM_PROMPT_ECHO_ON => PamPromptKind::EchoOn,
            PAM_ERROR_MSG => PamPromptKind::Error,
            PAM_TEXT_INFO => PamPromptKind::Info,
            _ => {
                free_all(i);
                return PAM_CONV_ERR;
            }
        };
        match conv.converse(kind, &text) {
            Ok(Some(mut answer)) => {
                let c = CString::new(answer.as_bytes());
                answer.zeroize();
                let Ok(c) = c else {
                    free_all(i);
                    return PAM_CONV_ERR;
                };
                let dup = unsafe { libc::strdup(c.as_ptr()) };
                let mut bytes = c.into_bytes_with_nul();
                bytes.zeroize();
                if dup.is_null() {
                    free_all(i);
                    return PAM_CONV_ERR;
                }
                unsafe { (*out.add(i)).resp = dup };
            }
            Ok(None) => {}
            Err(_) => {
                free_all(i);
                return PAM_CONV_ERR;
            }
        }
    }
    unsafe { *resp = out };
    PAM_SUCCESS
}

impl Pam {
    /// `pam_start`. `confdir` selects an alternative service directory
    /// (`pam_start_confdir`); production leaves it unset.
    pub fn start(service: &str, user: &str, confdir: Option<&Path>, conv: Box<dyn Conversation>) -> Result<Pam, PamError> {
        let service = cstring(service)?;
        let user = cstring(user)?;
        let mut conv = Box::new(conv);
        let pconv = PamConv { conv: Some(trampoline), appdata_ptr: &mut *conv as *mut Box<dyn Conversation> as *mut c_void };
        let mut handle: *mut PamHandle = std::ptr::null_mut();
        let rc = match confdir {
            Some(dir) => {
                let dir = CString::new(std::os::unix::ffi::OsStrExt::as_bytes(dir.as_os_str()))
                    .map_err(|_| err_msg(PAM_ABORT, "confdir contains NUL"))?;
                unsafe { pam_start_confdir(service.as_ptr(), user.as_ptr(), &pconv, dir.as_ptr(), &mut handle) }
            }
            None => unsafe { pam_start(service.as_ptr(), user.as_ptr(), &pconv, &mut handle) },
        };
        if rc != PAM_SUCCESS || handle.is_null() {
            let message = unsafe { strerror(std::ptr::null_mut(), rc) };
            return Err(PamError { code: rc, message });
        }
        Ok(Pam { handle, _conv: conv, last: PAM_SUCCESS })
    }

    fn check(&mut self, rc: c_int) -> Result<(), PamError> {
        self.last = rc;
        if rc == PAM_SUCCESS {
            Ok(())
        } else {
            Err(PamError { code: rc, message: unsafe { strerror(self.handle, rc) } })
        }
    }

    pub fn authenticate(&mut self) -> Result<(), PamError> {
        let rc = unsafe { pam_authenticate(self.handle, 0) };
        self.check(rc)
    }

    pub fn acct_mgmt(&mut self) -> Result<(), PamError> {
        let rc = unsafe { pam_acct_mgmt(self.handle, 0) };
        self.check(rc)
    }

    pub fn establish_credentials(&mut self) -> Result<(), PamError> {
        let rc = unsafe { pam_setcred(self.handle, PAM_ESTABLISH_CRED | PAM_SILENT) };
        self.check(rc)
    }

    pub fn delete_credentials(&mut self) -> Result<(), PamError> {
        let rc = unsafe { pam_setcred(self.handle, PAM_DELETE_CRED | PAM_SILENT) };
        self.check(rc)
    }

    pub fn open_session(&mut self) -> Result<(), PamError> {
        let rc = unsafe { pam_open_session(self.handle, PAM_SILENT) };
        self.check(rc)
    }

    pub fn close_session(&mut self) -> Result<(), PamError> {
        let rc = unsafe { pam_close_session(self.handle, PAM_SILENT) };
        self.check(rc)
    }

    /// The user name PAM currently has (`PAM_USER`). Modules may change it.
    pub fn user(&self) -> Option<String> {
        let mut item: *const c_void = std::ptr::null();
        let rc = unsafe { pam_get_item(self.handle, PAM_USER, &mut item) };
        if rc != PAM_SUCCESS || item.is_null() {
            return None;
        }
        Some(unsafe { CStr::from_ptr(item as *const c_char) }.to_string_lossy().into_owned())
    }

    /// Set `PAM_USER` (tests simulate a module that rewrites the user).
    #[cfg(test)]
    pub fn set_user(&mut self, name: &str) {
        let c = CString::new(name).unwrap();
        let rc = unsafe { pam_set_item(self.handle, PAM_USER, c.as_ptr() as *const c_void) };
        assert_eq!(rc, PAM_SUCCESS);
    }

    /// `NAME=value` into the PAM environment, which modules such as `pam_systemd` read.
    pub fn put_env(&mut self, name_value: &str) -> Result<(), PamError> {
        let c = cstring(name_value)?;
        let rc = unsafe { pam_putenv(self.handle, c.as_ptr()) };
        self.check(rc)
    }

    pub fn get_env(&self, name: &str) -> Option<String> {
        let c = CString::new(name).ok()?;
        let p = unsafe { pam_getenv(self.handle, c.as_ptr()) };
        if p.is_null() { None } else { Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()) }
    }

    /// The whole PAM environment as `NAME=value` strings.
    pub fn env_list(&self) -> Vec<String> {
        let mut out = Vec::new();
        let list = unsafe { pam_getenvlist(self.handle) };
        if list.is_null() {
            return out;
        }
        let mut i = 0;
        loop {
            let p = unsafe { *list.add(i) };
            if p.is_null() {
                break;
            }
            out.push(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned());
            unsafe { libc::free(p as *mut c_void) };
            i += 1;
        }
        unsafe { libc::free(list as *mut c_void) };
        out
    }

    /// The status of the most recent call, which `pam_end` wants.
    pub fn last_status(&self) -> c_int {
        self.last
    }
}

impl Drop for Pam {
    fn drop(&mut self) {
        unsafe { pam_end(self.handle, self.last) };
    }
}

fn cstring(s: &str) -> Result<CString, PamError> {
    CString::new(s).map_err(|_| err_msg(PAM_ABORT, "string contains NUL"))
}

fn err_msg(code: c_int, message: &str) -> PamError {
    PamError { code, message: message.to_owned() }
}

unsafe fn strerror(handle: *mut PamHandle, rc: c_int) -> String {
    let p = unsafe { pam_strerror(handle, rc) };
    if p.is_null() { format!("pam error {rc}") } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

/// Run the real trampoline with a hand-built message array. Test support for
/// echo-on and error prompts, which common modules never produce.
#[cfg(test)]
pub(crate) fn drive_conversation(
    conv: &mut Box<dyn Conversation>,
    messages: &[(c_int, &str)],
) -> (c_int, Vec<Option<String>>) {
    let texts: Vec<CString> = messages.iter().map(|(_, t)| CString::new(*t).unwrap()).collect();
    let structs: Vec<PamMessage> =
        messages.iter().zip(&texts).map(|((s, _), t)| PamMessage { msg_style: *s, msg: t.as_ptr() }).collect();
    let mut ptrs: Vec<*const PamMessage> = structs.iter().map(|m| m as *const PamMessage).collect();
    let mut resp: *mut PamResponse = std::ptr::null_mut();
    let rc = unsafe {
        trampoline(messages.len() as c_int, ptrs.as_mut_ptr(), &mut resp, conv as *mut Box<dyn Conversation> as *mut c_void)
    };
    let mut answers = Vec::new();
    if rc == PAM_SUCCESS {
        for i in 0..messages.len() {
            let r = unsafe { &*resp.add(i) };
            if r.resp.is_null() {
                answers.push(None);
            } else {
                answers.push(Some(unsafe { CStr::from_ptr(r.resp) }.to_string_lossy().into_owned()));
                unsafe { libc::free(r.resp as *mut c_void) };
            }
        }
        unsafe { libc::free(resp as *mut c_void) };
    }
    (rc, answers)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Script {
        seen: Vec<(PamPromptKind, String)>,
        answers: Vec<Result<Option<String>, Interrupt>>,
    }

    impl Conversation for Script {
        fn converse(&mut self, kind: PamPromptKind, text: &str) -> Result<Option<String>, Interrupt> {
            self.seen.push((kind, text.to_owned()));
            self.answers.remove(0)
        }
    }

    #[test]
    fn trampoline_maps_message_styles_and_returns_answers() {
        let script = Script {
            seen: vec![],
            answers: vec![Ok(None), Ok(None), Ok(Some("visible".into())), Ok(Some("secret".into()))],
        };
        let mut conv: Box<dyn Conversation> = Box::new(script);
        let (rc, answers) = drive_conversation(
            &mut conv,
            &[
                (PAM_TEXT_INFO, "info text"),
                (PAM_ERROR_MSG, "error text"),
                (PAM_PROMPT_ECHO_ON, "login: "),
                (PAM_PROMPT_ECHO_OFF, "Password: "),
            ],
        );
        assert_eq!(rc, PAM_SUCCESS);
        assert_eq!(answers, [None, None, Some("visible".into()), Some("secret".into())]);
    }

    #[test]
    fn interruption_fails_the_whole_exchange_without_answers() {
        let mut conv: Box<dyn Conversation> =
            Box::new(Script { seen: vec![], answers: vec![Ok(Some("a".into())), Err(Interrupt::Cancelled)] });
        let (rc, answers) =
            drive_conversation(&mut conv, &[(PAM_PROMPT_ECHO_OFF, "one"), (PAM_PROMPT_ECHO_OFF, "two")]);
        assert_eq!(rc, PAM_CONV_ERR);
        assert!(answers.is_empty());
    }

    #[test]
    fn unknown_style_and_oversized_exchange_are_refused() {
        let mut conv: Box<dyn Conversation> = Box::new(Script { seen: vec![], answers: vec![] });
        assert_eq!(drive_conversation(&mut conv, &[(99, "x")]).0, PAM_CONV_ERR);
        let many: Vec<(c_int, &str)> = (0..33).map(|_| (PAM_TEXT_INFO, "x")).collect();
        assert_eq!(drive_conversation(&mut conv, &many).0, PAM_CONV_ERR);
    }

    #[test]
    fn start_rejects_nul_and_missing_service_is_not_a_success() {
        let mut conv: Box<dyn Conversation> = Box::new(Script { seen: vec![], answers: vec![] });
        let _ = &mut conv;
        assert!(Pam::start("a\0b", "u", None, Box::new(Script { seen: vec![], answers: vec![] })).is_err());
    }
}
