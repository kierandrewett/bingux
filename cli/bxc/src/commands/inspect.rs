use crate::output;

pub fn run(package: &str) {
    output::status("inspect", &format!("would show sandbox config for {package}"));
}
