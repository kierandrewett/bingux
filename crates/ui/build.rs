//! Records the toolchain and the Freya version bingux-ui is built with, for the
//! About window's "Copy details" (`about::details`). Both are plain
//! `cargo:rustc-env` values; a failure to read either leaves the line out.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    if let Ok(output) = Command::new(rustc).arg("--version").output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        println!("cargo:rustc-env=BINGUX_UI_RUSTC={version}");
    }

    // `freya = { version = "0.5.0-rc.10", ... }`
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("Cargo.toml");
    if let Ok(text) = fs::read_to_string(manifest)
        && let Some(line) = text.lines().find(|line| line.trim_start().starts_with("freya = "))
        && let Some(start) = line.find("version = \"")
    {
        let rest = &line[start + "version = \"".len()..];
        if let Some(end) = rest.find('"') {
            println!("cargo:rustc-env=BINGUX_UI_FREYA={}", &rest[..end]);
        }
    }
}
