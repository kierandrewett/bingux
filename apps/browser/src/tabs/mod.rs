//! The tabs of a window: the list, what each holds, the strip that shows them
//! and the list a phone shows instead.

mod list;
mod sheet;
mod strip;
mod tab;

pub use list::{Place, TabList};
pub use sheet::open_tabs;
pub use strip::{icon_of, strip};
pub use tab::{Page, Tab, TabKind};
