pub mod categories;
pub mod syscalls;

pub use categories::PermissionCategory;
pub use syscalls::{SyscallMapping, category_for_syscall, sensitive_syscall_mappings};
