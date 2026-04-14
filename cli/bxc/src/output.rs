/// Print a status line for bxc operations.
pub fn status(action: &str, detail: &str) {
    eprintln!("[bxc] {action}: {detail}");
}

/// Print a stub message indicating a command is not yet implemented.
pub fn stub(command: &str) {
    eprintln!("[bxc] {command}: not yet implemented (stub)");
}
