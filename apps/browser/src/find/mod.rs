//! Find in page.

mod bar;

pub use bar::FindBar;

/// The search of the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Find {
    pub open: bool,
    pub text: String,
    pub match_case: bool,
    pub whole_words: bool,
    pub highlight_all: bool,
    /// `(current, total)` of the last search; `None` before one.
    pub result: Option<(u32, u32)>,
}

impl Find {
    /// The search found nothing.
    pub fn not_found(&self) -> bool {
        !self.text.is_empty() && self.result.is_some_and(|(_, total)| total == 0)
    }
}

#[cfg(test)]
mod tests;
