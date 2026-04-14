use crate::output;

pub fn apply() {
    output::status("apply", "would recompose system profile");
}

pub fn rollback(generation: Option<u64>) {
    match generation {
        Some(g) => output::status("rollback", &format!("would roll back to generation {g}")),
        None => output::status("rollback", "would roll back to previous generation"),
    }
}

pub fn history() {
    output::status("history", "would list system generations");
}

pub fn diff(gen1: u64, gen2: u64) {
    output::status("diff", &format!("would diff generations {gen1} and {gen2}"));
}

pub fn upgrade(package: Option<&str>, all: bool) {
    if all {
        output::status("upgrade", "would upgrade all packages");
    } else if let Some(pkg) = package {
        output::status("upgrade", &format!("would upgrade {pkg}"));
    } else {
        output::status("upgrade", "would interactively select packages to upgrade");
    }
}
