//! Component names for the inspector.
//!
//! Freya keeps no record of which component produced an element, so the
//! inspector would show every container as `rect`. A component names its root
//! element with [`ComponentName::component`]; the inspector reads the name
//! back from the element's accessibility node (`class_name`) and shows it in
//! the Elements tree and the breadcrumb. Any application component can call it,
//! not only the ones in this crate.
//!
//! ```ignore
//! rect().component("SidebarRow").child(label)
//! ```
//!
//! The name does not change layout or style. It is stored as the element's
//! accessibility class name, which the inspector reads back.

use freya::prelude::*;

/// Adds [`component`](ComponentName::component) to every element builder that
/// carries accessibility data.
pub trait ComponentName: AccessibilityExt + Sized {
    /// Records `name` as the component that built this element.
    fn component(self, name: &'static str) -> Self {
        self.a11y_builder(move |builder| builder.set_class_name(name))
    }
}

impl<T: AccessibilityExt> ComponentName for T {}
