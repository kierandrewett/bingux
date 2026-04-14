use crate::output;

pub fn run(package: &str, keep: bool) {
    let mode = if keep { "persistent" } else { "volatile" };
    output::status("add", &format!("would install {package} as {mode}"));
}
