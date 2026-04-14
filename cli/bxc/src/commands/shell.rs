use crate::output;

pub fn run(package: &str) {
    output::status("shell", &format!("would open interactive shell for {package}"));
}
