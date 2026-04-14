use crate::output;

pub fn run(package: &str, reset: bool) {
    if reset {
        output::status("perms", &format!("would reset permissions for {package}"));
    } else {
        output::status("perms", &format!("would show permissions for {package}"));
    }
}
