//! What a chooser is asked, what it answers, and what Open or Save does.
//!
//! The types map one to one onto `org.freedesktop.portal.FileChooser`, so a
//! portal backend can build a [`ChooserRequest`] from the method call's
//! options and answer with a [`Chosen`]. The mapping, option by option:
//!
//! | Portal | Here |
//! | ------ | ---- |
//! | `OpenFile` / `SaveFile` / `SaveFiles` | [`Action::Open`] or [`Action::SelectFolder`] / [`Action::Save`] / [`Action::SaveFiles`] |
//! | `title` (method argument) | [`ChooserRequest::title`] |
//! | `accept_label` | [`ChooserRequest::accept_label`] |
//! | `modal` | [`ChooserRequest::modal`] (default true) |
//! | `multiple` | [`ChooserRequest::multiple`] |
//! | `directory` | [`Action::SelectFolder`] |
//! | `filters` `a(sa(us))` | [`ChooserRequest::filters`], see [`Filter::from_portal`] |
//! | `current_filter` `(sa(us))` | [`ChooserRequest::current_filter`], an index into `filters` |
//! | `choices` `a(ssa(ss)s)` | [`ChooserRequest::choices`], see [`Choice`] |
//! | `current_name` | [`ChooserRequest::current_name`] |
//! | `current_folder` `ay` | [`ChooserRequest::current_folder`] |
//! | `current_file` `ay` | [`ChooserRequest::current_file`] |
//! | `files` `aay` (SaveFiles) | [`ChooserRequest::files`] |
//! | response `uris` `as` | [`Chosen::paths`] through [`crate::location::file_uri`] |
//! | response `current_filter`, `choices` | [`Chosen::current_filter`], [`Chosen::choices`] |
//! | response code 1 (cancelled) | the caller got `None` instead of a [`Chosen`] |

use std::fs;
use std::path::{Path, PathBuf};

use crate::entry::Entry;
use crate::filter::Filter;
use crate::location::{self, check_file_name};

/// Which of the portal's three methods this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Choose existing files (`OpenFile`).
    Open,
    /// Choose existing folders (`OpenFile` with `directory`).
    SelectFolder,
    /// Name a file to write (`SaveFile`).
    Save,
    /// Choose the folder to write several named files in (`SaveFiles`).
    SaveFiles,
}

/// How a chooser first shows a folder. A hint: the person can switch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    /// A list, or a grid of thumbnails when the filter only passes images.
    #[default]
    Auto,
    List,
    Grid,
}

/// An extra option shown with the file list: the portal's "choices". With no
/// `options` it is a yes/no switch whose values are `"true"` and `"false"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    /// `(option id, label)` pairs.
    pub options: Vec<(String, String)>,
    /// The option id that starts selected, or `"true"`/`"false"`.
    pub selected: String,
}

impl Choice {
    /// A yes/no option.
    pub fn toggle(id: impl Into<String>, label: impl Into<String>, on: bool) -> Self {
        Choice {
            id: id.into(),
            label: label.into(),
            options: Vec::new(),
            selected: on.to_string(),
        }
    }
}

/// A request to choose files.
#[derive(Clone, Debug, PartialEq)]
pub struct ChooserRequest {
    pub action: Action,
    pub title: String,
    pub accept_label: Option<String>,
    pub modal: bool,
    pub multiple: bool,
    pub filters: Vec<Filter>,
    /// Also offer the built-in filters (Images, Audio, ...) after the
    /// request's own. See [`ChooserRequest::effective_filters`].
    pub presets: bool,
    /// An index into [`ChooserRequest::effective_filters`] (which starts with
    /// the request's own `filters`, so the portal's index is the same); the
    /// first when `None`.
    pub current_filter: Option<usize>,
    /// The suggested name of the file to save.
    pub current_name: Option<String>,
    /// The folder to start in.
    pub current_folder: Option<PathBuf>,
    /// An existing file to select: its folder is opened and its name filled in.
    pub current_file: Option<PathBuf>,
    /// The names to write, for [`Action::SaveFiles`].
    pub files: Vec<String>,
    pub choices: Vec<Choice>,
    pub view: ViewMode,
}

impl ChooserRequest {
    fn new(action: Action, title: &str) -> Self {
        ChooserRequest {
            action,
            title: title.into(),
            accept_label: None,
            modal: true,
            multiple: false,
            filters: Vec::new(),
            presets: false,
            current_filter: None,
            current_name: None,
            current_folder: None,
            current_file: None,
            files: Vec::new(),
            choices: Vec::new(),
            view: ViewMode::Auto,
        }
    }

    /// Choose one existing file.
    pub fn open() -> Self {
        Self::new(Action::Open, "Open file")
    }

    /// Choose one existing folder.
    pub fn select_folder() -> Self {
        Self::new(Action::SelectFolder, "Choose folder")
    }

    /// Name a new file, starting with `name`.
    pub fn save(name: impl Into<String>) -> Self {
        let mut request = Self::new(Action::Save, "Save file");
        request.current_name = Some(name.into());
        request
    }

    /// Choose the folder that `names` are written into.
    pub fn save_files(names: Vec<String>) -> Self {
        let mut request = Self::new(Action::SaveFiles, "Save files");
        request.files = names;
        request
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn accept_label(mut self, label: impl Into<String>) -> Self {
        self.accept_label = Some(label.into());
        self
    }

    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// Allow several files (or folders).
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// Add a filter. The first one added is shown first and is the default.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Add a filter from what an app author writes: file extensions, globs
    /// and MIME types, such as `"png jpg image/webp"`. See [`Filter::parse`].
    pub fn filter_spec(mut self, name: impl Into<String>, spec: &str) -> Self {
        self.filters.push(Filter::parse(name, spec));
        self
    }

    /// Replace the filters.
    pub fn filters(mut self, filters: Vec<Filter>) -> Self {
        self.filters = filters;
        self
    }

    /// Offer the built-in filters (All files, Images, Audio, Video,
    /// Documents, Archives, Text) after the request's own.
    pub fn presets(mut self, on: bool) -> Self {
        self.presets = on;
        self
    }

    pub fn current_filter(mut self, index: usize) -> Self {
        self.current_filter = Some(index);
        self
    }

    pub fn current_folder(mut self, folder: impl Into<PathBuf>) -> Self {
        self.current_folder = Some(folder.into());
        self
    }

    pub fn current_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.current_file = Some(file.into());
        self
    }

    pub fn choice(mut self, choice: Choice) -> Self {
        self.choices.push(choice);
        self
    }

    pub fn view(mut self, view: ViewMode) -> Self {
        self.view = view;
        self
    }

    /// The text of the accept button: the request's, or the action's.
    pub fn accept_text(&self) -> String {
        self.accept_label.clone().unwrap_or_else(|| {
            match self.action {
                Action::Open => "Open",
                Action::SelectFolder => "Select",
                Action::Save | Action::SaveFiles => "Save",
            }
            .to_string()
        })
    }

    /// The filters the chooser's menu lists, in order.
    ///
    /// * No filters in the request: the presets, "All files" first.
    /// * Filters, and `presets` off: the request's own.
    /// * Filters, and `presets` on: the request's own, then every preset
    ///   that is not the same as one of them (by name or by rules).
    ///
    /// [`ChooserRequest::current_filter`], [`ChooserRequest::initial_filter`]
    /// and [`Chosen::current_filter`] all index this list.
    pub fn effective_filters(&self) -> Vec<Filter> {
        if self.filters.is_empty() {
            return Filter::presets();
        }
        let mut all = self.filters.clone();
        if self.presets {
            for preset in Filter::presets() {
                let known = self.filters.iter().any(|own| {
                    own.name.eq_ignore_ascii_case(&preset.name) || own.rules == preset.rules
                });
                if !known {
                    all.push(preset);
                }
            }
        }
        all
    }

    /// The filter that starts active: `current_filter` if it is in range of
    /// [`ChooserRequest::effective_filters`], else the first.
    pub fn initial_filter(&self) -> Option<usize> {
        let count = self.effective_filters().len();
        match self.current_filter {
            Some(index) if index < count => Some(index),
            _ if count == 0 => None,
            _ => Some(0),
        }
    }

    /// The folder to open first: that of `current_file`, else
    /// `current_folder`, else `home`.
    pub fn initial_folder(&self, home: &Path) -> PathBuf {
        self.current_file
            .as_deref()
            .and_then(Path::parent)
            .or(self.current_folder.as_deref())
            .unwrap_or(home)
            .to_path_buf()
    }

    /// The name that starts in the name field: the name of `current_file`,
    /// else `current_name`.
    pub fn initial_name(&self) -> String {
        self.current_file
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .or_else(|| self.current_name.clone())
            .unwrap_or_default()
    }
}

/// A chooser's answer. Cancelling is the absence of one.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    /// The files or folders, absolute, in the order chosen. One for Save.
    pub paths: Vec<PathBuf>,
    /// The filter that was active when the person accepted: an index into
    /// [`ChooserRequest::effective_filters`].
    pub current_filter: Option<usize>,
    /// `(choice id, selected option id)` for every choice in the request.
    pub choices: Vec<(String, String)>,
}

/// What pressing the accept button, or Enter in the name field, means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Accept {
    /// These are the answer.
    Done(Vec<PathBuf>),
    /// The answer, but these already exist: ask before replacing them.
    Confirm(Vec<PathBuf>),
    /// Go into this folder instead.
    Enter(PathBuf),
    /// Say this to the person and stay.
    Invalid(String),
}

/// Decide what accepting does. `folder` is the folder being shown, `home`
/// the home folder (for `~` in a typed name), `selected` the selected
/// entries and `name` the text of the name field (Save only). Save and Save
/// files stat the target paths: microseconds on a local disk, but a network
/// folder that has stopped answering holds the call, so run it on a worker
/// (`spawn_blocking`) when `folder` is on a network mount
/// (`mounts::kind_of(folder) == MountKind::Network`). Open and Select folder
/// touch no disk.
pub fn accept(
    request: &ChooserRequest,
    folder: &Path,
    home: &Path,
    selected: &[Entry],
    name: &str,
) -> Accept {
    match request.action {
        Action::Open => accept_open(request, selected),
        Action::SelectFolder => accept_folders(request, folder, selected),
        Action::Save => accept_save(folder, home, name),
        Action::SaveFiles => accept_files(request, folder),
    }
}

fn accept_open(request: &ChooserRequest, selected: &[Entry]) -> Accept {
    if let [only] = selected
        && only.is_dir()
    {
        return Accept::Enter(only.path.clone());
    }
    let mut paths: Vec<PathBuf> = selected
        .iter()
        .filter(|entry| !entry.is_dir())
        .map(|entry| entry.path.clone())
        .collect();
    if !request.multiple {
        paths.truncate(1);
    }
    if paths.is_empty() {
        Accept::Invalid("Choose a file.".into())
    } else {
        Accept::Done(paths)
    }
}

fn accept_folders(request: &ChooserRequest, folder: &Path, selected: &[Entry]) -> Accept {
    if selected.is_empty() {
        return Accept::Done(vec![folder.to_path_buf()]);
    }
    let mut paths: Vec<PathBuf> = selected
        .iter()
        .filter(|entry| entry.is_dir())
        .map(|entry| entry.path.clone())
        .collect();
    if !request.multiple {
        paths.truncate(1);
    }
    if paths.is_empty() {
        Accept::Invalid("Choose a folder.".into())
    } else {
        Accept::Done(paths)
    }
}

fn accept_save(folder: &Path, home: &Path, name: &str) -> Accept {
    let name = name.trim();
    if name.is_empty() {
        return Accept::Invalid(location::NameError::Empty.to_string());
    }
    let target = location::resolve(name, folder, home);
    let Some(file_name) = target.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Accept::Invalid(location::NameError::Reserved.to_string());
    };
    if let Err(error) = check_file_name(&file_name) {
        return Accept::Invalid(error.to_string());
    }
    if target.is_dir() {
        return Accept::Enter(target);
    }
    let parent = target.parent().unwrap_or(folder);
    if !parent.is_dir() {
        return Accept::Invalid(format!("The folder {} does not exist.", parent.display()));
    }
    // The name may be typed with a path; saving over a file is the one thing
    // that needs a second question.
    if fs::symlink_metadata(&target).is_ok() {
        Accept::Confirm(vec![target])
    } else {
        Accept::Done(vec![target])
    }
}

fn accept_files(request: &ChooserRequest, folder: &Path) -> Accept {
    if request.files.is_empty() {
        return Accept::Invalid("There is nothing to save.".into());
    }
    let mut paths = Vec::new();
    for name in &request.files {
        if name.contains('/') {
            return Accept::Invalid(format!("{name} is not a file name."));
        }
        if let Err(error) = check_file_name(name) {
            return Accept::Invalid(error.to_string());
        }
        paths.push(folder.join(name));
    }
    let existing: Vec<PathBuf> = paths
        .iter()
        .filter(|path| fs::symlink_metadata(path).is_ok())
        .cloned()
        .collect();
    if existing.is_empty() {
        Accept::Done(paths)
    } else {
        Accept::Confirm(existing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bingux-files-req-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.txt"), b"x").unwrap();
        dir
    }

    #[test]
    fn open_enters_a_folder_and_returns_files() {
        let dir = scratch("open");
        let file = Entry::from_path(&dir.join("a.txt"), false).unwrap();
        let sub = Entry::from_path(&dir.join("sub"), false).unwrap();
        let request = ChooserRequest::open();
        assert_eq!(
            accept(&request, &dir, &dir, &[], ""),
            Accept::Invalid("Choose a file.".into())
        );
        assert_eq!(
            accept(&request, &dir, &dir, &[sub.clone()], ""),
            Accept::Enter(sub.path.clone())
        );
        assert_eq!(
            accept(&request, &dir, &dir, &[file.clone()], ""),
            Accept::Done(vec![file.path])
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_choice_defaults_to_the_current_folder() {
        let dir = scratch("folder");
        let request = ChooserRequest::select_folder();
        assert_eq!(
            accept(&request, &dir, &dir, &[], ""),
            Accept::Done(vec![dir.clone()])
        );
        let file = Entry::from_path(&dir.join("a.txt"), false).unwrap();
        assert_eq!(
            accept(&request, &dir, &dir, &[file], ""),
            Accept::Invalid("Choose a folder.".into())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_asks_before_replacing() {
        let dir = scratch("save");
        let request = ChooserRequest::save("x");
        assert_eq!(
            accept(&request, &dir, &dir, &[], "new.txt"),
            Accept::Done(vec![dir.join("new.txt")])
        );
        assert_eq!(
            accept(&request, &dir, &dir, &[], "a.txt"),
            Accept::Confirm(vec![dir.join("a.txt")])
        );
        assert_eq!(
            accept(&request, &dir, &dir, &[], "sub"),
            Accept::Enter(dir.join("sub"))
        );
        assert!(matches!(
            accept(&request, &dir, &dir, &[], "  "),
            Accept::Invalid(_)
        ));
        assert!(matches!(
            accept(&request, &dir, &dir, &[], "missing/x.txt"),
            Accept::Invalid(_)
        ));
        assert_eq!(
            accept(&request, &dir, &dir, &[], "sub/x.txt"),
            Accept::Done(vec![dir.join("sub/x.txt")])
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn request_defaults() {
        let request = ChooserRequest::open()
            .filter(Filter::new("A").glob("*.a"))
            .filter(Filter::new("B").glob("*.b"))
            .current_filter(7)
            .current_file("/x/y/z.txt");
        assert_eq!(request.initial_filter(), Some(0));
        assert_eq!(request.initial_folder(Path::new("/h")), Path::new("/x/y"));
        assert_eq!(request.initial_name(), "z.txt");
        assert_eq!(request.accept_text(), "Open");
    }

    #[test]
    fn effective_filters_follow_the_request() {
        let none = ChooserRequest::open();
        assert_eq!(none.effective_filters()[0].name, "All files");
        assert_eq!(none.effective_filters().len(), 7);
        assert_eq!(none.initial_filter(), Some(0));

        let own = ChooserRequest::open()
            .filter_spec("Pictures", "png jpg")
            .filter(Filter::new("All files"));
        assert_eq!(own.effective_filters().len(), 2);
        assert!(
            own.clone()
                .presets(false)
                .effective_filters()
                .iter()
                .all(|f| f.name != "Audio")
        );
        let with_presets = own.clone().presets(true);
        let names: Vec<String> = with_presets
            .effective_filters()
            .into_iter()
            .map(|f| f.name)
            .collect();
        // "All files" is not repeated; the rest follow the request's own.
        assert_eq!(
            names,
            [
                "Pictures",
                "All files",
                "Images",
                "Audio",
                "Video",
                "Documents",
                "Archives",
                "Text"
            ]
        );
        assert_eq!(
            with_presets.clone().current_filter(3).initial_filter(),
            Some(3)
        );
        assert_eq!(with_presets.current_filter(99).initial_filter(), Some(0));
    }
}
