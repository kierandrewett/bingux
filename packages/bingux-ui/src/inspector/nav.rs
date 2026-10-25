//! What the inspector window's tabs share: the active tab and the selected
//! node. Created once by the window root and read with [`use_nav`].

use freya::prelude::*;

/// The three tabs, in header order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Elements,
    Performance,
    Accessibility,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Elements, Tab::Performance, Tab::Accessibility];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Elements => "Elements",
            Tab::Performance => "Performance",
            Tab::Accessibility => "Accessibility",
        }
    }
}

/// The shared state. `Copy`, so closures can capture it.
#[derive(Clone, Copy)]
pub struct Nav {
    pub tab: State<Tab>,
    pub selected: State<Option<u64>>,
}

impl Nav {
    /// Selects `node` and switches to the Elements tab, where it is shown.
    /// The accessibility list calls this when a row is pressed.
    pub fn reveal(mut self, node: u64) {
        self.selected.set(Some(node));
        self.tab.set(Tab::Elements);
    }
}

/// Creates the shared state for the window root.
pub fn provide_nav() -> Nav {
    use_provide_context(|| Nav {
        tab: State::create(Tab::Elements),
        selected: State::create(None),
    })
}

/// The shared state, from any component below the window root.
pub fn use_nav() -> Nav {
    use_consume::<Nav>()
}
