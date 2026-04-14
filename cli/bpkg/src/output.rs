use std::fmt;

// ANSI colour codes
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Print a success message in green.
pub fn print_success(msg: &str) {
    println!("{GREEN}{BOLD}ok{RESET} {msg}");
}

/// Print a warning message in yellow.
pub fn print_warning(msg: &str) {
    println!("{YELLOW}{BOLD}warning{RESET} {msg}");
}

/// Print an error message in red.
pub fn print_error(msg: &str) {
    eprintln!("{RED}{BOLD}error{RESET} {msg}");
}

/// Print an info/status message.
pub fn print_status(action: &str, msg: &str) {
    println!("{BOLD}{action}{RESET} {msg}");
}

/// The status of a user-installed package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageStatus {
    Volatile,
    Kept,
    Pinned(String),
}

impl fmt::Display for PackageStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageStatus::Volatile => write!(f, "{DIM}[volatile]{RESET}"),
            PackageStatus::Kept => write!(f, "{GREEN}[kept]{RESET}"),
            PackageStatus::Pinned(v) => write!(f, "{YELLOW}[pin: {v}]{RESET}"),
        }
    }
}

/// An entry for the package list table.
#[derive(Debug, Clone)]
pub struct PackageListEntry {
    pub name: String,
    pub version: String,
    pub status: PackageStatus,
}

/// Print a formatted table of packages.
pub fn print_package_list(packages: &[PackageListEntry]) {
    if packages.is_empty() {
        println!("No packages installed.");
        return;
    }

    // Compute column widths
    let name_width = packages
        .iter()
        .map(|p| p.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let version_width = packages
        .iter()
        .map(|p| p.version.len())
        .max()
        .unwrap_or(7)
        .max(7);

    // Header
    println!(
        "{BOLD}{:<name_width$}  {:<version_width$}  STATUS{RESET}",
        "NAME", "VERSION"
    );

    for entry in packages {
        println!(
            "{:<name_width$}  {:<version_width$}  {}",
            entry.name, entry.version, entry.status
        );
    }
}

/// Print package details.
pub fn print_package_info(
    name: &str,
    version: &str,
    description: &str,
    license: &str,
    scope: &str,
    deps: &[String],
    exports_bins: &[String],
) {
    println!("{BOLD}Name:{RESET}         {name}");
    println!("{BOLD}Version:{RESET}      {version}");
    println!("{BOLD}Scope:{RESET}        @{scope}");
    if !description.is_empty() {
        println!("{BOLD}Description:{RESET}  {description}");
    }
    if !license.is_empty() {
        println!("{BOLD}License:{RESET}      {license}");
    }
    if !deps.is_empty() {
        println!("{BOLD}Dependencies:{RESET} {}", deps.join(", "));
    }
    if !exports_bins.is_empty() {
        println!("{BOLD}Binaries:{RESET}     {}", exports_bins.join(", "));
    }
}

/// Print a simple spinner-style status message (no real animation in stub mode).
pub fn print_spinner(msg: &str) {
    println!("{DIM}::{RESET} {msg}");
}

/// Print search results.
pub fn print_search_results(results: &[(String, String, String)]) {
    if results.is_empty() {
        println!("No packages found.");
        return;
    }
    let name_width = results
        .iter()
        .map(|(n, _, _)| n.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let ver_width = results
        .iter()
        .map(|(_, v, _)| v.len())
        .max()
        .unwrap_or(7)
        .max(7);

    println!(
        "{BOLD}{:<name_width$}  {:<ver_width$}  DESCRIPTION{RESET}",
        "NAME", "VERSION"
    );
    for (name, version, desc) in results {
        println!("{:<name_width$}  {:<ver_width$}  {desc}", name, version);
    }
}

/// Print generation history entries.
pub fn print_history(entries: &[(u64, String, String)]) {
    if entries.is_empty() {
        println!("No profile generations found.");
        return;
    }

    println!("{BOLD}GEN  DATE                 ACTION{RESET}");
    for (number, date, action) in entries {
        println!("{number:<4} {date:<20} {action}");
    }
}
