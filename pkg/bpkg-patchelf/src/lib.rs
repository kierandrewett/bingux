pub mod analyzer;
pub mod log;
pub mod planner;
pub mod scanner;
pub mod shebang;

pub use analyzer::{ElfAnalysis, analyze_elf};
pub use log::{generate_log, write_log};
pub use planner::{ElfPatch, PatchPlan};
pub use scanner::{ElfKind, ScanResult, ScannedElf, scan_package_dir};
pub use shebang::{ShebangRewrite, scan_shebangs};
