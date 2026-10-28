//! The Reduce size half of the Compress dialog: the groups for pictures,
//! films and songs, and the result. See [`super::compress`].
//!
//! A kind this machine cannot make smaller is not hidden quietly: it shows a
//! disabled row that says why ("ffmpeg is not installed."), so a person knows
//! what to install. An encoder it lacks is a disabled option in its dropdown
//! with the same kind of reason.

use bingux_files::display::size;
use bingux_files::ops::{
    AUDIO_KBPS, AudioCodec, DIMENSIONS, Estimate, FRAME_RATES, HEIGHTS, ImageFormat, Level, MediaKind,
    SOUND_KBPS, Sound, Tools, VideoCodec, VideoQuality,
};
use freya::prelude::*;

use super::compress::{Fields, location_row};
use super::compress_model::{
    Check, Draft, Subject, audio_codecs_unavailable, dimension_label, height_label,
    image_formats_unavailable, quality_key, rate_label, summary, video_codecs_unavailable,
};
use crate::controls::{dropdown, form_field, segmented};
use crate::group::{Group, form_row};
use crate::motion::reveal_block;
use crate::rows::{property_row, row, slider_row, switch_row};
use crate::style::*;

/// What the media groups read.
pub(super) struct Media<'a> {
    pub fields: Fields,
    pub draft: &'a Draft,
    pub check: &'a Check,
    pub subject: &'a Subject,
    /// `None` while the machine is still being looked at.
    pub tools: Option<&'a Tools>,
    pub estimate: Option<&'a Estimate>,
    /// A sample is being encoded.
    pub estimating: bool,
    pub estimate_error: Option<&'a str>,
    /// Where the result goes, and what opens the chooser for it.
    pub destination: &'a std::path::Path,
    pub choose: EventHandler<()>,
}

fn keyed<T: Copy>(items: &[T], key: impl Fn(T) -> String, label: impl Fn(T) -> String) -> Vec<(String, String)> {
    items.iter().map(|item| (key(*item), label(*item))).collect()
}

/// A group, or, when the kind cannot be made here, the one row that says why.
fn missing(kind: MediaKind, reason: String) -> Element {
    Group::titled(kind.label())
        .row(row(format!("{} cannot be made smaller", kind.label())).subtitle(reason).enabled(false))
        .into()
}

pub(super) fn media_groups(m: &Media<'_>) -> Vec<Element> {
    let mut groups = Vec::new();
    let Some(tools) = m.tools else {
        // Still looking: the card is already the right size.
        groups.push(Group::titled("Checking").row(row("Looking for the tools that shrink media…").enabled(false)).into());
        return groups;
    };
    for kind in m.subject.kinds_present() {
        if let Some(reason) = tools.unavailable(kind) {
            groups.push(missing(kind, reason));
            continue;
        }
        groups.push(match kind {
            MediaKind::Image => images(m, tools),
            MediaKind::Video => films(m, tools),
            MediaKind::Audio => songs(m, tools),
        });
    }
    // Nothing to reduce here means nothing to size or place.
    if m.subject.kinds_present().iter().any(|kind| tools.unavailable(*kind).is_none()) {
        groups.push(result(m));
    }
    groups
}

fn images(m: &Media<'_>, tools: &Tools) -> Element {
    let options = &m.draft.reduce.image;
    let mut state = m.fields.draft;
    let formats = keyed(&ImageFormat::ALL, |f| f.key().to_string(), |f| f.label().to_string());
    let format_row = form_row(
        "Format",
        dropdown(formats, options.format.key(), move |key: String| {
            if let Some(format) = ImageFormat::from_key(&key) {
                state.write().reduce.image.format = format;
            }
        })
        .unavailable(image_formats_unavailable(tools))
        .title("Format"),
    );
    // PNG has no quality: it is lossless, and the slider says so.
    let lossless = matches!(options.format, ImageFormat::Png);
    let quality = slider_row("Quality", f64::from(options.quality), move |value: f64| {
        state.write().reduce.image.quality = value.round().clamp(1.0, 100.0) as u8;
    })
    .max(100.0)
    .value_text(if lossless { "Lossless".to_string() } else { options.quality.to_string() })
    .subtitle(if lossless { "PNG keeps every pixel; only the packing changes." } else { "Lower is smaller." })
    .enabled(!lossless);
    let mut sizes = vec![("original".to_string(), dimension_label(None))];
    sizes.extend(keyed(&DIMENSIONS, |px| px.to_string(), |px| dimension_label(Some(px))));
    sizes.push(("custom".to_string(), "Other…".to_string()));
    let selected = if m.draft.custom_dimension {
        "custom".to_string()
    } else {
        options.max_dimension.map_or_else(|| "original".to_string(), |px| px.to_string())
    };
    let cap_row = form_row(
        "Longest side",
        dropdown(sizes, &selected, move |key: String| {
            let mut draft = state.write();
            draft.custom_dimension = key == "custom";
            draft.reduce.image.max_dimension = key.parse().ok();
        })
        .title("Longest side"),
    );
    let custom = reveal_block(
        m.draft.custom_dimension,
        S0,
        rect().child(Element::from(
            form_field("Pixels", m.fields.dimension).suffix("px").error(m.check.dimension.clone()),
        )),
    )
    .lazy();
    Group::titled("Pictures")
        .row(format_row)
        .row(quality)
        .row(cap_row)
        .row(custom)
        .row(
            switch_row("Remove location and camera data", options.strip_metadata, move |on: bool| {
                state.write().reduce.image.strip_metadata = on;
            })
            .subtitle("Takes out the EXIF, including where and with what it was taken. The colours stay."),
        )
        .into()
}

fn films(m: &Media<'_>, tools: &Tools) -> Element {
    let options = &m.draft.reduce.video;
    let mut state = m.fields.draft;
    let codecs = keyed(&VideoCodec::ALL, |c| c.key().to_string(), |c| c.label().to_string());
    let codec_row = form_row(
        "Format",
        dropdown(codecs, options.codec.key(), move |key: String| {
            if let Some(codec) = VideoCodec::from_key(&key) {
                state.write().reduce.video.codec = codec;
            }
        })
        .unavailable(video_codecs_unavailable(tools))
        .title("Film format"),
    );
    let levels = vec![
        ("fastest".to_string(), "Fastest".to_string()),
        ("balanced".to_string(), "Balanced".to_string()),
        ("smallest".to_string(), "Smallest".to_string()),
        ("custom".to_string(), "Custom".to_string()),
    ];
    let quality_row = form_row(
        "Quality",
        segmented(levels, quality_key(options.quality), move |key: String| {
            let mut draft = state.write();
            let codec = draft.reduce.video.codec;
            draft.reduce.video.quality = match key.as_str() {
                "fastest" => VideoQuality::Level(Level::Fastest),
                "smallest" => VideoQuality::Level(Level::Smallest),
                "custom" => VideoQuality::Crf(bingux_files::ops::VideoCodec::crf_for(codec, Level::Balanced)),
                _ => VideoQuality::Level(Level::Balanced),
            };
        }),
    );
    let (low, high) = options.codec.crf_range();
    let crf = match options.quality {
        VideoQuality::Crf(crf) => crf.clamp(low, high),
        VideoQuality::Level(_) => options.codec.crf_for(Level::Balanced),
    };
    let crf_row = reveal_block(
        matches!(options.quality, VideoQuality::Crf(_)),
        S0,
        rect().child(Element::from(
            slider_row("Constant rate factor", f64::from(high - crf) / f64::from(high - low) * 100.0, move |value: f64| {
                let mut draft = state.write();
                let (low, high) = draft.reduce.video.codec.crf_range();
                let crf = f64::from(high) - value / 100.0 * f64::from(high - low);
                draft.reduce.video.quality = VideoQuality::Crf(crf.round().clamp(f64::from(low), f64::from(high)) as u8);
            })
            .value_text(format!("CRF {crf}"))
            .subtitle("Higher quality on the right: a bigger file."),
        )),
    )
    .lazy();
    let mut heights = vec![("original".to_string(), height_label(None))];
    heights.extend(keyed(&HEIGHTS, |px| px.to_string(), |px| height_label(Some(px))));
    let height_row = form_row(
        "Resolution",
        dropdown(
            heights,
            &options.max_height.map_or_else(|| "original".to_string(), |px| px.to_string()),
            move |key: String| state.write().reduce.video.max_height = key.parse().ok(),
        )
        .title("Resolution"),
    );
    let mut rates = vec![("original".to_string(), rate_label(None))];
    rates.extend(keyed(&FRAME_RATES, |fps| fps.to_string(), |fps| rate_label(Some(fps))));
    let rate_row = form_row(
        "Frame rate",
        dropdown(
            rates,
            &options.max_fps.map_or_else(|| "original".to_string(), |fps| fps.to_string()),
            move |key: String| state.write().reduce.video.max_fps = key.parse().ok(),
        )
        .title("Frame rate"),
    );
    let mut sounds = vec![("remove".to_string(), "Remove sound".to_string())];
    sounds.extend(keyed(&SOUND_KBPS, |kbps| kbps.to_string(), |kbps| format!("{kbps} kbit/s")));
    let sound_row = form_row(
        "Sound",
        dropdown(
            sounds,
            &match options.sound {
                Sound::Remove => "remove".to_string(),
                Sound::Kbps(kbps) => kbps.to_string(),
            },
            move |key: String| {
                state.write().reduce.video.sound = key.parse().map_or(Sound::Remove, Sound::Kbps);
            },
        )
        .title("Sound"),
    );
    Group::titled("Films")
        .row(codec_row)
        .row(quality_row)
        .row(crf_row)
        .row(height_row)
        .row(rate_row)
        .row(sound_row)
        .row(
            switch_row("Remove location and other tags", options.strip_metadata, move |on: bool| {
                state.write().reduce.video.strip_metadata = on;
            })
            .subtitle("Takes out the title, the place and the other tags."),
        )
        .into()
}

fn songs(m: &Media<'_>, tools: &Tools) -> Element {
    let options = &m.draft.reduce.audio;
    let mut state = m.fields.draft;
    let codecs = keyed(&AudioCodec::ALL, |c| c.key().to_string(), |c| c.label().to_string());
    let codec_row = form_row(
        "Format",
        dropdown(codecs, options.codec.key(), move |key: String| {
            if let Some(codec) = AudioCodec::from_key(&key) {
                state.write().reduce.audio.codec = codec;
            }
        })
        .unavailable(audio_codecs_unavailable(tools))
        .title("Song format"),
    );
    let rates = keyed(&AUDIO_KBPS, |kbps| kbps.to_string(), |kbps| format!("{kbps} kbit/s"));
    let lossless = !options.codec.has_bitrate();
    let rate_row = row("Bitrate")
        .subtitle(if lossless { "FLAC keeps every sample; only the packing changes." } else { "Opus sounds fine at 96; MP3 wants 128 or more." })
        .trailing(
            dropdown(rates, &options.kbps.to_string(), move |key: String| {
                if let Ok(kbps) = key.parse() {
                    state.write().reduce.audio.kbps = kbps;
                }
            })
            .enabled(!lossless)
            .title("Bitrate"),
        );
    Group::titled("Songs")
        .row(codec_row)
        .row(rate_row)
        .row(
            switch_row("Remove tags and cover", options.strip_metadata, move |on: bool| {
                state.write().reduce.audio.strip_metadata = on;
            })
            .subtitle("Takes out the artist, the title and the picture."),
        )
        .into()
}

/// The size, where it goes and whether it replaces the original.
fn result(m: &Media<'_>) -> Element {
    let mut state = m.fields.draft;
    let reducible: Vec<usize> = m
        .subject
        .kinds
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| kind.map(|_| index))
        .collect();
    let original: u64 = reducible.iter().map(|&index| m.subject.sizes[index]).sum();
    let estimated = m.estimate.map(|estimate| estimate.estimated);
    let summary_line = match (m.estimate_error, estimated) {
        (Some(error), _) if !error.is_empty() && !error.contains("cancelled") => format!("Original {}, no estimate: {error}", size(original)),
        (_, Some(estimated)) if !m.estimating => summary(original, Some(estimated)),
        _ => summary(original, None),
    };
    let mut group = Group::titled("Result").row(row("Size").subtitle(summary_line));
    // Each file's own estimate, for a handful of them.
    const LISTED: usize = 6;
    for (shown, &index) in reducible.iter().enumerate() {
        if shown == LISTED {
            group = group.row(row(format!("and {} more", reducible.len() - LISTED)).enabled(false));
            break;
        }
        let was = m.subject.sizes[index];
        let now = m
            .estimate
            .and_then(|estimate| estimate.per_file.get(shown).copied().flatten())
            .map_or_else(|| "…".to_string(), |bytes| if bytes >= was { "kept as it is".to_string() } else { format!("about {}", size(bytes)) });
        group = group.row(property_row(m.subject.names[index].clone(), format!("{}, {now}", size(was))));
    }
    let beside = m.draft.location.is_none();
    group = group
        .row(location_row(m.destination, m.choose.clone()))
        .row(
            switch_row("Replace the originals", m.draft.reduce.replace_original && beside, move |on: bool| {
                state.write().reduce.replace_original = on;
            })
            .subtitle(if beside {
                "The originals go to the Trash once the smaller files are made and checked. You will be asked."
            } else {
                "Replacing needs the result to sit beside the original."
            })
            .enabled(beside),
        );
    group.into()
}
