//! The Basic tab of the Properties dialog: the picture, name and type, then
//! what it is and where it lives, the times, the drive it is on with a free
//! space bar, and the applications that can open it. Several items show
//! their combined count, size and types, where they are, and the drive.
//!
//! Every row exists from the first frame. A fact that has not arrived shows
//! "…" and one that does not exist shows "—", so nothing moves when the
//! answers come.

use bingux_files::ops::Volume;
use bingux_files::{Category, display};
use freya::prelude::*;
use freya::torin::prelude::Size2D;

use super::actions::{apps_for, launch};
use super::file_icon::{FileIcon, FileKind, file_icon_view};
use super::properties::Page;
use super::properties_words::{
    NONE, PENDING, common_parent, contents, home_relative, size_with_bytes, time, total_size, types_line,
};
use crate::controls::{path_label, usage_bar};
use crate::group::Group;
use crate::rows::{property_row, row};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink, TITLE};

/// How many applications "Open with" lists.
const APP_ROWS: usize = 6;

pub(super) fn basic(page: &Page) -> Vec<Element> {
    let mut groups: Vec<Element> = vec![head(page), general(page).into()];
    if page.single() {
        groups.push(
            Group::titled("Times")
                .row(property_row("Created", page.fact(|facts| time(facts.props.created))))
                .row(property_row("Modified", page.fact(|facts| time(facts.props.modified))))
                .row(property_row("Accessed", page.fact(|facts| time(facts.props.accessed))))
                .into(),
        );
    }
    let volume = page.loaded.and_then(|facts| facts.props.volume.clone());
    groups.push(volume_group(&volume, page.loaded.is_some() || page.failed.is_some()).into());
    if let Some(open) = open_with(page) {
        groups.push(open.into());
    }
    groups
}

/// The picture or icon, the name and the kind.
fn head(page: &Page) -> Element {
    let tokens = page.tokens;
    let first = page.first();
    let single = page.single();
    let head_icon = if single {
        FileIcon::for_entry(first)
    } else {
        shared_icon(page)
    };
    let glyph = |scale: f32| {
        let edge = (IconSize::Hero.px() * scale).round() as u16;
        rect()
            .tw(&tokens, &[w_full(), h_full(), center()])
            .child(file_icon_view(&tokens, &head_icon, edge, false))
            .into_element()
    };
    let picture: Element = {
        let poster = page.loaded.and_then(|facts| facts.poster.clone());
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
        format!("{} items", page.entries.len())
    };
    let subtitle = if single {
        first.mime.description.to_owned()
    } else {
        types_line(page.entries)
    };
    let status = page
        .failed
        .map(|message| format!("Could not read everything: {message}"))
        .unwrap_or_default();

    rect()
        .tw(&tokens, &[flex_col(), items(Align::Center), w_full(), gap(S2), py(S2)])
        .child(
            rect()
                .tw(&tokens, &[w(Dim::Box24), h(Dim::Box24), rounded(Radius::Control), clip()])
                .child(picture),
        )
        .child(
            // A long name loses its middle, not its extension; the full
            // name is the tooltip and Copy name.
            path_label(name)
                .name()
                .style(TITLE)
                .centre()
                .selectable(true)
                .copyable(true),
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
        )
        .into()
}

/// Type, size, contents, where it is and where a link leads.
fn general(page: &Page) -> Group {
    let tokens = page.tokens;
    let first = page.first();
    let single = page.single();
    let counted = page.counted();
    let mut general = Group::titled("General");
    general = general.row(property_row(
        "Type",
        if single {
            first.mime.description.to_owned()
        } else {
            types_line(page.entries)
        },
    ));
    if single {
        general = general.row(row("MIME type").mono_value(first.mime.mime));
    }
    let size_text = if counted {
        total_size(page.totals)
    } else if first.sized {
        size_with_bytes(first.size)
    } else {
        page.fact(|facts| Some(size_with_bytes(facts.props.size)))
    };
    general = general.row(
        row("Size").trailing(
            rect()
                .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
                .child(BODY.in_ink(Ink::Muted).selectable(&tokens, size_text))
                .maybe_child(page.stop_button()),
        ),
    );
    if counted {
        general = general.row(property_row("Contents", contents(page.totals)));
    }
    let location = common_parent(page.entries);
    let location_text = location.as_ref().map_or_else(
        || "Several folders".to_owned(),
        |folder| home_relative(folder, page.home),
    );
    let overlays = page.overlays;
    let open_location = location.clone().map(|folder| {
        let open_folder = page.open_folder.clone();
        EventHandler::new(move |()| {
            overlays.dismiss_dialog();
            open_folder.call(folder.clone());
        })
    });
    general = general.row(match open_location {
        Some(open) => row("Location").path_value(location_text).on_press(open),
        None => row("Location").path_value(location_text),
    });
    // An item of the Trash says where it was and when it was deleted, from its
    // info file.
    if single && let Some(trash) = first.trash.as_ref() {
        general = general.row(property_row(
            "Original location",
            trash
                .original
                .as_deref()
                .map_or_else(|| "Unknown".to_owned(), |original| home_relative(original, page.home)),
        ));
        general = general.row(property_row(
            "Deleted",
            super::properties_words::time(trash.deleted).unwrap_or_else(|| "Unknown".to_owned()),
        ));
    }
    if single && first.symlink {
        general = general.row(property_row(
            "Link target",
            page.fact(|facts| {
                facts
                    .props
                    .link_target
                    .as_ref()
                    .map(|target| target.to_string_lossy().into_owned())
            }),
        ));
    }
    general
}

/// The applications that can open a file. A folder, several items and a view
/// that cannot open with another application have none.
fn open_with(page: &Page) -> Option<Group> {
    let first = page.first();
    if !page.single() || page.is_folder() || !page.open_with {
        return None;
    }
    let overlays = page.overlays;
    let apps = apps_for(first.mime.mime);
    let mut open = Group::titled("Open with");
    if apps.is_empty() {
        open = open.row(property_row("Applications", "None found"));
    }
    for app in apps.into_iter().take(APP_ROWS) {
        let path = first.path.clone();
        let name = app.name.clone();
        open = open.row(row(name.clone()).leading_app(app.icon.clone()).on_press(EventHandler::new(move |()| {
            overlays.dismiss_dialog();
            if let Err(error) = launch(&app, &path) {
                overlays.toast(format!("Could not start {name}: {error}."));
            }
        })));
    }
    Some(open)
}

/// The icon of several items: their kind's when they are all one kind, else
/// a plain file.
fn shared_icon(page: &Page) -> FileIcon {
    let first = FileIcon::for_entry(&page.entries[0]);
    if page.entries.iter().all(|entry| FileIcon::for_entry(entry).kind == first.kind) {
        FileIcon::for_kind(first.kind)
    } else {
        FileIcon::for_kind(FileKind::Unknown)
    }
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
