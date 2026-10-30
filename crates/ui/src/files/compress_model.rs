//! The Compress dialog's rules, with no window: what a selection can be
//! made into, what the typed values mean, and why the Create button is
//! disabled.
//!
//! The dialog ([`super::compress`]) draws these and nothing else decides: a
//! test here is a test of what a person sees.
//!
//! # Modes
//!
//! A selection that is all pictures, films and songs may be **reduced**:
//! made smaller by re-encoding, as Photos and Image Optimizer do. Anything
//! else, or a mix of media and other files, can only be **archived**. A
//! selection of pure media opens on Reduce size, because that is what a
//! person who selected three photos almost always wants; Archive is one
//! segment away.

use std::collections::HashSet;
use std::path::PathBuf;

use bingux_files::ops::{
    Excludes, Format, ImageFormat, MediaKind, NameCheck, OnConflict, PackOptions, Password,
    ReduceOptions, Tools, VideoCodec, VideoQuality, check_name, file_name, free_base,
    suggested_base,
};
use bingux_files::Entry;

/// What the dialog makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One archive of everything.
    Archive,
    /// Smaller copies of pictures, films and songs.
    Reduce,
}

/// The selection, with only what the dialog needs to know of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub paths: Vec<PathBuf>,
    pub names: Vec<String>,
    /// The kind of each item when it is a picture, a film or a song.
    pub kinds: Vec<Option<MediaKind>>,
    pub sizes: Vec<u64>,
    /// One item, and it is a folder.
    pub single_folder: bool,
    /// The folder the selection is in.
    pub folder: PathBuf,
}

impl Subject {
    pub fn of(entries: &[Entry]) -> Subject {
        let folder = entries
            .first()
            .and_then(|entry| entry.path.parent())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        Subject {
            paths: entries.iter().map(|entry| entry.path.clone()).collect(),
            names: entries.iter().map(|entry| entry.name.clone()).collect(),
            kinds: entries
                .iter()
                .map(|entry| {
                    if entry.is_dir() {
                        None
                    } else {
                        MediaKind::of(entry.category())
                    }
                })
                .collect(),
            sizes: entries.iter().map(|entry| entry.size).collect(),
            single_folder: matches!(entries, [only] if only.is_dir()),
            folder,
        }
    }

    /// Every item is a picture, a film or a song.
    pub fn is_pure_media(&self) -> bool {
        !self.kinds.is_empty() && self.kinds.iter().all(Option::is_some)
    }

    /// The kinds present, in the order pictures, films, songs.
    pub fn kinds_present(&self) -> Vec<MediaKind> {
        [MediaKind::Image, MediaKind::Video, MediaKind::Audio]
            .into_iter()
            .filter(|kind| self.kinds.contains(&Some(*kind)))
            .collect()
    }

    /// The title of the dialog: `Compress “name”` or `Compress 3 items`.
    pub fn title(&self) -> String {
        match self.names.as_slice() {
            [only] => format!("Compress “{only}”"),
            many => format!("Compress {} items", many.len()),
        }
    }

    /// The name offered for the archive.
    pub fn default_base(&self) -> String {
        suggested_base(&self.names, self.single_folder)
    }

    /// The mode the dialog opens in.
    pub fn default_mode(&self) -> Mode {
        if self.is_pure_media() {
            Mode::Reduce
        } else {
            Mode::Archive
        }
    }
}

/// What the person has chosen. The typed texts are kept apart (they are
/// fields with their own state) and read through [`Typed`].
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub mode: Mode,
    pub pack: PackOptions,
    /// Whether a password is asked for (zip, 7z).
    pub locked: bool,
    /// Where the archive goes; `None` is beside the originals.
    pub location: Option<PathBuf>,
    pub conflict: OnConflict,
    pub reduce: ReduceOptions,
    /// The picture size cap is a number the person types.
    pub custom_dimension: bool,
}

impl Draft {
    pub fn new(subject: &Subject, prefs: bingux_files::ops::prefs::Prefs) -> Draft {
        Draft {
            mode: subject.default_mode(),
            pack: PackOptions {
                password: None,
                delete_originals: false,
                split_mib: None,
                hide_names: false,
                ..prefs.pack
            },
            locked: false,
            location: None,
            conflict: OnConflict::KeepBoth,
            reduce: ReduceOptions {
                replace_original: false,
                ..prefs.reduce
            },
            custom_dimension: false,
        }
    }
}

/// The fields the person types in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typed {
    pub name: String,
    pub excludes: String,
    pub password: String,
    pub confirm: String,
    pub dimension: String,
}

/// What the dialog knows about the world it is in.
pub struct World<'a> {
    pub subject: &'a Subject,
    /// The file names in the folder the result goes to.
    pub existing: &'a HashSet<String>,
    /// `None` while the machine's tools are still being looked at.
    pub tools: Option<&'a Tools>,
}

/// What is wrong, field by field, and the one reason for the footer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Check {
    pub name: Option<String>,
    pub excludes: Option<String>,
    pub password: Option<String>,
    pub confirm: Option<String>,
    pub dimension: Option<String>,
    /// Why Create cannot run, or `None` when it can.
    pub blocked: Option<String>,
}

/// The longest side a typed picture size may be.
pub const MAX_DIMENSION: u32 = 20_000;
/// The shortest.
pub const MIN_DIMENSION: u32 = 16;

/// Read a typed picture size.
pub fn parse_dimension(text: &str) -> Result<u32, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Enter a size in pixels.".into());
    }
    let number: u32 = text
        .parse()
        .map_err(|_| "Use digits only, such as 1600.".to_string())?;
    if !(MIN_DIMENSION..=MAX_DIMENSION).contains(&number) {
        return Err(format!("Use a size from {MIN_DIMENSION} to {MAX_DIMENSION} pixels."));
    }
    Ok(number)
}

/// Is `format` the one this machine can make? The first that can, in the
/// dropdown's order, for a remembered format that no longer works.
pub fn usable_format(wanted: Format) -> Format {
    if wanted.is_available() {
        return wanted;
    }
    Format::ALL
        .into_iter()
        .find(|format| format.is_available())
        .unwrap_or(wanted)
}

/// The formats a dropdown lists as unavailable, with the reason each is.
pub fn unavailable_formats() -> Vec<(String, String)> {
    Format::ALL
        .into_iter()
        .filter_map(|format| format.unavailable().map(|why| (format.label().to_string(), why)))
        .collect()
}

/// The check of a form as typed.
pub fn check(draft: &Draft, typed: &Typed, world: &World<'_>) -> Check {
    let mut found = Check::default();
    match draft.mode {
        Mode::Archive => check_archive(draft, typed, world, &mut found),
        Mode::Reduce => check_reduce(draft, typed, world, &mut found),
    }
    found
}

fn check_archive(draft: &Draft, typed: &Typed, world: &World<'_>, found: &mut Check) {
    let format = draft.pack.format;
    let taken = |name: &str| world.existing.contains(name);
    let name = check_name(&typed.name, format, taken);
    // A taken name is not an error: the person picks keep both or replace.
    if !name.is_usable() {
        found.name = name.message().map(str::to_string);
    }
    if let Err(error) = Excludes::parse(&typed.excludes) {
        found.excludes = Some(format!("{error}."));
    }
    if draft.locked && format.can_lock() {
        if typed.password.is_empty() {
            found.password = Some("Enter a password, or turn the password off.".into());
        } else if typed.confirm != typed.password {
            found.confirm = Some("The passwords do not match.".into());
        }
    }
    found.blocked = found
        .name
        .clone()
        .or_else(|| format.unavailable())
        .or_else(|| found.excludes.clone())
        .or_else(|| found.password.clone())
        .or_else(|| found.confirm.clone());
}

fn check_reduce(draft: &Draft, typed: &Typed, world: &World<'_>, found: &mut Check) {
    let Some(tools) = world.tools else {
        found.blocked = Some("Looking for the tools that shrink media…".into());
        return;
    };
    let kinds = world.subject.kinds_present();
    if kinds.is_empty() {
        found.blocked = Some("There is nothing here that can be made smaller.".into());
        return;
    }
    if let Some(reason) = kinds.iter().find_map(|kind| tools.unavailable(*kind)) {
        found.blocked = Some(reason);
        return;
    }
    if kinds.contains(&MediaKind::Image) {
        if draft.custom_dimension
            && let Err(message) = parse_dimension(&typed.dimension)
        {
            found.dimension = Some(message);
        }
        if let Some(reason) = tools.image_format_unavailable(draft.reduce.image.format) {
            found.blocked = Some(reason);
        }
    }
    if kinds.contains(&MediaKind::Video)
        && let Some(reason) = tools.video_codec_unavailable(draft.reduce.video.codec)
    {
        found.blocked = Some(reason);
    }
    if kinds.contains(&MediaKind::Audio)
        && let Some(reason) = tools.audio_codec_unavailable(draft.reduce.audio.codec)
    {
        found.blocked = Some(reason);
    }
    found.blocked = found.blocked.take().or_else(|| found.dimension.clone());
}

/// The archive's file name for what is typed, after the conflict choice:
/// `Archive.zip`, or `Archive 2.zip` when keeping both.
pub fn result_name(draft: &Draft, typed: &Typed, existing: &HashSet<String>) -> String {
    let format = draft.pack.format;
    let wanted = file_name(&typed.name, format);
    if existing.contains(&wanted) && draft.conflict == OnConflict::KeepBoth {
        file_name(&free_base(&typed.name, format, |name| existing.contains(name)), format)
    } else {
        wanted
    }
}

/// Whether the typed name is taken, for the conflict row.
pub fn name_is_taken(draft: &Draft, typed: &Typed, existing: &HashSet<String>) -> bool {
    check_name(&typed.name, draft.pack.format, |name| existing.contains(name)) == NameCheck::Taken
}

/// The pack options of a form, with its password.
pub fn pack_options(draft: &Draft, typed: &Typed) -> Result<PackOptions, String> {
    let excludes = Excludes::parse(&typed.excludes).map_err(|error| error.to_string())?;
    let mut options = draft.pack.clone();
    options.excludes = excludes;
    options.password = (draft.locked && options.format.can_lock() && !typed.password.is_empty())
        .then(|| Password::new(typed.password.clone()));
    Ok(options.normalised())
}

/// The reduce options of a form, with the typed picture size.
pub fn reduce_options(draft: &Draft, typed: &Typed) -> ReduceOptions {
    let mut options = draft.reduce.clone();
    if draft.custom_dimension
        && let Ok(size) = parse_dimension(&typed.dimension)
    {
        options.image.max_dimension = Some(size);
    }
    options
}

/// The words for the picture size cap.
pub fn dimension_label(cap: Option<u32>) -> String {
    match cap {
        None => "Original size".to_string(),
        Some(px) => format!("{px} px"),
    }
}

/// The words for a video height cap.
pub fn height_label(cap: Option<u32>) -> String {
    match cap {
        None => "Original size".to_string(),
        Some(px) => format!("{px}p"),
    }
}

/// The words for a frame rate cap.
pub fn rate_label(cap: Option<u32>) -> String {
    match cap {
        None => "Original rate".to_string(),
        Some(fps) => format!("{fps} fps"),
    }
}

/// A quality as the segmented control's key.
pub fn quality_key(quality: VideoQuality) -> &'static str {
    match quality {
        VideoQuality::Level(bingux_files::ops::Level::Fastest) => "fastest",
        VideoQuality::Level(bingux_files::ops::Level::Balanced) => "balanced",
        VideoQuality::Level(bingux_files::ops::Level::Smallest) => "smallest",
        VideoQuality::Crf(_) => "custom",
    }
}

/// Which picture formats this machine can write, with the reason for the
/// ones it cannot.
pub fn image_formats_unavailable(tools: &Tools) -> Vec<(String, String)> {
    ImageFormat::ALL
        .into_iter()
        .filter_map(|format| {
            tools
                .image_format_unavailable(format)
                .map(|why| (format.key().to_string(), why))
        })
        .collect()
}

/// Which film codecs this machine cannot make, with the reason.
pub fn video_codecs_unavailable(tools: &Tools) -> Vec<(String, String)> {
    VideoCodec::ALL
        .into_iter()
        .filter_map(|codec| {
            tools
                .video_codec_unavailable(codec)
                .map(|why| (codec.key().to_string(), why))
        })
        .collect()
}

/// Which song codecs this machine cannot make, with the reason.
pub fn audio_codecs_unavailable(tools: &Tools) -> Vec<(String, String)> {
    bingux_files::ops::AudioCodec::ALL
        .into_iter()
        .filter_map(|codec| {
            tools
                .audio_codec_unavailable(codec)
                .map(|why| (codec.key().to_string(), why))
        })
        .collect()
}

/// The sentence of the footer's summary row: `Original 842 MB, estimated 120 MB`.
pub fn summary(original: u64, estimated: Option<u64>) -> String {
    use bingux_files::display::size;
    match estimated {
        Some(estimated) => format!("Original {}, estimated {}", size(original), size(estimated)),
        None => format!("Original {}, estimating…", size(original)),
    }
}

/// How a finished archive reads in a toast: `Created “Photos.zip”`.
pub fn created_text(name: &str) -> String {
    format!("Created “{name}”")
}

/// How a finished reduction reads in a toast.
pub fn reduced_text(reduced: usize, kept: usize, failed: usize, from: u64, to: u64) -> String {
    use bingux_files::display::size;
    let mut parts = Vec::new();
    if reduced == 1 && kept == 0 && failed == 0 {
        parts.push(format!("Reduced from {} to {}", size(from), size(to)));
    } else if reduced > 0 {
        parts.push(format!(
            "Reduced {reduced} file{} from {} to {}",
            if reduced == 1 { "" } else { "s" },
            size(from),
            size(to)
        ));
    }
    if kept > 0 {
        parts.push(if reduced == 0 && kept == 1 {
            "Already small: kept original".to_string()
        } else {
            format!("{kept} already small: kept")
        });
    }
    if failed > 0 {
        parts.push(format!("{failed} could not be reduced"));
    }
    if parts.is_empty() {
        "Nothing was reduced".to_string()
    } else {
        parts.join(". ")
    }
}

#[cfg(test)]
mod tests {
    use bingux_files::Kind;
    use bingux_files::ops::prefs::Prefs;

    use super::*;

    fn entry(name: &str, folder: bool) -> Entry {
        let kind = if folder { Kind::Directory } else { Kind::File };
        let mut entry = Entry::named(PathBuf::from("/home/u/Pictures").join(name), name.to_string(), kind, false, false);
        entry.size = 1000;
        entry
    }

    fn subject(names: &[&str]) -> Subject {
        let entries: Vec<Entry> = names.iter().map(|name| entry(name, !name.contains('.'))).collect();
        Subject::of(&entries)
    }

    fn typed(name: &str) -> Typed {
        Typed {
            name: name.into(),
            excludes: String::new(),
            password: String::new(),
            confirm: String::new(),
            dimension: String::new(),
        }
    }

    fn world<'a>(subject: &'a Subject, existing: &'a HashSet<String>, tools: Option<&'a Tools>) -> World<'a> {
        World { subject, existing, tools }
    }

    #[test]
    fn titles_and_default_names_follow_the_selection() {
        assert_eq!(subject(&["holiday"]).title(), "Compress “holiday”");
        assert_eq!(subject(&["holiday"]).default_base(), "holiday");
        assert_eq!(subject(&["a.txt", "b.txt", "c"]).title(), "Compress 3 items");
        assert_eq!(subject(&["a.txt", "b.txt"]).default_base(), "Archive");
        assert_eq!(subject(&["report.pdf"]).default_base(), "report");
    }

    #[test]
    fn pure_media_opens_on_reduce_and_anything_else_on_archive() {
        assert_eq!(subject(&["a.jpg", "b.png"]).default_mode(), Mode::Reduce);
        assert_eq!(subject(&["a.mp4"]).default_mode(), Mode::Reduce);
        assert_eq!(subject(&["a.flac", "b.jpg", "c.mkv"]).default_mode(), Mode::Reduce);
        assert_eq!(subject(&["a.jpg", "notes.txt"]).default_mode(), Mode::Archive, "mixed: archive only");
        assert_eq!(subject(&["photos"]).default_mode(), Mode::Archive, "a folder is not media");
        assert!(!subject(&["a.jpg", "photos"]).is_pure_media());
        let kinds = subject(&["a.flac", "b.jpg", "c.mkv", "d.jpg"]);
        assert_eq!(kinds.kinds_present(), [MediaKind::Image, MediaKind::Video, MediaKind::Audio]);
    }

    #[test]
    fn a_name_is_checked_as_it_is_typed() {
        let subject = subject(&["a.txt"]);
        let none = HashSet::new();
        let draft = Draft::new(&subject, Prefs::default());
        let check_of = |name: &str, existing: &HashSet<String>| check(&draft, &typed(name), &world(&subject, existing, None));
        assert_eq!(check_of("", &none).name.as_deref(), Some("Enter a name for the archive."));
        assert!(check_of("", &none).blocked.is_some());
        assert_eq!(check_of("a/b", &none).name.as_deref(), Some("A name cannot contain a slash."));
        assert!(check_of("fine", &none).blocked.is_none() || !Format::Zip.is_available());
        // A taken name is no error; the conflict row asks what to do.
        let taken: HashSet<String> = ["fine.zip".to_string()].into();
        assert_eq!(check_of("fine", &taken).name, None);
        assert!(name_is_taken(&draft, &typed("fine"), &taken));
    }

    #[test]
    fn a_taken_name_is_numbered_or_replaced() {
        let subject = subject(&["a.txt"]);
        let taken: HashSet<String> = ["Archive.zip".to_string(), "Archive 2.zip".to_string()].into();
        let mut draft = Draft::new(&subject, Prefs::default());
        draft.pack.format = Format::Zip;
        assert_eq!(result_name(&draft, &typed("Archive"), &taken), "Archive 3.zip");
        draft.conflict = OnConflict::Replace;
        assert_eq!(result_name(&draft, &typed("Archive"), &taken), "Archive.zip");
        assert_eq!(result_name(&draft, &typed("Free"), &taken), "Free.zip");
    }

    #[test]
    fn passwords_must_be_there_and_match() {
        let subject = subject(&["a.txt"]);
        let none = HashSet::new();
        let mut draft = Draft::new(&subject, Prefs::default());
        draft.locked = true;
        let mut input = typed("x");
        let check_now = |draft: &Draft, input: &Typed| check(draft, input, &world(&subject, &none, None));
        assert!(check_now(&draft, &input).password.is_some());
        input.password = "secret".into();
        assert!(check_now(&draft, &input).confirm.is_some());
        input.confirm = "secret".into();
        let ok = check_now(&draft, &input);
        assert!(ok.password.is_none() && ok.confirm.is_none());
        // A format with no lock ignores the password.
        draft.pack.format = Format::TarGz;
        input.confirm.clear();
        assert!(check_now(&draft, &input).confirm.is_none());
        assert!(pack_options(&draft, &input).unwrap().password.is_none());
        draft.pack.format = Format::Zip;
        input.confirm = "secret".into();
        assert!(pack_options(&draft, &input).unwrap().password.is_some());
    }

    #[test]
    fn exclude_patterns_are_validated() {
        let subject = subject(&["a.txt"]);
        let none = HashSet::new();
        let draft = Draft::new(&subject, Prefs::default());
        let mut input = typed("x");
        input.excludes = "*.tmp, node_modules".into();
        assert!(check(&draft, &input, &world(&subject, &none, None)).excludes.is_none());
        input.excludes = "a[bc".into();
        let bad = check(&draft, &input, &world(&subject, &none, None));
        assert!(bad.excludes.as_deref().is_some_and(|text| text.contains("never closed")), "{bad:?}");
        assert!(bad.blocked.is_some());
        assert!(pack_options(&draft, &input).is_err());
    }

    #[test]
    fn picture_sizes_are_read_and_bounded() {
        assert_eq!(parse_dimension(" 1600 "), Ok(1600));
        assert!(parse_dimension("").is_err());
        assert!(parse_dimension("12px").is_err());
        assert!(parse_dimension("15").is_err());
        assert!(parse_dimension("20001").is_err());
        assert_eq!(parse_dimension("16"), Ok(16));
    }

    fn tools() -> Tools {
        Tools {
            ffmpeg: Some("/usr/bin/ffmpeg".into()),
            ffprobe: Some("/usr/bin/ffprobe".into()),
            magick: Some("/usr/bin/magick".into()),
            encoders: ["libx264", "aac", "libopus", "libwebp", "mjpeg", "png"].map(String::from).to_vec(),
            magick_formats: ["JPEG", "PNG", "WEBP"].map(String::from).to_vec(),
        }
    }

    #[test]
    fn reduce_is_blocked_while_tools_are_unknown_or_missing() {
        let subject = subject(&["a.jpg", "b.mp4"]);
        let none = HashSet::new();
        let mut draft = Draft::new(&subject, Prefs::default());
        assert_eq!(draft.mode, Mode::Reduce);
        let waiting = check(&draft, &typed(""), &world(&subject, &none, None));
        assert!(waiting.blocked.as_deref().is_some_and(|text| text.contains("Looking")));
        let ready = tools();
        assert!(check(&draft, &typed(""), &world(&subject, &none, Some(&ready))).blocked.is_none());
        let no_ffmpeg = Tools { ffmpeg: None, ffprobe: None, ..tools() };
        assert_eq!(
            check(&draft, &typed(""), &world(&subject, &none, Some(&no_ffmpeg))).blocked.as_deref(),
            Some("ffmpeg is not installed.")
        );
        // A codec the machine lacks blocks too.
        draft.reduce.video.codec = VideoCodec::H265;
        assert_eq!(
            check(&draft, &typed(""), &world(&subject, &none, Some(&ready))).blocked.as_deref(),
            Some("ffmpeg here has no H.265 (MP4) encoder.")
        );
    }

    #[test]
    fn a_custom_picture_size_must_be_a_number() {
        let subject = subject(&["a.jpg"]);
        let none = HashSet::new();
        let ready = tools();
        let mut draft = Draft::new(&subject, Prefs::default());
        draft.custom_dimension = true;
        let mut input = typed("");
        assert!(check(&draft, &input, &world(&subject, &none, Some(&ready))).dimension.is_some());
        input.dimension = "1600".into();
        assert!(check(&draft, &input, &world(&subject, &none, Some(&ready))).blocked.is_none());
        assert_eq!(reduce_options(&draft, &input).image.max_dimension, Some(1600));
    }

    #[test]
    fn what_is_dangerous_is_not_remembered_into_a_new_draft() {
        let subject = subject(&["a.txt"]);
        let mut prefs = Prefs::default();
        prefs.pack.delete_originals = true;
        prefs.reduce.replace_original = true;
        prefs.pack.password = Some(Password::new("x"));
        let draft = Draft::new(&subject, prefs);
        assert!(!draft.pack.delete_originals && !draft.reduce.replace_original && draft.pack.password.is_none());
    }

    #[test]
    fn labels_read_like_the_dialog() {
        assert_eq!(dimension_label(None), "Original size");
        assert_eq!(dimension_label(Some(1920)), "1920 px");
        assert_eq!(height_label(Some(1080)), "1080p");
        assert_eq!(rate_label(Some(30)), "30 fps");
        assert_eq!(quality_key(VideoQuality::Crf(30)), "custom");
    }

    #[test]
    fn the_summary_and_toasts_read_like_the_spec() {
        assert_eq!(summary(842_000_000, Some(120_000_000)), "Original 842 MB, estimated 120 MB");
        assert!(summary(842_000_000, None).ends_with("estimating…"));
        assert_eq!(created_text("Photos.zip"), "Created “Photos.zip”");
        assert_eq!(reduced_text(3, 0, 0, 1_200_000_000, 410_000_000), "Reduced 3 files from 1.2 GB to 410 MB");
        assert_eq!(reduced_text(0, 1, 0, 0, 0), "Already small: kept original");
        assert_eq!(reduced_text(2, 1, 1, 2_000_000, 1_000_000), "Reduced 2 files from 2.0 MB to 1.0 MB. 1 already small: kept. 1 could not be reduced");
        assert_eq!(reduced_text(0, 0, 0, 0, 0), "Nothing was reduced");
    }

    #[test]
    fn unavailable_things_carry_their_reason() {
        let none = Tools::default();
        let images = image_formats_unavailable(&none);
        assert_eq!(images.len(), ImageFormat::ALL.len());
        assert!(video_codecs_unavailable(&none).iter().all(|(_, why)| why.contains("ffmpeg")));
        assert!(audio_codecs_unavailable(&tools()).iter().all(|(key, _)| key != "opus"));
        for (label, why) in unavailable_formats() {
            assert!(label.starts_with('.') && why.ends_with('.'), "{label}: {why}");
        }
    }
}
