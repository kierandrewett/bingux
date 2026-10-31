//! The completion engine of the location field, on real folders: a typed path
//! completed one segment at a time, hidden names, links, folders that cannot
//! be read, folders of tens of thousands of names, a call that is cancelled, and
//! names that read right to left.
//!
//! [`location::Completer`] is what the field asks on every keystroke from a
//! worker, so what matters here is what it answers and how long it takes to.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::listing::Cancel;
use crate::location::{CompleteOptions, Completer, Completion};

/// A folder of this test's own, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("bingux-complete-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a scratch folder");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn folder(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).expect("a folder");
        path
    }

    fn file(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().expect("a parent")).expect("a parent folder");
        fs::write(&path, b"x").expect("a file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // A folder a test made unreadable must be readable to be removed.
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ask(input: &str, base: &Path, home: &Path, options: CompleteOptions) -> Completion {
    Completer::new().complete(input, base, home, options, &Cancel::new())
}

fn names(found: &Completion) -> Vec<&str> {
    found
        .candidates
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect()
}

#[test]
fn an_absolute_path_completes_one_segment_at_a_time() {
    let scratch = Scratch::new("segments");
    scratch.folder("Documents/Reports/2024");
    scratch.folder("Documents/Receipts");
    scratch.file("Documents/Readme.txt");
    let root = scratch.path().display().to_string();
    let options = CompleteOptions::default();
    // The first segment: what the folder holds that starts with it.
    let first = ask(
        &format!("{root}/Doc"),
        scratch.path(),
        scratch.path(),
        options,
    );
    assert_eq!(names(&first), ["Documents"]);
    assert_eq!(
        first.apply(&format!("{root}/Doc")),
        format!("{root}/Documents/"),
        "the only match is a folder: a slash goes after it"
    );
    // The next segment is asked of the folder the first named.
    let second = ask(
        &format!("{root}/Documents/Re"),
        scratch.path(),
        scratch.path(),
        options,
    );
    assert_eq!(names(&second), ["Readme.txt", "Receipts", "Reports"]);
    assert_eq!(second.common, "Re", "what they share is what Tab fills in");
    let third = ask(
        &format!("{root}/Documents/Rep"),
        scratch.path(),
        scratch.path(),
        options,
    );
    assert_eq!(
        third.apply(&format!("{root}/Documents/Rep")),
        format!("{root}/Documents/Reports/")
    );
    // A slash with nothing after it lists the folder.
    let inside = ask(
        &format!("{root}/Documents/Reports/"),
        scratch.path(),
        scratch.path(),
        options,
    );
    assert_eq!(names(&inside), ["2024"]);
    assert!(inside.candidates[0].is_dir);
    // `..` walks up by name.
    let up = ask(
        &format!("{root}/Documents/Reports/../Rec"),
        scratch.path(),
        scratch.path(),
        options,
    );
    assert_eq!(names(&up), ["Receipts"]);
}

#[test]
fn a_home_path_and_a_relative_path_start_where_they_mean() {
    let home = Scratch::new("home");
    home.folder("Music/Albums");
    home.folder("Pictures");
    let here = home.folder("Pictures");
    fs::create_dir_all(here.join("2024")).unwrap();
    let options = CompleteOptions::default();
    let tilde = ask("~/Mu", &here, home.path(), options);
    assert_eq!(names(&tilde), ["Music"]);
    assert_eq!(tilde.dir, home.path());
    let deeper = ask("~/Music/Al", &here, home.path(), options);
    assert_eq!(names(&deeper), ["Albums"]);
    let just_home = ask("~/", &here, home.path(), options);
    assert_eq!(names(&just_home), ["Music", "Pictures"]);
    // Relative to the folder you are in, with and without `./`.
    assert_eq!(names(&ask("20", &here, home.path(), options)), ["2024"]);
    assert_eq!(names(&ask("./20", &here, home.path(), options)), ["2024"]);
    assert_eq!(
        names(&ask("../Mus", &here, home.path(), options)),
        ["Music"]
    );
}

#[test]
fn hidden_names_are_offered_after_a_dot_or_when_hidden_files_show() {
    let scratch = Scratch::new("hidden");
    for name in [".config", ".cache", "config.txt", "cache.txt"] {
        scratch.file(name);
    }
    let plain = ask(
        "c",
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert_eq!(
        names(&plain),
        ["cache.txt", "config.txt"],
        "dot files stay out of the way"
    );
    let dotted = ask(
        ".c",
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert_eq!(
        names(&dotted),
        [".cache", ".config"],
        "a typed dot asks for them"
    );
    let shown = ask(
        "c",
        scratch.path(),
        scratch.path(),
        CompleteOptions {
            show_hidden: true,
            ..CompleteOptions::default()
        },
    );
    assert_eq!(
        shown.candidates.len(),
        4,
        "Show hidden files offers them all"
    );
}

#[test]
fn a_link_to_a_folder_is_a_folder_and_a_broken_or_circular_link_is_not() {
    let scratch = Scratch::new("links");
    let target = scratch.folder("real");
    scratch.file("plain.txt");
    symlink(&target, scratch.path().join("ln-folder")).unwrap();
    symlink(
        scratch.path().join("plain.txt"),
        scratch.path().join("ln-file"),
    )
    .unwrap();
    symlink(
        scratch.path().join("nowhere"),
        scratch.path().join("ln-broken"),
    )
    .unwrap();
    symlink(
        scratch.path().join("ln-self"),
        scratch.path().join("ln-self"),
    )
    .unwrap();
    symlink("..", target.join("up")).unwrap();
    let found = ask(
        "ln-",
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    let kind = |name: &str| {
        found
            .candidates
            .iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| candidate.is_dir)
    };
    assert_eq!(kind("ln-folder"), Some(true));
    assert_eq!(kind("ln-file"), Some(false));
    assert_eq!(kind("ln-broken"), Some(false));
    assert_eq!(
        kind("ln-self"),
        Some(false),
        "a link to itself does not hang or count as a folder"
    );
    let folders = ask(
        "ln-",
        scratch.path(),
        scratch.path(),
        CompleteOptions {
            dirs_only: true,
            ..CompleteOptions::default()
        },
    );
    assert_eq!(names(&folders), ["ln-folder"]);
    // A path through a link continues inside what it leads to, and a loop of links is walked as far as it is typed.
    let through = ask(
        &format!("{}/ln-folder/u", scratch.path().display()),
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert_eq!(names(&through), ["up"]);
    let loops = ask(
        &format!("{}/real/up/real/up/re", scratch.path().display()),
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert_eq!(names(&loops), ["real"]);
}

#[test]
fn a_folder_that_cannot_be_read_answers_with_nothing_and_says_so_to_nobody() {
    let scratch = Scratch::new("denied");
    let locked = scratch.folder("locked");
    scratch.file("locked/secret.txt");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        // Running as a user the mode does not bind: there is nothing to prove here.
        return;
    }
    let started = Instant::now();
    let found = ask(
        &format!("{}/locked/s", scratch.path().display()),
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert!(
        found.candidates.is_empty(),
        "no names are made up for a folder that was not read"
    );
    assert!(
        !found.incomplete,
        "it is not a slow folder: asking again will not help"
    );
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "and it does not wait for it"
    );
    // The folder above is still completed, and the locked one is offered by name.
    let above = ask(
        "lo",
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert_eq!(names(&above), ["locked"]);
    // A path that is not there is the same: nothing, at once.
    let missing = ask(
        "no/such/th",
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert!(missing.candidates.is_empty() && !missing.incomplete);
}

#[test]
fn a_huge_folder_is_capped_by_the_limit_and_still_quick() {
    let scratch = Scratch::new("huge");
    for number in 0..20_000 {
        fs::write(scratch.path().join(format!("report-{number:05}.txt")), b"").unwrap();
    }
    scratch.folder("reports");
    let started = Instant::now();
    let found = ask(
        "rep",
        scratch.path(),
        scratch.path(),
        CompleteOptions {
            limit: 50,
            ..CompleteOptions::default()
        },
    );
    assert_eq!(
        found.candidates.len(),
        50,
        "no more than the limit are returned"
    );
    assert!(found.truncated, "and the caller is told there were more");
    assert!(
        found
            .candidates
            .iter()
            .all(|candidate| candidate.name.starts_with("report")),
        "all of them match"
    );
    assert_eq!(
        found.common, "report",
        "the common start is still known of all 20,001"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // The second keystroke is answered from the names already read.
    let again = Instant::now();
    let narrow = ask(
        "report-0199",
        scratch.path(),
        scratch.path(),
        CompleteOptions {
            limit: 50,
            ..CompleteOptions::default()
        },
    );
    assert_eq!(narrow.candidates.len(), 10);
    assert!(!narrow.truncated);
    assert!(
        again.elapsed() < Duration::from_millis(500),
        "{:?}",
        again.elapsed()
    );
}

#[test]
fn a_cancelled_request_comes_back_at_once_whatever_the_folder_holds() {
    let scratch = Scratch::new("cancel");
    for number in 0..30_000 {
        fs::write(scratch.path().join(format!("name-{number:05}")), b"").unwrap();
    }
    let cancel = Cancel::new();
    cancel.cancel();
    let started = Instant::now();
    let found = Completer::new().complete(
        "na",
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
        &cancel,
    );
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "a request that was overtaken does not hold the worker: {:?}",
        started.elapsed()
    );
    assert!(!found.incomplete, "being cancelled is not being slow");
    // And it is still usable: the next request, not cancelled, gets the names.
    let next = ask(
        "name-0000",
        scratch.path(),
        scratch.path(),
        CompleteOptions::default(),
    );
    assert_eq!(next.candidates.len(), 10);
}

#[test]
fn names_that_read_right_to_left_complete_by_their_bytes_and_keep_their_order() {
    let scratch = Scratch::new("rtl");
    for name in [
        "שלום עולם.txt",
        "שלום.txt",
        "مرحبا.txt",
        "report-שלום.txt",
        "plain.txt",
    ] {
        scratch.file(name);
    }
    let options = CompleteOptions::default();
    let hebrew = ask("של", scratch.path(), scratch.path(), options);
    let mut both = names(&hebrew);
    both.sort_unstable();
    assert_eq!(both.len(), 3, "two start with it and one has it inside");
    assert_eq!(hebrew.prefix_matches, 2);
    assert!(
        both.contains(&"שלום.txt")
            && both.contains(&"שלום עולם.txt")
            && both.contains(&"report-שלום.txt")
    );
    assert_eq!(hebrew.prefix, "של");
    assert_eq!(
        hebrew.common, "שלום",
        "what they share is cut on a character, never inside one"
    );
    assert_eq!(hebrew.apply("של"), "שלום");
    // Contained, not started: found after the names that start with it.
    let inside = ask("שלום", scratch.path(), scratch.path(), options);
    assert_eq!(inside.prefix_matches, 2);
    assert_eq!(inside.candidates.len(), 3);
    assert_eq!(inside.candidates[2].name, "report-שלום.txt");
    let arabic = ask("مر", scratch.path(), scratch.path(), options);
    assert_eq!(names(&arabic), ["مرحبا.txt"]);
}
