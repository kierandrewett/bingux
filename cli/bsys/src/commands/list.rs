use crate::output;

pub fn run() {
    output::status("list", "would list installed system packages");
}

pub fn info(package: &str) {
    output::status("info", &format!("would show details for {package}"));
}
