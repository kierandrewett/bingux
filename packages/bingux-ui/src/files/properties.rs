//! The Properties dialog: what a file, a folder or a selection is.
//!
//! One file shows, top to bottom: its picture or icon, name and type; what
//! it is (type, size with the byte count, where it lives, where a link
//! leads); for a picture its size and camera, for a recording its length and
//! codecs; its times; its owner and group; its permissions as a grid of
//! boxes, editable by the owner, with the numeric mode and "allow executing
//! as program"; the drive it is on with a free space bar; and the
//! applications that can open it. Several items show their combined count,
//! size and types, where they are, and the drive.
//!
//! Nothing blocks the window. The facts of one `stat` and the file's header
//! are read on a pool worker; the size of a folder or a selection is added
//! up by a counting thread that reports as it goes, so the figure grows on
//! screen, can be stopped with a button, and stops when the dialog closes.
//!
//! Every row exists from the first frame. A fact that has not arrived shows
//! "…" and one that does not exist shows "—", so nothing moves when the
//! answers come. The permission boxes are laid out disabled until the mode
//! is known.
//!
//! The dialog is a [`framed_dialog`]: Escape and the scrim close it, and so
//! does the close button.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use bingux_files::media::{self, AvInfo, ImageInfo};
use bingux_files::ops::{self, Counting, Properties, Totals, Volume};
use bingux_files::thumbs::{self, Bucket};
use bingux_files::{Category, Entry, Kind, display};
use freya::prelude::*;
use freya::torin::prelude::Size2D;

use super::actions::{Env, apps_for, launch};
use super::properties_permissions::{PermissionGrid, is_executable, mode_line, with_execute};
use super::glyphs::category_icon;
use super::view::entry_glyph;
use crate::component::ComponentName;
use crate::controls::{action_button, icon_button, usage_bar};
use crate::group::Group;
use crate::icons::{self, Icon};
use crate::rows::{property_row, row, switch_row};
use crate::shell::{Overlays, framed_dialog};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::{spawn_blocking, use_resource};
use crate::typography::{BODY, CAPTION, Ink, SECTION, TITLE};

/// How often a running total is read.
const TICK: Duration = Duration::from_millis(150);
/// How many applications "Open with" lists.
const APP_ROWS: usize = 6;
/// A fact that has not arrived yet, and one that does not exist.
const PENDING: &str = "…";
const NONE: &str = "—";

/// Open the Properties dialog for `selected`, or for the folder on show when
/// nothing is selected.
pub(super) fn show_properties(env: &Env, selected: &[Entry]) {
    let entries: Vec<Entry> = if selected.is_empty() {
        let Some(folder) = &env.folder else {
            return;
        };
        let name = folder
            .file_name()
            .map_or_else(|| folder.to_string_lossy(), |name| name.to_string_lossy())
            .into_owned();
        vec![Entry::named(folder.clone(), name, Kind::Directory, false, false)]
    } else {
        selected.to_vec()
    };
    let title = match entries.as_slice() {
        [only] => only.name.clone(),
        many => format!("{} items", many.len()),
    };
    let focus = env.focus;
    env.overlays.show_dialog(
        framed_dialog(
            format!("Properties of {title}"),
            Element::from(Body {
                entries,
                overlays: env.overlays,
                open_folder: env.open_folder.clone(),
                open_with: env.caps.open_with,
                home: env.home.clone(),
            }),
            Dim::PopupWidth.px(),
            Dim::PopupHeight.px() + 120.0,
        )
        // Opened from the keyboard: give it back.
        .on_closed(move |()| focus.request_focus()),
    );
}

// ---------------------------------------------------------------------------
// What is read off the UI thread
// ---------------------------------------------------------------------------

/// What one file's facts load needs. `details` is false for the first of
/// several items, which only supplies the drive.
#[derive(Clone, Debug, PartialEq)]
struct Subject {
    path: PathBuf,
    category: Category,
    details: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Facts {
    props: Properties,
    image: Option<ImageInfo>,
    av: Option<AvInfo>,
    /// The desktop's thumbnail of a document or recording.
    poster: Option<PathBuf>,
}

fn load_facts(subject: Subject) -> Result<Facts, String> {
    let props = ops::stat_properties(&subject.path).map_err(|error| error.to_string())?;
    let (mut image, mut av, mut poster) = (None, None, None);
    if subject.details {
        match subject.category {
            Category::Image => image = media::image_info(&subject.path),
            Category::Audio | Category::Video => {
                av = media::av_info(&subject.path);
                poster = find_poster(&props);
            }
            Category::Document => poster = find_poster(&props),
            _ => {}
        }
    }
    Ok(Facts {
        props,
        image,
        av,
        poster,
    })
}

fn find_poster(props: &Properties) -> Option<PathBuf> {
    [Bucket::Large, Bucket::XLarge, Bucket::XXLarge, Bucket::Normal]
        .into_iter()
        .find_map(|bucket| thumbs::lookup(&props.path, props.modified, bucket))
}

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/// `2,500,000`.
fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `2.5 MB (2,500,000 bytes)`, or `512 bytes` when the unit is bytes.
fn size_with_bytes(bytes: u64) -> String {
    let size = display::size(bytes);
    if bytes < 1000 {
        size
    } else {
        format!("{size} ({} bytes)", grouped(bytes))
    }
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The size line of a folder or a selection while and after it is counted.
fn total_size(totals: Totals) -> String {
    let size = size_with_bytes(totals.bytes);
    if totals.cancelled {
        format!("At least {size} (stopped)")
    } else if !totals.done {
        format!("{size} so far…")
    } else if totals.unreadable > 0 {
        format!("At least {size}")
    } else {
        size
    }
}

/// How many items a folder or selection holds, while and after it is counted.
fn contents(totals: Totals) -> String {
    let files = plural(totals.files, "file", "files");
    let folders = plural(totals.folders, "folder", "folders");
    let mut text = format!("{} ({files}, {folders})", plural(totals.items(), "item", "items"));
    if totals.cancelled {
        text.insert_str(0, "At least ");
        text.push_str(" (stopped)");
    } else if !totals.done {
        text.push_str(" so far…");
    } else if totals.unreadable > 0 {
        text.insert_str(0, "At least ");
    }
    text
}

/// The kinds of several items: "PNG image (3), Folder (2)", the most first.
fn types_line(entries: &[Entry]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for entry in entries {
        match counts
            .iter_mut()
            .find(|(name, _)| *name == entry.mime.description)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((entry.mime.description, 1)),
        }
    }
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
        .iter()
        .map(|(name, count)| format!("{name} ({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The folder every one of `entries` is in, or `None` when they are in
/// several.
fn common_parent(entries: &[Entry]) -> Option<PathBuf> {
    let first = entries.first()?.path.parent()?;
    entries
        .iter()
        .all(|entry| entry.path.parent() == Some(first))
        .then(|| first.to_path_buf())
}

/// `~/Pictures` for a folder under the home folder, else the whole path.
fn home_relative(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

fn time(value: Option<SystemTime>) -> Option<String> {
    value.map(display::full)
}

// ---------------------------------------------------------------------------
// The dialog's body
// ---------------------------------------------------------------------------

/// Count `roots` in the background and return the running total and the
/// handle that stops it. The count stops when the dialog goes. With no roots
/// nothing is counted.
fn use_counting(roots: Option<Vec<PathBuf>>) -> (State<Totals>, Option<Rc<Counting>>) {
    let mut totals = use_state(Totals::default);
    let counting = use_hook(move || {
        roots.map(|roots| {
            let counting = Rc::new(Counting::start(roots));
            let reader = counting.clone();
            spawn(async move {
                loop {
                    let now = reader.totals();
                    totals.set(now);
                    if now.done {
                        break;
                    }
                    timer(TICK).await;
                }
            });
            counting
        })
    });
    (totals, counting)
}

#[derive(PartialEq)]
struct Body {
    entries: Vec<Entry>,
    overlays: Overlays,
    open_folder: EventHandler<PathBuf>,
    /// The view may open files with another application.
    open_with: bool,
    /// The home folder, which a location under it is written relative to.
    home: PathBuf,
}

impl Component for Body {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let entries = &self.entries;
        let first = &entries[0];
        let single = entries.len() == 1;
        let is_folder = single && first.is_dir();

        let facts = use_resource(
            Subject {
                path: first.path.clone(),
                category: first.category(),
                details: single,
            },
            load_facts,
        );
        let loaded = facts.with(|facts| facts.value().cloned());
        let failed = facts.with(|facts| facts.error().map(str::to_owned));

        let counted = !single || is_folder;
        let (totals, counting) = use_counting(counted.then(|| {
            entries.iter().map(|entry| entry.path.clone()).collect()
        }));
        // Each folder picked is one of the items, though the count does not
        // include the folder it starts from.
        let totals = {
            let counted = totals();
            let roots = if single {
                0
            } else {
                entries.iter().filter(|entry| entry.is_dir()).count() as u64
            };
            Totals {
                folders: counted.folders + roots,
                ..counted
            }
        };

        // The permissions the person has set in this dialog, until the next
        // time it is opened and read again.
        let mut edited = use_state(|| None::<u32>);
        let mut mode_error = use_state(|| None::<String>);

        // A fact from the load: the placeholder before it arrives, a dash
        // if there is none, or what `of` makes of it.
        let fact = |of: &dyn Fn(&Facts) -> Option<String>| -> String {
            match (&loaded, &failed) {
                (Some(loaded), _) => of(loaded).unwrap_or_else(|| NONE.to_owned()),
                (None, Some(_)) => NONE.to_owned(),
                (None, None) => PENDING.to_owned(),
            }
        };

        // ---- the head: picture, name, type ----------------------------
        let muted = Paint::Muted.color(&tokens);
        let head_icon = if single {
            entry_glyph(first)
        } else {
            shared_glyph(entries)
        };
        let glyph = |scale: f32| {
            rect()
                .tw(&tokens, &[w_full(), h_full(), center()])
                .child(icons::icon_scaled(
                    head_icon,
                    IconSize::Hero,
                    scale,
                    if is_folder {
                        Paint::Accent.color(&tokens)
                    } else {
                        muted
                    },
                ))
                .into_element()
        };
        let picture: Element = {
            let poster = loaded.as_ref().and_then(|loaded| loaded.poster.clone());
            let shows_file = single && first.category() == Category::Image;
            let source = if shows_file {
                Some(first.path.clone())
            } else {
                poster
            };
            match source {
                Some(path) => {
                    let fallback = glyph(2.0);
                    ImageViewer::new(path)
                        .decode_mode(DecodeMode::Custom(Size2D::new(256.0, 256.0)))
                        .width(Size::fill())
                        .height(Size::fill())
                        .aspect_ratio(AspectRatio::Min)
                        .image_cover(ImageCover::Center)
                        .loading_placeholder(fallback.clone())
                        .error_renderer(move |_: String| fallback.clone())
                        .into()
                }
                None => glyph(2.0),
            }
        };
        let name = if single {
            first.name.clone()
        } else {
            format!("{} items", entries.len())
        };
        let subtitle = if single {
            first.mime.description.to_owned()
        } else {
            types_line(entries)
        };
        let status = failed
            .clone()
            .map(|message| format!("Could not read everything: {message}"))
            .unwrap_or_default();

        let head = rect()
            .tw(&tokens, &[flex_col(), items(Align::Center), w_full(), gap(S2), py(S2)])
            .child(
                rect()
                    .tw(&tokens, &[w(Dim::Box24), h(Dim::Box24), rounded(Radius::Control), clip()])
                    .child(picture),
            )
            .child(
                TITLE
                    .label(&tokens, name)
                    .width(Size::fill())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .text_align(TextAlign::Center),
            )
            .child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, subtitle)
                    .width(Size::fill())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .text_align(TextAlign::Center),
            )
            // The line is always there, so an error appearing moves nothing.
            .child(
                CAPTION
                    .in_ink(Ink::Danger)
                    .label(&tokens, if status.is_empty() { " ".to_owned() } else { status })
                    .width(Size::fill())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .text_align(TextAlign::Center),
            );

        // ---- General --------------------------------------------------
        let location = common_parent(entries);
        let mut general = Group::titled("General");
        general = general.row(property_row(
            "Type",
            if single {
                format!("{} ({})", first.mime.description, first.mime.mime)
            } else {
                types_line(entries)
            },
        ));
        let size_text = if counted {
            total_size(totals)
        } else if first.sized {
            size_with_bytes(first.size)
        } else {
            fact(&|facts| Some(size_with_bytes(facts.props.size)))
        };
        let stop = counting
            .clone()
            .filter(|_| counted && !totals.done)
            .map(|counting| {
                action_button("Stop", move |_| counting.cancel())
                    .compact()
                    .into_element()
            });
        general = general.row(
            row("Size").trailing(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
                    .child(BODY.in_ink(Ink::Muted).selectable(&tokens, size_text))
                    .maybe_child(stop),
            ),
        );
        if counted {
            general = general.row(property_row("Contents", contents(totals)));
        }
        let location_text = location.as_ref().map_or_else(
            || "Several folders".to_owned(),
            |folder| home_relative(folder, &self.home),
        );
        let open_location = location.clone().map(|folder| {
            let open_folder = self.open_folder.clone();
            EventHandler::new(move |()| {
                overlays.dismiss_dialog();
                open_folder.call(folder.clone());
            })
        });
        general = general.row(match open_location {
            Some(open) => row("Location").value(location_text).on_press(open),
            None => property_row("Location", location_text),
        });
        if single && first.symlink {
            general = general.row(property_row(
                "Link target",
                fact(&|facts| {
                    facts
                        .props
                        .link_target
                        .as_ref()
                        .map(|target| target.to_string_lossy().into_owned())
                }),
            ));
        }

        let mut groups: Vec<Element> = vec![general.into()];

        // ---- Picture or recording -------------------------------------
        if single && first.category() == Category::Image {
            let mut image = Group::titled("Image").row(property_row(
                "Dimensions",
                fact(&|facts| {
                    facts.image.as_ref().map(|image| match image.megapixels() {
                        Some(mega) => format!("{} ({mega})", image.dimensions()),
                        None => image.dimensions(),
                    })
                }),
            ));
            // Only JPEG carries the camera's notes that are read.
            if first.mime.mime == "image/jpeg" {
                for (title, of) in [
                    ("Camera", exif_camera as fn(&ImageInfo) -> Option<String>),
                    ("Lens", exif_lens),
                    ("Taken", exif_taken),
                    ("Exposure", exif_exposure),
                ] {
                    image = image.row(property_row(
                        title,
                        fact(&|facts| facts.image.as_ref().and_then(of)),
                    ));
                }
            }
            groups.push(image.into());
        }
        if single && matches!(first.category(), Category::Audio | Category::Video) {
            groups.push(
                Group::titled(if first.category() == Category::Audio {
                    "Audio"
                } else {
                    "Video"
                })
                .row(property_row(
                    "Duration",
                    fact(&|facts| {
                        facts
                            .av
                            .as_ref()
                            .and_then(|av| av.seconds)
                            .map(media::clock)
                    }),
                ))
                .row(property_row(
                    "Codec",
                    fact(&|facts| facts.av.as_ref().and_then(AvInfo::codec_line)),
                ))
                .into(),
            );
        }

        // ---- Times, owner, permissions --------------------------------
        if single {
            groups.push(
                Group::titled("Times")
                    .row(property_row("Created", fact(&|facts| time(facts.props.created))))
                    .row(property_row("Modified", fact(&|facts| time(facts.props.modified))))
                    .row(property_row("Accessed", fact(&|facts| time(facts.props.accessed))))
                    .into(),
            );
            groups.push(
                Group::titled("Owner")
                    .row(property_row("Owner", fact(&|facts| facts.props.owner.clone())))
                    .row(property_row("Group", fact(&|facts| facts.props.group.clone())))
                    .into(),
            );

            let mode = edited().or_else(|| loaded.as_ref().map(|loaded| loaded.props.mode & 0o7777));
            let read_only = loaded
                .as_ref()
                .and_then(|loaded| loaded.props.volume.as_ref())
                .is_some_and(|volume| volume.read_only);
            let may_change = loaded.as_ref().is_some_and(|loaded| loaded.props.can_chmod());
            let editable = may_change && !read_only;
            let description = match (&loaded, editable) {
                (None, _) => "Reading permissions…",
                (Some(_), true) => "You own this, so you can change who may use it.",
                (Some(_), false) if read_only => "The drive is read-only.",
                (Some(_), false) => "Only the owner can change these.",
            };
            let path = first.path.clone();
            let on_change = EventHandler::new(move |new_mode: u32| {
                let path = path.clone();
                spawn(async move {
                    match spawn_blocking(move || ops::chmod(&path, new_mode)).await {
                        Ok(()) => {
                            edited.set(Some(new_mode));
                            mode_error.set(None);
                        }
                        Err(error) => mode_error.set(Some(error.to_string())),
                    }
                });
            });
            let mode_text = match (mode_error.read().clone(), mode) {
                (Some(error), _) => error,
                (None, Some(mode)) => mode_line(mode),
                (None, None) => PENDING.to_owned(),
            };
            let mut permissions = Group::titled("Permissions")
                .description(description)
                .content(PermissionGrid {
                    mode,
                    editable,
                    execute: if is_folder { "Access" } else { "Execute" },
                    on_change: on_change.clone(),
                })
                .row(row("Mode").mono_value(mode_text));
            if !is_folder {
                let toggle = EventHandler::new({
                    let on_change = on_change.clone();
                    move |on: bool| {
                        if let Some(mode) = mode {
                            on_change.call(with_execute(mode, on));
                        }
                    }
                });
                permissions = permissions.row(
                    switch_row(
                        "Allow executing as program",
                        mode.is_some_and(is_executable),
                        toggle,
                    )
                    .enabled(editable && mode.is_some()),
                );
            }
            groups.push(permissions.into());
        }

        // ---- The drive ------------------------------------------------
        let volume = loaded.as_ref().and_then(|loaded| loaded.props.volume.clone());
        groups.push(volume_group(&volume, loaded.is_some() || failed.is_some()).into());

        // ---- Open with ------------------------------------------------
        if single && !is_folder && self.open_with {
            let apps = apps_for(first.mime.mime);
            let mut open = Group::titled("Open with");
            if apps.is_empty() {
                open = open.row(property_row("Applications", "None found"));
            }
            for app in apps.into_iter().take(APP_ROWS) {
                let path = first.path.clone();
                let name = app.name.clone();
                open = open.row(
                    row(name.clone()).on_press(EventHandler::new(move |()| {
                        overlays.dismiss_dialog();
                        if let Err(error) = launch(&app, &path) {
                            overlays.toast(format!("Could not start {name}: {error}."));
                        }
                    })),
                );
            }
            groups.push(open.into());
        }

        let header = rect()
            .tw(
                &tokens,
                &[flex_row(), grow_children(), items(Align::Center), w_full(), h(Dim::Header), px(S4), gap(S3)],
            )
            .child(
                rect()
                    .tw(&tokens, &[w_flex()])
                    .child(SECTION.label(&tokens, "Properties").max_lines(1)),
            )
            .child(
                icon_button(Icon::WindowClose, move |_| overlays.dismiss_dialog())
                    .tooltip("Close"),
            );

        rect()
            .component("Properties")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .child(header)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(
                        ScrollView::new().child(
                            rect()
                                .tw(&tokens, &[flex_col(), w_full(), gap(S4), p(S4)])
                                .child(head)
                                .children(groups),
                        ),
                    ),
            )
    }
}

/// The icon of several items: their kind's when they are all one kind, else
/// a plain file.
fn shared_glyph(entries: &[Entry]) -> Icon {
    let kind = entries[0].category();
    if entries.iter().all(|entry| entry.category() == kind) {
        category_icon(kind)
    } else {
        Icon::TextXGeneric
    }
}

fn exif_camera(image: &ImageInfo) -> Option<String> {
    image.exif.camera.clone()
}

fn exif_lens(image: &ImageInfo) -> Option<String> {
    image.exif.lens.clone()
}

fn exif_taken(image: &ImageInfo) -> Option<String> {
    image.exif.taken.clone()
}

fn exif_exposure(image: &ImageInfo) -> Option<String> {
    image.exif.exposure.clone()
}

/// The drive the item is on, with how full it is. `known`: the load has
/// ended, so a missing drive is a dash and not a placeholder.
fn volume_group(volume: &Option<Volume>, known: bool) -> Group {
    let text = |of: &dyn Fn(&Volume) -> String| -> String {
        match (volume, known) {
            (Some(volume), _) => of(volume),
            (None, true) => NONE.to_owned(),
            (None, false) => PENDING.to_owned(),
        }
    };
    let fraction = volume.as_ref().and_then(Volume::used_fraction);
    let space = match volume {
        Some(Volume {
            total_bytes: Some(total),
            free_bytes: Some(free),
            ..
        }) => format!("{} free of {}", display::size(*free), display::size(*total)),
        // The line is always one line high, so the bar does not move.
        _ => " ".to_owned(),
    };
    Group::titled("Volume")
        .row(property_row(
            "Name",
            text(&|volume| format!("{} ({})", volume.label, volume.fs_type)),
        ))
        .row(row("Device").mono_value(text(&|volume| volume.device.clone())))
        .row(property_row(
            "Mounted at",
            text(&|volume| volume.mount_point.to_string_lossy().into_owned()),
        ))
        .content(VolumeSpace { fraction, space })
}

/// The free space bar and its line.
#[derive(PartialEq)]
struct VolumeSpace {
    fraction: Option<f32>,
    space: String,
}

impl Component for VolumeSpace {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S2)])
            .child(usage_bar(self.fraction))
            .child(CAPTION.in_ink(Ink::Muted).label(&tokens, self.space.clone()).max_lines(1))
    }
}

#[cfg(test)]
mod tests {
    use bingux_files::Kind;

    use super::*;

    fn entry(path: &str, kind: Kind) -> Entry {
        let path = PathBuf::from(path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        Entry::named(path, name, kind, false, false)
    }

    #[test]
    fn numbers_are_grouped_in_threes() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(2_500_000), "2,500,000");
        assert_eq!(grouped(123_456_789_012), "123,456,789,012");
    }

    #[test]
    fn a_size_has_its_byte_count_past_a_kilobyte() {
        assert_eq!(size_with_bytes(512), "512 bytes");
        assert_eq!(size_with_bytes(1), "1 byte");
        assert_eq!(size_with_bytes(2_500_000), "2.5 MB (2,500,000 bytes)");
    }

    #[test]
    fn a_running_count_says_so_and_a_stopped_one_says_at_least() {
        let running = Totals {
            bytes: 1_500_000,
            files: 3,
            folders: 1,
            ..Totals::default()
        };
        assert_eq!(total_size(running), "1.5 MB (1,500,000 bytes) so far…");
        assert_eq!(contents(running), "4 items (3 files, 1 folder) so far…");
        let done = Totals {
            done: true,
            ..running
        };
        assert_eq!(total_size(done), "1.5 MB (1,500,000 bytes)");
        assert_eq!(contents(done), "4 items (3 files, 1 folder)");
        let stopped = Totals {
            done: true,
            cancelled: true,
            ..running
        };
        assert_eq!(total_size(stopped), "At least 1.5 MB (1,500,000 bytes) (stopped)");
        assert_eq!(contents(stopped), "At least 4 items (3 files, 1 folder) (stopped)");
        let locked = Totals {
            done: true,
            unreadable: 1,
            ..running
        };
        assert_eq!(total_size(locked), "At least 1.5 MB (1,500,000 bytes)");
    }

    #[test]
    fn several_items_name_their_types_most_first() {
        let entries = [
            entry("/a/one.png", Kind::File),
            entry("/a/dir", Kind::Directory),
            entry("/a/two.png", Kind::File),
        ];
        assert_eq!(types_line(&entries), "PNG image (2), Folder (1)");
    }

    #[test]
    fn a_location_under_home_is_written_with_a_tilde() {
        let home = Path::new("/home/ana");
        assert_eq!(home_relative(Path::new("/home/ana/Pictures"), home), "~/Pictures");
        assert_eq!(home_relative(Path::new("/home/ana"), home), "~");
        assert_eq!(home_relative(Path::new("/srv/data"), home), "/srv/data");
        assert_eq!(home_relative(Path::new("/home/anabel"), home), "/home/anabel");
    }

    #[test]
    fn items_share_a_location_only_in_one_folder() {
        let together = [entry("/a/one", Kind::File), entry("/a/two", Kind::File)];
        assert_eq!(common_parent(&together), Some(PathBuf::from("/a")));
        let apart = [entry("/a/one", Kind::File), entry("/b/two", Kind::File)];
        assert_eq!(common_parent(&apart), None);
    }

    #[test]
    fn a_fact_load_reads_a_picture_header_and_the_drive() {
        let dir = std::env::temp_dir().join(format!("bingux-props-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pic.png");
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&64u32.to_be_bytes());
        png.extend_from_slice(&32u32.to_be_bytes());
        std::fs::write(&path, png).unwrap();
        let facts = load_facts(Subject {
            path: path.clone(),
            category: Category::Image,
            details: true,
        })
        .unwrap();
        assert_eq!(facts.image.unwrap().dimensions(), "64 × 32");
        assert!(facts.props.volume.is_some());
        // The first of several items is read for its drive only.
        let plain = load_facts(Subject {
            path,
            category: Category::Image,
            details: false,
        })
        .unwrap();
        assert_eq!(plain.image, None);
        assert!(
            load_facts(Subject {
                path: dir.join("gone.png"),
                category: Category::Image,
                details: true,
            })
            .is_err()
        );
    }
}
