use crate::output;

/// List running sandboxed processes.
/// In a full implementation, this would query the PID registry in bingux-gated.
/// For now, show a placeholder indicating the feature architecture.
pub fn run() {
    output::status("ps", "sandboxed processes:");
    println!();
    println!("  \x1b[1mPID     PACKAGE              SANDBOX    USER      SINCE\x1b[0m");
    println!("  (no sandboxed processes running)");
    println!();
    output::status("ps", "processes run in sandboxes when launched via bxc-shim");
    output::status("ps", "the bingux-gated daemon tracks active sandboxes");
}
