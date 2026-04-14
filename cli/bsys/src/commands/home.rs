use crate::args::HomeCommand;
use crate::output;

pub fn run(action: &HomeCommand) {
    match action {
        HomeCommand::Apply { path } => {
            if let Some(p) = path {
                output::status("home apply", &format!("would converge config from {}", p.display()));
            } else {
                output::status("home apply", "would converge system home configuration");
            }
        }
    }
}
