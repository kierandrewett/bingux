//! Random walks over one tab's history, checked against a browser's.
//!
//! The reference is the smallest thing that is a browser history: a list of
//! places and an index. Every operation the file view has is run on the real
//! [`NavModel`] through [`super::history`], and on the reference, and the two
//! must agree after every step. Beyond the comparison, each step checks what
//! must hold of any history (consistent ids and stacks, no invalid or
//! duplicate neighbouring entries, forward gone after a move, the cap, menus
//! that match the lists). The tabs around a history are fuzzed the same way in
//! the Files application (`bingux-files-app`, `fuzz`).

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

use super::directory::Where;
use super::folder_table::{folder_table, location_of, place_of};
use super::history::{self, Trail};
use crate::nav::{HISTORY_LIMIT, Location, NavModel, Outcome, RouteTable};

/// How many random histories are walked.
const SEQUENCES: u64 = 12_000;
/// How many operations each walk makes.
const STEPS: usize = 60;

/// A small deterministic generator, so a failing walk is its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// Folders a walk goes to: ordinary names, the root, one that is not UTF-8,
/// names that look like options and the recent files.
fn pool() -> Vec<Where> {
    let mut places: Vec<Where> = [
        "/",
        "/home/me",
        "/home/me/Documents",
        "/home/me/Documents/a",
        "/home/me/Documents/a/b",
        "/home/me/Downloads",
        "/tmp",
        "/tmp/x y",
        "/tmp/-x",
        "/tmp/--snapshot",
        "/var/lib/caf\u{e9}",
    ]
    .iter()
    .map(|path| Where::Folder(PathBuf::from(path)))
    .collect();
    places.push(Where::Folder(PathBuf::from(OsStr::from_bytes(b"/tmp/caf\xe9-\xff"))));
    places.push(Where::Recent);
    places
}

/// What a person or a command line might put in the address bar that is not
/// a place, or is one in disguise.
const TYPED: [&str; 14] = [
    "", "--snapshot", "-h", "rel", ".", "..", "~", "~/x", "/a/../b", "/home//me/",
    "/home/me/./Documents/../Documents", "/../..", "//", "/tmp/x y/",
];

/// The cleaned path a browser history would hold for `text`, written out
/// again here so the test does not lean on the code it tests.
fn cleaned(path: &std::path::Path) -> Option<PathBuf> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.first() != Some(&b'/') {
        return None;
    }
    let mut parts: Vec<&[u8]> = Vec::new();
    for part in bytes.split(|byte| *byte == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    let mut out = vec![b'/'];
    out.extend(parts.join(&b'/'));
    Some(PathBuf::from(OsStr::from_bytes(&out)))
}

fn reference_place(at: &Where) -> Option<Where> {
    match at {
        Where::Recent => Some(Where::Recent),
        Where::Folder(path) => cleaned(path).map(Where::Folder),
    }
}

/// A browser's history: places and where in them it is.
#[derive(Clone, Debug)]
struct Reference {
    chain: Vec<Where>,
    at: usize,
}

impl Reference {
    fn new(start: Where) -> Self {
        Self { chain: vec![start], at: 0 }
    }

    fn visit(&mut self, to: &Where) {
        let Some(to) = reference_place(to) else { return };
        if to == self.chain[self.at] {
            return;
        }
        self.chain.truncate(self.at + 1);
        self.chain.push(to);
        if self.chain.len() > HISTORY_LIMIT + 1 {
            self.chain.remove(0);
        }
        self.at = self.chain.len() - 1;
    }

    fn back(&mut self, steps: usize) {
        self.at -= steps.min(self.at);
    }

    fn forward(&mut self, steps: usize) {
        self.at = (self.at + steps).min(self.chain.len() - 1);
    }

    fn up(&mut self) {
        let Where::Folder(path) = &self.chain[self.at] else { return };
        if let Some(parent) = path.parent() {
            let parent = Where::Folder(parent.to_path_buf());
            self.visit(&parent);
        }
    }

    fn trail(&self) -> Trail {
        Trail {
            back: self.chain[..self.at].to_vec(),
            current: self.chain[self.at].clone(),
            forward: self.chain[self.at + 1..].to_vec(),
        }
    }
}

/// Everything that must be true of a history, whatever was done to it.
fn check(model: &NavModel, reference: &Reference, what: &str) {
    let trail = Trail::of(model);
    let wanted = reference.trail();
    assert_eq!(trail.back, wanted.back, "back after {what}");
    assert_eq!(trail.current, wanted.current, "current after {what}");
    // `Trail::forward` is nearest first, the reference's list runs the same way.
    assert_eq!(trail.forward, wanted.forward, "forward after {what}");

    assert!(model.back_entries().len() <= HISTORY_LIMIT, "the cap, after {what}");
    assert!(
        model.back_entries().len() + model.forward_entries().len() <= HISTORY_LIMIT,
        "back and forward together stay under the cap, after {what}"
    );

    // Entries: valid places, ids that are never shared, locations that read
    // back as themselves.
    let mut ids = HashSet::new();
    let entries = model
        .back_entries()
        .iter()
        .chain(std::iter::once(model.current()))
        .chain(model.forward_entries().iter());
    for entry in entries {
        assert!(ids.insert(entry.id), "entry id {} is in two places, after {what}", entry.id);
        match place_of(&entry.location) {
            Where::Folder(path) => {
                assert!(path.is_absolute(), "{path:?} is not absolute, after {what}");
                let text = path.as_os_str().as_bytes();
                assert!(!text.is_empty() && !text.windows(2).any(|pair| pair == b"//"), "{path:?}, after {what}");
                assert!(!text.split(|byte| *byte == b'/').any(|part| part == b"." || part == b".."), "{path:?}, after {what}");
            }
            Where::Recent => {}
        }
        // Reading back is the slow part: the newest entry is checked every step,
        // so every entry is checked once, as it arrives.
        if entry.id == model.current().id {
            let again = Location::parse(&entry.location.to_string());
            assert_eq!(again.as_ref(), Some(&entry.location), "{} reads back, after {what}", entry.location);
        }
    }

    // Neighbours are never the same place: going where you are records nothing.
    let chain: Vec<&Location> = model
        .back_entries()
        .iter()
        .chain(std::iter::once(model.current()))
        .chain(model.forward_entries().iter().rev())
        .map(|entry| &entry.location)
        .collect();
    for pair in chain.windows(2) {
        assert!(!pair[0].same_place(pair[1]), "{} twice in a row, after {what}", pair[0]);
    }

    // The buttons and their menus say what the lists hold.
    assert_eq!(model.can_go_back(), reference.at > 0, "Back enabled, after {what}");
    assert_eq!(model.can_forward(), reference.at + 1 < reference.chain.len(), "Forward enabled, after {what}");
    let back = model.back_menu();
    assert_eq!(back.len(), reference.at, "back menu length, after {what}");
    for (index, row) in back.iter().enumerate() {
        assert_eq!(row.steps, index + 1, "back menu steps, after {what}");
        assert!(!row.title.is_empty(), "a back row has a title, after {what}");
        assert_eq!(row.location, location_of(&reference.chain[reference.at - 1 - index]), "back row {index}, after {what}");
    }
    let forward = model.forward_menu();
    assert_eq!(forward.len(), reference.chain.len() - 1 - reference.at, "forward menu length, after {what}");
    for (index, row) in forward.iter().enumerate() {
        assert_eq!(row.steps, index + 1, "forward menu steps, after {what}");
        assert!(!row.title.is_empty(), "a forward row has a title, after {what}");
        assert_eq!(row.location, location_of(&reference.chain[reference.at + 1 + index]), "forward row {index}, after {what}");
    }
}

fn walk(table: &Arc<RouteTable>, places: &[Where], seed: u64) -> Coverage {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let start = places[rng.below(places.len())].clone();
    let mut model = NavModel::new(table.clone(), &location_of(&start));
    let mut reference = Reference::new(start);
    check(&model, &reference, "the start");
    // Some walks run long enough to reach the cap.
    let steps = if seed % 50 == 0 { HISTORY_LIMIT * 6 } else { STEPS };
    let mut seen = Coverage::default();
    for step in 0..steps {
        // Long walks mostly go somewhere new, so they reach the cap.
        let operation = if steps > STEPS && rng.below(4) != 0 { 3 + rng.below(2) } else { rng.below(12) };
        let was_forward = model.forward_entries().len();
        let (what, outcome) = match operation {
            0..=2 => {
                let to = places[rng.below(places.len())].clone();
                reference.visit(&to);
                (format!("visit {to:?}"), history::visit(&mut model, &to))
            }
            3 | 4 => {
                // A path that is made up, so the cap is reached too.
                let to = Where::Folder(PathBuf::from(format!("/walk/{}", rng.below(1000))));
                reference.visit(&to);
                (format!("visit {to:?}"), history::visit(&mut model, &to))
            }
            5 => {
                let text = TYPED[rng.below(TYPED.len())];
                let to = Where::Folder(PathBuf::from(text));
                reference.visit(&to);
                (format!("type {text:?}"), history::visit(&mut model, &to))
            }
            6 => {
                reference.up();
                ("up".to_string(), history::up(&mut model))
            }
            7 | 8 => {
                reference.back(1);
                ("back".to_string(), model.back())
            }
            9 => {
                reference.forward(1);
                ("forward".to_string(), model.forward())
            }
            10 => {
                let menu = model.back_menu();
                if menu.is_empty() {
                    continue;
                }
                let row = &menu[rng.below(menu.len())];
                reference.back(row.steps);
                (format!("back menu row {}", row.steps), model.pop_to(row.entry))
            }
            _ => {
                let menu = model.forward_menu();
                if menu.is_empty() {
                    continue;
                }
                let row = &menu[rng.below(menu.len())];
                reference.forward(row.steps);
                (format!("forward menu row {}", row.steps), model.forward_to(row.entry))
            }
        };
        let what = format!("step {step} ({what}) of seed {seed}");
        // A move that happened forgets forward; one that did not leaves it.
        match operation {
            0..=6 => match outcome {
                Outcome::Changed(_) => {
                    assert!(model.forward_entries().is_empty(), "forward is forgotten by a move, after {what}");
                    seen.moved += 1;
                    seen.forgot_forward += usize::from(was_forward > 0);
                }
                _ => {
                    assert_eq!(model.forward_entries().len(), was_forward, "a move that did nothing changed forward, after {what}");
                    seen.ignored += 1;
                }
            },
            10 | 11 => seen.menu_picks += 1,
            _ => {}
        }
        seen.capped += usize::from(model.back_entries().len() == HISTORY_LIMIT);
        check(&model, &reference, &what);
    }
    seen
}

/// What a run of walks did, so a test that passes on walks that did nothing
/// cannot pass.
#[derive(Default)]
struct Coverage {
    moved: usize,
    ignored: usize,
    forgot_forward: usize,
    menu_picks: usize,
    capped: usize,
}

#[test]
fn twelve_thousand_random_histories_agree_with_a_browsers() {
    let table = Arc::new(folder_table());
    let places = pool();
    let mut total = Coverage::default();
    for seed in 1..=SEQUENCES {
        let seen = walk(&table, &places, seed);
        total.moved += seen.moved;
        total.ignored += seen.ignored;
        total.forgot_forward += seen.forgot_forward;
        total.menu_picks += seen.menu_picks;
        total.capped += seen.capped;
    }
    eprintln!(
        "{SEQUENCES} walks: {} moves, {} ignored, forward forgotten {} times, {} menu picks, {} steps at the cap",
        total.moved, total.ignored, total.forgot_forward, total.menu_picks, total.capped
    );
    assert!(total.moved > 100_000 && total.ignored > 50_000, "moves and refusals both happen");
    assert!(total.forgot_forward > 10_000 && total.menu_picks > 10_000, "forward is forgotten and menus are used");
    assert!(total.capped > 1_000, "walks reach the cap");
}

#[test]
fn the_start_is_never_added_twice_whatever_it_was_asked_for() {
    let table = Arc::new(folder_table());
    for place in pool() {
        let mut model = NavModel::new(table.clone(), &location_of(&place));
        assert_eq!(history::visit(&mut model, &place), Outcome::Unchanged, "{place:?}");
        assert!(model.back_entries().is_empty() && model.forward_entries().is_empty());
    }
}
