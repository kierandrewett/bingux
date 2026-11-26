//! The tabs of a window: the list, what each holds, the strip that shows them
//! and the list a phone shows instead.

mod list;
mod sheet;
#[cfg(feature = "snapshot")]
mod snapshot;
mod strip;
mod tab;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;

pub use list::{Place, TabList};
pub use sheet::TabsSheet;
pub use strip::Strip;
pub use tab::{Page, Tab, TabKind};
