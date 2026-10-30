//! The modem as ModemManager reports it, read with `mmcli -J`, and the
//! `mmcli` commands that change it.
//!
//! `mmcli -J` prints one JSON object per call: `-L` the list of modems,
//! `-m <id>` one modem (`modem.generic.*`, `modem.3gpp.*`), `-i <id>` one SIM
//! (`sim.properties.*`). Absent values are the string `"--"`, and list values
//! are arrays, or `"--"` when empty; the helpers below read all of those
//! shapes so a ModemManager that prints one a little differently is still
//! understood.
//!
//! Everything here is plain data and functions, so every parser has a test
//! against the documented output. Snapshot builds can stand in a modem with
//! `BINGUX_SETTINGS_FAKE_MODEM`; the stand-in is recorded `mmcli` output that
//! goes through the same parsers (see `fixtures`).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::system::{self, CommandError};

/// How long a read of the modem may take.
const READ: Duration = Duration::from_secs(8);
/// How long a SIM or mode command may take.
const COMMAND: Duration = Duration::from_secs(20);
/// A network scan asks the modem to listen on every band, which takes
/// minutes on a slow modem.
pub const SCAN: Duration = Duration::from_secs(150);
/// `--timeout` handed to `mmcli` for the scan and for registering, in seconds,
/// a little under [`SCAN`] so `mmcli` reports its own timeout.
const SCAN_ARG: &str = "--timeout=140";

fn mmcli(args: &[&str], timeout: Duration) -> Result<String, CommandError> {
    system::run("mmcli", args, timeout).map_err(|error| CommandError(friendly(error.0)))
}

/// ModemManager's own sentences, in words a person can act on.
fn friendly(message: String) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("incorrectpassword") || lower.contains("incorrect password") {
        "That PIN is not right.".to_string()
    } else if lower.contains("simpuk") || lower.contains("sim puk") {
        "The SIM is blocked and needs its PUK code.".to_string()
    } else if lower.contains("simpin") && lower.contains("required") {
        "The SIM needs its PIN first.".to_string()
    } else {
        message
    }
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

fn lookup<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, key| value.get(key))
}

/// The text at `path`, or an empty string for a missing or `--` value.
fn text(value: &Value, path: &[&str]) -> String {
    match lookup(value, path) {
        Some(Value::String(text)) if text != "--" => text.trim().to_string(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

/// The list at `path`: an array of strings, one string, or nothing.
fn list(value: &Value, path: &[&str]) -> Vec<String> {
    match lookup(value, path) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str())
            .filter(|item| *item != "--" && !item.is_empty())
            .map(|item| item.trim().to_string())
            .collect(),
        Some(Value::String(item)) if item != "--" && !item.is_empty() => vec![item.clone()],
        _ => Vec::new(),
    }
}

/// The last path segment of a D-Bus object path, `0` for
/// `/org/freedesktop/ModemManager1/Modem/0`.
fn object_id(path: &str) -> Option<String> {
    let id = path.trim().rsplit('/').next()?;
    (!id.is_empty() && id != "--" && id.chars().all(|c| c.is_ascii_digit())).then(|| id.to_string())
}

// ---------------------------------------------------------------------------
// Network generations and mode sets
// ---------------------------------------------------------------------------

/// A generation of mobile network, as the modes ModemManager lists.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Serialize, Deserialize)]
pub enum Generation {
    G2,
    G3,
    G4,
    G5,
}

impl Generation {
    pub const ALL: [Self; 4] = [Self::G2, Self::G3, Self::G4, Self::G5];

    fn from_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "2g" => Some(Self::G2),
            "3g" => Some(Self::G3),
            "4g" => Some(Self::G4),
            "5g" => Some(Self::G5),
            _ => None,
        }
    }

    /// The word `mmcli --set-allowed-modes` takes.
    fn argument(self) -> &'static str {
        match self {
            Self::G2 => "2g",
            Self::G3 => "3g",
            Self::G4 => "4g",
            Self::G5 => "5g",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::G2 => "2G",
            Self::G3 => "3G",
            Self::G4 => "4G",
            Self::G5 => "5G",
        }
    }
}

/// The generations a modem may use, and the one it tries first.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Modes {
    /// Ascending, without repeats.
    pub allowed: Vec<Generation>,
    pub preferred: Option<Generation>,
}

impl Modes {
    /// `allowed: 2g, 3g, 4g; preferred: 4g`. `any` stands for `every`, the
    /// generations the modem supports.
    fn parse(text: &str, every: &[Generation]) -> Option<Self> {
        let (allowed, preferred) = text.split_once(';')?;
        let allowed = allowed.trim().strip_prefix("allowed:")?;
        let preferred = preferred.trim().strip_prefix("preferred:")?;
        let mut modes: Vec<Generation> = Vec::new();
        for token in allowed.split(',') {
            if token.trim().eq_ignore_ascii_case("any") {
                modes.extend_from_slice(every);
            } else if let Some(generation) = Generation::from_token(token) {
                modes.push(generation);
            }
        }
        modes.sort();
        modes.dedup();
        (!modes.is_empty()).then(|| Self {
            allowed: modes,
            preferred: Generation::from_token(preferred),
        })
    }

    fn highest(&self) -> Option<Generation> {
        self.allowed.last().copied()
    }
}

/// One entry of the Preferred network type list.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ModeChoice {
    pub label: String,
    pub modes: Modes,
}

impl ModeChoice {
    /// The value of the list entry: the allowed generations.
    pub fn key(&self) -> String {
        mode_key(&self.modes.allowed)
    }
}

pub fn mode_key(allowed: &[Generation]) -> String {
    allowed
        .iter()
        .map(|generation| generation.argument())
        .collect::<Vec<_>>()
        .join("|")
}

/// `5G/4G/3G`, highest first.
fn generation_list(allowed: &[Generation]) -> String {
    allowed
        .iter()
        .rev()
        .map(|generation| generation.label())
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------
// The modem
// ---------------------------------------------------------------------------

/// What the SIM needs before the modem can use it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Lock {
    None,
    Pin,
    Puk,
    /// A code Settings cannot ask for, such as a network personalisation
    /// code; the value is ModemManager's name for it.
    Other(String),
}

impl Lock {
    fn parse(value: &str) -> Self {
        match value {
            "" => Self::None,
            "sim-pin" => Self::Pin,
            "sim-puk" => Self::Puk,
            other => Self::Other(other.to_string()),
        }
    }
}

/// The SIM card of a modem, from `mmcli -i`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Sim {
    pub id: String,
    pub operator: String,
    pub iccid: String,
    /// `physical` or `esim`.
    pub kind: String,
}

/// One modem and what it reports right now.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Modem {
    pub id: String,
    /// `failed`, `locked`, `disabled`, `enabled`, `searching`, `registered`,
    /// `connecting`, `connected`, ...
    pub state: String,
    /// Why the state is `failed`, such as `sim-missing`.
    pub failed_reason: String,
    pub lock: Lock,
    /// Attempts left per code: `("sim-pin", 3)`.
    pub retries: Vec<(String, u32)>,
    /// `home`, `roaming`, `searching`, `denied`, `idle`, ...
    pub registration: String,
    pub operator: String,
    pub operator_code: String,
    /// ModemManager's access technology names, such as `lte`.
    pub technologies: Vec<String>,
    /// Signal quality, 0 to 100.
    pub signal: Option<u8>,
    pub manufacturer: String,
    pub model: String,
    pub revision: String,
    pub imei: String,
    pub numbers: Vec<String>,
    /// The sysfs device of the modem.
    pub device: String,
    pub primary_port: String,
    /// The network interface the data session uses, such as `wwan0`.
    pub interface: String,
    pub supported: Vec<Modes>,
    pub current: Option<Modes>,
    /// The SIM object's id, when the modem has a SIM.
    pub sim_id: Option<String>,
    /// Whether the SIM asks for its PIN at start (the `sim` facility lock).
    pub pin_lock: bool,
    pub sim: Option<Sim>,
}

impl Modem {
    /// The technology to show: `5G`, `4G`, `3G` or `2G`.
    pub fn technology(&self) -> Option<&'static str> {
        technology_of(&self.technologies)
    }

    /// Signal as 0 to 4 bars.
    pub fn bars(&self) -> u8 {
        match self.signal {
            None | Some(0) => 0,
            Some(1..=25) => 1,
            Some(26..=50) => 2,
            Some(51..=75) => 3,
            Some(_) => 4,
        }
    }

    pub fn signal_label(&self) -> &'static str {
        match self.bars() {
            0 => "No signal",
            1 => "Weak",
            2 => "Fair",
            3 => "Good",
            _ => "Excellent",
        }
    }

    /// The modem has no SIM card to use.
    pub fn no_sim(&self) -> bool {
        self.sim_id.is_none() && self.lock == Lock::None
    }

    pub fn locked(&self) -> bool {
        self.lock != Lock::None
    }

    /// The modem is switched off (low power or disabled).
    pub fn off(&self) -> bool {
        matches!(self.state.as_str(), "disabled" | "disabling")
    }

    pub fn roaming(&self) -> bool {
        self.registration.starts_with("roaming")
    }

    pub fn registered(&self) -> bool {
        matches!(
            self.registration.as_str(),
            "home" | "roaming" | "home-sms-only" | "roaming-sms-only" | "attached-rlos"
        ) || self.registration.starts_with("home-csfb")
            || self.registration.starts_with("roaming-csfb")
    }

    /// The carrier's name: the network the modem is on, else the SIM's.
    pub fn carrier(&self) -> Option<String> {
        [
            Some(&self.operator),
            self.sim.as_ref().map(|sim| &sim.operator),
        ]
        .into_iter()
        .flatten()
        .find(|name| !name.is_empty())
        .cloned()
    }

    /// The registration as a sentence.
    pub fn registration_label(&self) -> &'static str {
        match self.registration.as_str() {
            "home" => "Home network",
            "roaming" => "Roaming",
            "searching" => "Searching",
            "denied" => "Refused",
            "idle" => "Not registered",
            "home-sms-only" | "roaming-sms-only" => "Texts only",
            "emergency-only" => "Emergency only",
            "" | "unknown" => "Unknown",
            _ => "Registered",
        }
    }

    /// Attempts left for the code the SIM asks for now.
    pub fn attempts_left(&self) -> Option<u32> {
        let name = match &self.lock {
            Lock::Pin => "sim-pin",
            Lock::Puk => "sim-puk",
            _ => return None,
        };
        self.retries
            .iter()
            .find(|(code, _)| code == name)
            .map(|(_, left)| *left)
    }

    /// The network interface of the data session: the modem's own, else the
    /// one NetworkManager lists for it.
    pub fn interface_or(&self, fallback: &str) -> String {
        if self.interface.is_empty() {
            fallback.to_string()
        } else {
            self.interface.clone()
        }
    }

    /// The Preferred network type list: one entry per set of allowed
    /// generations the modem supports, ordered from the widest.
    pub fn mode_choices(&self) -> Vec<ModeChoice> {
        let mut every: Vec<Generation> = self
            .supported
            .iter()
            .flat_map(|modes| modes.allowed.iter().copied())
            .chain(
                self.current
                    .iter()
                    .flat_map(|modes| modes.allowed.iter().copied()),
            )
            .collect();
        every.sort();
        every.dedup();
        let mut choices: Vec<ModeChoice> = Vec::new();
        // Each set once, with the combination that tries its newest
        // generation first; the first listed one when none does.
        for modes in &self.supported {
            let better = |other: &Modes| other.preferred == other.highest();
            match choices
                .iter_mut()
                .find(|choice| choice.modes.allowed == modes.allowed)
            {
                Some(choice) if !better(&choice.modes) && better(modes) => {
                    choice.modes = modes.clone();
                }
                Some(_) => {}
                None => choices.push(ModeChoice {
                    label: String::new(),
                    modes: modes.clone(),
                }),
            }
        }
        // A set set from elsewhere is still shown, and selected.
        if let Some(current) = &self.current
            && !choices
                .iter()
                .any(|choice| choice.modes.allowed == current.allowed)
        {
            choices.push(ModeChoice {
                label: String::new(),
                modes: current.clone(),
            });
        }
        for choice in &mut choices {
            let allowed = &choice.modes.allowed;
            choice.label = if *allowed == every && allowed.len() > 1 {
                "Automatic".to_string()
            } else if let [only] = allowed.as_slice() {
                format!("{} only", only.label())
            } else {
                generation_list(allowed)
            };
        }
        choices.sort_by(|a, b| {
            b.modes
                .allowed
                .len()
                .cmp(&a.modes.allowed.len())
                .then(b.modes.allowed.cmp(&a.modes.allowed))
        });
        choices
    }

    /// The modem's modem-like face while a page waits for its first read:
    /// the same shape as a registered modem, with no values.
    pub fn placeholder() -> Self {
        Self {
            id: "0".into(),
            state: "registered".into(),
            failed_reason: String::new(),
            lock: Lock::None,
            retries: Vec::new(),
            registration: "home".into(),
            operator: "Carrier".into(),
            operator_code: String::new(),
            technologies: vec!["lte".into()],
            signal: Some(75),
            manufacturer: "Manufacturer".into(),
            model: "Model".into(),
            revision: "Revision".into(),
            imei: "000000000000000".into(),
            numbers: Vec::new(),
            device: String::new(),
            primary_port: String::new(),
            interface: String::new(),
            supported: vec![
                Modes {
                    allowed: vec![Generation::G2, Generation::G3, Generation::G4],
                    preferred: Some(Generation::G4),
                },
                Modes {
                    allowed: vec![Generation::G3],
                    preferred: None,
                },
            ],
            current: Some(Modes {
                allowed: vec![Generation::G2, Generation::G3, Generation::G4],
                preferred: Some(Generation::G4),
            }),
            sim_id: Some("0".into()),
            pin_lock: false,
            sim: Some(Sim {
                id: "0".into(),
                operator: "Carrier".into(),
                iccid: "0000000000000000000".into(),
                kind: "physical".into(),
            }),
        }
    }
}

/// The newest generation among ModemManager's access technology names.
fn technology_of(names: &[String]) -> Option<&'static str> {
    let rank = |name: &str| match name {
        "5gnr" => 5,
        "lte" | "lte-cat-m" | "lte-nb-iot" => 4,
        "hspa-plus" | "hspa" | "hsupa" | "hsdpa" | "umts" | "evdo0" | "evdoa" | "evdob" => 3,
        "edge" | "gprs" | "gsm" | "gsm-compact" | "1xrtt" => 2,
        _ => 0,
    };
    match names.iter().map(|name| rank(name)).max()? {
        5 => Some("5G"),
        4 => Some("4G"),
        3 => Some("3G"),
        2 => Some("2G"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

fn parse_json(output: &str) -> Result<Value, CommandError> {
    serde_json::from_str(output)
        .map_err(|_| CommandError("ModemManager gave an answer that cannot be read.".into()))
}

/// The ids of the modems in `mmcli -J -L` output.
pub fn parse_modem_list(output: &str) -> Result<Vec<String>, CommandError> {
    let value = parse_json(output)?;
    Ok(list(&value, &["modem-list"])
        .iter()
        .filter_map(|path| object_id(path))
        .collect())
}

/// `sim-pin (3)` as `("sim-pin", 3)`.
fn parse_retry(entry: &str) -> Option<(String, u32)> {
    let (name, rest) = entry.split_once('(')?;
    let left = rest.trim_end_matches(')').trim().parse().ok()?;
    Some((name.trim().to_string(), left))
}

/// `wwan0 (net)` among a modem's ports.
fn net_port(ports: &[String]) -> String {
    ports
        .iter()
        .find_map(|port| {
            let (name, kind) = port.split_once('(')?;
            (kind.trim_end_matches(')').trim() == "net").then(|| name.trim().to_string())
        })
        .unwrap_or_default()
}

/// One modem from `mmcli -J -m <id>` output; the SIM is read separately.
pub fn parse_modem(output: &str) -> Result<Modem, CommandError> {
    let value = parse_json(output)?;
    let modem = value
        .get("modem")
        .ok_or_else(|| CommandError("ModemManager did not describe a modem.".into()))?;
    let generic = |key: &str| text(modem, &["generic", key]);
    let id = object_id(&text(modem, &["dbus-path"])).unwrap_or_default();

    let supported_text = list(modem, &["generic", "supported-modes"]);
    let mut every: Vec<Generation> = Generation::ALL.to_vec();
    let supported: Vec<Modes> = supported_text
        .iter()
        .filter_map(|entry| Modes::parse(entry, &every))
        .collect();
    // `any` means everything the modem can do.
    every = supported
        .iter()
        .flat_map(|modes| modes.allowed.iter().copied())
        .collect();
    every.sort();
    every.dedup();
    let current = Modes::parse(&generic("current-modes"), &every);

    let signal = text(modem, &["generic", "signal-quality", "value"])
        .parse::<u8>()
        .ok()
        .map(|quality| quality.min(100));
    let sim_path = generic("sim");
    Ok(Modem {
        id,
        state: generic("state"),
        failed_reason: generic("state-failed-reason"),
        lock: Lock::parse(&generic("unlock-required")),
        retries: list(modem, &["generic", "unlock-retries"])
            .iter()
            .filter_map(|entry| parse_retry(entry))
            .collect(),
        registration: text(modem, &["3gpp", "registration-state"]),
        operator: text(modem, &["3gpp", "operator-name"]),
        operator_code: text(modem, &["3gpp", "operator-code"]),
        technologies: list(modem, &["generic", "access-technologies"]),
        signal,
        manufacturer: generic("manufacturer"),
        model: generic("model"),
        revision: generic("revision"),
        imei: {
            let imei = text(modem, &["3gpp", "imei"]);
            if imei.is_empty() {
                generic("equipment-identifier")
            } else {
                imei
            }
        },
        numbers: list(modem, &["generic", "own-numbers"]),
        device: generic("device"),
        primary_port: generic("primary-port"),
        interface: net_port(&list(modem, &["generic", "ports"])),
        supported,
        current,
        sim_id: object_id(&sim_path),
        pin_lock: list(modem, &["3gpp", "enabled-locks"])
            .iter()
            .any(|lock| lock == "sim" || lock.starts_with("sim-pin")),
        sim: None,
    })
}

/// One SIM from `mmcli -J -i <id>` output.
pub fn parse_sim(output: &str) -> Result<Sim, CommandError> {
    let value = parse_json(output)?;
    let sim = value
        .get("sim")
        .ok_or_else(|| CommandError("ModemManager did not describe a SIM.".into()))?;
    Ok(Sim {
        id: object_id(&text(sim, &["dbus-path"])).unwrap_or_default(),
        operator: text(sim, &["properties", "operator-name"]),
        iccid: text(sim, &["properties", "iccid"]),
        kind: text(sim, &["properties", "sim-type"]),
    })
}

/// One carrier from a network scan.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Carrier {
    /// The operator code, MCC and MNC together: `23415`.
    pub code: String,
    pub name: String,
    /// `5G`, `4G`, ... or empty.
    pub technology: String,
    /// `available`, `current`, `forbidden` or `unknown`.
    pub availability: String,
}

impl Carrier {
    /// `23415 - Vodafone UK (lte, available)`.
    fn parse(entry: &str) -> Option<Self> {
        let (code, rest) = entry.split_once(" - ")?;
        let (name, detail) = rest.rsplit_once(" (")?;
        let detail = detail.strip_suffix(')')?;
        let (technologies, availability) = detail.rsplit_once(',')?;
        let technology = technologies
            .split(',')
            .filter_map(|name| technology_of(&[name.trim().to_string()]))
            .max()
            .unwrap_or_default()
            .to_string();
        Some(Self {
            code: code.trim().to_string(),
            name: if name.trim() == "--" {
                String::new()
            } else {
                name.trim().to_string()
            },
            technology,
            availability: availability.trim().to_string(),
        })
    }

    pub fn current(&self) -> bool {
        self.availability == "current"
    }

    pub fn forbidden(&self) -> bool {
        self.availability == "forbidden"
    }

    pub fn title(&self) -> String {
        if self.name.is_empty() {
            self.code.clone()
        } else {
            self.name.clone()
        }
    }
}

/// The carriers of `mmcli -J -m <id> --3gpp-scan` output.
pub fn parse_scan(output: &str) -> Result<Vec<Carrier>, CommandError> {
    let value = parse_json(output)?;
    let mut carriers: Vec<Carrier> = list(&value, &["modem", "3gpp", "scan-networks"])
        .iter()
        .filter_map(|entry| Carrier::parse(entry))
        .collect();
    // The same operator is listed once per technology; keep the newest.
    carriers.sort_by(|a, b| a.code.cmp(&b.code).then(b.technology.cmp(&a.technology)));
    carriers.dedup_by(|later, first| later.code == first.code);
    carriers.sort_by_key(|carrier| (!carrier.current(), carrier.forbidden(), carrier.title()));
    Ok(carriers)
}

// ---------------------------------------------------------------------------
// Reading and commands
// ---------------------------------------------------------------------------

/// Read the first modem. `Ok(None)` when ModemManager has none.
pub fn load_modem() -> Result<Option<Modem>, CommandError> {
    #[cfg(feature = "snapshot")]
    if let Some(scenario) = fake::scenario() {
        return fake::modem(&scenario);
    }
    let Some(id) = parse_modem_list(&mmcli(&["-J", "-L"], READ)?)?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let mut modem = parse_modem(&mmcli(&["-J", "-m", &id], READ)?)?;
    if modem.id.is_empty() {
        modem.id = id;
    }
    if let Some(sim) = modem.sim_id.clone() {
        // A SIM that cannot be read leaves the modem without SIM details;
        // the modem's own state still says what is wrong.
        modem.sim = mmcli(&["-J", "-i", &sim], READ)
            .and_then(|output| parse_sim(&output))
            .ok();
    }
    Ok(Some(modem))
}

/// Allow only `modes.allowed`, trying `modes.preferred` first.
pub fn set_modes(modem: &str, modes: &Modes) -> Result<(), CommandError> {
    let allowed = format!("--set-allowed-modes={}", mode_key(&modes.allowed));
    let preferred = modes
        .preferred
        .map(|generation| format!("--set-preferred-mode={}", generation.argument()));
    let mut args = vec!["-m", modem, allowed.as_str()];
    if let Some(preferred) = preferred.as_deref() {
        args.push(preferred);
    }
    mmcli(&args, COMMAND).map(|_| ())
}

/// Scan for carriers. Slow: the modem listens on every band.
pub fn scan(modem: &str) -> Result<Vec<Carrier>, CommandError> {
    #[cfg(feature = "snapshot")]
    if let Some(scenario) = fake::scenario() {
        return fake::scan(&scenario);
    }
    parse_scan(&mmcli(&["-J", "-m", modem, "--3gpp-scan", SCAN_ARG], SCAN)?)
}

/// Register on the carrier with `code`.
pub fn register(modem: &str, code: &str) -> Result<(), CommandError> {
    let operator = format!("--3gpp-register-in-operator={code}");
    mmcli(&["-m", modem, operator.as_str(), SCAN_ARG], SCAN).map(|_| ())
}

/// Register on the home network, as an automatic choice does.
pub fn register_home(modem: &str) -> Result<(), CommandError> {
    mmcli(&["-m", modem, "--3gpp-register-home", SCAN_ARG], SCAN).map(|_| ())
}

// The PIN codes travel as `mmcli` arguments, which other users of this
// computer can see in the process list for the moment the command runs.
// ModemManager has no other command line interface for them.

/// Send the SIM's PIN.
pub fn unlock_pin(sim: &str, pin: &str) -> Result<(), CommandError> {
    let pin = format!("--pin={pin}");
    mmcli(&["-i", sim, pin.as_str()], COMMAND).map(|_| ())
}

/// Send the PUK and the PIN that replaces the blocked one.
pub fn unlock_puk(sim: &str, puk: &str, new_pin: &str) -> Result<(), CommandError> {
    let puk = format!("--puk={puk}");
    let pin = format!("--pin={new_pin}");
    mmcli(&["-i", sim, puk.as_str(), pin.as_str()], COMMAND).map(|_| ())
}

/// Ask for the PIN at start, or stop asking.
pub fn set_pin_lock(sim: &str, pin: &str, on: bool) -> Result<(), CommandError> {
    let pin = format!("--pin={pin}");
    let action = if on { "--enable-pin" } else { "--disable-pin" };
    mmcli(&["-i", sim, pin.as_str(), action], COMMAND).map(|_| ())
}

/// Replace the PIN.
pub fn change_pin(sim: &str, old: &str, new: &str) -> Result<(), CommandError> {
    let old = format!("--pin={old}");
    let new = format!("--change-pin={new}");
    mmcli(&["-i", sim, old.as_str(), new.as_str()], COMMAND).map(|_| ())
}

/// A SIM PIN is four to eight digits.
pub fn pin_error(text: &str) -> Result<(), String> {
    if (4..=8).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err("Enter 4 to 8 digits.".to_string())
    }
}

/// A PUK is eight digits.
pub fn puk_error(text: &str) -> Result<(), String> {
    if text.len() == 8 && text.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err("The PUK has 8 digits.".to_string())
    }
}

// ---------------------------------------------------------------------------
// Snapshot stand-in
// ---------------------------------------------------------------------------

/// `BINGUX_SETTINGS_FAKE_MODEM=ok|no-sim|locked|roaming|none` stands in a modem
/// in snapshot builds; `puk` (a blocked SIM) and `pin-lock` (the `ok` modem
/// with the SIM lock on) are two more. The answers are recorded `mmcli` output, read by the
/// parsers above, so the screens are drawn from the same code path a real
/// modem takes. Release builds never read the variable.
#[cfg(feature = "snapshot")]
pub mod fake {
    use super::fixtures::*;
    use super::*;

    pub fn scenario() -> Option<String> {
        std::env::var("BINGUX_SETTINGS_FAKE_MODEM").ok()
    }

    pub fn modem(scenario: &str) -> Result<Option<Modem>, CommandError> {
        let Some(id) = parse_modem_list(list_json(scenario))?.into_iter().next() else {
            return Ok(None);
        };
        let mut modem = parse_modem(&modem_json(scenario))?;
        if modem.id.is_empty() {
            modem.id = id;
        }
        if modem.sim_id.is_some() {
            modem.sim = Some(parse_sim(SIM)?);
        }
        Ok(Some(modem))
    }

    pub fn scan(scenario: &str) -> Result<Vec<Carrier>, CommandError> {
        if scenario == "none" {
            return Err(CommandError("No modems were found".into()));
        }
        // A real scan takes a minute or more; a few seconds is enough to
        // see the searching state.
        std::thread::sleep(std::time::Duration::from_secs(3));
        parse_scan(SCAN_RESULT)
    }

    /// The locked SIM after three wrong PINs: it asks for the PUK.
    fn puk_locked() -> String {
        MODEM_LOCKED
            .replace(
                "\"unlock-required\": \"sim-pin\"",
                "\"unlock-required\": \"sim-puk\"",
            )
            .replace("sim-pin (3)", "sim-pin (0)")
    }

    /// The modem of `ok` whose SIM asks for its PIN at start.
    fn pin_locked() -> String {
        MODEM_OK.replace(
            "\"enabled-locks\": [\"fixed-dialing\"]",
            "\"enabled-locks\": [\"sim\", \"fixed-dialing\"]",
        )
    }

    pub fn list_json(scenario: &str) -> &'static str {
        if scenario == "none" { LIST_NONE } else { LIST }
    }

    /// The `mmcli -J -m 0` text of `scenario`.
    pub fn modem_json(scenario: &str) -> String {
        match scenario {
            "no-sim" => MODEM_NO_SIM.to_string(),
            "locked" => MODEM_LOCKED.to_string(),
            "roaming" => MODEM_ROAMING.to_string(),
            "puk" => puk_locked(),
            "pin-lock" => pin_locked(),
            _ => MODEM_OK.to_string(),
        }
    }
}

/// Recorded `mmcli -J` output, the documented shape of ModemManager 1.2x.
#[cfg(any(test, feature = "snapshot"))]
pub mod fixtures {
    pub const LIST: &str = r#"{"modem-list":["/org/freedesktop/ModemManager1/Modem/0"]}"#;
    pub const LIST_NONE: &str = r#"{"modem-list":[]}"#;

    /// A 5G phone modem, registered and connected.
    pub const MODEM_OK: &str = r#"{
    "modem": {
        "3gpp": {
            "5gnr": {"registration-settings": {"drx-cycle": "--", "mico-mode": "--"}},
            "enabled-locks": ["fixed-dialing"],
            "eps": {"initial-bearer": {"dbus-path": "--", "settings": {"apn": "--", "ip-type": "--", "password": "--", "user": "--"}}, "ue-mode-operation": "csps-1"},
            "imei": "353879234252633",
            "operator-code": "23415",
            "operator-name": "Vodafone UK",
            "packet-service-state": "attached",
            "pco": "--",
            "registration-state": "home"
        },
        "cdma": {"activation-state": "--", "cdma1x-registration-state": "--", "esn": "--", "evdo-registration-state": "--", "meid": "--", "nid": "--", "sid": "--"},
        "dbus-path": "/org/freedesktop/ModemManager1/Modem/0",
        "generic": {
            "access-technologies": ["5gnr"],
            "bearers": ["/org/freedesktop/ModemManager1/Bearer/0"],
            "carrier-configuration": "--",
            "carrier-configuration-revision": "--",
            "current-bands": ["utran-1", "utran-8", "eutran-1", "eutran-3", "eutran-20", "utran-1", "ngran-78"],
            "current-capabilities": ["gsm-umts, lte, 5gnr"],
            "current-modes": "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
            "device": "/sys/devices/platform/soc/1c00000.pcie/pci0000:00/0000:00:00.0",
            "device-identifier": "3a8f2c1d6e9b4f05a7c3d2e1f0b9a8c7d6e5f4a3",
            "drivers": ["mhi_net"],
            "equipment-identifier": "353879234252633",
            "hardware-revision": "10000",
            "manufacturer": "Qualcomm",
            "model": "Snapdragon X55 5G",
            "own-numbers": ["447700900123"],
            "plugin": "generic",
            "ports": ["mhi_hwip0 (net)", "wwan0qmi0 (qmi)", "wwan0mbim0 (mbim)"],
            "power-state": "on",
            "primary-port": "wwan0qmi0",
            "primary-sim-slot": "1",
            "revision": "MPSS.HI.4.3.c3-00330",
            "signal-quality": {"recent": "yes", "value": "74"},
            "sim": "/org/freedesktop/ModemManager1/SIM/0",
            "sim-slots": ["/org/freedesktop/ModemManager1/SIM/0", "/"],
            "state": "connected",
            "state-failed-reason": "--",
            "supported-bands": ["utran-1", "utran-8", "eutran-1", "eutran-3", "eutran-20", "ngran-78"],
            "supported-capabilities": ["gsm-umts, lte, 5gnr"],
            "supported-ip-families": ["ipv4", "ipv6", "ipv4v6"],
            "supported-modes": [
                "allowed: 2g; preferred: none",
                "allowed: 3g; preferred: none",
                "allowed: 4g; preferred: none",
                "allowed: 5g; preferred: none",
                "allowed: 2g, 3g; preferred: 3g",
                "allowed: 2g, 3g; preferred: 2g",
                "allowed: 3g, 4g; preferred: 4g",
                "allowed: 3g, 4g; preferred: 3g",
                "allowed: 3g, 4g, 5g; preferred: 5g",
                "allowed: 3g, 4g, 5g; preferred: 4g",
                "allowed: 4g, 5g; preferred: 5g",
                "allowed: 4g, 5g; preferred: 4g",
                "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
                "allowed: 2g, 3g, 4g, 5g; preferred: 4g"
            ],
            "unlock-required": "--",
            "unlock-retries": ["sim-pin (3)", "sim-puk (10)", "sim-pin2 (3)", "sim-puk2 (10)"]
        }
    }
}"#;

    /// The same modem with no SIM card: ModemManager reports it `failed`.
    pub const MODEM_NO_SIM: &str = r#"{
    "modem": {
        "3gpp": {"enabled-locks": "--", "imei": "353879234252633", "operator-code": "--", "operator-name": "--", "packet-service-state": "--", "pco": "--", "registration-state": "--"},
        "dbus-path": "/org/freedesktop/ModemManager1/Modem/0",
        "generic": {
            "access-technologies": "--",
            "bearers": "--",
            "current-modes": "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
            "device": "/sys/devices/platform/soc/1c00000.pcie/pci0000:00/0000:00:00.0",
            "drivers": ["mhi_net"],
            "equipment-identifier": "353879234252633",
            "hardware-revision": "10000",
            "manufacturer": "Qualcomm",
            "model": "Snapdragon X55 5G",
            "own-numbers": "--",
            "plugin": "generic",
            "ports": ["mhi_hwip0 (net)", "wwan0qmi0 (qmi)"],
            "power-state": "on",
            "primary-port": "wwan0qmi0",
            "revision": "MPSS.HI.4.3.c3-00330",
            "signal-quality": {"recent": "no", "value": "0"},
            "sim": "--",
            "state": "failed",
            "state-failed-reason": "sim-missing",
            "supported-modes": [
                "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
                "allowed: 3g, 4g, 5g; preferred: 5g"
            ],
            "unlock-required": "--",
            "unlock-retries": "--"
        }
    }
}"#;

    /// A SIM that asks for its PIN, with three tries left.
    pub const MODEM_LOCKED: &str = r#"{
    "modem": {
        "3gpp": {"enabled-locks": ["sim"], "imei": "353879234252633", "operator-code": "--", "operator-name": "--", "packet-service-state": "--", "pco": "--", "registration-state": "--"},
        "dbus-path": "/org/freedesktop/ModemManager1/Modem/0",
        "generic": {
            "access-technologies": "--",
            "bearers": "--",
            "current-modes": "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
            "device": "/sys/devices/platform/soc/1c00000.pcie/pci0000:00/0000:00:00.0",
            "drivers": ["mhi_net"],
            "equipment-identifier": "353879234252633",
            "hardware-revision": "10000",
            "manufacturer": "Qualcomm",
            "model": "Snapdragon X55 5G",
            "own-numbers": "--",
            "plugin": "generic",
            "ports": ["mhi_hwip0 (net)", "wwan0qmi0 (qmi)"],
            "power-state": "on",
            "primary-port": "wwan0qmi0",
            "revision": "MPSS.HI.4.3.c3-00330",
            "signal-quality": {"recent": "no", "value": "0"},
            "sim": "/org/freedesktop/ModemManager1/SIM/0",
            "state": "locked",
            "state-failed-reason": "--",
            "supported-modes": [
                "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
                "allowed: 3g, 4g, 5g; preferred: 5g"
            ],
            "unlock-required": "sim-pin",
            "unlock-retries": ["sim-pin (3)", "sim-puk (10)", "sim-pin2 (3)", "sim-puk2 (10)"]
        }
    }
}"#;

    /// Registered on a foreign network over LTE.
    pub const MODEM_ROAMING: &str = r#"{
    "modem": {
        "3gpp": {"enabled-locks": ["fixed-dialing"], "imei": "353879234252633", "operator-code": "20801", "operator-name": "Orange F", "packet-service-state": "attached", "pco": "--", "registration-state": "roaming"},
        "dbus-path": "/org/freedesktop/ModemManager1/Modem/0",
        "generic": {
            "access-technologies": ["lte"],
            "bearers": ["/org/freedesktop/ModemManager1/Bearer/1"],
            "current-modes": "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
            "device": "/sys/devices/platform/soc/1c00000.pcie/pci0000:00/0000:00:00.0",
            "drivers": ["mhi_net"],
            "equipment-identifier": "353879234252633",
            "hardware-revision": "10000",
            "manufacturer": "Qualcomm",
            "model": "Snapdragon X55 5G",
            "own-numbers": ["447700900123"],
            "plugin": "generic",
            "ports": ["mhi_hwip0 (net)", "wwan0qmi0 (qmi)"],
            "power-state": "on",
            "primary-port": "wwan0qmi0",
            "revision": "MPSS.HI.4.3.c3-00330",
            "signal-quality": {"recent": "yes", "value": "48"},
            "sim": "/org/freedesktop/ModemManager1/SIM/0",
            "state": "connected",
            "state-failed-reason": "--",
            "supported-modes": [
                "allowed: 2g, 3g, 4g, 5g; preferred: 5g",
                "allowed: 3g, 4g, 5g; preferred: 5g",
                "allowed: 4g; preferred: none"
            ],
            "unlock-required": "--",
            "unlock-retries": ["sim-pin (3)", "sim-puk (10)"]
        }
    }
}"#;

    pub const SIM: &str = r#"{
    "sim": {
        "dbus-path": "/org/freedesktop/ModemManager1/SIM/0",
        "properties": {
            "active": "yes",
            "emergency-numbers": ["999", "112"],
            "eid": "--",
            "esim-status": "--",
            "gid1": "--",
            "gid2": "--",
            "iccid": "8944100000000012345",
            "imsi": "234150000012345",
            "operator-code": "23415",
            "operator-name": "Vodafone UK",
            "removability": "removable",
            "sim-type": "physical"
        }
    }
}"#;

    /// What `mmcli --3gpp-scan` printed on a UK phone: one operator listed
    /// once per technology, one forbidden.
    pub const SCAN_RESULT: &str = r#"{
    "modem": {
        "3gpp": {
            "scan-networks": [
                "23415 - Vodafone UK (lte, current)",
                "23415 - Vodafone UK (umts, available)",
                "23410 - O2 - UK (lte, available)",
                "23430 - EE (lte, available)",
                "23430 - EE (gsm, available)",
                "23420 - 3 (umts, available)",
                "23455 - Sure Mobile (gsm, forbidden)"
            ]
        }
    }
}"#;
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn modem(json: &str) -> Modem {
        let mut modem = parse_modem(json).unwrap();
        if modem.sim_id.is_some() {
            modem.sim = Some(parse_sim(SIM).unwrap());
        }
        modem
    }

    #[test]
    fn lists_modem_ids() {
        assert_eq!(parse_modem_list(LIST).unwrap(), vec!["0".to_string()]);
        assert!(parse_modem_list(LIST_NONE).unwrap().is_empty());
        assert!(parse_modem_list("not json").is_err());
    }

    #[test]
    fn registered_5g_modem() {
        let modem = modem(MODEM_OK);
        assert_eq!(modem.id, "0");
        assert_eq!(modem.state, "connected");
        assert_eq!(modem.technology(), Some("5G"));
        assert_eq!(modem.signal, Some(74));
        assert_eq!(modem.bars(), 3);
        assert_eq!(modem.signal_label(), "Good");
        assert_eq!(modem.registration, "home");
        assert!(modem.registered() && !modem.roaming());
        assert_eq!(modem.carrier().as_deref(), Some("Vodafone UK"));
        assert_eq!(modem.imei, "353879234252633");
        assert_eq!(modem.numbers, vec!["447700900123".to_string()]);
        assert_eq!(modem.interface, "mhi_hwip0");
        assert_eq!(modem.primary_port, "wwan0qmi0");
        assert_eq!(modem.lock, Lock::None);
        assert_eq!(modem.sim_id.as_deref(), Some("0"));
        assert!(!modem.pin_lock, "fixed-dialing is not the PIN lock");
        assert_eq!(
            modem.retries,
            vec![
                ("sim-pin".to_string(), 3),
                ("sim-puk".to_string(), 10),
                ("sim-pin2".to_string(), 3),
                ("sim-puk2".to_string(), 10)
            ]
        );
        assert!(!modem.no_sim() && !modem.locked() && !modem.off());
    }

    #[test]
    fn no_sim_modem() {
        let modem = modem(MODEM_NO_SIM);
        assert_eq!(modem.state, "failed");
        assert_eq!(modem.failed_reason, "sim-missing");
        assert!(modem.no_sim());
        assert!(modem.numbers.is_empty());
        assert!(modem.technologies.is_empty());
        assert_eq!(modem.technology(), None);
        assert_eq!(modem.carrier(), None);
        assert_eq!(modem.registration_label(), "Unknown");
    }

    #[test]
    fn locked_modem() {
        let modem = modem(MODEM_LOCKED);
        assert_eq!(modem.lock, Lock::Pin);
        assert!(modem.locked() && !modem.no_sim());
        assert_eq!(modem.attempts_left(), Some(3));
        assert!(modem.pin_lock, "the sim facility lock is the PIN lock");
    }

    #[cfg(feature = "snapshot")]
    #[test]
    fn derived_scenarios_parse() {
        use super::fake;
        let puk = parse_modem(&fake::modem_json("puk")).unwrap();
        assert_eq!(puk.lock, Lock::Puk);
        assert_eq!(puk.attempts_left(), Some(10));
        assert!(puk.retries.contains(&("sim-pin".to_string(), 0)));
        let locked = parse_modem(&fake::modem_json("pin-lock")).unwrap();
        assert!(locked.pin_lock);
        assert_eq!(locked.lock, Lock::None);
    }

    #[test]
    fn puk_lock_counts_puk_attempts() {
        let mut modem = modem(MODEM_LOCKED);
        modem.lock = Lock::Puk;
        assert_eq!(modem.attempts_left(), Some(10));
        modem.lock = Lock::Other("ph-net-pin".into());
        assert_eq!(modem.attempts_left(), None);
    }

    #[test]
    fn roaming_modem() {
        let modem = modem(MODEM_ROAMING);
        assert!(modem.roaming() && modem.registered());
        assert_eq!(modem.technology(), Some("4G"));
        assert_eq!(modem.carrier().as_deref(), Some("Orange F"));
        assert_eq!(modem.registration_label(), "Roaming");
    }

    #[test]
    fn technology_is_the_newest_listed() {
        let mut modem = modem(MODEM_OK);
        for (names, expected) in [
            (vec!["gsm"], Some("2G")),
            (vec!["edge", "gprs"], Some("2G")),
            (vec!["umts", "hspa-plus"], Some("3G")),
            (vec!["lte", "umts"], Some("4G")),
            (vec!["5gnr", "lte"], Some("5G")),
            (vec!["unknown"], None),
        ] {
            modem.technologies = names.into_iter().map(String::from).collect();
            assert_eq!(modem.technology(), expected);
        }
    }

    #[test]
    fn signal_bars() {
        let mut modem = modem(MODEM_OK);
        for (signal, bars) in [
            (None, 0),
            (Some(0), 0),
            (Some(1), 1),
            (Some(25), 1),
            (Some(26), 2),
            (Some(50), 2),
            (Some(51), 3),
            (Some(75), 3),
            (Some(76), 4),
            (Some(100), 4),
        ] {
            modem.signal = signal;
            assert_eq!(modem.bars(), bars, "{signal:?}");
        }
    }

    #[test]
    fn sim_details() {
        let sim = parse_sim(SIM).unwrap();
        assert_eq!(sim.id, "0");
        assert_eq!(sim.operator, "Vodafone UK");
        assert_eq!(sim.iccid, "8944100000000012345");
        assert_eq!(sim.kind, "physical");
        assert!(parse_sim("{}").is_err());
    }

    #[test]
    fn carrier_falls_back_to_the_sim() {
        let mut modem = modem(MODEM_LOCKED);
        assert_eq!(modem.carrier(), Some("Vodafone UK".to_string()));
        modem.sim = None;
        assert_eq!(modem.carrier(), None);
    }

    #[test]
    fn mode_sets_parse() {
        let every = Generation::ALL;
        let parsed = Modes::parse("allowed: 3g, 2g, 4g; preferred: 4g", &every).unwrap();
        assert_eq!(
            parsed.allowed,
            vec![Generation::G2, Generation::G3, Generation::G4]
        );
        assert_eq!(parsed.preferred, Some(Generation::G4));
        let none = Modes::parse("allowed: 2g; preferred: none", &every).unwrap();
        assert_eq!(none.preferred, None);
        let any = Modes::parse(
            "allowed: any; preferred: none",
            &[Generation::G3, Generation::G4],
        )
        .unwrap();
        assert_eq!(any.allowed, vec![Generation::G3, Generation::G4]);
        assert!(Modes::parse("nonsense", &every).is_none());
        assert!(Modes::parse("allowed: ; preferred: none", &every).is_none());
    }

    #[test]
    fn mode_choices_are_one_per_allowed_set() {
        let modem = modem(MODEM_OK);
        let choices = modem.mode_choices();
        let labels: Vec<&str> = choices.iter().map(|choice| choice.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Automatic",
                "5G/4G/3G",
                "5G/4G",
                "4G/3G",
                "3G/2G",
                "5G only",
                "4G only",
                "3G only",
                "2G only"
            ]
        );
        // The set tries its newest generation first.
        assert_eq!(choices[0].modes.preferred, Some(Generation::G5));
        assert_eq!(choices[3].modes.preferred, Some(Generation::G4));
        assert_eq!(choices[0].key(), "2g|3g|4g|5g");
        // The one that is current has the same key.
        let current = modem.current.as_ref().unwrap();
        assert_eq!(mode_key(&current.allowed), choices[0].key());
    }

    #[test]
    fn a_custom_current_set_is_listed() {
        let mut modem = modem(MODEM_ROAMING);
        modem.current = Some(Modes {
            allowed: vec![Generation::G2, Generation::G4],
            preferred: None,
        });
        assert!(
            modem
                .mode_choices()
                .iter()
                .any(|choice| choice.key() == "2g|4g" && choice.label == "4G/2G")
        );
    }

    #[test]
    fn scan_results() {
        let carriers = parse_scan(SCAN_RESULT).unwrap();
        let summary: Vec<(String, String, String)> = carriers
            .iter()
            .map(|c| (c.title(), c.technology.clone(), c.availability.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Vodafone UK".into(), "4G".into(), "current".into()),
                ("3".into(), "3G".into(), "available".into()),
                ("EE".into(), "4G".into(), "available".into()),
                ("O2 - UK".into(), "4G".into(), "available".into()),
                ("Sure Mobile".into(), "2G".into(), "forbidden".into()),
            ]
        );
        assert!(carriers[0].current());
        assert!(carriers[4].forbidden());
        assert_eq!(carriers[3].code, "23410");
    }

    #[test]
    fn scan_entry_without_a_name_uses_the_code() {
        let carrier = Carrier::parse("26201 - -- (umts, available)").unwrap();
        assert_eq!(carrier.title(), "26201");
        assert!(Carrier::parse("garbage").is_none());
    }

    #[test]
    fn empty_scan_is_empty() {
        assert!(
            parse_scan(r#"{"modem":{"3gpp":{"scan-networks":"--"}}}"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn pins_are_four_to_eight_digits() {
        assert!(pin_error("1234").is_ok());
        assert!(pin_error("12345678").is_ok());
        for bad in ["", "123", "123456789", "12a4", "12 34", "١٢٣٤"] {
            assert!(pin_error(bad).is_err(), "{bad:?}");
        }
        assert!(puk_error("12345678").is_ok());
        assert!(puk_error("1234567").is_err());
        assert!(puk_error("1234567a").is_err());
    }

    #[test]
    fn friendly_errors() {
        assert_eq!(
            friendly("Couldn't send PIN to the SIM: 'GDBus.Error:org.freedesktop.ModemManager1.Error.MobileEquipment.IncorrectPassword: Incorrect password'".into()),
            "That PIN is not right."
        );
        assert_eq!(friendly("Something else".into()), "Something else");
    }

    #[test]
    fn retry_entries() {
        assert_eq!(parse_retry("sim-pin (3)"), Some(("sim-pin".into(), 3)));
        assert_eq!(parse_retry("sim-pin"), None);
    }

    #[test]
    fn ports_find_the_net_interface() {
        let ports = vec!["ttyUSB0 (ignored)".to_string(), "wwan0 (net)".to_string()];
        assert_eq!(net_port(&ports), "wwan0");
        assert_eq!(net_port(&[]), "");
    }
}
