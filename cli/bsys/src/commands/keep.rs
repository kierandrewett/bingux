use crate::output;

pub fn run_keep(package: &str) {
    output::status("keep", &format!("would promote {package} to persistent"));
}

pub fn run_unkeep(package: &str, force: bool) {
    if force {
        output::status("unkeep", &format!("would demote {package} (forced)"));
    } else {
        output::status("unkeep", &format!("would demote {package}"));
    }
}
