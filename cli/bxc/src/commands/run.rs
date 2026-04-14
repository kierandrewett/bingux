use crate::output;

pub fn run(package: &str, args: &[String]) {
    if args.is_empty() {
        output::status("run", &format!("would launch {package} in sandbox"));
    } else {
        output::status(
            "run",
            &format!("would launch {package} in sandbox with args: {}", args.join(" ")),
        );
    }
}
