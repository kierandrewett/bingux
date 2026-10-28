//! [`ErrorBoundary`]: a component whose subtree may fail without taking the
//! window with it.
//!
//! The runtime catches a panic in a component's render ([`freya_core::guard`])
//! and hands the failure to the nearest boundary above it. The boundary swaps
//! its content for an error card (the message, the component path, Retry and
//! Copy details) and everything outside it keeps running. Retry mounts the
//! content again under a new key: a new scope, so the content starts with fresh
//! hook state and nothing of the failed run is reused.
//!
//! `bingux-ui` installs boundaries by construction: around the window's root,
//! every page, every popup and sheet body and every dialog body. An app adds
//! its own with [`error_boundary`] where a part of a page may fail on its own.

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use freya::prelude::*;
use freya_core::guard::{Failure, NoticeSink, RenderSink};

use super::log::{display_path, short_name};
use crate::component::ComponentName;
use crate::controls::button::{action_button, row_button};
use crate::group::centered_caption;
use crate::icons::{self, Icon};
use crate::shell::overlays::Overlays;
use crate::shell::status_page;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, reduced_motion_from_env};
use crate::typography::{BODY, CAPTION, HEADING, Ink};

/// How a boundary draws its error card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// The whole window: a centred message on the window background.
    Window,
    /// In the flow of a page, a popup or a sheet: a card with the message and
    /// both actions.
    Block,
    /// One row of a list or a group: a single line and a Retry button.
    Row,
}

/// What a boundary shows.
#[derive(Clone)]
enum Content {
    /// An element built by the caller. Panics in the components below it are
    /// caught; a panic in the code that built it belongs to the caller's own
    /// boundary.
    Built(Element),
    /// A closure the boundary calls inside its own scope, so a panic while
    /// building (a hook out of order, an `unwrap`) is caught here too.
    Build {
        build: Rc<dyn Fn() -> Element>,
        /// Set by [`ErrorBoundary::built_from`]: `build` is a function of this
        /// value alone, so two builds with the same value are the same.
        from: Option<u64>,
    },
}

impl PartialEq for Content {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Content::Built(a), Content::Built(b)) => a == b,
            (
                Content::Build { from: Some(a), .. },
                Content::Build { from: Some(b), .. },
            ) => a == b,
            (Content::Build { build: a, .. }, Content::Build { build: b, .. }) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// A subtree that may fail on its own. See the [module](self).
#[derive(Clone, PartialEq)]
pub struct ErrorBoundary {
    name: String,
    content: Content,
    look: Look,
    key: Option<u64>,
}

/// A boundary around `content`. `name` is what a person calls it: the page's
/// title, "Popup", "Switches".
pub fn error_boundary(name: impl Into<String>, content: impl Into<Element>) -> ErrorBoundary {
    ErrorBoundary {
        name: name.into(),
        content: Content::Built(content.into()),
        look: Look::Block,
        key: None,
    }
}

/// A boundary around what `build` returns. `build` runs inside the boundary's
/// own scope: hooks it calls belong to the content, and a panic in it is caught
/// by this boundary.
pub fn error_boundary_with(
    name: impl Into<String>,
    build: impl Fn() -> Element + 'static,
) -> ErrorBoundary {
    ErrorBoundary {
        name: name.into(),
        content: Content::Build {
            build: Rc::new(build),
            from: None,
        },
        look: Look::Block,
        key: None,
    }
}

/// The window's boundary: `root` is the whole window's content, shown full
/// window when it fails.
pub(crate) fn window_boundary(name: impl Into<String>, root: Rc<dyn Fn() -> Element>) -> ErrorBoundary {
    ErrorBoundary {
        name: name.into(),
        content: Content::Build { build: root, from: None },
        look: Look::Window,
        key: None,
    }
}

impl ErrorBoundary {
    /// How the error card looks. The default is [`Look::Block`].
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }

    /// Declare that the closure given to [`error_boundary_with`] depends on
    /// nothing but `from`: a parent that renders again with the same value does
    /// not rebuild the content. Without it, a new closure means a rebuild.
    pub fn built_from(mut self, from: impl Hash) -> Self {
        if let Content::Build { from: slot, .. } = &mut self.content {
            let mut hasher = std::hash::DefaultHasher::new();
            from.hash(&mut hasher);
            *slot = Some(hasher.finish());
        }
        self
    }

    /// Mount a new boundary, with the error cleared, when `key` changes. A
    /// page's boundary is keyed by the page, so leaving a failed page and
    /// coming back starts clean.
    pub fn key(mut self, key: impl Hash) -> Self {
        let mut hasher = std::hash::DefaultHasher::new();
        key.hash(&mut hasher);
        self.key = Some(hasher.finish());
        self
    }
}

/// What a boundary remembers: how many times it was retried, and the failure it
/// is showing, if any.
#[derive(Clone, Default)]
struct Slot {
    epoch: u64,
    failure: Option<Rc<Failure>>,
    /// The standard card failed to draw (no tokens, no focus core): use the
    /// plain one.
    bare: bool,
}

impl Component for ErrorBoundary {
    fn render_key(&self) -> DiffKey {
        match self.key {
            Some(key) => DiffKey::U64(key),
            None => self.default_key(),
        }
    }

    fn render(&self) -> impl IntoElement {
        let slot = use_state(Slot::default);
        // Failures from the content, anywhere below, arrive here. The first one
        // wins; a failure while the card is up is the card's own, and the plain
        // card takes over.
        use_provide_context(move || {
            RenderSink(Rc::new(move |failure: Failure| {
                let mut slot = slot;
                let (failed, bare) = {
                    let current = slot.peek();
                    (current.failure.is_some(), current.bare)
                };
                if !failed {
                    slot.write().failure = Some(Rc::new(failure));
                } else if !bare && failure.path.iter().any(|name| name.ends_with("::RichCard")) {
                    slot.write().bare = true;
                }
            }))
        });
        // Event handlers, effects and tasks below tell the user with a toast.
        use_provide_context(|| NoticeSink(Rc::new(notify_user)));

        let current = slot.read().clone();
        let retry = EventHandler::new(move |()| {
            let mut slot = slot;
            let mut slot = slot.write();
            slot.epoch += 1;
            slot.failure = None;
            slot.bare = false;
        });
        match current.failure {
            None => BoundaryBody {
                content: self.content.clone(),
                epoch: current.epoch,
            }
            .into_element(),
            Some(failure) if current.bare => BareCard {
                name: self.name.clone(),
                failure: FailureRef(failure),
                retry,
            }
            .into_element(),
            Some(failure) => RichCard {
                name: self.name.clone(),
                look: self.look,
                failure: FailureRef(failure),
                retry,
            }
            .into_element(),
        }
    }
}

/// The content, in a scope of its own that a retry replaces: the key is the
/// retry count, so a retry mounts a new scope with no hook state from the
/// failed run.
#[derive(PartialEq)]
struct BoundaryBody {
    content: Content,
    epoch: u64,
}

impl Component for BoundaryBody {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.epoch)
    }

    fn render(&self) -> impl IntoElement {
        match &self.content {
            Content::Built(element) => element.clone(),
            Content::Build { build, .. } => build(),
        }
    }
}

#[derive(Clone)]
struct FailureRef(Rc<Failure>);

impl PartialEq for FailureRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The first line of a panic message, cut to `limit` characters.
fn headline(message: &str, limit: usize) -> String {
    let line = message.lines().next().unwrap_or("").trim();
    if line.chars().count() <= limit {
        line.to_string()
    } else {
        let mut cut: String = line.chars().take(limit).collect();
        cut.push('…');
        cut
    }
}

/// Copy `details` to the clipboard and say so.
fn copy_details(details: &str, overlays: Option<Overlays>) {
    let copied = freya::clipboard::Clipboard::set(details.to_string()).is_ok();
    if let Some(overlays) = overlays {
        overlays.toast(if copied {
            "Details copied."
        } else {
            "The details could not be copied."
        });
    }
}

/// The toast for an event handler, effect or task that failed: what failed and
/// a button that copies the details.
fn notify_user(failure: &Failure) {
    let Some(overlays) = try_consume_context::<Overlays>() else {
        return;
    };
    let what = if failure.component.is_empty() {
        failure.kind.label().to_string()
    } else {
        format!("{} ({})", short_name(failure.component), failure.kind.label())
    };
    let details = failure.details();
    overlays.toast_with_action(
        format!("Something went wrong in {what}: {}", headline(&failure.message, 90)),
        "Copy details",
        move |()| copy_details(&details, Some(overlays)),
    );
}

/// The palette for a card that may sit above the window's own tokens. The root
/// context can name tokens whose owner is gone: the window's root provides them,
/// and when it is the part that failed its state was dropped with it. A dropped
/// state is skipped, not read.
fn tokens_or_default() -> Tokens {
    let provided = try_consume_root_context::<SharedTokens>()
        .and_then(|shared| shared.0.try_read().map(|tokens| *tokens));
    provided.unwrap_or_else(|| {
        Tokens::new(DEFAULT_ACCENT, Scheme::PreferDark, reduced_motion_from_env())
    })
}

/// The text under the message: where it failed and which line panicked.
fn where_line(failure: &Failure) -> String {
    let path = display_path(&failure.path);
    match (&failure.location, path.is_empty()) {
        (Some(location), false) => format!("{path} · {location}"),
        (Some(location), true) => location.clone(),
        (None, _) => path,
    }
}

/// The card, drawn with the standard components.
#[derive(PartialEq)]
struct RichCard {
    name: String,
    look: Look,
    failure: FailureRef,
    retry: EventHandler<()>,
}

impl Component for RichCard {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let failure = &self.failure.0;
        let overlays = try_consume_context::<Overlays>();
        let details = failure.details();
        let copy = EventHandler::new(move |()| copy_details(&details, overlays));
        let title = format!("{} ran into a problem", self.name);
        let message = headline(&failure.message, 240);
        let place = where_line(failure);
        let alert = format!("{title}: {message}");
        match self.look {
            Look::Window => rect()
                .component("ErrorCard")
                .a11y_alt(alert)
                .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                .child(
                    status_page(Icon::DialogError, title)
                        .description(message)
                        .action("Retry", self.retry.clone())
                        .child(
                            rect()
                                .tw(&tokens, &[flex_col(), items(Align::Center), gap(S2), mt(S3), w_full()])
                                .child(centered_caption(place))
                                .child(action_button("Copy details", copy).flat()),
                        ),
                )
                .into_element(),
            Look::Block => rect()
                .component("ErrorCard")
                .a11y_alt(alert)
                .tw(&tokens, BOX)
                .tw(&tokens, &[bg(Paint::DangerSoft)])
                .child(
                    rect()
                        .tw(&tokens, BOX_BODY)
                        .child(
                            rect()
                                .tw(&tokens, &[flex_row(), items(Align::Center), gap(S3), w_full()])
                                .child(icons::icon(
                                    Icon::DialogError,
                                    IconSize::Row,
                                    Ink::Danger.color(&tokens),
                                ))
                                .child(HEADING.in_ink(Ink::Text).label(&tokens, title)),
                        )
                        .child(BODY.in_ink(Ink::Text).paragraph(&tokens, message))
                        .child(CAPTION.in_ink(Ink::Muted).paragraph(&tokens, place))
                        .child(
                            rect()
                                .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
                                .child(action_button("Retry", self.retry.clone()).suggested())
                                .child(action_button("Copy details", copy).flat()),
                        ),
                )
                .into_element(),
            Look::Row => rect()
                .component("ErrorCard")
                .a11y_alt(alert)
                .tw(&tokens, ROW)
                .tw(&tokens, &[bg(Paint::DangerSoft)])
                .child(icons::icon(
                    Icon::DialogError,
                    IconSize::Row,
                    Ink::Danger.color(&tokens),
                ))
                .child(
                    BODY.in_ink(Ink::Danger)
                        .paragraph(&tokens, format!("{}: {}", self.name, headline(&failure.message, 90)))
                        .max_lines(1)
                        .width(Size::flex(1.0)),
                )
                .child(row_button("Retry", self.retry.clone()))
                .into_element(),
        }
    }
}

/// The card when the standard one cannot be drawn, because the tokens or the
/// focus core a standard control reads were never provided (the window's root
/// failed before it set them up). It reads nothing from the window and has no
/// hooks, so nothing in it can fail the way the standard card did.
#[derive(PartialEq)]
struct BareCard {
    name: String,
    failure: FailureRef,
    retry: EventHandler<()>,
}

/// A pressable label with no hooks and no context.
fn bare_button(tokens: &Tokens, text: &str, on_press: EventHandler<()>) -> Rect {
    let on_key = on_press.clone();
    rect()
        .tw(tokens, BUTTON)
        .tw(
            tokens,
            &[
                flex_row(),
                items(Align::Center),
                justify(Align::Center),
                px(S3),
                bg(Paint::Control),
            ],
        )
        .a11y_focusable(true)
        .a11y_role(AccessibilityRole::Button)
        .on_press(move |event: Event<PressEventData>| {
            event.stop_propagation();
            on_press.call(());
        })
        .on_key_down(move |event: Event<KeyboardEventData>| {
            if event.key == Key::Named(NamedKey::Enter) || event.key == Key::Character(" ".into()) {
                event.stop_propagation();
                on_key.call(());
            }
        })
        .child(BODY.in_ink(Ink::Text).label(tokens, text.to_string()).max_lines(1))
}

impl Component for BareCard {
    fn render(&self) -> impl IntoElement {
        let tokens = tokens_or_default();
        let failure = &self.failure.0;
        let details = failure.details();
        let copy = EventHandler::new(move |()| copy_details(&details, None));
        let title = format!("{} ran into a problem", self.name);
        rect()
            .component("ErrorCard")
            .a11y_alt(format!("{title}: {}", headline(&failure.message, 240)))
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    flex_col(),
                    center(),
                    p(S8),
                    gap(S3),
                    bg(Paint::Bg),
                ],
            )
            .child(HEADING.in_ink(Ink::Text).label(&tokens, title))
            .child(
                BODY.in_ink(Ink::Muted)
                    .paragraph(&tokens, headline(&failure.message, 240))
                    .text_align(TextAlign::Center),
            )
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), gap(S2), mt(S3)])
                    .child(bare_button(&tokens, "Retry", self.retry.clone()))
                    .child(bare_button(&tokens, "Copy details", copy)),
            )
    }
}
