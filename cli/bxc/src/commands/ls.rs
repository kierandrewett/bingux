use crate::output;

pub fn run(package: &str) {
    output::status("ls", &format!("would list home contents for {package}"));
}
