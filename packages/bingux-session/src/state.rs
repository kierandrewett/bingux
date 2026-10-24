//! The last successful login choice, kept in the greeter user's state directory.

use serde::{Deserialize, Serialize};
use std::{env, fs, io, path::PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub session: String,
}

pub fn state_path() -> Option<PathBuf> {
    let home = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(home.join("bingux/greeter.json"))
}

pub fn remember(user: &str, session: &str) -> io::Result<Choice> {
    let choice = Choice {
        user: user.to_string(),
        session: session.to_string(),
    };
    let path =
        state_path().ok_or_else(|| io::Error::other("neither XDG_STATE_HOME nor HOME is set"))?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("state path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(&choice)? + "\n")?;
    fs::rename(&temporary, &path)?;
    Ok(choice)
}

pub fn recall() -> Choice {
    state_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_state_directory() {
        let root = env::temp_dir().join(format!("bingux-session-state-{}", std::process::id()));
        // SAFETY: the test binary runs this module's tests on one thread.
        unsafe { env::set_var("XDG_STATE_HOME", &root) };
        assert_eq!(recall(), Choice::default());
        remember("ada", "gnoblin").unwrap();
        assert_eq!(
            recall(),
            Choice {
                user: "ada".into(),
                session: "gnoblin".into()
            }
        );
        fs::write(root.join("bingux/greeter.json"), "{not json").unwrap();
        assert_eq!(recall(), Choice::default());
        fs::remove_dir_all(root).unwrap();
    }
}
