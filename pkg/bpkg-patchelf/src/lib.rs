pub mod analyzer;
pub mod planner;
pub mod scanner;

pub use analyzer::{ElfAnalysis, analyze_elf};
pub use planner::{ElfPatch, PatchPlan};
pub use scanner::{ElfKind, ScanResult, ScannedElf, scan_package_dir};
