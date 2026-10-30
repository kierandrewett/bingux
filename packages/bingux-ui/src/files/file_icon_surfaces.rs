//! The same path has the same icon on every surface, in the headless runner.
//!
//! Regression for a sidebar whose bookmarks (Documents, Downloads, Music...)
//! all had the plain folder while the list drew the right ones, because the
//! sidebar and the list chose their glyphs in different places. Each surface
//! is rendered for real (a sidebar row, a list row, a grid tile and a crumb
//! menu row) and the glyph of every `FileIcon` it drew is read back.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bingux_files::{Entry, Kind, Listing, Place, PlaceKind, Selection};
use freya::prelude::*;
use freya_components::cache::AssetCacher;

use super::address_crumbs::subfolder_icon;
use super::address_list::{Lead, ListRow, Part};
use super::file_icon::{FileIcon, file_icon_view};
use crate::component::Published;
use super::places_view::probe_row;
use super::view::FileView;
use crate::shell::OverlayHost;
use crate::shell::menu_query::Mark;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (700.0, 400.0);

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
}

/// The glyph name of every `FileIcon` on screen, in tree order.
fn glyphs(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| {
        let builder = &element.accessibility().builder;
        if builder.class_name() != Some("FileIcon") {
            return None;
        }
        let call = Published::decode(builder.author_id()?)?;
        call.props().find(|prop| prop.name == "glyph").map(|prop| prop.value.clone())
    })
}

fn render(app: impl Fn() -> Element + 'static) -> Vec<String> {
    let mut shot = Snapshot::new(app, SIZE);
    shot.settle(Duration::from_millis(400));
    glyphs(&mut shot)
}

#[derive(PartialEq)]
struct Scene {
    what: &'static str,
    name: &'static str,
    px: u16,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        let path = home().join(self.name);
        match self.what {
            "sidebar" => {
                let place = Place {
                    kind: PlaceKind::Bookmark,
                    name: self.name.to_owned(),
                    path: Some(path),
                };
                OverlayHost::new(probe_row(FileIcon::for_place(&place), false)).into_element()
            }
            "crumb" => OverlayHost::new(Element::from(ListRow {
                icon: Lead::File(subfolder_icon(&home(), self.name)),
                parts: vec![Part::plain(self.name)],
                current: false,
                highlighted: false,
                mark: Mark::Plain,
                on_press: EventHandler::new(|_: ()| {}),
                on_hover: EventHandler::new(|_: bool| {}),
            }))
            .into_element(),
            _ => {
                // A link, as the user's folders are: the glyph is still the
                // path's, and the link emblem is on top.
                let mut listing = Listing::new(home());
                listing.entries.push(Entry::named(
                    path,
                    self.name.to_owned(),
                    Kind::Directory,
                    true,
                    false,
                ));
                listing.complete = true;
                let px = self.px;
                let level = use_state(move || px);
                let selection = use_state(Selection::default);
                OverlayHost::new(
                    FileView::new(Arc::new(listing), Arc::new(vec![0]), selection)
                        .icon_px(level)
                        .thumbnails(false),
                )
                .into_element()
            }
        }
    }
}

fn surface(what: &'static str, name: &'static str, px: u16) -> Vec<String> {
    render(move || Scene { what, name, px }.into())
}

#[test]
fn a_user_folder_has_its_glyph_in_the_sidebar_the_list_the_grid_and_a_crumb_menu() {
    for (name, wanted) in [
        ("Downloads", "FolderDownload"),
        ("Documents", "FolderDocuments"),
        ("Music", "FolderMusic"),
        ("Pictures", "FolderPictures"),
        ("Videos", "FolderVideos"),
    ] {
        for (what, px) in [("sidebar", 0), ("crumb", 0), ("list", 16), ("grid", 64), ("grid", 128)] {
            let drawn = surface(what, name, px);
            assert!(
                drawn == [wanted],
                "{what} at {px} px draws {name} as {drawn:?}, not {wanted}"
            );
        }
    }
}

#[test]
fn a_bookmark_that_is_no_user_folder_is_the_plain_folder_everywhere() {
    for name in ["Projects", "Notes"] {
        for (what, px) in [("sidebar", 0), ("crumb", 0), ("list", 16), ("grid", 64)] {
            let drawn = surface(what, name, px);
            assert_eq!(
                drawn,
                ["Folder"],
                "{what} at {px} px draws {name} as {drawn:?}, not the plain folder"
            );
        }
    }
}

/// Whether the page has asked for icons and every one has landed: the rasteriser's worker is done
/// with them all.
fn icons_landed(shot: &mut Snapshot) -> bool {
    shot.runner().run_in(|| {
        AssetCacher::try_get().is_some_and(|cache| cache.cached_size() > 0 && cache.cached_size() == cache.size())
    })
}

/// A special folder as a listing row carries it, a link or not, drawn alone on
/// the page at `px`, as PNG bytes.
fn pixels(link: bool, name: &'static str, px: u16, selected: bool, scale: f64) -> Vec<u8> {
    let entry = Entry::named(
        home().join(name),
        name.to_owned(),
        Kind::Directory,
        link,
        false,
    );
    let edge = f32::from(px) + 8.0;
    let mut shot = Snapshot::with_scale(
        move || {
            let tokens = provide_tokens();
            let spec = FileIcon::for_entry(&entry);
            rect()
                .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg), p(S1)])
                .child(file_icon_view(&tokens, &spec, px, selected))
                .into()
        },
        (edge, edge),
        scale,
    );
    // Icons are rasterised on a worker, behind those of every other test of the run: a frame
    // taken before they land has a blank where the picture goes, and differs between two
    // identical pages. Wait until every icon the page asked for has landed.
    shot.settle(Duration::from_millis(50));
    shot.settle_until(Duration::from_secs(60), icons_landed);
    shot.settle(Duration::from_millis(50));
    // A file of this thread's own: a run of the same test in another process, or this test's own
    // next frame, never overwrites the one being read.
    let path = std::env::temp_dir().join(format!(
        "link-emblem-probe-{}-{:?}.png",
        std::process::id(),
        std::thread::current().id()
    ));
    shot.save(&path);
    let frame = crate::runtime::read_file(&path).expect("the frame was saved");
    let _ = std::fs::remove_file(&path);
    frame
}

#[test]
fn a_linked_user_folder_is_drawn_as_the_plain_one_below_64_px_and_has_the_emblem_at_64() {
    for name in ["Documents", "Downloads"] {
        for selected in [false, true] {
            for scale in [1.0, 2.0] {
                for px in [16u16, 18, 20, 24, 32] {
                    assert!(
                        pixels(true, name, px, selected, scale) == pixels(false, name, px, selected, scale),
                        "{name} at {px} px (selected {selected}, {scale}x) differs when linked"
                    );
                }
                for px in [64u16, 128] {
                    assert!(
                        pixels(true, name, px, selected, scale) != pixels(false, name, px, selected, scale),
                        "{name} at {px} px (selected {selected}, {scale}x) has no emblem when linked"
                    );
                }
            }
        }
    }
}
