use crate::output;

pub fn run(package: &str) {
    output::status("mounts", &format!("would show mount set for {package}"));
}
