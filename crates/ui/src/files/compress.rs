//! The Compress dialog: make an archive of the selection, or, when it is all
//! pictures, films and songs, make them smaller.
//!
//! It is a [form dialog](crate::shell::form_dialog): a card of one fixed size
//! (520 x 680 px, a sheet that fills the window on a phone) whose body
//! scrolls, with Cancel (Escape) and Create (Enter) in the footer. Create is
//! disabled while the form is not valid, and the footer says why. Options
//! that depend on others (the level of a tar, the password of a zip) open and
//! close with a height animation, so nothing moves under the pointer.
//!
//! | Mode | When | What it asks |
//! | ---- | ---- | ------------ |
//! | Archive | always; the only mode for a mixed selection | name, format, where, level, password, volumes, what to include, what to leave out, delete the originals |
//! | Reduce size | the whole selection is pictures, films and songs; opens here | the format or codec, the quality, a size cap, whether to strip location and camera data, where the result goes, whether it replaces the original |
//!
//! The rules (what is valid, what each option becomes) are in
//! [`super::compress_model`]; what runs is in
//! `bingux_files::ops::{pack, reduce}`; the busy line and the toast are in
//! [`super::compress_run`]. This file is the form: the controls and what they
//! change.
//!
//! The last format, level and options are remembered in
//! `$XDG_CONFIG_HOME/bingux/files/compress.conf`; the name, a password,
//! "Delete originals" and "Replace original" never are.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex, PoisonError};

use bingux_files::ops::prefs::Prefs;
use bingux_files::ops::{
    Estimate, Format, Level, OnConflict, PackJob, ReduceJob, ReduceOptions, SPLIT_SIZES_MIB, Tools,
    estimate, parallelism, split_label,
};
use bingux_files::{Cancel, ChooserRequest, Entry};
use freya::prelude::*;

use super::actions::Env;
use super::chooser::choose;
use super::compress_media::{Media, media_groups};
use super::compress_model::{
    Check, Draft, Mode, Subject, Typed, World, check, name_is_taken, pack_options, reduce_options,
    result_name, unavailable_formats, usable_format,
};
use super::compress_run::{run_pack, run_reduce};
use crate::controls::{dropdown, form_field, segmented};
use crate::group::{Group, form_row};
use crate::motion::reveal_block;
use crate::rows::{row, switch_row};
use crate::shell::{ResponseKind, dialog, form_dialog, open_form};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::{spawn_detached, use_resource};
use crate::window::Window;

/// What the dialog is opened with: a selection and the view it came from.
pub(super) struct Launch {
    subject: Subject,
    /// A fixed world for the taxonomy and the tests: tools, names and an
    /// estimate that do not come from this machine.
    preview: Option<Prepared>,
    /// The names in the folder on show, so the first frame already knows
    /// which names are taken.
    view_names: HashSet<String>,
    prefs: Prefs,
    /// What Create asked for, waiting for the dialog to be gone (and, for a
    /// job that removes originals, for the person to confirm).
    pending: RefCell<Option<Request>>,
    /// The window the dialog was opened from. The dialog renders in the
    /// overlay layer, which sits outside the window's context, so it cannot
    /// look the window up itself.
    window: Option<Window>,
}

#[derive(Clone)]
enum Request {
    Pack {
        job: PackJob,
        whole: String,
        prefs: Prefs,
    },
    Reduce {
        job: ReduceJob,
        whole: String,
        prefs: Prefs,
    },
}

impl Request {
    /// Does it take the originals out of their place when done?
    fn removes_originals(&self) -> bool {
        match self {
            Request::Pack { job, .. } => job.options.delete_originals,
            Request::Reduce { job, .. } => job.options.replace_original,
        }
    }
}

/// A fixed world for the dialog: what a taxonomy page or a test shows, with
/// no tools, names or estimates read from this machine. See
/// [`compress_preview`].
#[derive(Clone, Debug)]
pub struct CompressPreview {
    pub entries: Vec<Entry>,
    pub tools: Tools,
    /// File names already in the folder.
    pub taken: Vec<String>,
    /// The typed name; `None` is the default for the selection.
    pub name: Option<String>,
    /// Open on Reduce size (`Some(true)`) or Archive; `None` is the default for the selection.
    pub reduce: Option<bool>,
    pub format: Option<Format>,
    pub locked: bool,
    pub password: String,
    pub confirm: String,
    pub excludes: String,
    pub replace: bool,
    pub estimate: Option<Estimate>,
    pub custom_dimension: bool,
}

impl CompressPreview {
    pub fn new(entries: Vec<Entry>, tools: Tools) -> CompressPreview {
        CompressPreview {
            entries,
            tools,
            taken: Vec::new(),
            name: None,
            reduce: None,
            format: None,
            locked: false,
            password: String::new(),
            confirm: String::new(),
            excludes: String::new(),
            replace: false,
            estimate: None,
            custom_dimension: false,
        }
    }
}

/// [`CompressPreview`] made ready to read.
#[derive(Clone, Debug)]
pub(super) struct Prepared {
    tools: Tools,
    taken: HashSet<String>,
    name: Option<String>,
    password: String,
    confirm: String,
    excludes: String,
    estimate: Option<Estimate>,
    reduce: Option<bool>,
    format: Option<Format>,
    locked: bool,
    replace: bool,
    custom_dimension: bool,
}

impl Prepared {
    fn draft(&self, mut draft: Draft) -> Draft {
        if let Some(reduce) = self.reduce {
            draft.mode = if reduce { Mode::Reduce } else { Mode::Archive };
        }
        if let Some(format) = self.format {
            draft.pack.format = format;
        }
        draft.locked = self.locked;
        draft.custom_dimension = self.custom_dimension;
        if self.replace {
            draft.conflict = OnConflict::Replace;
        }
        draft
    }
}

/// The dialog's contents with a fixed world, for the taxonomy and the
/// tests. It renders the same component the Files window opens, so a state
/// seen here is a state a person sees.
pub fn compress_preview(preview: CompressPreview) -> Element {
    let subject = Subject::of(&preview.entries);
    let launch = Launch {
        subject,
        view_names: HashSet::new(),
        prefs: Prefs::default(),
        pending: RefCell::new(None),
        window: None,
        preview: Some(Prepared {
            tools: preview.tools,
            taken: preview.taken.into_iter().collect(),
            name: preview.name,
            password: preview.password,
            confirm: preview.confirm,
            excludes: preview.excludes,
            estimate: preview.estimate,
            reduce: preview.reduce,
            format: preview.format,
            locked: preview.locked,
            replace: preview.replace,
            custom_dimension: preview.custom_dimension,
        }),
    };
    Element::from(CompressDialog {
        launch: Shared(Rc::new(launch)),
    })
}

/// A `Rc` that is equal to itself only, so a component that holds shared
/// non-comparable state can be a [`Component`].
struct Shared<T>(Rc<T>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The remembered choices, read once at start so opening the dialog does not
/// touch the disk.
static PREFS: LazyLock<Mutex<Option<Prefs>>> = LazyLock::new(|| Mutex::new(None));

/// Read what the machine has and what the person chose last time, off the UI
/// thread, so the dialog opens with both at hand. Call it when a file view
/// starts.
pub(super) fn warm() {
    spawn_detached("compress-warm", || {
        let _ = Tools::get();
        *PREFS.lock().unwrap_or_else(PoisonError::into_inner) = Some(Prefs::load());
    });
}

fn remembered() -> Prefs {
    PREFS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

/// Open the dialog for `selected`.
pub(super) fn show_compress(env: &Env, selected: &[Entry]) {
    if selected.is_empty() {
        return;
    }
    let subject = Subject::of(selected);
    let view_names = if env.folder.as_deref() == Some(subject.folder.as_path()) {
        env.listing
            .entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    } else {
        HashSet::new()
    };
    let launch = Rc::new(Launch {
        subject: subject.clone(),
        preview: None,
        view_names,
        prefs: remembered(),
        pending: RefCell::new(None),
        window: env.window,
    });
    let title = subject.title();
    let after = launch.clone();
    let focus = env.focus;
    let runner = env.clone();
    let closed = EventHandler::new(move |()| {
        let request = after.pending.borrow_mut().take();
        match request {
            Some(request) if request.removes_originals() => confirm(&runner, request),
            Some(request) => start(&runner, request),
            None => focus.request_focus(),
        }
    });
    open_form(
        env.overlays,
        title,
        Element::from(CompressDialog {
            launch: Shared(launch),
        }),
        Some(closed),
    );
}

fn start(env: &Env, request: Request) {
    match request {
        Request::Pack { job, whole, prefs } => run_pack(env, job, whole, Some(prefs)),
        Request::Reduce { job, whole, prefs } => run_reduce(env, job, whole, Some(prefs)),
    }
}

/// Ask before anything is taken out of its place. Enter keeps the originals.
fn confirm(env: &Env, request: Request) {
    let (title, body, go) = match &request {
        Request::Pack { job, .. } => (
            crate::tr!("ui-compress-confirm-pack-title"),
            crate::tr!("ui-compress-confirm-pack-body", count = job.roots.len()),
            crate::tr!("ui-compress-confirm-pack-go"),
        ),
        Request::Reduce { job, .. } => (
            crate::tr!("ui-compress-confirm-reduce-title"),
            crate::tr!("ui-compress-confirm-reduce-body", count = job.files.len()),
            crate::tr!("ui-compress-confirm-reduce-go"),
        ),
    };
    let overlays = env.overlays;
    let env = env.clone();
    overlays.show_dialog(
        dialog(title, body)
            .cancel(crate::tr!("ui-dialog-cancel"))
            .response(
                go,
                ResponseKind::Destructive,
                EventHandler::new(move |()| start(&env, request.clone())),
            )
            .default_action(crate::shell::CANCEL_ID)
            .icon(crate::shell::DialogIcon::Warning),
    );
}

// ---------------------------------------------------------------------------
// What loads off the UI thread
// ---------------------------------------------------------------------------

fn load_tools(_: ()) -> Result<Tools, String> {
    Ok(Tools::get().clone())
}

/// The file names in a folder.
fn load_names(dir: PathBuf) -> Result<HashSet<String>, String> {
    let entries = std::fs::read_dir(&dir).map_err(|error| error.to_string())?;
    Ok(entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect())
}

/// What an estimate is asked for. The quality is rounded, so a drag of the
/// slider asks for one sample every five steps, not every step.
#[derive(Clone, PartialEq)]
struct EstimateKey {
    files: Vec<PathBuf>,
    options: ReduceOptions,
    on: bool,
}

/// The estimate that is running, so a newer one can stop it.
static RUNNING: Mutex<Option<Cancel>> = Mutex::new(None);

fn load_estimate(key: EstimateKey) -> Result<Option<Estimate>, String> {
    if !key.on || key.files.is_empty() {
        return Ok(None);
    }
    let cancel = Cancel::new();
    if let Some(older) = RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .replace(cancel.clone())
    {
        older.cancel();
    }
    estimate(&key.files, &key.options, &cancel)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn round_quality(options: &ReduceOptions) -> ReduceOptions {
    let mut rounded = options.clone();
    rounded.image.quality = ((u32::from(rounded.image.quality) + 2) / 5 * 5).clamp(1, 100) as u8;
    rounded.replace_original = false;
    rounded
}

// ---------------------------------------------------------------------------
// The dialog
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
struct CompressDialog {
    launch: Shared<Launch>,
}

/// The states of the form, copied into closures.
#[derive(Clone, Copy)]
pub(super) struct Fields {
    pub draft: State<Draft>,
    pub name: State<String>,
    pub excludes: State<String>,
    pub password: State<String>,
    pub confirm: State<String>,
    pub dimension: State<String>,
}

impl Component for CompressDialog {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let in_context = use_try_consume::<Window>();
        let window = self.launch.0.window.or(in_context);
        let launch = self.launch.0.clone();
        let subject = &launch.subject;
        let fields = Fields {
            draft: use_state({
                let (subject, prefs, preview) = (
                    subject.clone(),
                    launch.prefs.clone(),
                    launch.preview.clone(),
                );
                move || {
                    let mut draft = Draft::new(&subject, prefs);
                    draft.pack.format = usable_format(draft.pack.format);
                    match preview {
                        Some(prepared) => prepared.draft(draft),
                        None => draft,
                    }
                }
            }),
            name: use_state({
                let base = launch
                    .preview
                    .as_ref()
                    .and_then(|p| p.name.clone())
                    .unwrap_or_else(|| subject.default_base());
                move || base
            }),
            excludes: use_state({
                let text = launch
                    .preview
                    .as_ref()
                    .map_or_else(|| launch.prefs.pack.excludes.text(), |p| p.excludes.clone());
                move || text
            }),
            password: use_state({
                let text = launch
                    .preview
                    .as_ref()
                    .map(|p| p.password.clone())
                    .unwrap_or_default();
                move || text
            }),
            confirm: use_state({
                let text = launch
                    .preview
                    .as_ref()
                    .map(|p| p.confirm.clone())
                    .unwrap_or_default();
                move || text
            }),
            dimension: use_state(|| "1600".to_string()),
        };
        let draft = fields.draft.read().clone();
        let typed = Typed {
            name: fields.name.read().clone(),
            excludes: fields.excludes.read().clone(),
            password: fields.password.read().clone(),
            confirm: fields.confirm.read().clone(),
            dimension: fields.dimension.read().clone(),
        };

        // ---- what is read off the UI thread ----------------------------------
        let tools = use_resource((), load_tools);
        let tools_now: Option<Tools> = match &launch.preview {
            Some(prepared) => Some(prepared.tools.clone()),
            None => tools.with(|tools| tools.value().cloned()),
        };
        let destination = draft
            .location
            .clone()
            .unwrap_or_else(|| subject.folder.clone());
        let names = use_resource(destination.clone(), load_names);
        let same_folder = destination == subject.folder;
        let existing: HashSet<String> = match &launch.preview {
            Some(prepared) => prepared.taken.clone(),
            None => names
                .with(|names| names.value().cloned())
                .unwrap_or_else(|| {
                    if same_folder {
                        launch.view_names.clone()
                    } else {
                        HashSet::new()
                    }
                }),
        };
        let media_files: Vec<PathBuf> = subject
            .paths
            .iter()
            .zip(&subject.kinds)
            .filter_map(|(path, kind)| kind.map(|_| path.clone()))
            .collect();
        let estimating = draft.mode == Mode::Reduce && tools_now.is_some();
        let reduce_now = reduce_options(&draft, &typed);
        let estimate_now = use_resource(
            EstimateKey {
                files: media_files.clone(),
                options: round_quality(&reduce_now),
                on: estimating && launch.preview.is_none(),
            },
            load_estimate,
        );
        let (estimated, estimate_busy, estimate_error) = match &launch.preview {
            Some(prepared) => (prepared.estimate.clone(), false, None),
            None => estimate_now.with(|estimate| {
                (
                    estimate.value().cloned().flatten(),
                    matches!(
                        estimate,
                        crate::task::Resource::Loading | crate::task::Resource::Refreshing(_)
                    ) && estimating,
                    estimate.error().map(str::to_owned),
                )
            }),
        };

        // ---- what is valid ---------------------------------------------------
        let world = World {
            subject,
            existing: &existing,
            tools: tools_now.as_ref(),
        };
        let verdict = check(&draft, &typed, &world);

        // ---- Create ----------------------------------------------------------
        let create = {
            let launch = launch.clone();
            let (draft, typed, verdict) = (draft.clone(), typed.clone(), verdict.clone());
            let media_files = media_files.clone();
            let existing = existing.clone();
            EventHandler::new(move |()| {
                if verdict.blocked.is_some() {
                    return;
                }
                let subject = &launch.subject;
                let mut prefs = Prefs {
                    pack: draft.pack.clone(),
                    reduce: draft.reduce.clone(),
                };
                prefs.pack.password = None;
                let request = match draft.mode {
                    Mode::Archive => {
                        let Ok(options) = pack_options(&draft, &typed) else {
                            return;
                        };
                        prefs.pack.excludes = options.excludes.clone();
                        let dest_dir = draft
                            .location
                            .clone()
                            .unwrap_or_else(|| subject.folder.clone());
                        let whole = match subject.names.as_slice() {
                            [only] => format!("“{only}”"),
                            many => format!("{} items", many.len()),
                        };
                        let _ = &existing;
                        Request::Pack {
                            job: PackJob {
                                roots: subject.paths.clone(),
                                dest_dir,
                                base: typed.name.clone(),
                                options,
                                on_conflict: draft.conflict,
                            },
                            whole,
                            prefs,
                        }
                    }
                    Mode::Reduce => {
                        let options = reduce_options(&draft, &typed);
                        prefs.reduce = options.clone();
                        prefs.reduce.replace_original = false;
                        let whole = match media_files.as_slice() {
                            [only] => format!(
                                "“{}”",
                                only.file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            ),
                            many => format!("{} files", many.len()),
                        };
                        Request::Reduce {
                            job: ReduceJob {
                                parallel: parallelism(&media_files),
                                files: media_files.clone(),
                                dest_dir: draft.location.clone(),
                                options,
                            },
                            whole,
                            prefs,
                        }
                    }
                };
                *launch.pending.borrow_mut() = Some(request);
            })
        };

        // ---- the body --------------------------------------------------------
        let choose_where = {
            let mut draft_state = fields.draft;
            let start = destination.clone();
            EventHandler::new(move |()| {
                let Some(window) = window else { return };
                choose(
                    window,
                    ChooserRequest::select_folder()
                        .title(crate::tr!("ui-compress-save-in"))
                        .accept_label(crate::tr!("ui-compress-choose"))
                        .current_folder(start.clone()),
                    move |chosen| {
                        if let Some(folder) =
                            chosen.and_then(|chosen| chosen.paths.into_iter().next())
                        {
                            draft_state.write().location = Some(folder);
                        }
                    },
                );
            })
        };
        let mut body: Vec<Element> = Vec::new();
        if subject.is_pure_media() {
            let mut state = fields.draft;
            body.push(
                Group::new(None)
                    .row(form_row(
                        crate::tr!("ui-compress-type"),
                        segmented(
                            vec![
                                ("archive".into(), crate::tr!("ui-compress-archive")),
                                ("reduce".into(), crate::tr!("ui-compress-reduce-size")),
                            ],
                            if draft.mode == Mode::Reduce {
                                "reduce"
                            } else {
                                "archive"
                            },
                            move |key: String| {
                                state.write().mode = if key == "reduce" {
                                    Mode::Reduce
                                } else {
                                    Mode::Archive
                                };
                            },
                        )
                        .fill(),
                    ))
                    .into(),
            );
        }
        let archive = draft.mode == Mode::Archive;
        let archive_groups = archive_groups(
            &tokens,
            &fields,
            &draft,
            &typed,
            &verdict,
            &existing,
            subject,
            &destination,
            choose_where.clone(),
            create.clone(),
        );
        let media = Media {
            fields,
            draft: &draft,
            check: &verdict,
            subject,
            tools: tools_now.as_ref(),
            estimate: estimated.as_ref(),
            estimating: estimate_busy,
            estimate_error: estimate_error.as_deref(),
            destination: &destination,
            choose: choose_where,
        };
        let reduce_groups = media_groups(&media);
        // Both bodies exist at all times and one is open, so the card never
        // changes size and what is closed cannot take focus.
        body.push(
            reveal_block(
                archive,
                S4,
                rect()
                    .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
                    .children(archive_groups),
            )
            .lazy()
            .into(),
        );
        body.push(
            reveal_block(
                !archive,
                S4,
                rect()
                    .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
                    .children(reduce_groups),
            )
            .lazy()
            .into(),
        );

        let busy = (draft.mode == Mode::Reduce
            && launch.preview.is_none()
            && tools.with(|tools| tools.is_loading()))
        .then(|| crate::tr!("ui-compress-tools-busy"));
        form_dialog(
            subject.title(),
            rect()
                .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
                .children(body),
            crate::tr!("ui-compress-create"),
            create,
        )
        .blocked(verdict.blocked.clone())
        .busy(busy)
    }
}

/// The Save in row: the folder, shortened by what matters least, and a press
/// anywhere on the row opens the folder chooser.
pub(super) fn location_row(folder: &std::path::Path, choose: EventHandler<()>) -> crate::rows::Row {
    row(crate::tr!("ui-compress-save-in"))
        .path_value(folder.to_string_lossy().into_owned())
        .navigates()
        .on_press(choose)
}

// ---------------------------------------------------------------------------
// Archive mode
// ---------------------------------------------------------------------------

fn format_options() -> Vec<(String, String)> {
    Format::ALL
        .into_iter()
        .map(|format| {
            (
                format.label().to_string(),
                format!("{}, {}", format.label(), format.note().to_lowercase()),
            )
        })
        .collect()
}

fn format_from_key(key: &str) -> Option<Format> {
    Format::ALL.into_iter().find(|format| format.label() == key)
}

#[allow(clippy::too_many_arguments)]
fn archive_groups(
    tokens: &crate::theme::Tokens,
    fields: &Fields,
    draft: &Draft,
    typed: &Typed,
    verdict: &Check,
    existing: &HashSet<String>,
    subject: &Subject,
    destination: &std::path::Path,
    choose: EventHandler<()>,
    create: EventHandler<()>,
) -> Vec<Element> {
    let format = draft.pack.format;
    let mut state = fields.draft;
    let taken = name_is_taken(draft, typed, existing);
    let shown_name = result_name(draft, typed, existing);

    // ---- the archive -------------------------------------------------------
    let name_field = form_field(crate::tr!("ui-compress-name"), fields.name)
        .placeholder(crate::tr!("ui-compress-name-placeholder"))
        .error(verdict.name.clone())
        .on_submit(create)
        .wide()
        .autofocus();
    let conflict = row(crate::tr!("ui-compress-conflict"))
        .subtitle(if draft.conflict == OnConflict::KeepBoth {
            crate::tr!(
                "ui-compress-conflict-keep-body",
                taken = bingux_files::ops::file_name(&typed.name, format),
                shown = shown_name.clone()
            )
        } else {
            crate::tr!(
                "ui-compress-conflict-replace-body",
                shown = shown_name.clone()
            )
        })
        .trailing(segmented(
            vec![
                ("keep".into(), crate::tr!("ui-compress-conflict-keep")),
                ("replace".into(), crate::tr!("ui-compress-conflict-replace")),
            ],
            if draft.conflict == OnConflict::KeepBoth {
                "keep"
            } else {
                "replace"
            },
            move |key: String| {
                state.write().conflict = if key == "replace" {
                    OnConflict::Replace
                } else {
                    OnConflict::KeepBoth
                };
            },
        ));
    let format_row = form_row(
        crate::tr!("ui-compress-format"),
        dropdown(format_options(), format.label(), move |key: String| {
            if let Some(chosen) = format_from_key(&key) {
                state.write().pack.format = chosen;
            }
        })
        .unavailable(
            unavailable_formats()
                .into_iter()
                .map(|(key, why)| (key, why))
                .collect(),
        )
        .title(crate::tr!("ui-compress-format")),
    );
    let archive = Group::titled(&crate::tr!("ui-compress-archive"))
        .row(name_field)
        .row(reveal_block(taken, S0, rect().child(Element::from(conflict))).lazy())
        .row(format_row)
        .row(location_row(destination, choose));
    let _ = subject;

    // ---- how hard ----------------------------------------------------------
    let level = Group::titled(&crate::tr!("ui-compress-compression")).row(form_row(
        crate::tr!("ui-compress-level"),
        segmented(
            Level::ALL
                .into_iter()
                .map(|level| (level_key(level).to_string(), level.label().to_string()))
                .collect(),
            level_key(draft.pack.level),
            move |key: String| {
                if let Some(level) = Level::ALL
                    .into_iter()
                    .find(|level| level_key(*level) == key)
                {
                    state.write().pack.level = level;
                }
            },
        ),
    ));

    // ---- the password ------------------------------------------------------
    let lock_reason = format.lock_unavailable();
    let can_lock = format.can_lock() && lock_reason.is_none();
    let mut lock = Group::titled(&crate::tr!("ui-compress-password"));
    lock = lock.row(
        switch_row(
            crate::tr!("ui-compress-lock"),
            draft.locked && can_lock,
            move |on: bool| state.write().locked = on,
        )
        .subtitle(match (&lock_reason, format) {
            (Some(reason), Format::Zip | Format::SevenZip) => reason.clone(),
            (Some(_), _) => crate::tr!("ui-compress-lock-zip-only"),
            (None, _) => crate::tr!("ui-compress-lock-aes"),
        })
        .enabled(can_lock),
    );
    let locked_open = can_lock && draft.locked;
    let fields_group = Group::titled(&crate::tr!("ui-compress-password-choose"))
        .row(
            form_field(crate::tr!("ui-compress-password-field"), fields.password)
                .password()
                .error(verdict.password.clone()),
        )
        .row(
            form_field(crate::tr!("ui-compress-confirm-field"), fields.confirm)
                .password()
                .error(verdict.confirm.clone()),
        )
        .row(reveal_block(
            format.can_hide_names(),
            S0,
            rect().child(Element::from(
                switch_row(
                    crate::tr!("ui-compress-hide-names"),
                    draft.pack.hide_names,
                    move |on: bool| state.write().pack.hide_names = on,
                )
                .subtitle(crate::tr!("ui-compress-hide-names-hint")),
            )),
        ));

    // ---- volumes -----------------------------------------------------------
    let split_reason = format.split_unavailable();
    let mut split_options: Vec<(String, String)> =
        vec![("none".into(), crate::tr!("ui-compress-split-one"))];
    split_options.extend(SPLIT_SIZES_MIB.iter().map(|mib| {
        (
            mib.to_string(),
            crate::tr!("ui-compress-split-each", size = split_label(*mib)),
        )
    }));
    let volumes = Group::titled(&crate::tr!("ui-compress-volumes")).row(
        row(crate::tr!("ui-compress-split"))
            .subtitle(
                split_reason
                    .clone()
                    .unwrap_or_else(|| crate::tr!("ui-compress-split-hint")),
            )
            .trailing(
                dropdown(
                    split_options,
                    draft
                        .pack
                        .split_mib
                        .map_or_else(|| "none".to_string(), |mib| mib.to_string())
                        .as_str(),
                    move |key: String| state.write().pack.split_mib = key.parse::<u64>().ok(),
                )
                .enabled(split_reason.is_none())
                .title(crate::tr!("ui-compress-volumes")),
            ),
    );

    // ---- what goes in ------------------------------------------------------
    let is_tar = format.is_tar();
    let options = Group::titled(&crate::tr!("ui-compress-options"))
        .row(switch_row(
            crate::tr!("ui-compress-hidden"),
            draft.pack.include_hidden,
            move |on: bool| state.write().pack.include_hidden = on,
        ))
        .row(
            switch_row(
                crate::tr!("ui-compress-links"),
                !draft.pack.follow_links,
                move |on: bool| state.write().pack.follow_links = !on,
            )
            .subtitle(crate::tr!("ui-compress-links-hint")),
        )
        .row(
            switch_row(
                crate::tr!("ui-compress-owners"),
                draft.pack.keep_owner && is_tar,
                move |on: bool| state.write().pack.keep_owner = on,
            )
            .subtitle(if is_tar {
                crate::tr!("ui-compress-owners-hint")
            } else {
                crate::tr!("ui-compress-owners-tar-only")
            })
            .enabled(is_tar),
        )
        .row(
            form_field(crate::tr!("ui-compress-excludes"), fields.excludes)
                .placeholder(crate::tr!("ui-compress-excludes-placeholder"))
                .hint(crate::tr!("ui-compress-excludes-hint"))
                .error(verdict.excludes.clone()),
        );
    let after = Group::titled(&crate::tr!("ui-compress-after")).row(
        switch_row(
            crate::tr!("ui-compress-trash-originals"),
            draft.pack.delete_originals,
            move |on: bool| state.write().pack.delete_originals = on,
        )
        .subtitle(crate::tr!("ui-compress-trash-originals-hint")),
    );
    vec![
        archive.into(),
        reveal_block(format.has_levels(), S4, Element::from(level))
            .lazy()
            .into(),
        reveal_block(
            format.can_lock(),
            S4,
            rect()
                .tw(tokens, &[flex_col(), w_full(), gap(S4)])
                .child(Element::from(lock))
                .child(reveal_block(locked_open, S4, Element::from(fields_group)).lazy()),
        )
        .lazy()
        .into(),
        reveal_block(format.can_split(), S4, Element::from(volumes))
            .lazy()
            .into(),
        options.into(),
        after.into(),
    ]
}

fn level_key(level: Level) -> &'static str {
    match level {
        Level::Fastest => "fastest",
        Level::Balanced => "balanced",
        Level::Smallest => "smallest",
    }
}
