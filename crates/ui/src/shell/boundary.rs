//! Design boundaries (`design/boundaries.md`): which controls belong to the
//! chrome and which to the content, held by the types.
//!
//! A control that chooses what the content shows (a mode, a view, a page of a
//! set) is chrome. Such a control ([`ViewSwitcher`], [`PageDots`]) has no
//! `Into<Element>`: it cannot be a `.child(..)` of a content tree, and the
//! shell's content slots reject it with a message that names its slot.
//!
//! ```
//! use bingux_ui::prelude::*;
//! use freya::prelude::*;
//! let modes = vec![view("basic", "Basic"), view("scientific", "Scientific")];
//! let header = HeaderBar::new("Calculator").switcher(view_switcher(modes, "basic", |_| {}));
//! let _window = Screen::new(header).content(rect());
//! ```
//!
//! A view switcher is not content:
//!
//! ```compile_fail
//! use bingux_ui::prelude::*;
//! use freya::prelude::*;
//! let modes = vec![view("basic", "Basic")];
//! let _window = Screen::new(HeaderBar::new("Calculator"))
//!     .content(view_switcher(modes, "basic", |_| {}));
//! ```
//!
//! and cannot be a child of one:
//!
//! ```compile_fail
//! use bingux_ui::prelude::*;
//! use freya::prelude::*;
//! let modes = vec![view("basic", "Basic")];
//! let _body = rect().child(view_switcher(modes, "basic", |_| {}));
//! ```

use std::cell::RefCell;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::carousel::{PageDots, PageDotsView};
use crate::controls::segmented::segmented;
use crate::style::*;

mod sealed {
    pub trait Chrome {}
    pub trait Content {}
}

/// Something a chrome slot takes: any element (a button, a menu, a field), or
/// a chrome-only control such as a [`ViewSwitcher`].
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot go in a chrome slot",
    label = "not an element or a chrome control",
    note = "header, strip and sidebar slots take elements and chrome controls (see design/boundaries.md)"
)]
pub trait ChromeItem: sealed::Chrome {
    #[doc(hidden)]
    fn into_chrome(self) -> Element;
}

/// Something a content slot takes: any element. A control that chooses what
/// the content shows is not one.
#[diagnostic::on_unimplemented(
    message = "`{Self}` chooses what the content shows, so it cannot be content",
    label = "this control belongs to the chrome",
    note = "put a view switcher in `HeaderBar::switcher(..)` and page dots in `Screen::pager(..)`: a control that switches, filters, sorts or scales the content sits outside the content boundary (design/boundaries.md)"
)]
pub trait ContentItem: sealed::Content {
    #[doc(hidden)]
    fn into_content(self) -> Element;
}

impl<T: Into<Element>> sealed::Chrome for T {}
impl<T: Into<Element>> ChromeItem for T {
    fn into_chrome(self) -> Element {
        self.into()
    }
}
impl<T: Into<Element>> sealed::Content for T {}
impl<T: Into<Element>> ContentItem for T {
    fn into_content(self) -> Element {
        self.into()
    }
}

impl sealed::Chrome for ViewSwitcher {}
impl ChromeItem for ViewSwitcher {
    fn into_chrome(self) -> Element {
        SwitcherRail {
            switcher: self,
            menu: false,
        }
        .into()
    }
}

impl sealed::Chrome for PageDots {}
impl ChromeItem for PageDots {
    fn into_chrome(self) -> Element {
        PageDotsView { dots: self }.into()
    }
}

/// A view switcher: a few views of one thing (Basic and Scientific; Day,
/// Week and Month). It chooses what the content shows, so it lives in the
/// chrome: give it to [`HeaderBar::switcher`](crate::shell::HeaderBar::switcher),
/// or to [`use_page_switcher`](crate::shell::use_page_switcher) on a routed
/// page.
///
/// Where it draws:
///
/// | Window | Views | Drawn as |
/// | ------ | ----- | -------- |
/// | Desktop | any | A segmented control in the centre of the header |
/// | Phone, in a [`Screen`](crate::shell::Screen) | 2 to 5, each with an icon | The bottom navigation bar, at the thumb |
/// | Phone, otherwise | more than 5, or without icons | A menu of the views in the title's place |
///
/// A phone switcher with icons on some views and not others is reported in a
/// debug build ([`violations`]): give every view an icon, or none.
#[derive(Clone, PartialEq)]
pub struct ViewSwitcher {
    pub(crate) views: Vec<View>,
    pub(crate) selected: String,
    pub(crate) on_select: EventHandler<String>,
}

/// One view of a [`ViewSwitcher`]: its key, its short label and, for a phone's
/// bottom bar, its icon. Build with [`view`], or pass `(key, label)`.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) icon: Option<crate::icons::Icon>,
}

/// A view named `key` and shown as `label` (a word or two: "Scientific").
pub fn view(key: impl Into<String>, label: impl Into<String>) -> View {
    View {
        key: key.into(),
        label: label.into(),
        icon: None,
    }
}

impl View {
    /// The view's icon: what a phone's bottom bar shows above its label.
    pub fn icon(mut self, icon: crate::icons::Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl From<(String, String)> for View {
    fn from((key, label): (String, String)) -> Self {
        view(key, label)
    }
}

/// A switcher over `views` (in order) with `selected` on show. `on_select`
/// gets the key of the view chosen. A view is a [`view`] or `(key, label)`.
pub fn view_switcher(
    views: impl IntoIterator<Item = impl Into<View>>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> ViewSwitcher {
    ViewSwitcher {
        views: views.into_iter().map(Into::into).collect(),
        selected: selected.to_string(),
        on_select: on_select.into(),
    }
}

impl ViewSwitcher {
    /// `(key, label)` of each view, for the segmented rail.
    pub(crate) fn options(&self) -> Vec<(String, String)> {
        self.views
            .iter()
            .map(|view| (view.key.clone(), view.label.clone()))
            .collect()
    }

    /// Whether a phone draws it as the bottom navigation bar: two to five
    /// views, every one with an icon.
    pub(crate) fn fits_bottom_bar(&self) -> bool {
        (2..=5).contains(&self.views.len()) && self.views.iter().all(|view| view.icon.is_some())
    }

    /// Reports, in a debug build, a phone switcher that meant to be a bottom
    /// bar (some views have icons) but cannot be (not all do).
    pub(crate) fn check_icons(&self) {
        let with = self.views.iter().filter(|view| view.icon.is_some()).count();
        if with > 0 && with < self.views.len() {
            report(
                "a ViewSwitcher has icons on some views and not others, so a phone cannot draw \
                 it as the bottom bar: give every view an icon, or none"
                    .to_string(),
            );
        }
    }
}

/// Provided by a [`Screen`](crate::shell::Screen) on a phone: it draws a
/// header's switcher as the bottom bar, so the header leaves it out.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct BottomBarHost;

/// Which side of the content boundary a slot is on. The shell provides it to
/// each slot it draws, so a chrome-only control can tell, in a debug build,
/// that it was drawn inside the content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Chrome,
    Content,
}

#[derive(Clone, Copy, PartialEq)]
struct Scope(Region);

/// Draws `child` as part of `region`.
#[derive(PartialEq)]
pub(crate) struct Within {
    pub region: Region,
    pub child: Element,
}

impl Component for Within {
    fn render(&self) -> impl IntoElement {
        let region = self.region;
        use_provide_context(move || Scope(region));
        self.child.clone()
    }
}

/// Wraps `child` so the slots below it know their side of the boundary.
pub(crate) fn within(region: Region, child: impl Into<Element>) -> Element {
    Within {
        region,
        child: child.into(),
    }
    .into()
}

/// The side of the boundary the caller is drawn on, when a shell drew it.
pub fn current_region() -> Option<Region> {
    try_consume_context::<Scope>().map(|scope| scope.0)
}

thread_local! {
    static FOUND: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The chrome-only controls this process drew inside a content slot, one line
/// each. Recorded in debug builds only; always empty in a release build.
pub fn violations() -> Vec<String> {
    FOUND.with(|found| found.borrow().clone())
}

/// Forgets what [`violations`] recorded (between test cases).
pub fn clear_violations() {
    FOUND.with(|found| found.borrow_mut().clear());
}

/// Records `line` once and names it on stderr, in a debug build.
fn report(line: String) {
    if !cfg!(debug_assertions) {
        return;
    }
    FOUND.with(|found| {
        let mut found = found.borrow_mut();
        if !found.contains(&line) {
            eprintln!("[boundary] {line}");
            found.push(line);
        }
    });
}

/// Called by a chrome-only control as it renders. In a debug build, a control
/// under a content slot is recorded and named on stderr once.
pub(crate) fn check_chrome(control: &'static str) {
    if current_region() != Some(Region::Content) {
        return;
    }
    report(format!(
        "{control} is drawn inside the content. It chooses what the content shows, so it \
         belongs in the chrome: HeaderBar::switcher, Screen::pager or use_page_switcher \
         (design/boundaries.md)"
    ));
}

/// How a [`ViewSwitcher`] draws in a header: the segmented track as a tab bar
/// on a desktop, or, with `menu`, a menu of the views in the title's place on a
/// phone (a choice sheet when opened), so the header stays one row.
#[derive(PartialEq)]
pub(crate) struct SwitcherRail {
    pub switcher: ViewSwitcher,
    pub menu: bool,
}

impl Component for SwitcherRail {
    fn render(&self) -> impl IntoElement {
        check_chrome("ViewSwitcher");
        let switcher = &self.switcher;
        if self.menu {
            switcher.check_icons();
            return rect()
                .component("ViewSwitcher")
                .child(crate::controls::dropdown::dropdown(
                    switcher.options(),
                    switcher.selected.clone(),
                    switcher.on_select.clone(),
                ));
        }
        rect().component("ViewSwitcher").child(
            segmented(
                switcher.options(),
                &switcher.selected,
                switcher.on_select.clone(),
            )
            .tabs(),
        )
    }
}

/// A phone's bottom navigation bar: the views of a [`ViewSwitcher`] as icons
/// over short labels, one row at the thumb, the selected view in accent ink
/// and nothing drawn behind it. A [`Screen`](crate::shell::Screen) draws it
/// under its footer.
#[derive(PartialEq)]
pub(crate) struct BottomBar {
    pub switcher: ViewSwitcher,
}

impl Component for BottomBar {
    fn render(&self) -> impl IntoElement {
        check_chrome("ViewSwitcher");
        let tokens = crate::surface::use_tokens();
        let switcher = &self.switcher;
        let mut row = rect()
            .component("BottomBar")
            .a11y_role(AccessibilityRole::TabList)
            .width(Size::fill())
            // About 56 px: a 28 px icon box, the label, and S1 above and below.
            .tw(&tokens, &[flex_row(), grow_children(), px(S2), py(S1)]);
        for view in &switcher.views {
            let selected = view.key == switcher.selected;
            let key = view.key.clone();
            let on_select = switcher.on_select.clone();
            let ink = if selected {
                Paint::Accent
            } else {
                Paint::Muted
            };
            row = row.child(
                rect()
                    .component("BottomBarItem")
                    .a11y_role(AccessibilityRole::Tab)
                    .a11y_focusable(true)
                    .a11y_alt(view.label.clone())
                    .a11y_builder(move |builder| builder.set_selected(selected))
                    .width(Size::flex(1.0))
                    .tw(
                        &tokens,
                        &[
                            flex_col(),
                            items(Align::Center),
                            gap(S0_5),
                            min_w(Dim::Box11),
                        ],
                    )
                    .on_press(move |_| {
                        if !selected {
                            on_select.call(key.clone());
                        }
                    })
                    .child(rect().tw(&tokens, &[h(Dim::Box7), center()]).maybe_child(
                        view.icon.map(|icon| {
                            crate::icons::icon(icon, IconSize::Row, ink.color(&tokens))
                        }),
                    ))
                    .child(
                        crate::typography::CAPTION
                            .in_ink(if selected {
                                crate::typography::Ink::Accent
                            } else {
                                crate::typography::Ink::Muted
                            })
                            .label(&tokens, view.label.clone())
                            .max_lines(1),
                    ),
            );
        }
        row
    }
}
