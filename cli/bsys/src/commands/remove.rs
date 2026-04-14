use crate::output;

pub fn run(package: &str) {
    output::status("rm", &format!("would remove {package}"));
}
