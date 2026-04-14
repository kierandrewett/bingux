use crate::output;

pub fn run(dry_run: bool) {
    if dry_run {
        output::status("gc", "would show what can be collected (dry run)");
    } else {
        output::status("gc", "would garbage collect the store");
    }
}
