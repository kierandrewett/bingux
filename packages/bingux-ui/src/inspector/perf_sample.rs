//! Dependency-light CPU stack sampler for the inspector's Performance tab.
//!
//! `pprof` cannot be resolved offline in this workspace, so this module drives
//! a `SIGPROF` timer bound to the calling thread's CPU clock
//! (`CLOCK_THREAD_CPUTIME_ID` + `SIGEV_THREAD_ID`) and records raw return
//! addresses with glibc `backtrace(3)` into a fixed buffer. Symbolization runs
//! in `stop`, never inside the signal handler.
//!
//! `start` and `stop` MUST be called on the same thread (the UI thread).

use std::collections::HashMap;
use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

/// Frames kept per sample.
const MAX_DEPTH: usize = 64;
/// Sample buffer capacity in `u64` words (16 MiB). Recording stops when full.
const BUF_WORDS: usize = 1 << 21;
/// Sampling period of the thread CPU timer.
const PERIOD_NS: libc::c_long = 1_000_000;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);
static INSTALL: Once = Once::new();
static BUF: AtomicPtr<u64> = AtomicPtr::new(ptr::null_mut());
static CURSOR: AtomicUsize = AtomicUsize::new(0);
static FULL: AtomicBool = AtomicBool::new(false);
static TIMER: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// One sampled call stack, root first, leaf last.
pub struct Sample {
    pub stack: Vec<Box<str>>,
}

/// Result of a recording session.
#[derive(Default)]
pub struct Profile {
    pub samples: Vec<Sample>,
    /// The buffer filled up and sampling stopped early.
    pub truncated: bool,
}

/// Monotonic clock in nanoseconds (async-signal-safe).
pub fn now_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out-pointer and CLOCK_MONOTONIC always exists on Linux.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// Whether a recording is in progress.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

/// Starts sampling the calling thread. Returns `false` if the timer or the
/// signal handler could not be installed; the caller should then keep the UI
/// usable without stacks.
pub fn start() -> bool {
    if is_active() {
        return true;
    }
    if !install_handler() {
        return false;
    }
    if BUF.load(Ordering::Relaxed).is_null() {
        let boxed = vec![0u64; BUF_WORDS].into_boxed_slice();
        BUF.store(Box::into_raw(boxed) as *mut u64, Ordering::Relaxed);
    }
    CURSOR.store(0, Ordering::Relaxed);
    FULL.store(false, Ordering::Relaxed);

    // glibc loads libgcc's unwinder lazily; warm it up outside signal context.
    let mut warm = [ptr::null_mut::<c_void>(); 4];
    // SAFETY: `warm` has 4 writable slots.
    unsafe { libc::backtrace(warm.as_mut_ptr(), warm.len() as c_int) };

    let tid = gettid();
    let mut sev: libc::sigevent = unsafe { std::mem::zeroed() };
    sev.sigev_notify = libc::SIGEV_THREAD_ID;
    sev.sigev_signo = libc::SIGPROF;
    sev.sigev_notify_thread_id = tid;

    let mut timer: libc::timer_t = ptr::null_mut();
    // SAFETY: `sev` and `timer` are valid for the duration of the call.
    let created =
        unsafe { libc::timer_create(libc::CLOCK_THREAD_CPUTIME_ID, &mut sev, &mut timer) };
    if created != 0 {
        free_buffer();
        return false;
    }
    let period = libc::timespec {
        tv_sec: 0,
        tv_nsec: PERIOD_NS,
    };
    let spec = libc::itimerspec {
        it_interval: period,
        it_value: period,
    };
    // SAFETY: `timer` was just created; `spec` is a valid itimerspec.
    let armed = unsafe { libc::timer_settime(timer, 0, &spec, ptr::null_mut()) };
    if armed != 0 {
        // SAFETY: `timer` is the id returned by timer_create above.
        unsafe { libc::timer_delete(timer) };
        free_buffer();
        return false;
    }
    TIMER.store(timer, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Release);
    true
}

/// Stops sampling and returns the symbolized samples. Must run on the thread
/// that called [`start`].
pub fn stop() -> Profile {
    // Also clears ACTIVE when the handler stopped itself because the buffer filled.
    ACTIVE.store(false, Ordering::Release);
    let timer = TIMER.swap(ptr::null_mut(), Ordering::Relaxed);
    if !timer.is_null() {
        // SAFETY: `timer` was produced by timer_create and not yet deleted.
        unsafe { libc::timer_delete(timer) };
    }
    let buf = BUF.swap(ptr::null_mut(), Ordering::Relaxed);
    let used = CURSOR.swap(0, Ordering::Relaxed).min(BUF_WORDS);
    let truncated = FULL.swap(false, Ordering::Relaxed);
    if buf.is_null() {
        return Profile::default();
    }

    // SAFETY: the timer is deleted and ACTIVE is false, so the handler on this
    // thread can no longer write into `buf`; the first `used` words are initialized.
    let raw = unsafe { std::slice::from_raw_parts(buf, used) };
    let mut symbols = Symbolizer::default();
    let mut samples = Vec::new();
    let mut at = 0;
    while at + 2 <= used {
        // raw[at] is the sample time; the tab shows counts, not times.
        let depth = raw[at + 1] as usize;
        if at + 2 + depth > used {
            break;
        }
        let mut stack: Vec<Box<str>> = raw[at + 2..at + 2 + depth]
            .iter()
            .rev()
            .map(|pc| symbols.name(*pc as usize))
            .collect();
        // Leaf frames are the signal machinery; they sit at the end (root first).
        while stack.last().is_some_and(|n| is_handler_frame(n)) {
            stack.pop();
        }
        if !stack.is_empty() {
            samples.push(Sample { stack });
        }
        at += 2 + depth;
    }

    // SAFETY: `buf` came from `Box::into_raw` of a `BUF_WORDS`-long boxed slice in `start`.
    drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buf, BUF_WORDS)) });
    Profile { samples, truncated }
}

fn is_handler_frame(name: &str) -> bool {
    name.contains("on_sigprof") || name.contains("__restore_rt")
}

fn free_buffer() {
    let buf = BUF.swap(ptr::null_mut(), Ordering::Relaxed);
    if !buf.is_null() {
        // SAFETY: same provenance as in `stop`.
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buf, BUF_WORDS)) });
    }
}

fn gettid() -> c_int {
    // SAFETY: SYS_gettid takes no arguments.
    unsafe { libc::syscall(libc::SYS_gettid) as c_int }
}

fn install_handler() -> bool {
    INSTALL.call_once(|| {
        // SAFETY: zeroed sigaction is a valid starting point; fields are set below.
        let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
        sa.sa_sigaction = on_sigprof as *const () as libc::sighandler_t;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        // SAFETY: `sa.sa_mask` is a valid sigset_t.
        unsafe { libc::sigemptyset(&mut sa.sa_mask) };
        // SAFETY: `sa` is fully initialized; no old action is requested.
        let rc = unsafe { libc::sigaction(libc::SIGPROF, &sa, ptr::null_mut()) };
        INSTALLED.store(rc == 0, Ordering::Relaxed);
    });
    INSTALLED.load(Ordering::Relaxed)
}

/// Signal handler: unwind and append one record `[t_ns, depth, pc...]`.
/// Only async-signal-safe calls on the hot path (`backtrace` is warmed up first).
extern "C" fn on_sigprof(_sig: c_int, _info: *mut libc::siginfo_t, _ctx: *mut c_void) {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let buf = BUF.load(Ordering::Relaxed);
    if buf.is_null() {
        return;
    }
    let mut frames = [ptr::null_mut::<c_void>(); MAX_DEPTH];
    // SAFETY: `frames` has MAX_DEPTH writable slots.
    let depth = unsafe { libc::backtrace(frames.as_mut_ptr(), MAX_DEPTH as c_int) }.max(0) as usize;
    let at = CURSOR.load(Ordering::Relaxed);
    let need = depth + 2;
    if at + need > BUF_WORDS {
        FULL.store(true, Ordering::Relaxed);
        ACTIVE.store(false, Ordering::Release);
        return;
    }
    // SAFETY: `at + need <= BUF_WORDS`, and only this thread writes while ACTIVE.
    unsafe {
        let slot = buf.add(at);
        *slot = now_ns();
        *slot.add(1) = depth as u64;
        for (i, f) in frames[..depth].iter().enumerate() {
            *slot.add(2 + i) = *f as u64;
        }
    }
    CURSOR.store(at + need, Ordering::Relaxed);
}

/// Resolves return addresses to names. Results are cached per address.
#[derive(Default)]
struct Symbolizer {
    cache: HashMap<usize, Box<str>>,
    modules: HashMap<String, Option<SymTab>>,
}

impl Symbolizer {
    fn name(&mut self, pc: usize) -> Box<str> {
        if let Some(n) = self.cache.get(&pc) {
            return n.clone();
        }
        let n = self.resolve(pc);
        self.cache.insert(pc, n.clone());
        n
    }

    fn resolve(&mut self, pc: usize) -> Box<str> {
        let lookup = pc.wrapping_sub(1);
        let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a valid out-pointer; dladdr only reads `lookup`'s module table.
        if unsafe { libc::dladdr(lookup as *const c_void, &mut info) } == 0 {
            return format!("?? 0x{pc:x}").into_boxed_str();
        }
        let base = info.dli_fbase as usize;
        let module = if info.dli_fname.is_null() {
            String::new()
        } else {
            // SAFETY: dladdr guarantees a NUL-terminated module path.
            unsafe { CStr::from_ptr(info.dli_fname) }
                .to_string_lossy()
                .into_owned()
        };
        let module = if module.is_empty() {
            "/proc/self/exe".to_string()
        } else {
            module
        };
        let offset = lookup.wrapping_sub(base) as u64;
        let table = self
            .modules
            .entry(module.clone())
            .or_insert_with(|| SymTab::load(&module));
        if let Some(raw) = table.as_ref().and_then(|t| t.lookup(offset)) {
            return demangle(raw).into_boxed_str();
        }
        if !info.dli_sname.is_null() {
            // SAFETY: dladdr guarantees a NUL-terminated symbol name when non-null.
            let raw = unsafe { CStr::from_ptr(info.dli_sname) }.to_string_lossy();
            return demangle(&raw).into_boxed_str();
        }
        let basename = module.rsplit('/').next().unwrap_or(&module);
        format!("{basename} +0x{offset:x}").into_boxed_str()
    }
}

/// Function symbols of one ELF64 little-endian module, sorted by start address.
struct SymTab {
    syms: Vec<(u64, u64, Box<str>)>,
}

impl SymTab {
    fn load(path: &str) -> Option<SymTab> {
        let mut f = File::open(path).ok()?;
        let header = read_at(&mut f, 0, 64)?;
        if header.get(0..4)? != b"\x7fELF" || header[4] != 2 || header[5] != 1 {
            return None;
        }
        let shoff = le_u64(&header, 0x28)?;
        let shentsize = le_u16(&header, 0x3a)? as usize;
        let shnum = le_u16(&header, 0x3c)? as usize;
        if shentsize < 64 {
            return None;
        }
        let table = read_at(&mut f, shoff, shentsize * shnum)?;
        let sections: Vec<Section> = (0..shnum)
            .map(|i| Section::parse(&table[i * shentsize..]))
            .collect::<Option<_>>()?;

        // Prefer the full symbol table; fall back to the dynamic one (stripped binaries).
        let symtab = sections
            .iter()
            .find(|s| s.ty == SHT_SYMTAB)
            .or_else(|| sections.iter().find(|s| s.ty == SHT_DYNSYM))?;
        let strtab = sections.get(symtab.link as usize)?;
        let symbols = read_at(&mut f, symtab.offset, symtab.size as usize)?;
        let strings = read_at(&mut f, strtab.offset, strtab.size as usize)?;

        let mut syms = Vec::new();
        for entry in symbols.chunks_exact(24) {
            let name_off = le_u32(entry, 0)? as usize;
            let kind = entry[4] & 0xf;
            let value = le_u64(entry, 8)?;
            let size = le_u64(entry, 16)?;
            if (kind != STT_FUNC && kind != STT_GNU_IFUNC) || value == 0 {
                continue;
            }
            let Some(tail) = strings.get(name_off..) else {
                continue;
            };
            let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
            let name = String::from_utf8_lossy(&tail[..end]).into_owned();
            syms.push((value, size, name.into_boxed_str()));
        }
        syms.sort_unstable_by_key(|s| s.0);
        Some(SymTab { syms })
    }

    fn lookup(&self, addr: u64) -> Option<&str> {
        let idx = self.syms.partition_point(|s| s.0 <= addr);
        let (start, size, name) = self.syms.get(idx.checked_sub(1)?)?;
        // Size-0 symbols (hand-written asm) get a short window.
        let window = if *size == 0 { 0x1000 } else { *size };
        (addr < start + window).then_some(&**name)
    }
}

struct Section {
    ty: u32,
    offset: u64,
    size: u64,
    link: u32,
}

impl Section {
    fn parse(b: &[u8]) -> Option<Section> {
        Some(Section {
            ty: le_u32(b, 4)?,
            offset: le_u64(b, 24)?,
            size: le_u64(b, 32)?,
            link: le_u32(b, 40)?,
        })
    }
}

const SHT_SYMTAB: u32 = 2;
const SHT_DYNSYM: u32 = 11;
const STT_FUNC: u8 = 2;
const STT_GNU_IFUNC: u8 = 10;

fn read_at(f: &mut File, off: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn le_u16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn le_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn le_u64(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

/// Demangles legacy Rust (`_ZN…17h<hash>E`) symbols; anything else is returned as-is.
fn demangle(raw: &str) -> String {
    let Some(mut rest) = raw.strip_prefix("_ZN") else {
        return raw.to_string();
    };
    let mut out = String::new();
    while !rest.starts_with('E') {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return raw.to_string();
        }
        let Ok(len) = rest[..digits].parse::<usize>() else {
            return raw.to_string();
        };
        let Some(seg) = rest.get(digits..digits + len) else {
            return raw.to_string();
        };
        rest = &rest[digits + len..];
        // Legacy hash segment: `h` followed by 16 hex digits.
        if len == 17 && seg.starts_with('h') {
            continue;
        }
        if !out.is_empty() {
            out.push_str("::");
        }
        out.push_str(&unescape_legacy(seg));
    }
    out
}

fn unescape_legacy(seg: &str) -> String {
    seg.replace("$LT$", "<")
        .replace("$GT$", ">")
        .replace("$RF$", "&")
        .replace("$BP$", "*")
        .replace("$SP$", "@")
        .replace("$C$", ",")
        .replace("$u20$", " ")
        .replace("$u7b$", "{")
        .replace("$u7d$", "}")
        .replace("$u5b$", "[")
        .replace("$u5d$", "]")
        .replace("$u27$", "'")
        .replace("$u22$", "\"")
}
