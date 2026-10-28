//! The Properties dialog: what a file, a folder or a selection is, in tabs.
//!
//! | Tab | Holds |
//! | --- | ----- |
//! | Basic | picture or icon, name and type; type, size with the byte count, location, where a link leads; times; the drive with a free space bar; the applications that can open it |
//! | Permissions | the read, write and execute grid, editable by the owner, with the numeric mode, "allow executing as program", the special bits, ownership, and for a folder an explicit, confirmed "apply to enclosed items"; the SELinux context and ACL where the system keeps them |
//! | Details | a picture's size and camera, a recording's length and codecs; the file's SHA-256 on demand; inode, device, links, block size and disk use; extended attributes |
//! | Contents | one folder's item count, size (with Stop) and largest items |
//!
//! Several items show Basic, with the combined count, size and types, and
//! Permissions, which act on all of them; where they differ a box is mixed.
//! One file has the first three tabs, one folder all four.
//!
//! The tabs are a segmented control with the tab variant under the header,
//! as the inspector's are. Left, Right, Home and End move between them;
//! Ctrl+1 to Ctrl+4 pick one and Ctrl+Tab and Ctrl+Shift+Tab step through
//! them. The last tab picked is the one the next dialog opens on. On a phone
//! the bar scrolls sideways.
//!
//! # Nothing moves
//!
//! The card is one fixed size for every tab: the header and the tab bar stay,
//! and the content scrolls inside. Each tab's rows exist from the first frame.
//! A fact that has not arrived shows "…" and one that does not exist shows
//! "—". What can only be listed once read (extended attributes, the biggest
//! items of a folder, the security context) is the last group of its tab. The
//! state of the tabs that are not shown (a checksum being worked out, a count,
//! the modes just set, the scroll position) is kept while the dialog is open.
//!
//! # Nothing blocks the window
//!
//! The facts of one `stat` and the file's header are read on a pool worker;
//! the modes of every item on another. A folder or selection is added up by a
//! counting thread that reports as it goes, so the figure grows on screen, can
//! be stopped with a button, and stops when the dialog closes. So do the
//! checksum and "apply to enclosed items".
//!
//! The dialog is a [`framed_dialog`]: Escape and the scrim close it, and so
//! does the close button. Its focus scope is the one every modal layer has.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use bingux_files::media::{self, AvInfo, ImageInfo};
use bingux_files::ops::{self, ChildSize, Counting, FileDetails, HashProgress, Hashing, Permissions, Properties, Totals};
use bingux_files::thumbs::{self, Bucket};
use bingux_files::{Cancel, Category, Entry, Kind};
use freya::prelude::*;

use super::actions::Env;
use super::properties_access::{Access, TreePhase, permissions_tab};
use super::properties_basic::basic;
use super::properties_contents::{LARGEST, contents_tab};
use super::properties_details::{HashView, details_tab};
use super::properties_permissions::ModeSet;
use super::properties_tabs::{self as tab_model, Tab};
use super::properties_words::{NONE, PENDING};
use crate::component::ComponentName;
use crate::controls::{action_button, icon_button, segmented, text_width};
use crate::icons::Icon;
use crate::shell::dialog::frame_width;
use crate::shell::{HeaderBar, Overlays, framed_dialog};
use crate::prelude::Tokens;
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::{spawn_blocking, use_resource};
use crate::typography::BODY;

/// How often a running total is read.
const TICK: Duration = Duration::from_millis(150);

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
    let focus = env.focus;
    show(
        Body {
            entries,
            overlays: env.overlays,
            open_folder: env.open_folder.clone(),
            open_with: env.caps.open_with,
            home: env.home.clone(),
        },
        // Opened from the keyboard: give it back.
        EventHandler::new(move |()| focus.request_focus()),
    );
}

/// Put the dialog for `body` on screen, calling `closed` when it has gone.
fn show(body: Body, closed: EventHandler<()>) {
    let title = match body.entries.as_slice() {
        [only] => only.name.clone(),
        many => format!("{} items", many.len()),
    };
    let overlays = body.overlays;
    overlays.show_dialog(
        framed_dialog(
            format!("Properties of {title}"),
            Element::from(body),
            Dim::PopupWidth.px(),
            Dim::PopupHeight.px() + 120.0,
        )
        .on_closed(closed),
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
pub(super) struct Facts {
    pub props: Properties,
    pub image: Option<ImageInfo>,
    pub av: Option<AvInfo>,
    /// The desktop's thumbnail of a document or recording.
    pub poster: Option<PathBuf>,
    /// Inode, device, links, disk use, security context, extended attributes.
    pub details: Option<FileDetails>,
}

fn load_facts(subject: Subject) -> Result<Facts, String> {
    let props = ops::stat_properties(&subject.path).map_err(|error| error.to_string())?;
    let (mut image, mut av, mut poster, mut details) = (None, None, None, None);
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
        details = ops::file_details(&subject.path).ok();
    }
    Ok(Facts {
        props,
        image,
        av,
        poster,
        details,
    })
}

fn find_poster(props: &Properties) -> Option<PathBuf> {
    [Bucket::Large, Bucket::XLarge, Bucket::XXLarge, Bucket::Normal]
        .into_iter()
        .find_map(|bucket| thumbs::lookup(&props.path, props.modified, bucket))
}

/// The owner, group and mode of every item, in order.
fn load_permissions(paths: Vec<PathBuf>) -> Result<Vec<Permissions>, String> {
    paths
        .iter()
        .map(|path| ops::permissions(path).map_err(|error| error.to_string()))
        .collect()
}

/// Set each item's mode to the new one where it differs from the old one.
/// Returns the mode every item has now, and the first failure. An item that
/// could not be changed keeps its old mode in the answer.
fn apply_modes(work: Vec<(PathBuf, u32, u32)>) -> (Vec<u32>, Option<String>) {
    let mut error = None;
    let modes = work
        .into_iter()
        .map(|(path, new, old)| {
            if new == old {
                return old;
            }
            match ops::chmod(&path, new) {
                Ok(()) => new,
                Err(failure) => {
                    error.get_or_insert_with(|| failure.to_string());
                    old
                }
            }
        })
        .collect();
    (modes, error)
}

// ---------------------------------------------------------------------------
// The dialog's body
// ---------------------------------------------------------------------------

/// Count `roots` in the background and return the running total, the biggest
/// items so far and the handle that stops it. A single folder is counted with
/// its breakdown. The count stops when the dialog goes. With no roots nothing
/// is counted.
fn use_counting(
    roots: Option<Vec<PathBuf>>,
    breakdown: bool,
) -> (State<Totals>, State<Vec<ChildSize>>, Option<Rc<Counting>>) {
    let mut totals = use_state(Totals::default);
    let mut largest = use_state(Vec::<ChildSize>::new);
    let counting = use_hook(move || {
        roots.map(|roots| {
            let counting = Rc::new(match roots.as_slice() {
                [only] if breakdown => Counting::start_breakdown(only.clone()),
                _ => Counting::start(roots),
            });
            let reader = counting.clone();
            spawn(async move {
                loop {
                    let now = reader.totals();
                    totals.set_if_modified(now);
                    if breakdown {
                        largest.set_if_modified(reader.largest(LARGEST));
                    }
                    if now.done {
                        break;
                    }
                    timer(TICK).await;
                }
            });
            counting
        })
    });
    (totals, largest, counting)
}

/// Everything a tab reads, built once per render of the dialog.
pub(super) struct Page<'a> {
    pub tokens: Tokens,
    pub entries: &'a [Entry],
    pub overlays: Overlays,
    pub open_folder: &'a EventHandler<PathBuf>,
    /// The view may open files with another application.
    pub open_with: bool,
    /// The home folder, which a location under it is written relative to.
    pub home: &'a Path,
    pub loaded: Option<&'a Facts>,
    pub failed: Option<&'a str>,
    pub totals: Totals,
    pub counting: Option<Rc<Counting>>,
    pub largest: Vec<ChildSize>,
}

impl Page<'_> {
    pub(super) fn first(&self) -> &Entry {
        &self.entries[0]
    }

    pub(super) fn single(&self) -> bool {
        self.entries.len() == 1
    }

    pub(super) fn is_folder(&self) -> bool {
        self.single() && self.first().is_dir()
    }

    /// The size and count come from a running count: a folder, or several
    /// items.
    pub(super) fn counted(&self) -> bool {
        !self.single() || self.is_folder()
    }

    /// A fact from the load: the placeholder before it arrives, a dash if
    /// there is none, or what `of` makes of it.
    pub(super) fn fact(&self, of: impl Fn(&Facts) -> Option<String>) -> String {
        match (self.loaded, self.failed) {
            (Some(loaded), _) => of(loaded).unwrap_or_else(|| NONE.to_owned()),
            (None, Some(_)) => NONE.to_owned(),
            (None, None) => PENDING.to_owned(),
        }
    }

    /// Stop, while a count is running.
    pub(super) fn stop_button(&self) -> Option<Element> {
        let counting = self.counting.clone().filter(|_| self.counted() && !self.totals.done)?;
        Some(
            action_button("Stop", move |_| counting.cancel())
                .compact()
                .into_element(),
        )
    }
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
        let paths: Vec<PathBuf> = entries.iter().map(|entry| entry.path.clone()).collect();

        // ---- what is read -------------------------------------------------
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
        let permissions = use_resource(paths.clone(), load_permissions);
        let perms = permissions.with(|read| read.value().cloned());
        let perms_failed = permissions.with(|read| read.error().map(str::to_owned));

        let counted = !single || is_folder;
        let (counted_totals, largest, counting) =
            use_counting(counted.then(|| paths.clone()), is_folder);
        // Each folder picked is one of the items, though the count does not
        // include the folder it starts from.
        let totals = {
            let counted = counted_totals();
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

        // ---- the tabs -----------------------------------------------------
        let tabs = tab_model::available(single, is_folder);
        let mut selected = use_state({
            let tabs = tabs.clone();
            move || tab_model::pick(tab_model::remembered(), &tabs)
        });
        let current = selected();
        let scrolls = [
            use_scroll_controller(ScrollConfig::default),
            use_scroll_controller(ScrollConfig::default),
            use_scroll_controller(ScrollConfig::default),
            use_scroll_controller(ScrollConfig::default),
        ];
        let choose = EventHandler::new(move |tab: Tab| {
            selected.set(tab);
            tab_model::remember(tab);
        });

        // ---- the modes ----------------------------------------------------
        // The modes the person has set in this dialog, until it is closed.
        let edited = use_state(|| None::<Vec<u32>>);
        let mode_error = use_state(|| None::<String>);
        let modes = edited
            .read()
            .clone()
            .or_else(|| perms.as_ref().map(|perms| perms.iter().map(|perm| perm.mode).collect()))
            .map(ModeSet::new);
        let read_only = loaded
            .as_ref()
            .and_then(|loaded| loaded.props.volume.as_ref())
            .is_some_and(|volume| volume.read_only);
        let may_change = perms
            .as_ref()
            .is_some_and(|perms| !perms.is_empty() && perms.iter().all(|perm| perm.can_chmod));
        let on_modes = {
            let paths = paths.clone();
            let current = modes.as_ref().map(|modes| modes.modes().to_vec());
            EventHandler::new(move |new: Vec<u32>| {
                let Some(old) = current.clone() else {
                    return;
                };
                let mut edited = edited;
                let mut mode_error = mode_error;
                // The boxes follow the press at once; what the disk said
                // replaces them when it answers.
                edited.set(Some(new.clone()));
                mode_error.set(None);
                let work: Vec<(PathBuf, u32, u32)> = paths
                    .iter()
                    .cloned()
                    .zip(new)
                    .zip(old)
                    .map(|((path, new), old)| (path, new, old))
                    .collect();
                spawn(async move {
                    let (now, error) = spawn_blocking(move || apply_modes(work)).await;
                    edited.set(Some(now));
                    mode_error.set(error);
                });
            })
        };

        // ---- apply to enclosed items --------------------------------------
        let tree = use_state(|| TreePhase::Idle);
        let tree_job = use_hook(|| Rc::new(RefCell::new(None::<Cancel>)));
        use_drop({
            let tree_job = tree_job.clone();
            move || {
                if let Some(cancel) = tree_job.borrow().as_ref() {
                    cancel.cancel();
                }
            }
        });
        let folder_mode = modes.as_ref().and_then(ModeSet::common);
        let on_tree_ask = EventHandler::new(move |()| {
            let mut tree = tree;
            tree.set(TreePhase::Confirming);
        });
        let on_tree_cancel = {
            let tree_job = tree_job.clone();
            EventHandler::new(move |()| {
                let mut tree = tree;
                if matches!(*tree.peek(), TreePhase::Running) {
                    if let Some(cancel) = tree_job.borrow().as_ref() {
                        cancel.cancel();
                    }
                } else {
                    tree.set(TreePhase::Idle);
                }
            })
        };
        let on_tree_apply = {
            let tree_job = tree_job.clone();
            let root = first.path.clone();
            EventHandler::new(move |()| {
                let Some(mode) = folder_mode else {
                    return;
                };
                let mut tree = tree;
                let cancel = Cancel::new();
                *tree_job.borrow_mut() = Some(cancel.clone());
                tree.set(TreePhase::Running);
                let root = root.clone();
                spawn(async move {
                    let result = spawn_blocking(move || ops::chmod_tree(&root, mode, &cancel)).await;
                    tree.set(match result {
                        Ok(change) => TreePhase::Done(change),
                        Err(ops::OpError::Cancelled) => TreePhase::Stopped,
                        Err(error) => TreePhase::Failed(error.to_string()),
                    });
                });
            })
        };

        // ---- the checksum -------------------------------------------------
        let hash_progress = use_state(|| None::<HashProgress>);
        let hashing = use_hook(|| Rc::new(RefCell::new(None::<Rc<Hashing>>)));
        let on_hash_start = {
            let hashing = hashing.clone();
            let path = first.path.clone();
            EventHandler::new(move |()| {
                let job = Rc::new(Hashing::start(path.clone()));
                *hashing.borrow_mut() = Some(job.clone());
                let mut progress = hash_progress;
                spawn(async move {
                    loop {
                        let now = job.progress();
                        let finished = now.outcome.is_some();
                        progress.set_if_modified(Some(now));
                        if finished {
                            break;
                        }
                        timer(TICK).await;
                    }
                });
            })
        };
        let on_hash_stop = {
            let hashing = hashing.clone();
            EventHandler::new(move |()| {
                if let Some(job) = hashing.borrow().as_ref() {
                    job.cancel();
                }
            })
        };

        // ---- the page the tabs read ---------------------------------------
        let page = Page {
            tokens,
            entries,
            overlays,
            open_folder: &self.open_folder,
            open_with: self.open_with,
            home: &self.home,
            loaded: loaded.as_ref(),
            failed: failed.as_deref(),
            totals,
            counting,
            largest: largest.read().clone(),
        };
        let groups: Vec<Element> = match current {
            Tab::Basic => basic(&page),
            Tab::Permissions => permissions_tab(
                &page,
                &Access {
                    perms: perms.as_deref(),
                    failed: perms_failed.as_deref(),
                    modes,
                    editable: may_change && !read_only,
                    read_only,
                    error: mode_error.read().clone(),
                    on_change: on_modes,
                    tree: tree.read().clone(),
                    on_tree_ask,
                    on_tree_cancel,
                    on_tree_apply,
                },
            ),
            Tab::Details => details_tab(
                &page,
                &HashView {
                    progress: hash_progress.read().clone(),
                    on_start: on_hash_start,
                    on_stop: on_hash_stop,
                    overlays,
                },
            ),
            Tab::Contents => contents_tab(&page),
        };

        // ---- the frame ----------------------------------------------------
        // The standard header bar: its icon buttons take the header size and
        // its insets, so Close sits where it does in every other header.
        let header = HeaderBar::new("Properties").draggable(false).trailing(
            icon_button(Icon::WindowClose, move |_| overlays.dismiss_dialog()).tooltip("Close"),
        );
        let bar = segmented(
            tabs.iter()
                .map(|tab| (tab.key().to_owned(), tab.title().to_owned()))
                .collect(),
            current.key(),
            {
                let choose = choose.clone();
                move |key: String| {
                    if let Some(tab) = Tab::from_key(&key) {
                        choose.call(tab);
                    }
                }
            },
        )
        .tabs();
        // Equal segments fill the card when the titles fit; on a phone four
        // may not, and then the bar scrolls sideways.
        let widest = tabs
            .iter()
            .map(|tab| text_width(tab.title(), BODY))
            .fold(0.0, f32::max);
        let card = frame_width(Dim::PopupWidth.px()) - 2.0 * S4.px();
        let bar = if tab_model::fits_filled(widest, tabs.len(), card, S2.px(), S1.px()) {
            bar.fill()
        } else {
            bar.scroll()
        };
        let scroll = scrolls[match current {
            Tab::Basic => 0,
            Tab::Permissions => 1,
            Tab::Details => 2,
            Tab::Contents => 3,
        }];

        let shortcuts = tabs.clone();
        rect()
            .component("Properties")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                // Read the tab first: a borrow held across the choice would
                // make writing the new one fail.
                let now = *selected.peek();
                if let Some(next) = tab_model::shortcut(&event.key, event.modifiers, &shortcuts, now) {
                    event.stop_propagation();
                    choose.call(next);
                }
            })
            .child(header)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .component("PropertiesTabs")
                    .tw(&tokens, &[w_full(), px(S4), pt(S3), pb(S2)])
                    .child(bar),
            )
            .child(
                rect()
                    .component("PropertiesPage")
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(
                        ScrollView::new_controlled(scroll)
                            .key(current.key())
                            .child(
                                rect()
                                    .tw(&tokens, &[flex_col(), w_full(), gap(S4), p(S4)])
                                    .children(groups),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fact_load_reads_a_picture_header_the_drive_and_the_details() {
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
        assert_eq!(facts.details.unwrap().hard_links, 1);
        // The first of several items is read for its drive only.
        let plain = load_facts(Subject {
            path,
            category: Category::Image,
            details: false,
        })
        .unwrap();
        assert_eq!(plain.image, None);
        assert_eq!(plain.details, None);
        assert!(
            load_facts(Subject {
                path: dir.join("gone.png"),
                category: Category::Image,
                details: true,
            })
            .is_err()
        );
    }

    #[test]
    fn modes_are_set_on_disk_where_they_differ_and_failures_keep_the_old_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("bingux-props-modes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (one, two) = (dir.join("one"), dir.join("two"));
        std::fs::write(&one, "x").unwrap();
        std::fs::write(&two, "x").unwrap();
        std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&two, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;

        let (now, error) = apply_modes(vec![(one.clone(), 0o664, 0o644), (two.clone(), 0o600, 0o600)]);
        assert_eq!((now, error), (vec![0o664, 0o600], None));
        assert_eq!((mode(&one), mode(&two)), (0o664, 0o600));

        // A file that is gone cannot be changed; the answer keeps its old mode.
        let gone = dir.join("gone");
        let (now, error) = apply_modes(vec![(gone, 0o755, 0o644), (one.clone(), 0o640, 0o664)]);
        assert_eq!(now, vec![0o644, 0o640]);
        assert_eq!(error.as_deref(), Some("That file or folder no longer exists."));
        assert_eq!(mode(&one), 0o640);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn permissions_of_every_item_are_read_in_order() {
        let dir = std::env::temp_dir().join(format!("bingux-props-perms-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, "x").unwrap();
        let read = load_permissions(vec![dir.clone(), file.clone()]).unwrap();
        assert!(read[0].is_dir && !read[1].is_dir);
        assert!(load_permissions(vec![file, dir.join("missing")]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// The dialog in a headless window, at desktop and phone size.
#[cfg(all(test, feature = "testing"))]
mod window_tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use torin::prelude::Area;

    use super::*;
    use crate::shell::{OverlayHost, use_overlays};
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    const DESKTOP: (f32, f32) = (1100.0, 820.0);
    const PHONE: (f32, f32) = (390.0, 800.0);

    /// Opens the dialog for `entries` as soon as it is mounted.
    #[derive(PartialEq)]
    struct Open {
        entries: Vec<Entry>,
    }

    impl Component for Open {
        fn render(&self) -> impl IntoElement {
            let overlays = use_overlays();
            let entries = self.entries.clone();
            use_hook(move || {
                show(
                    Body {
                        entries,
                        overlays,
                        open_folder: EventHandler::new(|_: PathBuf| {}),
                        open_with: false,
                        home: PathBuf::from("/home/nobody"),
                    },
                    EventHandler::new(|()| {}),
                );
            });
            rect()
        }
    }

    #[derive(PartialEq)]
    struct Root {
        entries: Vec<Entry>,
    }

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _tokens = provide_tokens();
            WindowHost::new(OverlayHost::new(Open {
                entries: self.entries.clone(),
            }))
        }
    }

    /// A folder with two files in it, and a file beside it.
    fn fixture(tag: &str) -> (PathBuf, Entry, Entry) {
        let root = std::env::temp_dir().join(format!("bingux-props-win-{}-{tag}", std::process::id()));
        let folder = root.join("folder");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.txt"), "hello").unwrap();
        std::fs::write(folder.join("b.bin"), vec![0u8; 3000]).unwrap();
        let file = root.join("notes.txt");
        std::fs::write(&file, "notes").unwrap();
        let entry = |path: PathBuf, kind| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            Entry::named(path, name, kind, false, false)
        };
        (root, entry(folder, Kind::Directory), entry(file, Kind::File))
    }

    fn open(entries: Vec<Entry>, size: (f32, f32)) -> Snapshot {
        tab_model::remember(Tab::Basic);
        let mut shot = Snapshot::new(move || Element::from(Root { entries: entries.clone() }), size);
        shot.settle(Duration::from_millis(900));
        shot
    }

    fn named(shot: &mut Snapshot, name: &'static str) -> Vec<Area> {
        shot.runner().find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
        })
    }

    fn shows(shot: &mut Snapshot, text: &str) -> bool {
        !shot
            .runner()
            .find_many(|_, element| Label::try_downcast(element).filter(|label| label.text == text).map(|_| ()))
            .is_empty()
    }

    /// The card, the tab bar and the page, which must not move between tabs.
    fn frame(shot: &mut Snapshot) -> [Area; 3] {
        ["Properties", "PropertiesTabs", "PropertiesPage"].map(|name| {
            let found = named(shot, name);
            assert_eq!(found.len(), 1, "one {name}");
            found[0]
        })
    }

    fn ctrl(shot: &mut Snapshot, key: Key) {
        shot.key_with(key, Modifiers::CONTROL);
        shot.settle(Duration::from_millis(300));
    }

    /// A text that only one tab has.
    const MARKS: [(&str, &str); 4] = [
        ("1", "General"),
        ("2", "Ownership"),
        ("3", "Storage"),
        ("4", "Largest items"),
    ];

    #[test]
    fn the_card_is_one_size_on_every_tab_at_desktop_and_phone_size() {
        let (root, folder, _) = fixture("size");
        for size in [DESKTOP, PHONE] {
            let mut shot = open(vec![folder.clone()], size);
            let before = frame(&mut shot);
            assert!(before[0].width() > 100.0 && before[2].height() > 100.0, "{size:?}: {before:?}");
            for (digit, mark) in MARKS {
                ctrl(&mut shot, Key::Character(digit.into()));
                assert!(shows(&mut shot, mark), "{size:?}: Ctrl+{digit} shows {mark}");
                assert_eq!(frame(&mut shot), before, "{size:?}: the frame on tab {digit}");
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn control_tab_steps_through_the_tabs_and_wraps() {
        let (root, folder, _) = fixture("steps");
        let mut shot = open(vec![folder], DESKTOP);
        // Four steps go round once and end where they began.
        for (_, mark) in MARKS.iter().cycle().skip(1).take(4) {
            ctrl(&mut shot, Key::Named(NamedKey::Tab));
            assert!(shows(&mut shot, mark), "Ctrl+Tab shows {mark}");
        }
        shot.key_with(Key::Named(NamedKey::Tab), Modifiers::CONTROL | Modifiers::SHIFT);
        shot.settle(Duration::from_millis(300));
        assert!(shows(&mut shot, "Largest items"), "Ctrl+Shift+Tab goes back");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_press_on_a_tab_title_switches_to_it() {
        let (root, _, file) = fixture("click");
        let mut shot = open(vec![file], DESKTOP);
        assert!(shows(&mut shot, "General"));
        assert!(shot.click_text("Permissions"));
        shot.settle(Duration::from_millis(300));
        assert!(shows(&mut shot, "Ownership") && !shows(&mut shot, "General"));
        assert!(shot.click_text("Details"));
        shot.settle(Duration::from_millis(300));
        assert!(shows(&mut shot, "Storage") && shows(&mut shot, "Checksum"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn several_items_have_basic_and_permissions_only() {
        let (root, folder, file) = fixture("several");
        let mut shot = open(vec![folder, file], DESKTOP);
        assert!(shows(&mut shot, "General"));
        ctrl(&mut shot, Key::Character("3".into()));
        assert!(shows(&mut shot, "General"), "there is no third tab");
        ctrl(&mut shot, Key::Character("2".into()));
        assert!(shows(&mut shot, "Ownership"));
        assert!(!shows(&mut shot, "Allow executing as program"), "a folder is among them");
        std::fs::remove_dir_all(root).unwrap();
    }
}
