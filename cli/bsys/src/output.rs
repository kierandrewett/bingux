/// Print a status line for bsys operations.
pub fn status(action: &str, detail: &str) {
    eprintln!("[bsys] {action}: {detail}");
}

/// Print a stub message indicating a command is not yet implemented.
pub fn stub(command: &str) {
    eprintln!("[bsys] {command}: not yet implemented (stub)");
}
