//! Opening a link from the About window, and finding the licence files a
//! package installed.

use std::process::{Command, Stdio};

/// Whether `url` is one the About window opens: a web page or a mail address.
/// Anything else (a `file:` path, a `javascript:` string, a custom scheme) is
/// not a link an application's metadata should be able to launch.
pub(crate) fn is_openable(url: &str) -> bool {
    ["https://", "http://", "mailto:"].iter().any(|scheme| url.starts_with(scheme))
}

/// Open `url` with `xdg-open`, which asks the desktop for the user's browser
/// or mail client. The child runs on its own and its output is dropped; a
/// thread reaps it. Returns whether it started. Call it from an event
/// handler, never from render.
pub(crate) fn open(url: &str) -> bool {
    if !is_openable(url) {
        return false;
    }
    let Ok(mut child) = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    true
}

/// "github.com/kierandrewett/bingux": a link without its scheme and trailing
/// slash, the way the row shows it.
pub(crate) fn shown(url: &str) -> String {
    let url = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("mailto:"))
        .unwrap_or(url);
    url.trim_end_matches('/').to_owned()
}

/// What the Legal page reads for an application: the list of the libraries it
/// links and their licences, and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Legal {
    pub text: String,
    /// The file the text was read from, for "open the full texts". `None` when
    /// the application supplied the text itself.
    pub path: Option<String>,
}

/// What [`find_third_party`] looks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lookup {
    /// Whether the Legal page is on screen: nothing is read before it is.
    pub wanted: bool,
    pub id: String,
    /// The text the application supplied; when it is not empty it is the answer.
    pub given: String,
}

/// Read the library licence list. The text the application supplied wins;
/// otherwise the file its package installed, `<id>.LICENSE.dependencies` in
/// `$BINGUX_LICENSES_DIR` (for a build tree) or in the package's directory
/// under `/usr/share/licenses`, where the RPM and the Arch package put it.
/// Runs on a worker.
pub(crate) fn find_third_party(lookup: Lookup) -> Result<Legal, String> {
    if !lookup.wanted {
        return Err(String::new());
    }
    if !lookup.given.is_empty() {
        return Ok(Legal { text: lookup.given, path: None });
    }
    let file = format!("{}.LICENSE.dependencies", lookup.id);
    let mut places: Vec<String> = Vec::new();
    if let Ok(dir) = std::env::var("BINGUX_LICENSES_DIR") {
        places.push(format!("{dir}/{file}"));
    }
    places.push(format!("/usr/share/licenses/{}/{file}", lookup.id));
    places.push(format!("/usr/local/share/licenses/{}/{file}", lookup.id));
    for path in places {
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Ok(Legal { text, path: Some(path) });
        }
    }
    Err(format!("{file} is not installed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_and_mail_links_open() {
        for ok in ["https://github.com/x", "http://example.org", "mailto:a@b.c"] {
            assert!(is_openable(ok), "{ok}");
        }
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "bingux-files://x", "github.com", ""] {
            assert!(!is_openable(bad), "{bad}");
        }
    }

    #[test]
    fn a_link_is_shown_without_its_scheme() {
        assert_eq!(shown("https://github.com/kierandrewett/bingux/"), "github.com/kierandrewett/bingux");
        assert_eq!(shown("mailto:a@b.c"), "a@b.c");
    }

    #[test]
    fn the_supplied_text_is_the_answer_and_nothing_is_read_before_the_page_is_open() {
        let lookup = |wanted, given: &str| Lookup { wanted, id: "bingux-none".into(), given: given.into() };
        assert_eq!(find_third_party(lookup(false, "x")), Err(String::new()));
        assert_eq!(find_third_party(lookup(true, "x")), Ok(Legal { text: "x".into(), path: None }));
        assert!(find_third_party(lookup(true, "")).is_err());
    }
}
