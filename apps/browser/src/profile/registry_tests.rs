//! The registry's rules, on a real folder.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use super::files::remove_tree;
use super::files::tests::temp;
use super::lock::ProfileLock;
use super::places::Places;
use super::registry::*;

struct Rig {
    dir: PathBuf,
    profiles: Profiles,
}

impl Rig {
    fn new() -> Self {
        let dir = temp();
        let places = Places {
            data: dir.join("data/bingux-browser"),
            cache: dir.join("cache/bingux-browser"),
            state: dir.join("state/bingux-browser"),
            runtime: Some(dir.join("run/bingux-browser")),
            named: None,
        };
        Self {
            profiles: Profiles::new(places),
            dir,
        }
    }

    fn places(&self) -> &Places {
        self.profiles.places()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = remove_tree(&self.dir);
    }
}

fn names(profiles: &Profiles) -> Vec<String> {
    profiles
        .list()
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

#[test]
fn no_registry_reads_as_none_and_the_first_profile_becomes_the_default() {
    let rig = Rig::new();
    assert_eq!(rig.profiles.read().unwrap(), None);
    let personal = rig.profiles.create("Personal", "blue").unwrap();
    let work = rig.profiles.create("Work", "orange").unwrap();
    let registry = rig.profiles.read().unwrap().unwrap();
    assert_eq!(registry.version, VERSION);
    assert_eq!(registry.default.as_ref(), Some(&personal.id));
    assert_eq!(names(&rig.profiles), ["Personal", "Work"]);
    assert_ne!(personal.id, work.id);
    assert!(
        rig.places().folders(&work.id).engine.is_dir(),
        "the engine's folder is made with the profile"
    );
}

#[test]
fn folders_are_0700_and_the_registry_0600() {
    let rig = Rig::new();
    let entry = rig.profiles.create("Personal", "blue").unwrap();
    let folders = rig.places().folders(&entry.id);
    for dir in [
        &rig.places().data,
        &rig.places().profiles(),
        &folders.root,
        &folders.engine,
    ] {
        assert_eq!(
            fs::metadata(dir).unwrap().mode() & 0o777,
            0o700,
            "{}",
            dir.display()
        );
    }
    assert_eq!(
        fs::metadata(rig.places().registry()).unwrap().mode() & 0o777,
        0o600
    );
}

#[test]
fn a_name_is_trimmed_unique_without_case_and_at_most_64_characters() {
    let rig = Rig::new();
    let entry = rig.profiles.create("  Personal  ", "blue").unwrap();
    assert_eq!(entry.name, "Personal");
    assert_eq!(
        rig.profiles.create("personal", "blue"),
        Err(ChangeError::Name(NameProblem::Taken))
    );
    assert_eq!(
        rig.profiles.create("   ", "blue"),
        Err(ChangeError::Name(NameProblem::Empty))
    );
    assert_eq!(
        rig.profiles.create(&"é".repeat(65), "blue"),
        Err(ChangeError::Name(NameProblem::TooLong))
    );
    assert!(
        rig.profiles.create(&"é".repeat(64), "blue").is_ok(),
        "64 characters, not bytes"
    );
    assert_eq!(
        rig.profiles.create("Work", "magenta"),
        Err(ChangeError::UnknownColour)
    );
}

#[test]
fn rename_and_colour_change_only_the_registry() {
    let rig = Rig::new();
    let personal = rig.profiles.create("Personal", "blue").unwrap();
    rig.profiles.create("Work", "orange").unwrap();
    rig.profiles.rename(&personal.id, "Home").unwrap();
    rig.profiles.set_colour(&personal.id, "teal").unwrap();
    assert_eq!(
        rig.profiles.rename(&personal.id, "WORK"),
        Err(ChangeError::Name(NameProblem::Taken))
    );
    rig.profiles.rename(&personal.id, "home").unwrap();
    let entry = rig.profiles.list().unwrap().remove(0);
    assert_eq!(
        (entry.name.as_str(), entry.colour.as_str()),
        ("home", "teal")
    );
    assert!(
        rig.places().folders(&personal.id).root.is_dir(),
        "no folder moved"
    );
    let missing = Id::parse("aaaaaaaa").unwrap();
    assert_eq!(
        rig.profiles.rename(&missing, "X"),
        Err(ChangeError::NoSuchProfile)
    );
}

#[test]
fn startup_opens_the_default_unless_asked_and_ask_needs_two_profiles() {
    let rig = Rig::new();
    let personal = rig.profiles.create("Personal", "blue").unwrap();
    rig.profiles.set_ask_at_start(true).unwrap();
    assert_eq!(
        rig.profiles.startup(None).unwrap(),
        Startup::Open(personal.clone()),
        "one profile: nothing to ask"
    );
    let work = rig.profiles.create("Work", "orange").unwrap();
    assert!(matches!(rig.profiles.startup(None).unwrap(), Startup::Ask(list) if list.len() == 2));
    assert_eq!(
        rig.profiles.startup(Some("work")).unwrap(),
        Startup::Open(work.clone()),
        "by name, any case"
    );
    assert_eq!(
        rig.profiles.startup(Some(work.id.as_str())).unwrap(),
        Startup::Open(work.clone()),
        "by id"
    );
    assert_eq!(
        rig.profiles.startup(Some("Study")).unwrap(),
        Startup::Unknown("Study".to_string())
    );
    rig.profiles.set_ask_at_start(false).unwrap();
    rig.profiles.set_default(&work.id).unwrap();
    assert_eq!(rig.profiles.startup(None).unwrap(), Startup::Open(work));
}

#[test]
fn delete_refuses_the_last_profile_and_an_open_one() {
    let rig = Rig::new();
    let personal = rig.profiles.create("Personal", "blue").unwrap();
    assert_eq!(
        rig.profiles.delete(&personal.id),
        Err(ChangeError::LastProfile)
    );
    let work = rig.profiles.create("Work", "orange").unwrap();
    let held = ProfileLock::take(&rig.places().folders(&work.id)).unwrap();
    assert_eq!(rig.profiles.delete(&work.id), Err(ChangeError::InUse));
    drop(held);
    rig.profiles.delete(&work.id).unwrap();
    assert_eq!(names(&rig.profiles), ["Personal"]);
}

#[test]
fn delete_removes_every_folder_of_the_profile_and_moves_the_default() {
    let rig = Rig::new();
    let personal = rig.profiles.create("Personal", "blue").unwrap();
    let work = rig.profiles.create("Work", "orange").unwrap();
    let folders = rig.places().folders(&personal.id);
    fs::create_dir_all(&folders.cache).unwrap();
    fs::create_dir_all(&folders.crash).unwrap();
    fs::write(folders.engine.join("cookies.sqlite"), b"x").unwrap();
    assert!(rig.profiles.size(&personal.id) >= 1);
    rig.profiles.delete(&personal.id).unwrap();
    for dir in [
        &folders.root,
        &folders.cache,
        folders.crash.parent().unwrap(),
    ] {
        assert!(!dir.exists(), "{} is gone", dir.display());
    }
    let registry = rig.profiles.read().unwrap().unwrap();
    assert_eq!(registry.default.as_ref(), Some(&work.id));
}

#[test]
fn a_delete_cut_short_is_finished_at_the_next_start() {
    let rig = Rig::new();
    rig.profiles.create("Personal", "blue").unwrap();
    let work = rig.profiles.create("Work", "orange").unwrap();
    // The process died after marking the profile and before removing it.
    let mut text: serde_json::Value =
        serde_json::from_slice(&fs::read(rig.places().registry()).unwrap()).unwrap();
    text["profiles"][1]["deleting"] = serde_json::Value::Bool(true);
    fs::write(rig.places().registry(), serde_json::to_vec(&text).unwrap()).unwrap();
    assert_eq!(
        names(&rig.profiles),
        ["Personal"],
        "a profile being deleted is never listed"
    );
    assert_eq!(
        rig.profiles.startup(Some("Work")).unwrap(),
        Startup::Unknown("Work".to_string())
    );
    rig.profiles.finish_deletions().unwrap();
    assert!(!rig.places().folders(&work.id).root.exists());
    assert_eq!(rig.profiles.read().unwrap().unwrap().profiles.len(), 1);
}

#[test]
fn a_damaged_or_hand_edited_registry_is_refused_and_never_overwritten() {
    let rig = Rig::new();
    rig.profiles.create("Personal", "blue").unwrap();
    let path = rig.places().registry();
    fs::write(&path, b"{ not json").unwrap();
    assert!(matches!(
        rig.profiles.read(),
        Err(RegistryProblem::Damaged(_))
    ));
    assert!(matches!(
        rig.profiles.create("Work", "blue"),
        Err(ChangeError::Registry(RegistryProblem::Damaged(_)))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"{ not json", "left as it was");
    // An id that is not one the browser makes could name another folder.
    fs::write(
        &path,
        br#"{"version":1,"default":"../../x","ask_at_start":false,"profiles":[{"id":"../../x","name":"P","colour":"blue","created":1}]}"#,
    )
    .unwrap();
    assert!(matches!(
        rig.profiles.read(),
        Err(RegistryProblem::Damaged(_))
    ));
}

#[test]
fn a_registry_from_a_newer_browser_is_read_by_nobody_and_written_by_nobody() {
    let rig = Rig::new();
    let path = rig.places().registry();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let newer = br#"{"version":99,"default":null,"ask_at_start":false,"profiles":[]}"#;
    fs::write(&path, newer).unwrap();
    assert_eq!(
        rig.profiles.read(),
        Err(RegistryProblem::TooNew {
            found: 99,
            supported: VERSION
        })
    );
    assert!(rig.profiles.create("Personal", "blue").is_err());
    assert_eq!(fs::read(&path).unwrap(), newer);
}

#[test]
fn fields_a_newer_version_added_survive_a_change() {
    let rig = Rig::new();
    let entry = rig.profiles.create("Personal", "blue").unwrap();
    let mut text: serde_json::Value =
        serde_json::from_slice(&fs::read(rig.places().registry()).unwrap()).unwrap();
    text["sync"] = serde_json::json!({"account": "a"});
    text["profiles"][0]["avatar"] = serde_json::json!("cat");
    fs::write(rig.places().registry(), serde_json::to_vec(&text).unwrap()).unwrap();
    rig.profiles.rename(&entry.id, "Home").unwrap();
    let text: serde_json::Value =
        serde_json::from_slice(&fs::read(rig.places().registry()).unwrap()).unwrap();
    assert_eq!(text["sync"]["account"], "a");
    assert_eq!(text["profiles"][0]["avatar"], "cat");
    assert_eq!(text["profiles"][0]["name"], "Home");
}

#[test]
fn ids_are_eight_base32_letters() {
    for _ in 0..100 {
        let id = Id::random();
        assert_eq!(id.as_str().len(), 8);
        assert!(
            id.as_str()
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'2'..=b'7'))
        );
    }
    assert!(Id::parse("ABCDEFGH").is_none());
    assert!(Id::parse("abcdefg").is_none());
    assert!(Id::parse("abcdefg1").is_none(), "1 is not base32");
}
