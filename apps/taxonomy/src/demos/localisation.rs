//! Localisation: the language switch, pseudo-locales, plurals, attributes,
//! fallback and isolation, from the taxonomy's own catalogue
//! (`apps/taxonomy/locale/<language>/taxonomy.ftl`) and bingux-ui's.

use bingux_ui::controls::direction::Direction;
use bingux_ui::i18n::{self, Arg, Pseudo};
use bingux_ui::locale::Locale;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::{rows, with};

/// The choices of the switch.
const LANGUAGES: [&str; 5] = ["system", "en-US", "de", "accented", "rtl"];

/// How a choice is shown. A language is named in its own language, as a
/// language menu names it; the others are messages.
fn language_name(key: &str) -> String {
    match key {
        "en-US" => "English (US)".to_owned(),
        "de" => "Deutsch".to_owned(),
        "accented" => tr!("taxonomy-l10n-accented"),
        "rtl" => tr!("taxonomy-l10n-rtl"),
        _ => tr!("taxonomy-l10n-system"),
    }
}

/// Shows every window in the language `key` names.
fn switch_to(key: &str) {
    match key {
        "accented" | "rtl" => {
            i18n::set_locale(None);
            i18n::set_pseudo(Pseudo::from_name(key));
        }
        "system" => {
            i18n::set_pseudo(None);
            i18n::set_locale(None);
        }
        tag => {
            i18n::set_pseudo(None);
            i18n::set_locale(Locale::parse(tag));
        }
    }
}

/// The key of the language shown now.
fn current_key() -> String {
    match i18n::pseudo() {
        Some(pseudo) => pseudo.name().to_owned(),
        None => {
            let language = i18n::language();
            if LANGUAGES.contains(&language.as_str()) {
                language
            } else {
                "system".to_owned()
            }
        }
    }
}

/// The live page: the switch and what it changes. Every string below comes
/// from a catalogue, so the switch rebuilds the window in the new language.
pub fn localisation() -> Element {
    let options: Vec<(String, String)> = LANGUAGES
        .iter()
        .map(|key| (key.to_string(), language_name(key)))
        .collect();
    let switch = dropdown(options, current_key(), move |key: String| switch_to(&key));
    let direction = match i18n::direction() {
        Direction::Rtl => tr!("taxonomy-l10n-direction-rtl"),
        _ => tr!("taxonomy-l10n-direction-ltr"),
    };
    // The check reads the source tree, so it runs only beside it, and on the
    // task pool: it reads files.
    let problems = use_state(|| None::<Option<usize>>);
    use_hook(move || {
        bingux_ui::runtime::background(problems, || {
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            source.join("src").is_dir().then(|| {
                i18n::check::crate_catalogue(source, "taxonomy")
                    .problems()
                    .len()
            })
        })
    });
    let check = match *problems.read() {
        None => String::new(),
        Some(None) => tr!("taxonomy-l10n-check-unavailable-status"),
        Some(Some(0)) => tr!("taxonomy-l10n-check-clean-status"),
        Some(Some(count)) => tr!("taxonomy-l10n-check-problems-status", count = count),
    };
    let plurals: Vec<Element> = [0usize, 1, 2, 5, 21, 1200]
        .into_iter()
        .map(|count| {
            property_row(count.to_string(), tr!("taxonomy-l10n-files", count = count))
                .into_element()
        })
        .collect();
    // A Hebrew file name inside the sentence: the isolation marks keep the
    // name and the full stop where they belong in either direction.
    let moved = tr!("taxonomy-l10n-moved", name = "דוח שנתי.pdf");
    let size = i18n::Message::new(i18n_id!("taxonomy-l10n-size"))
        .arg("size", Arg::number(1234.5, 1, 1))
        .format();
    vstack(S4)
        .child(with(&tr!("taxonomy-l10n-language"), switch))
        .child(rows(vec![
            property_row(tr!("taxonomy-l10n-shown"), i18n::language()).into_element(),
            property_row(tr!("taxonomy-l10n-chain"), i18n::languages().join(" → ")).into_element(),
            property_row(tr!("taxonomy-l10n-direction"), direction).into_element(),
            property_row(tr!("taxonomy-l10n-check"), check).into_element(),
        ]))
        .child(Group::new(Some(tr!("taxonomy-l10n-plurals-title"))).rows(plurals))
        .child(rows(vec![
            property_row(tr!("taxonomy-l10n-isolation"), moved).into_element(),
            property_row(tr!("taxonomy-l10n-number"), size).into_element(),
            property_row(
                tr!("taxonomy-l10n-fallback"),
                tr!("taxonomy-l10n-fallback-sample"),
            )
            .into_element(),
            property_row(
                tr!("taxonomy-l10n-shortcut"),
                Shortcut::parse("Ctrl+Shift+Delete")
                    .map(|shortcut| shortcut.label())
                    .unwrap_or_default(),
            )
            .into_element(),
            property_row(
                tr!("taxonomy-l10n-ui-string"),
                // An app may show bingux-ui's own messages.
                tr!("ui-command-palette"),
            )
            .into_element(),
            with(
                &tr!("taxonomy-l10n-attribute"),
                hover_tip(
                    action_button(tr!("taxonomy-l10n-save"), |_| {}),
                    tr!("taxonomy-l10n-save.tooltip"),
                ),
            ),
        ]))
        .child(caption(if i18n::has("taxonomy-l10n-fallback-sample") {
            tr!("taxonomy-l10n-note-description")
        } else {
            String::new()
        }))
        .into_element()
}
