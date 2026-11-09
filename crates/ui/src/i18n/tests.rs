// ftl-check: skip-file (these tests ask for IDs of the demo catalogue below)

use std::path::PathBuf;

use super::catalogue::Catalogue;
use super::localiser::{Localiser, build, format_in};
use super::{Message, Pseudo};
use crate::controls::direction::Direction;
use crate::locale::Locale;

const DEMO: &str = "\
-brand = Bingux
demo-title = { -brand } Files
demo-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
demo-delete = Delete { $name }?
    .tooltip = Move { $name } to the bin
demo-size = { NUMBER($size, maximumFractionDigits: 1) } GB
demo-only-english = Only in English
";

const DE: &str = "\
demo-title = { -brand } Dateien
demo-items = { $count ->
    [one] { $count } Objekt
   *[other] { $count } Objekte
}
demo-size = { NUMBER($size, maximumFractionDigits: 1) } GB
";

fn locale_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bingux-i18n-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("de")).unwrap();
    std::fs::write(dir.join("de/demo.ftl"), DE).unwrap();
    dir
}

fn localiser(name: &str, pseudo: Option<Pseudo>, requested: &[&str]) -> Localiser {
    let dir = locale_dir(name);
    let requested: Vec<Locale> = requested.iter().filter_map(|tag| Locale::parse(tag)).collect();
    let var = move |key: &str| match key {
        "BINGUX_LOCALE_DIR" => Some(dir.display().to_string()),
        "XDG_DATA_DIRS" => Some("/nonexistent".to_string()),
        _ => None,
    };
    build((0, 0), pseudo, &requested, &[Catalogue::new("demo", DEMO)], &var)
}

fn say(localiser: &Localiser, message: Message) -> String {
    format_in(localiser, message.id(), Some(message.args()))
}

#[test]
fn the_source_language_is_american_english() {
    let english = localiser("source", None, &["en-US"]);
    assert_eq!(english.chain(), ["en-US"]);
    assert_eq!(say(&english, Message::new("demo-title")), "Bingux Files");
    assert_eq!(say(&english, Message::new("demo-items").arg("count", 1)), "1 item");
    assert_eq!(say(&english, Message::new("demo-items").arg("count", 12_345)), "12,345 items");
    assert_eq!(
        say(&english, Message::new("demo-delete.tooltip").arg("name", "notes.txt")),
        "Move notes.txt to the bin",
        "an attribute, and no isolation marks around left-to-right text"
    );
}

#[test]
fn a_partial_translation_falls_back_message_by_message() {
    let german = localiser("fallback", None, &["de-AT", "fr"]);
    assert_eq!(german.chain(), ["de", "en-US"]);
    assert_eq!(say(&german, Message::new("demo-title")), "Bingux Dateien", "a term from the source");
    assert_eq!(say(&german, Message::new("demo-items").arg("count", 2)), "2 Objekte");
    assert_eq!(say(&german, Message::new("demo-only-english")), "Only in English");
    assert_eq!(
        say(&german, Message::new("demo-size").arg("size", 1234.56)),
        "1.234,6 GB",
        "NUMBER() is written by format::Number in German"
    );
}

#[test]
fn an_unknown_language_shows_the_source() {
    let other = localiser("unknown", None, &["ja-JP"]);
    assert_eq!(other.chain(), ["en-US"]);
    assert_eq!(other.direction(), Direction::Ltr);
}

#[test]
fn a_missing_message_shows_its_id() {
    let english = localiser("missing", None, &["en-US"]);
    assert_eq!(say(&english, Message::new("demo-nothing")), "demo-nothing");
}

#[test]
fn right_to_left_values_are_isolated_in_a_left_to_right_sentence() {
    let english = localiser("bidi", None, &["en-US"]);
    assert_eq!(
        say(&english, Message::new("demo-delete").arg("name", "שלום.txt")),
        "Delete \u{2068}שלום.txt\u{2069}?"
    );
}

#[test]
fn the_accented_pseudo_locale_rewrites_only_the_catalogue_text() {
    let pseudo = localiser("accented", Some(Pseudo::Accented), &["de"]);
    assert_eq!(pseudo.chain(), ["en-XA"]);
    let text = say(&pseudo, Message::new("demo-delete").arg("name", "notes.txt"));
    assert!(text.starts_with('[') && text.ends_with(']'), "{text}");
    assert!(text.contains("notes.txt"), "the placeable keeps its value: {text}");
    assert!(!text.contains("Delete"), "{text}");
    assert!(text.chars().count() > "[Delete notes.txt?]".chars().count());
}

#[test]
fn the_rtl_pseudo_locale_reads_right_to_left_and_isolates_placeables() {
    let pseudo = localiser("rtl", Some(Pseudo::Rtl), &["en-US"]);
    assert_eq!(pseudo.direction(), Direction::Rtl);
    let text = say(&pseudo, Message::new("demo-delete").arg("name", "notes.txt"));
    assert!(text.contains("\u{2068}notes.txt\u{2069}"), "{text}");
    assert!(super::localiser::has_rtl(&text));
}

#[test]
fn a_right_to_left_catalogue_turns_the_window() {
    let dir = locale_dir("hebrew");
    std::fs::create_dir_all(dir.join("he")).unwrap();
    std::fs::write(dir.join("he/demo.ftl"), "demo-title = קבצים\n").unwrap();
    let var = move |key: &str| match key {
        "BINGUX_LOCALE_DIR" => Some(dir.display().to_string()),
        "XDG_DATA_DIRS" => Some("/nonexistent".to_string()),
        _ => None,
    };
    let requested = [Locale::parse("he_IL.UTF-8").unwrap()];
    let hebrew = build((0, 0), None, &requested, &[Catalogue::new("demo", DEMO)], &var);
    assert_eq!(hebrew.chain(), ["he", "en-US"]);
    assert_eq!(hebrew.direction(), Direction::Rtl);
    // Arabic with no Arabic catalogue is shown in English, left to right.
    let arabic = localiser("arabic", None, &["ar-EG"]);
    assert_eq!(arabic.direction(), Direction::Ltr);
}

#[test]
fn bingux_ui_strings_are_there_with_no_install() {
    assert!(super::has("ui-dialog-cancel"));
    assert_eq!(crate::tr!("ui-dialog-cancel"), "Cancel");
}
