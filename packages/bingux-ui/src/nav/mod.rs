//! Navigation: a data-driven router every app gets by construction.
//!
//! * [`Location`]: a URI, the unit of "where". Path segments are the
//!   hierarchy, the query is one-off state, the fragment is an anchor in the
//!   page.
//! * [`RouteTable`]: the app's routes as data: URI patterns, titles, icons,
//!   parents.
//! * [`NavModel`]: the stack of locations and its operations (`push`, `pop`,
//!   `pop_to`, `up`, `replace`, `switch_root`, `navigate`, `back`, `forward`),
//!   pure and tested.
//! * [`Router`]: the model in a Freya state; [`use_router`], [`navigate`].
//! * [`BackGesture`], [`Pose`]: predictive back.
//!
//! Back, the header title, the history menu, the window title, search jumps and
//! the inspector are all derived from the one stack. See
//! `design/navigation.md` and the "Navigation" section of `docs/bingux-ui.md`.

pub mod anchor;
pub mod gesture;
pub mod location;
pub mod model;
pub mod pattern;
pub mod router;
pub mod table;

#[cfg(test)]
mod slide_users_tests;

pub use anchor::{Anchor, slug, use_anchor};
pub use gesture::{BackGesture, GestureConfig, Pose};
pub use location::{InvalidLocation, Location};
pub use model::{
    Change, Confirmation, Entry, EntryInfo, HISTORY_LIMIT, HistoryItem, Modal, ModalKind, NavModel,
    Op, Outcome, PopGuard, Snapshot,
};
pub use router::{
    Router, Target, navigate, provide_router, router, try_router, use_entry_id, use_location,
    use_page_title, use_pop_guard, use_retained_state, use_router,
};
pub use table::{Mode, Params, RouteDef, RouteKind, RouteTable, Transition};
