use crate::args::RepoAction;
use crate::output;

pub fn run(action: &RepoAction) {
    match action {
        RepoAction::Add { repo } => output::status("repo add", &format!("would add repository {repo}")),
        RepoAction::Rm { repo } => output::status("repo rm", &format!("would remove repository {repo}")),
        RepoAction::Sync => output::status("repo sync", "would sync all repositories"),
    }
}
