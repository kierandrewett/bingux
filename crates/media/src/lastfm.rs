//! Last.fm scrobbling: off until the person sets it up.
//!
//! Last.fm gives every application its own API key and shared secret, and
//! Bingux ships none (a secret in a free program is not a secret). The
//! person makes an API account at <https://www.last.fm/api/account/create>,
//! pastes the key and secret into Media's settings, and signs in through the
//! desktop authorisation flow: Media asks Last.fm for a token, opens the
//! Last.fm page that asks the person to allow it, and then trades the token
//! for a session key. The key, secret and session key are kept in
//! `$XDG_CONFIG_HOME/bingux-media/lastfm.json`, readable by the owner only.
//!
//! A play counts once half a song (or four minutes) has been heard, as
//! Last.fm's rules say. Plays wait in the library's index and are sent in
//! batches of up to 50, so a computer that is offline sends them later.
//! Every call here goes to the network: call them from a worker.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::library::Scrobble;

const API: &str = "https://ws.audioscrobbler.com/2.0/";
const AUTH_PAGE: &str = "https://www.last.fm/api/auth/";
/// The most plays one `track.scrobble` call takes.
pub const BATCH: usize = 50;

/// The keys and the signed-in account.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub api_key: String,
    pub secret: String,
    /// The session key, once signed in.
    #[serde(default)]
    pub session: Option<String>,
    /// The Last.fm user name, once signed in.
    #[serde(default)]
    pub user: Option<String>,
    /// A token waiting for the person to allow it on the Last.fm page.
    #[serde(default)]
    pub pending_token: Option<String>,
}

impl Account {
    pub fn signed_in(&self) -> bool {
        self.session.is_some()
    }

    pub fn has_keys(&self) -> bool {
        !self.api_key.trim().is_empty() && !self.secret.trim().is_empty()
    }
}

/// What went wrong talking to Last.fm, in words for the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// No connection, or Last.fm did not answer.
    Offline,
    /// The key or secret is wrong.
    BadKeys,
    /// The person has not allowed Media on the Last.fm page yet.
    NotAllowedYet,
    /// The session was revoked; sign in again.
    SignedOut,
    /// Anything else Last.fm said.
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Offline => write!(f, "Could not reach Last.fm"),
            Error::BadKeys => write!(f, "Last.fm did not accept the API key or secret"),
            Error::NotAllowedYet => write!(f, "Media is not allowed on Last.fm yet"),
            Error::SignedOut => write!(f, "Last.fm signed Media out"),
            Error::Other(message) => write!(f, "Last.fm refused the request: {message}"),
        }
    }
}

/// Where the account is kept.
pub fn account_file() -> PathBuf {
    crate::settings::config_dir().join("lastfm.json")
}

/// The saved account, if there is one.
pub fn load() -> Option<Account> {
    let text = std::fs::read_to_string(account_file()).ok()?;
    serde_json::from_str(&text).ok()
}

/// Saves the account, readable by the owner only.
pub fn save(account: &Account) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let file = account_file();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = file.with_extension("json.part");
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&partial)?;
    out.write_all(
        serde_json::to_string_pretty(account)
            .unwrap_or_default()
            .as_bytes(),
    )?;
    std::fs::rename(&partial, &file)
}

/// Forgets the account.
pub fn forget() {
    let _ = std::fs::remove_file(account_file());
}

/// The `api_sig` of a call: the MD5 of every parameter (but `format`) as
/// name and value in name order, then the secret.
pub fn signature(params: &BTreeMap<String, String>, secret: &str) -> String {
    let mut hasher = Md5::new();
    for (name, value) in params {
        if name == "format" || name == "callback" {
            continue;
        }
        hasher.update(name.as_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.update(secret.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn call(
    account: &Account,
    method: &str,
    mut params: BTreeMap<String, String>,
    post: bool,
) -> Result<serde_json::Value, Error> {
    params.insert("method".into(), method.into());
    params.insert("api_key".into(), account.api_key.trim().into());
    let signature = signature(&params, account.secret.trim());
    params.insert("api_sig".into(), signature);
    params.insert("format".into(), "json".into());
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let pairs: Vec<(&str, &str)> = params
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let response = if post {
        agent
            .post(API)
            .header("User-Agent", "Bingux Media")
            .send_form(pairs)
    } else {
        let mut request = agent.get(API).header("User-Agent", "Bingux Media");
        for (name, value) in pairs {
            request = request.query(name, value);
        }
        request.call()
    };
    let mut response = response.map_err(|_| Error::Offline)?;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|_| Error::Offline)?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|_| Error::Offline)?;
    if let Some(code) = json.get("error").and_then(serde_json::Value::as_i64) {
        let message = json
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        return Err(match code {
            10 | 13 | 26 => Error::BadKeys,
            14 => Error::NotAllowedYet,
            4 | 9 => Error::SignedOut,
            _ => Error::Other(message),
        });
    }
    Ok(json)
}

/// Starts signing in: asks Last.fm for a token and returns the page the
/// person opens to allow Media. Keep the token in the account.
pub fn begin_sign_in(account: &Account) -> Result<(String, String), Error> {
    let json = call(account, "auth.getToken", BTreeMap::new(), false)?;
    let token = json
        .get("token")
        .and_then(serde_json::Value::as_str)
        .ok_or(Error::BadKeys)?
        .to_owned();
    let page = format!(
        "{AUTH_PAGE}?api_key={}&token={token}",
        account.api_key.trim()
    );
    Ok((token, page))
}

/// Finishes signing in once the person allowed Media: the account with its
/// session key and user name.
pub fn finish_sign_in(account: &Account, token: &str) -> Result<Account, Error> {
    let mut params = BTreeMap::new();
    params.insert("token".to_owned(), token.to_owned());
    let json = call(account, "auth.getSession", params, false)?;
    let session = json.get("session").ok_or(Error::NotAllowedYet)?;
    let key = session
        .get("key")
        .and_then(serde_json::Value::as_str)
        .ok_or(Error::NotAllowedYet)?;
    let user = session
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(Account {
        session: Some(key.to_owned()),
        user,
        pending_token: None,
        ..account.clone()
    })
}

fn track_params(scrobble: &Scrobble, index: Option<usize>) -> BTreeMap<String, String> {
    let key = |name: &str| match index {
        Some(index) => format!("{name}[{index}]"),
        None => name.to_owned(),
    };
    let mut params = BTreeMap::new();
    params.insert(key("artist"), scrobble.artist.clone());
    params.insert(key("track"), scrobble.title.clone());
    if !scrobble.album.is_empty() {
        params.insert(key("album"), scrobble.album.clone());
    }
    if !scrobble.album_artist.is_empty() && scrobble.album_artist != scrobble.artist {
        params.insert(key("albumArtist"), scrobble.album_artist.clone());
    }
    if let Some(duration) = scrobble.duration {
        params.insert(key("duration"), format!("{}", duration.round() as i64));
    }
    if index.is_some() {
        params.insert(key("timestamp"), scrobble.at.to_string());
    }
    params
}

/// Tells Last.fm what is playing now.
pub fn now_playing(account: &Account, scrobble: &Scrobble) -> Result<(), Error> {
    let session = account.session.clone().ok_or(Error::SignedOut)?;
    let mut params = track_params(scrobble, None);
    params.insert("sk".into(), session);
    call(account, "track.updateNowPlaying", params, true).map(|_| ())
}

/// Sends up to [`BATCH`] plays.
pub fn scrobble(account: &Account, plays: &[Scrobble]) -> Result<(), Error> {
    let session = account.session.clone().ok_or(Error::SignedOut)?;
    let mut params = BTreeMap::new();
    for (index, play) in plays.iter().take(BATCH).enumerate() {
        params.extend(track_params(play, Some(index)));
    }
    params.insert("sk".into(), session);
    call(account, "track.scrobble", params, true).map(|_| ())
}

/// Whether a play of `duration` seconds can be scrobbled at all: Last.fm
/// takes songs longer than 30 seconds that have an artist and a title.
pub fn scrobblable(scrobble: &Scrobble) -> bool {
    scrobble.duration.is_none_or(|duration| duration > 30.0)
        && !scrobble.artist.trim().is_empty()
        && !scrobble.title.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signature_is_the_documented_one() {
        // The example of Last.fm's authentication specification, worked by hand.
        let mut params = BTreeMap::new();
        params.insert("api_key".to_owned(), "xxxxxxxxxx".to_owned());
        params.insert("method".to_owned(), "auth.getSession".to_owned());
        params.insert("token".to_owned(), "yyyyyy".to_owned());
        params.insert("format".to_owned(), "json".to_owned());
        let mut hasher = Md5::new();
        hasher.update(b"api_keyxxxxxxxxxxmethodauth.getSessiontokenyyyyyyilovecher");
        let expected: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(signature(&params, "ilovecher"), expected);
    }

    #[test]
    fn batches_are_numbered() {
        let play = Scrobble {
            artist: "A".into(),
            title: "T".into(),
            at: 5,
            duration: Some(200.0),
            ..Scrobble::default()
        };
        let params = track_params(&play, Some(3));
        assert_eq!(params.get("timestamp[3]").map(String::as_str), Some("5"));
        assert_eq!(params.get("duration[3]").map(String::as_str), Some("200"));
        assert!(!params.contains_key("album[3]"));
    }
}
