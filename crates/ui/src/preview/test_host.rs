//! The helper process for the tests that play a film, a song or a document.
//!
//! `bingux-preview-host` is a crate of its own (`services/preview-host`) so that
//! GStreamer and poppler stay out of the application, which means `cargo test`
//! in this crate does not build it. A test that needs it calls [`require`]: the
//! helper is used where [`host_available`](super::host_available) finds it (an
//! installed one, `BINGUX_PREVIEW_HOST`, or one built beside `deps/`), and when
//! there is none it is built once from `services/preview-host` into the target
//! directory this test binary is in. `cargo test --lib --features testing` is
//! all a checkout needs.

use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

/// The helper, found or built once for the whole test process.
static HOST: LazyLock<Result<(), String>> = LazyLock::new(prepare);

/// Make sure the helper can be started, building it on the first call when it
/// cannot, and let this process run as many helpers as the tests start side by
/// side. The tests run in one process and each starts a helper: the default of
/// two would end one test's helper to make room for another's.
///
/// Panics with the reason when there is no helper and it cannot be built (a
/// machine without the GStreamer and poppler development files).
pub(crate) fn require() {
    if let Err(reason) = &*HOST {
        panic!("{reason}");
    }
    super::allow_helpers(32);
}

fn prepare() -> Result<(), String> {
    if super::host_available() {
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|error| format!("this test binary: {error}"))?;
    // `<target>/<profile>/deps/<test binary>` (or `<target>/<profile>/build/<package>/<hash>/out/
    // <test binary>` in cargo's newer layout): the helper goes to `<target>/<profile>`, where
    // `host_program` looks.
    let target = super::host_link::cargo_profile_dir(&exe)
        .and_then(Path::parent)
        .ok_or_else(|| format!("{} is not inside a cargo target directory", exe.display()))?;
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../services/preview-host/Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let built = Command::new(&cargo)
        .args(["build", "--locked", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(target)
        .output()
        .map_err(|error| format!("could not run {cargo:?} to build the preview helper: {error}"))?;
    if !built.status.success() {
        return Err(format!(
            "building the preview helper (services/preview-host) failed, and these tests \
             need it: it takes GStreamer, poppler and libheif with their development files.\n{}",
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    if super::host_available() {
        Ok(())
    } else {
        Err(format!(
            "the preview helper was built into {} but is not where this test binary looks for it \
             (BINGUX_PREVIEW_HOST names a file that is not there?)",
            target.display()
        ))
    }
}
