//! The engine, the one thing behind the browser's pages.

mod status;

use std::sync::Arc;

use bingux_ui::web_view::BrowserBackend;

pub use status::Status;

/// The engine, comparable by identity so it can be a prop.
#[derive(Clone)]
pub struct Engine(pub Arc<dyn BrowserBackend>);

impl PartialEq for Engine {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
