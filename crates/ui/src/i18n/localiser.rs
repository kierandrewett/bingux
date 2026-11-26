//! The bundles of the negotiated languages, and formatting a message along
//! them.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, RwLock, Weak};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::types::{FluentNumberStyle, FluentType};
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use freya::prelude::*;
use intl_memoizer::Memoizable;
use unic_langid::LanguageIdentifier;

use super::catalogue::{self, Catalogue};
use super::pseudo::Pseudo;
use crate::controls::direction::Direction;
use crate::format::{self, Moment};
use crate::locale::Locale;

/// The language Bingux is written in.
pub(super) const SOURCE: &str = "en-US";

/// Unicode's first strong isolate and pop directional isolate.
const FSI: char = '\u{2068}';
const PDI: char = '\u{2069}';

struct Bundle {
    rtl: bool,
    bundle: FluentBundle<FluentResource>,
}

/// The languages in force and their bundles. Rebuilt when [`key`] changes.
pub(super) struct Localiser {
    key: (u64, u64),
    bundles: Vec<Bundle>,
    chain: Vec<String>,
    direction: Direction,
    pseudo: Option<Pseudo>,
}

impl Localiser {
    pub(super) fn chain(&self) -> &[String] {
        &self.chain
    }

    pub(super) fn direction(&self) -> Direction {
        self.direction
    }

    /// What the bundles were built under (see [`Holder`]'s `key`).
    pub(super) fn key(&self) -> Key {
        self.key
    }
}

/// What the bundles depend on: the locale's revision and the inputs' ([`super::inputs`]).
pub(super) type Key = (u64, u64);

/// The localiser that is shown, and what keeps it current.
///
/// Building one parses every catalogue of every language in the chain: tens
/// of milliseconds with a few catalogues, hundreds with many (the build is
/// measured in `docs/bingux-ui.md`, "Localisation"). The first build waits
/// for nobody: it happens at start-up, before the window is mounted. A later
/// change of locale, pseudo-locale or catalogues is different: the window is
/// on screen, so a lookup on the UI thread keeps answering with the bundles it
/// has while the pool builds the new ones, then swaps them in and tells the
/// windows ([`Holder::done`]) to ask again. Any other thread (a worker, a
/// test) that finds the bundles stale builds them itself, as it can wait.
pub(super) struct Holder {
    current: RwLock<Option<Arc<Localiser>>>,
    /// A rebuild is queued or running; only one is.
    rebuilding: AtomicBool,
    /// How many times the bundles were swapped, for [`super::revision`]: a
    /// window follows the language by it.
    swaps: AtomicU64,
    key: fn() -> Key,
    build: fn(Key) -> Localiser,
    done: fn(),
}

/// Clears [`Holder::rebuilding`] however the rebuild ends, a panic included.
struct Rebuilding<'a>(&'a AtomicBool);

impl Drop for Rebuilding<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Holder {
    pub(super) fn new(key: fn() -> Key, build: fn(Key) -> Localiser, done: fn()) -> Arc<Self> {
        Arc::new(Self {
            current: RwLock::new(None),
            rebuilding: AtomicBool::new(false),
            swaps: AtomicU64::new(0),
            key,
            build,
            done,
        })
    }

    pub(super) fn swaps(&self) -> u64 {
        self.swaps.load(Ordering::Acquire)
    }

    fn held(&self) -> Option<Arc<Localiser>> {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn store(&self, built: Localiser) -> Arc<Localiser> {
        let built = Arc::new(built);
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&built));
        built
    }

    /// The localiser for what is in force now. `ui_thread`: the caller must
    /// not wait, so stale bundles are answered while new ones are built.
    pub(super) fn get(self: &Arc<Self>, ui_thread: bool) -> Arc<Localiser> {
        let key = (self.key)();
        match self.held() {
            Some(held) if held.key == key => held,
            Some(stale) if ui_thread => {
                self.rebuild_on_the_pool();
                stale
            }
            _ => self.store((self.build)(key)),
        }
    }

    /// Something the bundles depend on changed. On the UI thread, with bundles
    /// shown, this starts the rebuild and returns `true`: the windows are told
    /// when it is done. Otherwise `false`: nothing is shown that could go
    /// stale, or the caller may wait, and the next lookup builds.
    pub(super) fn changed(self: &Arc<Self>, ui_thread: bool) -> bool {
        if !ui_thread || self.held().is_none() {
            return false;
        }
        self.rebuild_on_the_pool();
        true
    }

    fn rebuild_on_the_pool(self: &Arc<Self>) {
        if self.rebuilding.swap(true, Ordering::AcqRel) {
            return;
        }
        let this = Arc::clone(self);
        crate::task::spawn_detached("i18n-rebuild", move || this.rebuild());
    }

    /// Builds until what was built is what is in force, swapping each in, then
    /// tells the windows.
    fn rebuild(self: &Arc<Self>) {
        let guard = Rebuilding(&self.rebuilding);
        loop {
            let key = (self.key)();
            self.store((self.build)(key));
            self.swaps.fetch_add(1, Ordering::AcqRel);
            if (self.key)() == key {
                break;
            }
        }
        drop(guard);
        (self.done)();
        // A change that came after the last check and before the flag cleared
        // found a rebuild running and started none.
        if self.held().is_some_and(|held| held.key != (self.key)()) {
            self.rebuild_on_the_pool();
        }
    }
}

static HOLDER: LazyLock<Arc<Holder>> = LazyLock::new(|| Holder::new(key, build_current, notify));

fn key() -> Key {
    (Locale::revision(), super::inputs())
}

fn build_current(key: Key) -> Localiser {
    build(
        key,
        super::pseudo::current(),
        &Locale::preferred(),
        &catalogue::all(),
        &|name| std::env::var(name).ok(),
    )
}

/// The localiser for the languages in force now. On the UI thread it may be
/// the one for the languages just left, until the pool has built the new one.
pub(super) fn current() -> Arc<Localiser> {
    HOLDER.get(crate::runtime::on_ui_thread())
}

/// How many times the bundles were swapped for newly built ones.
pub(super) fn swaps() -> u64 {
    HOLDER.swaps()
}

/// The locale, pseudo-locale or catalogues changed. See [`Holder::changed`].
pub(super) fn changed() -> bool {
    HOLDER.changed(crate::runtime::on_ui_thread())
}

fn langid(tag: &str) -> Option<LanguageIdentifier> {
    tag.parse().ok()
}

/// The language identifier of `locale`, without the Unicode extensions
/// Fluent's identifiers cannot hold (`de-u-nu-latn` is `de`).
fn langid_of(locale: &Locale) -> Option<LanguageIdentifier> {
    let mut tag = locale.language().to_string();
    if let Some(script) = locale.script() {
        tag.push('-');
        tag.push_str(script);
    }
    if let Some(region) = locale.region() {
        tag.push('-');
        tag.push_str(region);
    }
    langid(&tag)
}

/// The bundles for `requested` (the person's languages, most wanted first),
/// or for `pseudo`, with translations found where `var` (the environment)
/// says.
pub(super) fn build(
    key: (u64, u64),
    pseudo: Option<Pseudo>,
    requested: &[Locale],
    catalogues: &[Catalogue],
    var: &dyn Fn(&str) -> Option<String>,
) -> Localiser {
    if let Some(pseudo) = pseudo {
        let mut bundle = new_bundle(SOURCE, pseudo == Pseudo::Rtl);
        bundle.bundle.set_transform(Some(pseudo.transform()));
        for catalogue in catalogues {
            add(
                &mut bundle.bundle,
                catalogue.domain(),
                catalogue.source().to_string(),
            );
        }
        return Localiser {
            key,
            bundles: vec![bundle],
            chain: vec![pseudo.tag().to_string()],
            direction: if pseudo == Pseudo::Rtl {
                Direction::Rtl
            } else {
                Direction::Ltr
            },
            pseudo: Some(pseudo),
        };
    }

    let mut dirs = Vec::new();
    for catalogue in catalogues {
        for dir in catalogue::search_dirs(catalogue, var) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    let source = langid(SOURCE).expect("en-US is a language tag");
    // Directory name for each language that has one: `de`, `pt-BR`.
    let installed: Vec<(LanguageIdentifier, String)> = catalogue::languages(&dirs, catalogues)
        .into_iter()
        .filter_map(|name| Some((langid(&name.replace('_', "-"))?, name)))
        .collect();
    let mut available: Vec<LanguageIdentifier> =
        installed.iter().map(|(id, _)| id.clone()).collect();
    if !available.contains(&source) {
        available.push(source.clone());
    }
    let requested: Vec<LanguageIdentifier> = requested.iter().filter_map(langid_of).collect();
    let mut chain: Vec<LanguageIdentifier> = negotiate_languages(
        &requested,
        &available,
        Some(&source),
        NegotiationStrategy::Filtering,
    )
    .into_iter()
    .cloned()
    .collect();
    if !chain.contains(&source) {
        chain.push(source.clone());
    }
    let english = langid("en").expect("en is a language tag");
    if available.contains(&english) && !chain.contains(&english) {
        chain.push(english);
    }

    let mut bundles = Vec::new();
    let mut names = Vec::new();
    for language in chain {
        let tag = language.to_string();
        let rtl = Direction::from_language(&tag).is_rtl();
        let mut bundle = new_bundle(&tag, rtl);
        let mut any = false;
        for catalogue in catalogues {
            let text = if language == source {
                Some(catalogue.source().to_string())
            } else {
                read(&dirs, &installed, &language, catalogue)
            };
            if let Some(text) = text {
                add(&mut bundle.bundle, catalogue.domain(), text);
                if language != source {
                    // The source's terms (`-brand-name`) serve every language
                    // that does not write its own.
                    let terms = source_terms(catalogue.source());
                    if let Ok(resource) = FluentResource::try_new(terms) {
                        // A term the translation has already is kept.
                        let _ = bundle.bundle.add_resource(resource);
                    }
                }
                any = true;
            }
        }
        if any {
            bundles.push(bundle);
            names.push(tag);
        }
    }
    Localiser {
        key,
        direction: Direction::from_language(&names[0]),
        bundles,
        chain: names,
        pseudo: None,
    }
}

fn read(
    dirs: &[std::path::PathBuf],
    installed: &[(LanguageIdentifier, String)],
    language: &LanguageIdentifier,
    catalogue: &Catalogue,
) -> Option<String> {
    let (_, name) = installed.iter().find(|(id, _)| id == language)?;
    let path = catalogue::find(dirs, name, catalogue)?;
    match crate::runtime::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) => {
            eprintln!("bingux-ui: could not read {}: {error}", path.display());
            None
        }
    }
}

fn new_bundle(tag: &str, rtl: bool) -> Bundle {
    let language = langid(tag).unwrap_or_else(|| langid(SOURCE).expect("en-US is a language tag"));
    let mut bundle = FluentBundle::new_concurrent(vec![language]);
    // Isolation marks around every placeable in a right-to-left language; a
    // left-to-right one isolates only the values that hold right-to-left
    // text ([`isolate_args`]), so English text stays free of them.
    bundle.set_use_isolating(rtl);
    bundle.set_formatter(Some(format_value));
    bundle
        .add_builtins()
        .expect("a new bundle has no functions yet");
    bundle
        .add_function("DATETIME", datetime)
        .expect("a new bundle has no DATETIME yet");
    Bundle { rtl, bundle }
}

/// Adds one catalogue's text. A syntax error drops only the entry it is in
/// (Fluent's recovery), and is reported once.
fn add(bundle: &mut FluentBundle<FluentResource>, domain: &str, text: String) {
    let resource = match FluentResource::try_new(text) {
        Ok(resource) => resource,
        Err((resource, errors)) => {
            for error in errors {
                eprintln!(
                    "bingux-ui: {domain}.ftl: {error} at byte {}",
                    error.pos.start
                );
            }
            resource
        }
    };
    if let Err(errors) = bundle.add_resource(resource) {
        for error in errors {
            eprintln!("bingux-ui: {domain}.ftl: {error}");
        }
    }
}

/// The terms of `source` (`-name = ...` and the lines that continue it), as
/// a catalogue of their own. An entry starts at the start of a line; its
/// continuation lines are indented, blank, or a select's closing `}`.
pub(super) fn source_terms(source: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in source.lines() {
        let starts_entry = line
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace() && c != '}');
        if starts_entry {
            inside = line.starts_with('-');
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Whether `text` holds a letter of a right-to-left script.
pub(super) fn has_rtl(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c as u32,
            0x0590..=0x08FF
            | 0xFB1D..=0xFDFF
            | 0xFE70..=0xFEFF
            | 0x10800..=0x10FFF
            | 0x1E800..=0x1EFFF
            | 0x200F | 0x202B | 0x202E | 0x2067)
    })
}

/// `args` with each text value that holds right-to-left letters wrapped in
/// isolation marks, for a left-to-right bundle. `None` when no value needs it.
fn isolate_args(args: &FluentArgs<'static>) -> Option<FluentArgs<'static>> {
    let needs = args
        .iter()
        .any(|(_, value)| matches!(value, FluentValue::String(text) if has_rtl(text)));
    if !needs {
        return None;
    }
    let mut out = FluentArgs::new();
    for (name, value) in args.iter() {
        let value = match value {
            FluentValue::String(text) if has_rtl(text) => {
                FluentValue::String(Cow::Owned(format!("{FSI}{text}{PDI}")))
            }
            other => other.into_owned(),
        };
        out.set(name.to_string(), value);
    }
    Some(out)
}

/// Whether a catalogue in force has `id` (or `id.attribute`).
pub(super) fn has(id: &str) -> bool {
    let (name, attribute) = split(id);
    current().bundles.iter().any(|bundle| {
        bundle
            .bundle
            .get_message(name)
            .is_some_and(|message| match attribute {
                Some(attribute) => message.get_attribute(attribute).is_some(),
                None => message.value().is_some(),
            })
    })
}

fn split(id: &str) -> (&str, Option<&str>) {
    match id.split_once('.') {
        Some((name, attribute)) => (name, Some(attribute)),
        None => (id, None),
    }
}

/// The message `id` from the first bundle of the chain that has it.
pub(super) fn format(id: &str, args: Option<&FluentArgs<'static>>) -> String {
    format_in(&current(), id, args)
}

/// [`format`] along `localiser`'s chain.
pub(super) fn format_in(
    localiser: &Localiser,
    id: &str,
    args: Option<&FluentArgs<'static>>,
) -> String {
    let (name, attribute) = split(id);
    let mut isolated: Option<Option<FluentArgs<'static>>> = None;
    for bundle in &localiser.bundles {
        let Some(message) = bundle.bundle.get_message(name) else {
            continue;
        };
        let pattern = match attribute {
            Some(attribute) => message
                .get_attribute(attribute)
                .map(|attribute| attribute.value()),
            None => message.value(),
        };
        let Some(pattern) = pattern else {
            continue;
        };
        let args = match args {
            Some(args) if !bundle.rtl => isolated
                .get_or_insert_with(|| isolate_args(args))
                .as_ref()
                .or(Some(args)),
            other => other,
        };
        let mut errors = Vec::new();
        let text = bundle
            .bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned();
        if !errors.is_empty() {
            report(id, &format!("{errors:?}"));
        }
        return match localiser.pseudo {
            Some(pseudo) => pseudo.wrap(text),
            None => text,
        };
    }
    report(id, "no catalogue has it");
    id.to_string()
}

/// Says once per message what went wrong with it: a missing message shows
/// its ID, a missing variable its `{$name}`, so the person sees something
/// and the developer is told.
fn report(id: &str, problem: &str) {
    static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    if seen.get_or_insert_with(HashSet::new).insert(id.to_string()) {
        eprintln!("bingux-ui: message {id}: {problem}");
    }
}

/// The [`Locale`] of a bundle, memoised in it, so a value is written in the
/// bundle's language.
struct BundleLocale(Locale);

impl Memoizable for BundleLocale {
    type Args = ();
    type Error = ();
    fn construct(language: LanguageIdentifier, _: ()) -> Result<BundleLocale, ()> {
        Ok(BundleLocale(
            Locale::parse(&language.to_string()).unwrap_or_else(Locale::fallback),
        ))
    }
}

fn locale_of(intls: &intl_memoizer::concurrent::IntlLangMemoizer) -> Locale {
    intls
        .with_try_get::<BundleLocale, _, _>((), |locale| locale.0.clone())
        .unwrap_or_else(|()| Locale::fallback())
}

/// Writes a number with [`format::Number`] (or `Percent`, `Currency`) in the
/// bundle's language; every other value as Fluent does.
fn format_value(
    value: &FluentValue,
    intls: &intl_memoizer::concurrent::IntlLangMemoizer,
) -> Option<String> {
    let FluentValue::Number(number) = value else {
        return None;
    };
    let locale = locale_of(intls);
    let options = &number.options;
    let digits = |least: Option<usize>, most: Option<usize>| {
        let least = least.unwrap_or(0).min(20) as u8;
        let most = most.map(|most| most.min(20) as u8);
        (least, most)
    };
    let (least, most) = digits(
        options.minimum_fraction_digits,
        options.maximum_fraction_digits,
    );
    let explicit =
        options.minimum_fraction_digits.is_some() || options.maximum_fraction_digits.is_some();
    Some(match options.style {
        FluentNumberStyle::Percent => {
            let mut percent = format::Percent::new(number.value).locale(&locale);
            if explicit {
                percent = percent.fraction_digits(least, most.unwrap_or(least));
            }
            percent.format()
        }
        FluentNumberStyle::Currency => match &options.currency {
            Some(code) => format::Currency::new(number.value, code)
                .locale(&locale)
                .format(),
            None => format::Number::new(number.value).locale(&locale).format(),
        },
        FluentNumberStyle::Decimal => {
            let mut out = format::Number::new(number.value).locale(&locale);
            if explicit {
                out = out.fraction_digits(least, most.unwrap_or(least.max(3)));
            }
            if let Some(significant) = options.maximum_significant_digits {
                out = out.significant_digits(significant.min(21) as u8);
            }
            if !options.use_grouping {
                out = out.grouping(format::Grouping::Never);
            }
            out.format()
        }
    })
}

/// A moment passed to a message, and how much of it `DATETIME()` asked for.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DateValue {
    moment: Moment,
    date: Option<format::Length>,
    time: Option<format::TimePrecision>,
}

impl DateValue {
    pub(super) fn new(moment: Moment) -> DateValue {
        DateValue {
            moment,
            date: Some(format::Length::Medium),
            time: None,
        }
    }

    fn write(&self, locale: &Locale) -> String {
        match (self.date, self.time) {
            (Some(length), Some(precision)) => format::Date::new(self.moment)
                .length(length)
                .with_time(precision)
                .locale(locale)
                .format(),
            (Some(length), None) => format::Date::new(self.moment)
                .length(length)
                .locale(locale)
                .format(),
            (None, Some(precision)) => format::Time::new(self.moment)
                .precision(precision)
                .locale(locale)
                .format(),
            (None, None) => format::Date::new(self.moment).locale(locale).format(),
        }
    }
}

impl FluentType for DateValue {
    fn duplicate(&self) -> Box<dyn FluentType + Send> {
        Box::new(self.clone())
    }

    fn as_string(&self, intls: &intl_memoizer::IntlLangMemoizer) -> Cow<'static, str> {
        let locale = intls
            .with_try_get::<BundleLocale, _, _>((), |locale| locale.0.clone())
            .unwrap_or_else(|()| Locale::fallback());
        Cow::Owned(self.write(&locale))
    }

    fn as_string_threadsafe(
        &self,
        intls: &intl_memoizer::concurrent::IntlLangMemoizer,
    ) -> Cow<'static, str> {
        Cow::Owned(self.write(&locale_of(intls)))
    }
}

/// `DATETIME($when, dateStyle: "long", timeStyle: "short")`: a moment with
/// the parts asked for. `dateStyle` is `short`, `medium` or `long` (`full`
/// is `long`); `timeStyle` is `short` (minutes) or `medium` (seconds). With
/// neither, the date at medium length.
fn datetime<'a>(positional: &[FluentValue<'a>], named: &FluentArgs) -> FluentValue<'a> {
    let Some(FluentValue::Custom(value)) = positional.first() else {
        return FluentValue::Error;
    };
    let Some(date) = value.as_any().downcast_ref::<DateValue>() else {
        return FluentValue::Error;
    };
    let text = |name: &str| match named.get(name) {
        Some(FluentValue::String(text)) => Some(text.to_string()),
        _ => None,
    };
    let date_style = text("dateStyle").map(|style| match style.as_str() {
        "short" => format::Length::Short,
        "long" | "full" => format::Length::Long,
        _ => format::Length::Medium,
    });
    let time_style = text("timeStyle").map(|style| match style.as_str() {
        "short" => format::TimePrecision::Minutes,
        _ => format::TimePrecision::Seconds,
    });
    let mut out = date.clone();
    if date_style.is_some() || time_style.is_some() {
        out.date = date_style;
        out.time = time_style;
    }
    FluentValue::Custom(Box::new(out))
}

/// Feeds of the windows following the language ([`use_language`]).
static LISTENERS: Mutex<Vec<Weak<crate::task::Feed<()>>>> = Mutex::new(Vec::new());

/// Tells every window following the language that it changed.
pub(super) fn notify() {
    let mut listeners = LISTENERS.lock().unwrap_or_else(PoisonError::into_inner);
    listeners.retain(|listener| match listener.upgrade() {
        Some(feed) => {
            feed.push(());
            true
        }
        None => false,
    });
}

pub(super) fn use_language() -> u64 {
    let state = use_hook(|| State::create(super::revision()));
    use_hook(move || {
        let feed = Arc::new(crate::task::Feed::<()>::default());
        LISTENERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Arc::downgrade(&feed));
        let mut state = state;
        spawn(async move {
            while feed.next().await.is_some() {
                state.set_if_modified(super::revision());
            }
        });
    });
    *state.read()
}
