//! The window: a title line over the picture or document on show, or the empty
//! state when nothing is open.
//!
//! The picture, the document, their zoom, turn, find, thumbnails and
//! information are `bingux_ui::preview` (`PreviewHost` in `Mode::Window`) and
//! `bingux_ui::zoom_view`; this file is what the Preview application adds: the
//! list of files the arrows reach, the title line and its menu, opening files
//! (the chooser, a drop, a second launch), full screen and the slideshow, Move
//! to Trash with Undo, Copy picture and Save a copy.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bingux_files::ops::{self, Trashed};
use bingux_ui::component::ComponentName;
use bingux_ui::files::{self, FileIcon, file_icon_view};
use bingux_ui::prelude::*;
use bingux_ui::preview::{Mode, PreviewHost, PreviewNav, PreviewSource, Step, ZoomMemory};
use bingux_ui::shell::MenuItem;
use bingux_ui::zoom_view::{FitMode, ZoomHandle, use_zoom_handle};
use freya::prelude::*;
use freya::winit::window::Fullscreen;

use crate::copy;
use crate::crop_mode::{CropEditor, CropJob};
use crate::edit::{self, Orientation};
use crate::playlist::{Family, Playlist, family_of};
use crate::prefs;

/// How long a slideshow shows each picture.
const SLIDE: Duration = Duration::from_secs(4);
/// The pointer this close to the top edge brings the title line back in full screen.
const EDGE: f32 = 6.0;

/// The extensions the chooser offers first.
const OPENABLE: &str = "png apng jpg jpeg jpe jfif gif webp bmp ico cur tif tiff heic heif hif avif hdr exr svg svgz cr2 cr3 crw nef nrw arw dng orf rw2 raf pef srw pdf";

/// Every path named on the command line, absolute, that exists.
fn launch_paths() -> Vec<PathBuf> {
    // `--snapshot DIR FILE` is the screenshot mode: its arguments are not files to open.
    if std::env::args_os().any(|argument| argument == "--snapshot") {
        return Vec::new();
    }
    std::env::args_os()
        .skip(1)
        .filter(|argument| !argument.to_string_lossy().starts_with('-'))
        .filter_map(|argument| crate::launch::path_of(&argument))
        .filter(|path| path.exists())
        .collect()
}

/// The window root. The content is below the overlay host, which provides the
/// window handle and the menus.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content))
    }
}

/// The last thing Undo can take back.
#[derive(Clone)]
enum Done {
    /// A file went to the Trash, with what Undo needs to bring it back.
    Binned {
        trashed: Vec<Trashed>,
        file: PathBuf,
        at: usize,
    },
    /// A cropped copy was saved; Undo moves it to the Trash.
    Copied { copy: PathBuf },
}

#[derive(PartialEq)]
struct Content;

fn is_picture(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| family_of(&name.to_string_lossy()) == Family::Picture)
}

fn is_document(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| family_of(&name.to_string_lossy()) == Family::Document)
}

fn is_gif(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
}

fn set_full_screen(on: bool) {
    Platform::get().with_window(Platform::window_id(), move |window| {
        window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
    });
}

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let maximized = freya::borderless::use_maximized();
        let location = use_location();
        use_hook(files::warm_open_with);

        let mut playlist = use_state({
            let location = location.clone();
            move || {
                let launched = launch_paths();
                if launched.len() > 1 {
                    Playlist::of_launch(&launched)
                } else if location.segments().is_empty() {
                    Playlist::empty()
                } else {
                    Playlist::of_launch(&[location.os_path()])
                }
            }
        });
        let info = use_state(|| false);
        let sidebar = use_state(|| false);
        let playing = use_state(|| true);
        let zoom = use_zoom_handle();
        let mut full_screen = use_state(|| false);
        let mut slideshow = use_state(|| false);
        let mut chrome_near = use_state(|| false);
        let mut over_file = use_state(|| false);
        let done = use_state(|| None::<Done>);
        let cropping = use_state(|| None::<CropJob>);

        // A second launch changes the location: its file opens here.
        let handled = use_hook({
            let first = location.clone();
            move || std::rc::Rc::new(std::cell::RefCell::new(first))
        });
        if *handled.borrow() != location {
            *handled.borrow_mut() = location.clone();
            if !location.segments().is_empty() {
                let path = location.os_path();
                spawn(async move {
                    playlist.set(Playlist::of_launch(&[path]));
                });
            }
        }

        // The slideshow turns the page on a timer.
        use_hook(move || {
            spawn(async move {
                loop {
                    timer(SLIDE).await;
                    if *slideshow.peek() {
                        playlist.write().advance_wrapping();
                    }
                }
            })
        });

        let list = playlist.read().clone();
        let current: Option<PathBuf> = list.current().map(Path::to_path_buf);
        let name = current
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());
        use_page_title(name.as_deref().unwrap_or("Preview"));
        let picture = current.as_deref().is_some_and(is_picture);
        let document = current.as_deref().is_some_and(is_document);
        let gif = current.as_deref().is_some_and(is_gif);
        let compact = is_compact();
        let full = *full_screen.read();
        let show_chrome = !full || *chrome_near.read();

        // Actions: each is a plain closure over Copy state.
        let open = move |_: ()| {
            let start = playlist
                .peek()
                .current()
                .and_then(Path::parent)
                .map(Path::to_path_buf);
            let mut request = ChooserRequest::open()
                .title(copy::OPEN_REQUEST_TITLE)
                .multiple(true)
                .filter_spec(copy::OPEN_FILTER, OPENABLE)
                .presets(true);
            if let Some(start) = start {
                request = request.current_folder(start);
            }
            files::choose(window, request, move |chosen| {
                if let Some(chosen) = chosen {
                    let mut playlist = playlist;
                    playlist.set(Playlist::of_launch(&chosen.paths));
                }
            });
        };
        let mut leave_full_screen = move || {
            slideshow.set(false);
            full_screen.set(false);
            set_full_screen(false);
        };
        let mut toggle_full_screen = move || {
            let on = !*full_screen.peek();
            full_screen.set(on);
            if !on {
                slideshow.set(false);
            }
            set_full_screen(on);
        };
        let mut toggle_slideshow = move || {
            if *slideshow.peek() {
                leave_full_screen();
            } else if playlist.peek().is_pictures() {
                slideshow.set(true);
                if !*full_screen.peek() {
                    full_screen.set(true);
                    set_full_screen(true);
                }
            }
        };
        // Undo takes back the last file moved to the Trash or the last cropped copy.
        let undo = move || {
            let mut done = done;
            let mut playlist = playlist;
            let Some(last) = done.peek().clone() else {
                return;
            };
            match last {
                Done::Binned { trashed, file, at } => match ops::untrash(&trashed) {
                    Ok(_) => {
                        playlist.write().put_back(file, at);
                        done.set(None);
                    }
                    Err(_) => overlays.toast(copy::UNDO_FAILED),
                },
                Done::Copied { copy: made } => match ops::trash(std::slice::from_ref(&made)) {
                    Ok(_) => {
                        playlist.write().take_out(&made);
                        done.set(None);
                    }
                    Err(_) => overlays.toast(copy::UNDO_COPY_FAILED),
                },
            }
        };
        let trash = {
            let current = current.clone();
            move || {
                let Some(file) = current.clone() else { return };
                let label = file
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match ops::trash(std::slice::from_ref(&file)) {
                    Ok(trashed) => {
                        let mut playlist = playlist;
                        let mut done = done;
                        let at = playlist.write().remove_current().map_or(0, |(_, at)| at);
                        done.set(Some(Done::Binned { trashed, file, at }));
                        overlays.toast_with_action(
                            copy::moved_to_trash(&label),
                            copy::UNDO,
                            move |_: ()| undo(),
                        );
                    }
                    Err(_) => overlays.toast(copy::trash_failed(&label)),
                }
            }
        };
        let orientation_of = move |zoom: ZoomHandle| {
            (
                Orientation::new(zoom.turns(), zoom.mirrored()),
                Orientation::new(zoom.home().0, zoom.home().1),
            )
        };
        let copy_picture = {
            let current = current.clone();
            move || {
                let Some(file) = current.clone() else { return };
                if !edit::can_copy(&file) {
                    return;
                }
                let (total, home) = orientation_of(zoom);
                spawn(async move {
                    let result = bingux_ui::task::spawn_blocking(move || {
                        edit::png_bytes(&file, total, home, None)
                            .and_then(|png| edit::copy_to_clipboard(&png))
                    })
                    .await;
                    match result {
                        Ok(()) => overlays.toast(copy::COPIED),
                        Err(_) => overlays.toast(copy::COPY_FAILED),
                    }
                });
            }
        };
        let save_copy = {
            let current = current.clone();
            move || {
                let Some(file) = current.clone() else { return };
                if !edit::can_copy(&file) {
                    return;
                }
                let (total, home) = orientation_of(zoom);
                let suggestion = edit::copy_name(&file);
                let name = suggestion
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut request = ChooserRequest::save(name).title(copy::SAVE_REQUEST_TITLE);
                if let Some(folder) = file.parent() {
                    request = request.current_folder(folder.to_path_buf());
                }
                files::choose(window, request, move |chosen| {
                    let Some(target) = chosen.and_then(|chosen| chosen.paths.into_iter().next())
                    else {
                        return;
                    };
                    let file = file.clone();
                    spawn(async move {
                        let shown = target
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let result = bingux_ui::task::spawn_blocking(move || {
                            edit::save_copy(&file, &target, total, home, None)
                        })
                        .await;
                        match result {
                            Ok(()) => overlays.toast(copy::saved_copy(&shown)),
                            Err(_) => overlays.toast(copy::SAVE_FAILED),
                        }
                    });
                });
            }
        };
        let begin_crop = {
            let current = current.clone();
            move || {
                let Some(file) = current.clone() else { return };
                if !edit::can_copy(&file) || *slideshow.peek() {
                    return;
                }
                match edit::upright_size(&file) {
                    Some((width, height)) => {
                        let mut cropping = cropping;
                        cropping.set(Some(CropJob {
                            file,
                            size: (width as f32, height as f32),
                            orientation: (zoom.turns(), zoom.mirrored()),
                        }));
                    }
                    None => overlays.toast(copy::CROP_FAILED),
                }
            }
        };
        let cancel_crop = move || {
            let mut cropping = cropping;
            cropping.set(None);
        };
        let save_crop = move |crop: edit::Crop| {
            let Some(job) = cropping.peek().clone() else {
                return;
            };
            let target = edit::copy_name(&job.file);
            let shown = target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            // The picture began upright, so how it was turned is what is done to the decode.
            let total = Orientation::new(job.orientation.0, job.orientation.1);
            spawn(async move {
                let file = job.file.clone();
                let made = target.clone();
                let result = bingux_ui::task::spawn_blocking(move || {
                    edit::save_copy(&file, &made, total, Orientation::UPRIGHT, Some(crop))
                })
                .await;
                let mut cropping = cropping;
                let mut playlist = playlist;
                let mut done = done;
                match result {
                    Ok(()) => {
                        cropping.set(None);
                        playlist.write().put_after_current(target.clone());
                        done.set(Some(Done::Copied { copy: target }));
                        overlays.toast_with_action(
                            copy::saved_crop(&shown),
                            copy::UNDO,
                            move |_: ()| undo(),
                        );
                    }
                    Err(_) => overlays.toast(copy::SAVE_FAILED),
                }
            });
        };
        let show_in_files = {
            let current = current.clone();
            move || {
                if let Some(file) = &current
                    && files::show_in_files(file).is_err()
                {
                    overlays.toast(copy::show_in_files_failed());
                }
            }
        };

        // The keys the window itself owns; the viewer owns the rest.
        let on_key = {
            let copy_picture = copy_picture.clone();
            let save_copy = save_copy.clone();
            let begin_crop = begin_crop.clone();
            let trash = trash.clone();
            move |event: Event<KeyboardEventData>| {
                // While cropping the box has the keys; the window keeps only Escape.
                if cropping.peek().is_some() {
                    if matches!(event.key, Key::Named(NamedKey::Escape)) {
                        cancel_crop();
                        event.stop_propagation();
                    }
                    return;
                }
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let shift = event.modifiers.contains(Modifiers::SHIFT);
                let alt = event.modifiers.contains(Modifiers::ALT);
                let handled = match &event.key {
                    Key::Character(text) if ctrl && !alt => {
                        match (text.to_ascii_lowercase().as_str(), shift) {
                            ("o", false) => {
                                open(());
                                true
                            }
                            ("c", false) if picture => {
                                copy_picture();
                                true
                            }
                            ("s", true) if picture => {
                                save_copy();
                                true
                            }
                            ("z", false) => {
                                undo();
                                true
                            }
                            _ => false,
                        }
                    }
                    // Pictures only: a document has a find field that takes these keys.
                    Key::Named(NamedKey::Delete) if picture && !ctrl && !alt => {
                        trash();
                        true
                    }
                    Key::Character(text)
                        if picture && !ctrl && !alt && !shift && text.as_str() == "c" =>
                    {
                        begin_crop();
                        true
                    }
                    Key::Character(text)
                        if picture && !ctrl && !alt && !shift && text.as_str() == "s" =>
                    {
                        toggle_slideshow();
                        true
                    }
                    Key::Character(text) if gif && !ctrl && !alt && text.as_str() == " " => {
                        let mut playing = playing;
                        let now = *playing.peek();
                        playing.set(!now);
                        true
                    }
                    _ => false,
                };
                if handled {
                    event.stop_propagation();
                }
            }
        };

        // The title line.
        let title_slot: Element = {
            let icon = name.as_deref().map(FileIcon::for_name);
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        gap(S3),
                        w_full(),
                    ],
                )
                .maybe_child(icon.map(|icon| file_icon_view(&tokens, &icon, IconSize::Row, false)))
                .child(
                    rect().tw(&tokens, &[w_flex()]).child(
                        path_label(name.clone().unwrap_or_else(|| "Preview".to_owned()))
                            .name()
                            .style(typography::SECTION),
                    ),
                )
                .maybe_child(list.position().map(|(at, of)| {
                    typography::CAPTION
                        .in_ink(Ink::Muted)
                        .label(&tokens, copy::position(at, of))
                        .max_lines(1)
                }))
                .into()
        };
        let mut header = HeaderBar::new("")
            .title_start(true)
            .title_slot(title_slot)
            .leading(
                icon_button(Icon::DocumentOpen, move |_| open(())).tooltip(copy::OPEN_TOOLTIP),
            );
        if document && !compact {
            let mut sidebar_state = sidebar;
            header = header.leading(
                icon_button(Icon::SidebarShow, move |_| {
                    let shown = *sidebar_state.peek();
                    sidebar_state.set(!shown);
                })
                .active(*sidebar.read())
                .tooltip(copy::PAGES_TOOLTIP),
            );
        }
        let cropping_now = cropping.read().is_some();
        if picture && !compact && !cropping_now {
            if current.as_deref().is_some_and(edit::can_copy) {
                let begin_crop = begin_crop.clone();
                header = header.trailing(
                    icon_button(Icon::Crop, move |_| begin_crop()).tooltip(copy::CROP_TOOLTIP),
                );
            }
            header = header
                .trailing(
                    icon_button(Icon::ObjectRotateLeft, move |_| zoom.rotate(false))
                        .tooltip(copy::TURN_LEFT_TOOLTIP),
                )
                .trailing(
                    icon_button(Icon::ObjectRotateRight, move |_| zoom.rotate(true))
                        .tooltip(copy::TURN_RIGHT_TOOLTIP),
                );
        }
        if current.is_some() && !compact && !cropping_now {
            let mut info_state = info;
            header = header.trailing(
                icon_button(Icon::DialogInformation, move |_| {
                    let shown = *info_state.peek();
                    info_state.set(!shown);
                })
                .active(*info.read())
                .tooltip(copy::INFO_TOOLTIP),
            );
        }
        let menu = main_menu(
            MenuState {
                current: current.clone(),
                picture,
                copyable: current.as_deref().is_some_and(edit::can_copy) && picture,
                slideshow: *slideshow.read(),
                can_slideshow: list.is_pictures(),
                full,
                can_undo: done.read().is_some(),
                can_crop: picture && current.as_deref().is_some_and(edit::can_copy) && !*slideshow.read(),
            },
            MenuActions {
                open: EventHandler::new(open),
                copy_picture: EventHandler::new({
                    let copy_picture = copy_picture.clone();
                    move |_: ()| copy_picture()
                }),
                save_copy: EventHandler::new({
                    let save_copy = save_copy.clone();
                    move |_: ()| save_copy()
                }),
                crop: EventHandler::new({
                    let begin_crop = begin_crop.clone();
                    move |_: ()| begin_crop()
                }),
                trash: EventHandler::new({
                    let trash = trash.clone();
                    move |_: ()| trash()
                }),
                undo: EventHandler::new(move |_: ()| undo()),
                slideshow: EventHandler::new(move |_: ()| toggle_slideshow()),
                full_screen: EventHandler::new(move |_: ()| toggle_full_screen()),
                show_in_files: EventHandler::new(move |_: ()| show_in_files()),
            },
            zoom,
            overlays,
        );
        header = header
            .trailing(menu_button(copy::MAIN_MENU, menu).icon(Icon::OpenMenu))
            .trailing(WindowControls);

        // What the arrows and the viewer's own buttons do.
        let on_close = EventHandler::new(move |_: ()| {
            if *full_screen.peek() || *slideshow.peek() {
                leave_full_screen();
            }
        });
        let mut nav = PreviewNav::new(on_close)
            .zoom(zoom)
            .zoom_memory(ZoomMemory::new(
                prefs::zoom_for,
                EventHandler::new(|(mime, mode): (String, FitMode)| {
                    prefs::remember_zoom(&mime, mode)
                }),
            ))
            .info(info)
            .sidebar(sidebar)
            .playing(playing)
            .on_step(move |step: Step| {
                let mut playlist = playlist;
                playlist.write().step(step);
            })
            .on_mode(move |mode: Mode| {
                if mode == Mode::Fullscreen {
                    toggle_full_screen();
                }
            });
        if let Some((at, of)) = list.position() {
            nav = nav.position(at, of);
        }

        // A crop belongs to the file it began on: another file, or none, ends it.
        let cropped = cropping.read().clone();
        if let Some(job) = &cropped
            && current.as_deref() != Some(job.file.as_path())
        {
            spawn(async move {
                let mut cropping = cropping;
                cropping.set(None);
            });
        }
        let crop_on_show = cropped.filter(|job| current.as_deref() == Some(job.file.as_path()));

        let body: Element = match (&current, crop_on_show) {
            (Some(_), Some(job)) => CropEditor {
                job,
                on_save: EventHandler::new(save_crop),
                on_cancel: EventHandler::new(move |_: ()| cancel_crop()),
            }
            .into_element(),
            (Some(path), None) => {
                PreviewHost::new(Some(PreviewSource::path(path.clone())), Mode::Window, nav)
                    .folder(false)
                    .into_element()
            }
            (None, _) => status_page(Icon::ImageGeneric, copy::EMPTY_TITLE)
                .description(copy::EMPTY_LINE)
                .action(copy::EMPTY_ACTION, EventHandler::new(open))
                .into_element(),
        };

        let drop_overlay = (*over_file.read()).then(|| {
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .tw(
                    &tokens,
                    &[
                        w_full(),
                        h_full(),
                        flex_col(),
                        center(),
                        bg(Paint::AccentSoft),
                    ],
                )
                .child(
                    typography::HEADING
                        .in_ink(Ink::Text)
                        .label(&tokens, copy::DROP_TITLE.to_owned()),
                )
        });

        rect()
            .component("PreviewWindow")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .on_global_key_down(on_key)
            .on_global_file_hover(move |_| over_file.set_if_modified(true))
            .on_global_file_hover_cancelled(move |_| over_file.set_if_modified(false))
            .on_file_drop(move |event: Event<FileEventData>| {
                over_file.set_if_modified(false);
                let paths = event.file_paths.clone();
                if !paths.is_empty() {
                    playlist.set(Playlist::of_launch(&paths));
                }
            })
            .on_sized(move |event: Event<SizedEventData>| {
                if !is_compact() && !*maximized.peek() && !*full_screen.peek() {
                    prefs::remember_window_size(event.area.width(), event.area.height());
                }
            })
            // In full screen the title line is out of the way until the pointer is at the top.
            .on_pointer_move(move |event: Event<PointerEventData>| {
                if *full_screen.peek() {
                    let near = event.element_location().y < f64::from(EDGE) * 6.0;
                    chrome_near.set_if_modified(near);
                }
            })
            .maybe_child(show_chrome.then(|| header.into_element()))
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                    .child(body)
                    .maybe_child(drop_overlay),
            )
    }
}

/// What the menu needs to know to say what it can do.
struct MenuState {
    current: Option<PathBuf>,
    picture: bool,
    copyable: bool,
    slideshow: bool,
    can_slideshow: bool,
    full: bool,
    can_undo: bool,
    can_crop: bool,
}

/// What its items do.
struct MenuActions {
    open: EventHandler<()>,
    copy_picture: EventHandler<()>,
    save_copy: EventHandler<()>,
    crop: EventHandler<()>,
    trash: EventHandler<()>,
    undo: EventHandler<()>,
    slideshow: EventHandler<()>,
    full_screen: EventHandler<()>,
    show_in_files: EventHandler<()>,
}

fn item(label: &str, shortcut: Option<&str>, on_press: &EventHandler<()>) -> MenuItem {
    let on_press = on_press.clone();
    let item = menu_item(label, move |_| on_press.call(()));
    match shortcut {
        Some(shortcut) => item.shortcut(shortcut),
        None => item,
    }
}

fn main_menu(
    state: MenuState,
    actions: MenuActions,
    zoom: ZoomHandle,
    overlays: Overlays,
) -> Vec<MenuItem> {
    let mut items = vec![item(copy::MENU_OPEN, Some("Ctrl+O"), &actions.open)];
    if let Some(path) = &state.current {
        let mime = bingux_files::mime::for_name(
            &path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
        .mime;
        let apps = files::open_with_items(path, mime, move |app: String| {
            overlays.toast(copy::app_failed(&app))
        });
        let mut with = submenu(copy::MENU_OPEN_WITH, apps.clone());
        if apps.is_empty() {
            with = with.disabled();
        }
        items.push(with);
        items.push(item(copy::MENU_SHOW_IN_FILES, None, &actions.show_in_files));
        items.push(menu_separator());
    }
    if state.picture {
        let mut copy_item = item(
            copy::MENU_COPY_PICTURE,
            Some("Ctrl+C"),
            &actions.copy_picture,
        );
        let mut save_item = item(
            copy::MENU_SAVE_COPY,
            Some("Ctrl+Shift+S"),
            &actions.save_copy,
        );
        if !state.copyable {
            copy_item = copy_item.disabled();
            save_item = save_item.disabled();
        }
        items.push(copy_item);
        items.push(save_item);
        let mut crop_item = item(copy::MENU_CROP, Some("C"), &actions.crop);
        if !state.can_crop {
            crop_item = crop_item.disabled();
        }
        items.push(crop_item);
        items.push(menu_separator());
        items
            .push(menu_item(copy::MENU_TURN_LEFT, move |_| zoom.rotate(false)).shortcut("Shift+R"));
        items.push(menu_item(copy::MENU_TURN_RIGHT, move |_| zoom.rotate(true)).shortcut("R"));
        items.push(
            menu_item(copy::MENU_FLIP_HORIZONTALLY, move |_| {
                zoom.flip_horizontal()
            })
            .shortcut("H"),
        );
        items.push(
            menu_item(copy::MENU_FLIP_VERTICALLY, move |_| zoom.flip_vertical()).shortcut("V"),
        );
        items.push(menu_separator());
        items.push(menu_item(copy::MENU_FIT, move |_| zoom.fit()).shortcut("0"));
        items.push(menu_item(copy::MENU_ACTUAL_SIZE, move |_| zoom.actual_size()).shortcut("1"));
        if state.can_slideshow {
            items.push(item(
                if state.slideshow {
                    copy::MENU_STOP_SLIDESHOW
                } else {
                    copy::MENU_SLIDESHOW
                },
                Some("S"),
                &actions.slideshow,
            ));
        }
        items.push(menu_separator());
    }
    if state.current.is_some() {
        items.push(item(
            if state.full {
                copy::MENU_LEAVE_FULL_SCREEN
            } else {
                copy::MENU_FULL_SCREEN
            },
            Some("F11"),
            &actions.full_screen,
        ));
        let mut trash = item(
            copy::MENU_TRASH,
            if state.picture { Some("Delete") } else { None },
            &actions.trash,
        )
        .destructive();
        if state.current.is_none() {
            trash = trash.disabled();
        }
        items.push(trash);
    }
    if state.can_undo {
        items.push(item(copy::MENU_UNDO, Some("Ctrl+Z"), &actions.undo));
    }
    items.push(menu_separator());
    items.push(menu_item(copy::MENU_SHORTCUTS, move |_| {
        overlays.show_dialog(dialog(copy::MENU_SHORTCUTS, copy::SHORTCUTS).cancel("Close"));
    }));
    // Every application's menu ends with About.
    items.extend(bingux_ui::about::app_menu_items(overlays));
    items
}
