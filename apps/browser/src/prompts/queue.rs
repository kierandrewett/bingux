//! The questions of one tab, addressed by the id the engine gave each.
//!
//! A question waits for the person for as long as the page waits for the
//! answer. It leaves the queue in one of four ways: the person answers it, the
//! engine ends it itself (its deadline passes, or the page goes on), the
//! content process goes away, or the engine does. Every answer names the
//! question it is for, so an answer for one that has already left answers
//! nothing, and never the question that happens to be first now.

use std::collections::VecDeque;

use bingux_ui::web_view::RequestId;

use super::Prompt;

/// The questions of one tab, oldest first. The first is the one on screen.
///
/// Invariant: no two questions have the same id (the engine numbers them).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prompts(VecDeque<Prompt>);

impl Prompts {
    /// A question arrived. It waits behind the ones before it.
    pub fn push(&mut self, prompt: Prompt) {
        self.0.push_back(prompt);
    }

    /// The question on screen: the oldest that is still waiting.
    pub fn front(&self) -> Option<&Prompt> {
        self.0.front()
    }

    /// The question `id`, if it is still waiting.
    pub fn get(&self, id: RequestId) -> Option<&Prompt> {
        self.0.iter().find(|prompt| prompt.id() == id)
    }

    /// How many questions wait.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether nothing waits.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The person answers question `id`: it leaves the queue. `None` when it
    /// is not there, because the engine ended it first. The caller must then
    /// send no answer, since the engine has already given the page its default.
    pub fn take(&mut self, id: RequestId) -> Option<Prompt> {
        let at = self.0.iter().position(|prompt| prompt.id() == id)?;
        self.0.remove(at)
    }

    /// The engine ended question `id` itself. True when it was waiting.
    pub fn closed(&mut self, id: RequestId) -> bool {
        self.take(id).is_some()
    }

    /// Nothing can be answered any more: the content process went away, the
    /// engine failed, or the tab closed. The page has the engine's defaults.
    pub fn drop_all(&mut self) {
        self.0.clear();
    }
}
