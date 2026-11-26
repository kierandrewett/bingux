//! What the browser takes off its command line: `--private` and `--profile`.

use std::ffi::OsString;

/// The browser's own options, taken off the command line; bingux-ui's launch
/// reads the rest (the addresses).
#[derive(Debug, Default, PartialEq)]
pub struct CommandLine {
    pub private: bool,
    pub profile: Option<String>,
    pub rest: Vec<OsString>,
}

impl CommandLine {
    /// What `--help` prints.
    pub const USAGE: &str = "Usage: bingux-browser [--private] [--profile NAME] [ADDRESS | WORDS | FILE]...\n\
    \n\
    Opens ADDRESS in a new tab of the browser window, starting it if it is not\n\
    running. WORDS are searched for with the search engine of the settings. A FILE\n\
    is a path starting with /, ./, ../ or ~/. With no argument a new tab opens.\n\
    \n\
    --profile NAME opens the profile called NAME (or with that id) instead of the\n\
    default one. Each profile keeps its own history, cookies and settings.\n\
    \n\
    --private starts a separate window whose tabs keep nothing: no history, no\n\
    cookies and no site data. It is a process of its own.\n";

    /// The environment variable that makes a copy of the browser a private window.
    pub const PRIVATE: &str = "BINGUX_BROWSER_PRIVATE";
    /// The environment variable that names the profile a copy opens (`--profile`).
    pub const PROFILE: &str = "BINGUX_BROWSER_OPEN_PROFILE";

    pub fn parse(arguments: &[OsString]) -> Result<Self, String> {
        let mut own = Self::default();
        let mut rest = arguments.iter();
        while let Some(argument) = rest.next() {
            let text = argument.to_string_lossy();
            if text == "--" {
                own.rest.push(argument.clone());
                own.rest.extend(rest.cloned());
                break;
            } else if text == "--private" {
                own.private = true;
            } else if text == "--profile" {
                let name = rest.next().ok_or("--profile needs the name of a profile")?;
                own.profile = Some(name.to_string_lossy().into_owned());
            } else if let Some(name) = text.strip_prefix("--profile=") {
                own.profile = Some(name.to_string());
            } else {
                own.rest.push(argument.clone());
            }
        }
        Ok(own)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own(arguments: &[&str]) -> CommandLine {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        CommandLine::parse(&arguments).unwrap()
    }

    #[test]
    fn the_browsers_own_options_come_off_the_command_line() {
        let parsed = own(&["--profile", "Work", "example.com", "--private"]);
        assert!(parsed.private);
        assert_eq!(parsed.profile.as_deref(), Some("Work"));
        assert_eq!(parsed.rest, [OsString::from("example.com")]);
        assert_eq!(own(&["--profile=Study"]).profile.as_deref(), Some("Study"));
        let after = own(&["--", "--profile", "x"]);
        assert_eq!(after.profile, None, "after -- everything is an address");
        assert_eq!(after.rest.len(), 3);
        let arguments = [OsString::from("--profile")];
        assert!(CommandLine::parse(&arguments).is_err());
    }
}
