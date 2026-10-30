//! The shapes the registry is made of.
//!
//! An [`Entry`] is one thing bingux-ui exports (a control, a container, a
//! token family, the icon set). It names the Rust items it shows in `covers`,
//! which is what `scripts/taxonomy-check.py` and the crate's tests compare
//! with bingux-ui's public surface. Its body is a list of [`Spec`]s, one per
//! state, or a custom view for a page that is not a list of states (the icon
//! set with its search field).

use bingux_ui::icons::Icon;
use freya::prelude::Element;
use std::sync::LazyLock;

/// The sidebar sections, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Foundations,
    Icons,
    Controls,
    Containers,
    Navigation,
    Data,
    Files,
    Inspector,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Foundations,
        Category::Icons,
        Category::Controls,
        Category::Containers,
        Category::Navigation,
        Category::Data,
        Category::Files,
        Category::Inspector,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::Foundations => "Foundations",
            Category::Icons => "Icons",
            Category::Controls => "Controls",
            Category::Containers => "Containers",
            Category::Navigation => "Navigation",
            Category::Data => "Data",
            Category::Files => "Files",
            Category::Inspector => "Inspector",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Category::Foundations => "foundations",
            Category::Icons => "icons",
            Category::Controls => "controls",
            Category::Containers => "containers",
            Category::Navigation => "navigation",
            Category::Data => "data",
            Category::Files => "files",
            Category::Inspector => "inspector",
        }
    }

    /// The sidebar glyph of every page in the section.
    pub fn icon(self) -> Icon {
        match self {
            Category::Foundations => Icon::Appearance,
            Category::Icons => Icon::Applications,
            Category::Controls => Icon::PreferencesSystem,
            Category::Containers => Icon::Display,
            Category::Navigation => Icon::SidebarShow,
            Category::Data => Icon::Computer,
            Category::Files => Icon::Folder,
            Category::Inspector => Icon::EmblemSystem,
        }
    }
}

/// How a state is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A real component with its own state: hover it, press it, Tab to it.
    Live,
    /// A state the component is told to show by its props (disabled, error,
    /// loading, selected, empty, long text), so it is always on screen.
    Forced,
}

/// The room a specimen gets in a stage: its width at desktop density and at
/// phone density, and whether it has a fixed height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frame {
    /// A control or a group of rows: as wide as a settings page column.
    Fit,
    /// A wide specimen: a chart, a table, a list.
    Wide,
    /// A whole view with a fixed height: a split view, a file view.
    Page(f32),
}

impl Frame {
    /// Width in logical pixels at regular and at compact density.
    pub fn width(self, compact: bool) -> f32 {
        match (self, compact) {
            (Frame::Fit, false) => 440.0,
            (Frame::Fit, true) => 340.0,
            (Frame::Wide | Frame::Page(_), false) => 720.0,
            (Frame::Wide | Frame::Page(_), true) => 390.0,
        }
    }

    pub fn height(self) -> Option<f32> {
        match self {
            Frame::Page(height) => Some(height),
            _ => None,
        }
    }
}

/// One state of one component, built fresh in every stage that shows it.
#[derive(Clone, Copy)]
pub struct Spec {
    pub label: &'static str,
    pub kind: Kind,
    pub frame: Frame,
    /// Builds the demo. It runs inside a component, so it may use hooks
    /// (`use_state`, `use_overlays`).
    pub build: fn() -> Element,
    /// Snapshot only: a script for `Snapshot::run_script` that puts the
    /// specimen in a state a still image cannot reach by itself (a menu open,
    /// a dialog up). Empty for no extra capture.
    pub script: &'static str,
}

impl Spec {
    pub const fn live(label: &'static str, build: fn() -> Element) -> Self {
        Self {
            label,
            kind: Kind::Live,
            frame: Frame::Fit,
            build,
            script: "",
        }
    }

    pub const fn forced(label: &'static str, build: fn() -> Element) -> Self {
        Self {
            label,
            kind: Kind::Forced,
            frame: Frame::Fit,
            build,
            script: "",
        }
    }

    pub const fn wide(mut self) -> Self {
        self.frame = Frame::Wide;
        self
    }

    pub const fn page(mut self, height: f32) -> Self {
        self.frame = Frame::Page(height);
        self
    }

    pub const fn script(mut self, script: &'static str) -> Self {
        self.script = script;
        self
    }
}

/// What an entry shows.
#[derive(Clone, Copy)]
pub enum Body {
    /// One row of stages per state.
    States(&'static [Spec]),
    /// A view of its own, built once on the page.
    Custom(fn() -> Element),
}

/// One thing bingux-ui exports, with how to see it.
pub struct Entry {
    pub name: &'static str,
    pub category: Category,
    /// The sidebar page the entry is listed on, within its category.
    pub page: &'static str,
    /// The Rust API in one line.
    pub api: &'static str,
    /// The heading anchor in docs/bingux-ui.md, without the `#`.
    pub doc: &'static str,
    /// The bingux-ui items this entry shows: struct, enum and trait names,
    /// free functions and constants. `scripts/taxonomy-check.py` fails when a
    /// public item is in neither a `covers` list nor `coverage-allowlist.txt`.
    pub covers: &'static [&'static str],
    pub body: Body,
}

/// A sidebar page: the entries that share a `page` in one category.
pub struct PageInfo {
    pub category: Category,
    pub title: &'static str,
    pub entries: Vec<&'static Entry>,
}

/// The sidebar pages in registry order.
pub fn pages() -> &'static [PageInfo] {
    static PAGES: LazyLock<Vec<PageInfo>> = LazyLock::new(|| {
        let mut pages: Vec<PageInfo> = Vec::new();
        for category in Category::ALL {
            for entry in crate::registry::ENTRIES
                .iter()
                .filter(|entry| entry.category == category)
            {
                match pages
                    .iter_mut()
                    .find(|page| page.category == category && page.title == entry.page)
                {
                    Some(page) => page.entries.push(entry),
                    None => pages.push(PageInfo {
                        category,
                        title: entry.page,
                        entries: vec![entry],
                    }),
                }
            }
        }
        pages
    });
    &PAGES
}

/// Index of a page in [`pages`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageId(pub usize);

impl PageId {
    pub fn info(self) -> &'static PageInfo {
        &pages()[self.0]
    }

    pub fn all() -> Vec<PageId> {
        (0..pages().len()).map(PageId).collect()
    }

    /// `controls/buttons`: the category and the page title as address
    /// segments.
    pub fn segments(self) -> [String; 2] {
        let info = self.info();
        [info.category.slug().to_string(), slug(info.title)]
    }

    pub fn from_segments(segments: &[String]) -> Option<PageId> {
        let [category, page] = segments else {
            return None;
        };
        (0..pages().len()).map(PageId).find(|id| {
            let [c, p] = id.segments();
            &c == category && &p == page
        })
    }
}

/// A page title as one address segment: lower case, words joined by `-`.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}
