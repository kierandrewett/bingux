//! Which window each inspector watches: the target model, as pure logic.
//!
//! Every inspector instance is bound to one *target* window. The Elements
//! tree, inspect mode, Properties, Performance, Accessibility and the phone
//! preview all act on that window and no other. An application window has
//! depth 0; an inspector over it has depth 1; an inspector over that
//! inspector has depth 2, and so on up to [`MAX_DEPTH`].
//!
//! The registry is generic over the window id so the rules can be tested
//! without a window system. The plugin instantiates it with winit's
//! `WindowId`.
//!
//! * One inspector per target: pressing the shortcut in a window that
//!   already has one closes it.
//! * An inspector never targets its own window: [`Registry::bind`] refuses.
//!   Since a target must exist before an inspector can be opened over it, the
//!   targets form a forest and no cycle can occur.
//! * Closing a window closes the inspectors over it, and theirs in turn.

use std::time::{Duration, Instant};

/// The deepest inspector nesting: an inspector of an inspector of an
/// inspector of an inspector of the application.
pub const MAX_DEPTH: usize = 4;

/// The least time between two snapshots of one target. A window that redraws
/// every frame (a spinner, an animation) must not make its inspector copy
/// the whole tree every frame, and an inspector of that inspector must not
/// follow suit.
pub const CAPTURE_GAP: Duration = Duration::from_millis(100);

/// One inspector instance. Created before its window exists, so the window
/// is unknown until [`Registry::bind`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct InstanceId(u64);

/// Why an inspector was not opened.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The target is already [`MAX_DEPTH`] inspectors deep.
    TooDeep,
}

impl OpenError {
    /// What the user is told.
    pub fn message(&self) -> String {
        match self {
            OpenError::TooDeep => format!("Inspectors nest at most {MAX_DEPTH} deep."),
        }
    }
}

/// What the shortcut does in a window.
#[derive(Debug, PartialEq, Eq)]
pub enum Toggle {
    /// No inspector watches the window: open one.
    Open,
    /// An inspector watches the window: close it.
    Close(InstanceId),
    /// The window cannot be inspected.
    Refuse(OpenError),
}

struct Binding<W> {
    id: InstanceId,
    target: W,
    /// The inspector's own window, once the event loop has created it.
    window: Option<W>,
    depth: usize,
}

/// The inspectors that exist and the windows they watch.
pub struct Registry<W> {
    bindings: Vec<Binding<W>>,
    next: u64,
}

/// What closing a window took with it.
#[derive(Debug, PartialEq, Eq)]
pub struct Closed<W> {
    /// Instances that no longer exist.
    pub removed: Vec<InstanceId>,
    /// Windows of instances whose target went away: still open, to be closed.
    pub close: Vec<W>,
}

impl<W> Default for Registry<W> {
    fn default() -> Self {
        Registry {
            bindings: Vec::new(),
            next: 1,
        }
    }
}

impl<W: Copy + PartialEq> Registry<W> {
    /// Nesting depth of `window`: 0 for a window that is not an inspector's.
    pub fn depth_of(&self, window: W) -> usize {
        self.bindings
            .iter()
            .find(|binding| binding.window == Some(window))
            .map_or(0, |binding| binding.depth)
    }

    /// Whether `window` is an inspector's own window.
    pub fn is_inspector_window(&self, window: W) -> bool {
        self.bindings
            .iter()
            .any(|binding| binding.window == Some(window))
    }

    /// The instance whose own window is `window`.
    pub fn instance_of_window(&self, window: W) -> Option<InstanceId> {
        self.bindings
            .iter()
            .find(|binding| binding.window == Some(window))
            .map(|binding| binding.id)
    }

    /// The inspector watching `target`, if any.
    pub fn inspector_of(&self, target: W) -> Option<InstanceId> {
        self.bindings
            .iter()
            .find(|binding| binding.target == target)
            .map(|binding| binding.id)
    }

    /// Every instance watching `window`. At most one, but callers loop.
    pub fn watching(&self, window: W) -> impl Iterator<Item = InstanceId> + '_ {
        self.bindings
            .iter()
            .filter(move |binding| binding.target == window)
            .map(|binding| binding.id)
    }

    pub fn target_of(&self, id: InstanceId) -> Option<W> {
        self.get(id).map(|binding| binding.target)
    }

    pub fn window_of(&self, id: InstanceId) -> Option<W> {
        self.get(id).and_then(|binding| binding.window)
    }

    pub fn depth(&self, id: InstanceId) -> Option<usize> {
        self.get(id).map(|binding| binding.depth)
    }

    pub fn contains(&self, id: InstanceId) -> bool {
        self.get(id).is_some()
    }

    fn get(&self, id: InstanceId) -> Option<&Binding<W>> {
        self.bindings.iter().find(|binding| binding.id == id)
    }

    /// What the shortcut does when pressed in `window`.
    pub fn toggle_for(&self, window: W) -> Toggle {
        if let Some(id) = self.inspector_of(window) {
            Toggle::Close(id)
        } else if self.depth_of(window) >= MAX_DEPTH {
            Toggle::Refuse(OpenError::TooDeep)
        } else {
            Toggle::Open
        }
    }

    /// Creates an inspector over `target`. The caller launches its window and
    /// then calls [`Registry::bind`].
    pub fn open(&mut self, target: W) -> Result<InstanceId, OpenError> {
        let depth = self.depth_of(target) + 1;
        if depth > MAX_DEPTH {
            return Err(OpenError::TooDeep);
        }
        let id = InstanceId(self.next);
        self.next += 1;
        self.bindings.push(Binding {
            id,
            target,
            window: None,
            depth,
        });
        Ok(id)
    }

    /// Records the inspector's own window. Refuses a window that is the
    /// instance's target (an inspector never watches itself) or an unknown
    /// instance. Binding the same window again is fine.
    pub fn bind(&mut self, id: InstanceId, window: W) -> bool {
        match self.bindings.iter_mut().find(|binding| binding.id == id) {
            Some(binding) if binding.target != window => {
                binding.window = Some(window);
                true
            }
            _ => false,
        }
    }

    /// `window` closed. Its own instance goes, and so does every instance
    /// watching it, and those watching those: their windows are returned to
    /// be closed. Instances that had no window yet are removed too (the
    /// caller closes the window when it appears).
    pub fn close(&mut self, window: W) -> Closed<W> {
        let mut removed = Vec::new();
        let mut close = Vec::new();
        let mut gone = vec![window];
        // Breadth-first: each gone window takes its own inspector, and every
        // inspector watching it.
        while let Some(window) = gone.pop() {
            let mut index = 0;
            while index < self.bindings.len() {
                let binding = &self.bindings[index];
                if binding.window == Some(window) || binding.target == window {
                    let binding = self.bindings.remove(index);
                    removed.push(binding.id);
                    if let Some(own) = binding.window
                        && own != window
                    {
                        close.push(own);
                        gone.push(own);
                    }
                } else {
                    index += 1;
                }
            }
        }
        Closed { removed, close }
    }
}

/// The inspector window's title: it names the window it watches, so a stack
/// of inspectors reads `Inspector: Inspector: Settings`.
pub fn window_title(target_title: &str) -> String {
    format!("Inspector: {target_title}")
}

/// Allows a capture at most once per [`CAPTURE_GAP`].
#[derive(Default)]
pub struct Throttle {
    last: Option<Instant>,
    /// A capture was refused and nothing has been taken since.
    skipped: bool,
}

impl Throttle {
    /// Whether a capture may run at `now`. A refusal is remembered, so the
    /// caller can ask for one more layout once the gap has passed.
    pub fn due(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < CAPTURE_GAP)
        {
            self.skipped = true;
            return false;
        }
        self.last = Some(now);
        self.skipped = false;
        true
    }

    /// Whether the last capture request was refused, so the tree the
    /// inspector shows is older than the window's.
    pub fn stale(&self) -> bool {
        self.skipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows are plain numbers here: the app is 1.
    const APP: u32 = 1;

    fn open_bound(registry: &mut Registry<u32>, target: u32, window: u32) -> InstanceId {
        let id = registry.open(target).expect("opens");
        assert!(registry.bind(id, window));
        id
    }

    #[test]
    fn an_inspector_is_bound_to_one_target() {
        let mut registry = Registry::default();
        let id = open_bound(&mut registry, APP, 10);
        assert_eq!(registry.target_of(id), Some(APP));
        assert_eq!(registry.window_of(id), Some(10));
        assert_eq!(registry.inspector_of(APP), Some(id));
        assert_eq!(registry.watching(APP).collect::<Vec<_>>(), vec![id]);
        assert_eq!(registry.watching(10).count(), 0);
        assert_eq!(registry.depth(id), Some(1));
        assert_eq!(registry.depth_of(10), 1);
        assert_eq!(registry.depth_of(APP), 0);
    }

    #[test]
    fn an_inspector_never_watches_its_own_window() {
        let mut registry = Registry::default();
        let id = registry.open(APP).unwrap();
        // The window the loop made for it cannot be its own target.
        assert!(!registry.bind(id, APP));
        assert_eq!(registry.window_of(id), None);
        assert!(registry.bind(id, 10));
        // Nothing watches window 10 until someone opens over it, so its tree
        // is not captured for itself: only `watching(10)` feeds snapshots.
        assert_eq!(registry.watching(10).count(), 0);
    }

    #[test]
    fn the_shortcut_toggles_per_window() {
        let mut registry = Registry::default();
        assert_eq!(registry.toggle_for(APP), Toggle::Open);
        let first = open_bound(&mut registry, APP, 10);
        assert_eq!(registry.toggle_for(APP), Toggle::Close(first));
        // In the inspector's window the shortcut opens an inspector of it.
        assert_eq!(registry.toggle_for(10), Toggle::Open);
        let second = open_bound(&mut registry, 10, 20);
        assert_eq!(registry.toggle_for(10), Toggle::Close(second));
        assert_eq!(registry.target_of(second), Some(10));
        assert_eq!(registry.depth(second), Some(2));
        assert_eq!(registry.toggle_for(20), Toggle::Open);
    }

    #[test]
    fn nesting_stops_at_the_depth_limit() {
        let mut registry = Registry::default();
        let mut target = APP;
        for level in 1..=MAX_DEPTH {
            let window = 10 * level as u32;
            let id = open_bound(&mut registry, target, window);
            assert_eq!(registry.depth(id), Some(level));
            target = window;
        }
        assert_eq!(
            registry.toggle_for(target),
            Toggle::Refuse(OpenError::TooDeep)
        );
        assert_eq!(registry.open(target), Err(OpenError::TooDeep));
        assert!(OpenError::TooDeep.message().contains(&MAX_DEPTH.to_string()));
    }

    #[test]
    fn closing_the_target_closes_its_inspectors_all_the_way_down() {
        let mut registry = Registry::default();
        let first = open_bound(&mut registry, APP, 10);
        let second = open_bound(&mut registry, 10, 20);
        let third = open_bound(&mut registry, 20, 30);
        let closed = registry.close(APP);
        let mut removed = closed.removed.clone();
        removed.sort();
        assert_eq!(removed, vec![first, second, third]);
        let mut close = closed.close.clone();
        close.sort();
        assert_eq!(close, vec![10, 20, 30]);
        assert!(registry.bindings.is_empty());
    }

    #[test]
    fn closing_an_inspector_keeps_what_it_watched() {
        let mut registry = Registry::default();
        let first = open_bound(&mut registry, APP, 10);
        let second = open_bound(&mut registry, 10, 20);
        // The user closes the outer inspector's window: the one over it goes
        // with it, the application is untouched.
        let closed = registry.close(10);
        let mut removed = closed.removed.clone();
        removed.sort();
        assert_eq!(removed, vec![first, second]);
        assert_eq!(closed.close, vec![20]);
        assert_eq!(registry.inspector_of(APP), None);
        // And closing the innermost leaves the others.
        let mut registry = Registry::default();
        let first = open_bound(&mut registry, APP, 10);
        let second = open_bound(&mut registry, 10, 20);
        let closed = registry.close(20);
        assert_eq!(closed.removed, vec![second]);
        assert!(closed.close.is_empty());
        assert_eq!(registry.inspector_of(APP), Some(first));
        // A window nobody watches changes nothing.
        assert_eq!(
            registry.close(99),
            Closed {
                removed: vec![],
                close: vec![]
            }
        );
    }

    #[test]
    fn an_instance_without_a_window_goes_with_its_target() {
        let mut registry = Registry::default();
        let id = registry.open(APP).unwrap();
        let closed = registry.close(APP);
        assert_eq!(closed.removed, vec![id]);
        assert!(closed.close.is_empty());
        assert!(!registry.contains(id));
        assert!(!registry.bind(id, 10));
    }

    #[test]
    fn two_applications_get_independent_inspectors() {
        let mut registry = Registry::default();
        let a = open_bound(&mut registry, APP, 10);
        let b = open_bound(&mut registry, 2, 11);
        assert_ne!(a, b);
        registry.close(APP);
        assert!(registry.contains(b));
        assert_eq!(registry.inspector_of(2), Some(b));
    }

    #[test]
    fn the_title_names_the_target() {
        assert_eq!(window_title("Settings"), "Inspector: Settings");
        assert_eq!(
            window_title(&window_title("Settings")),
            "Inspector: Inspector: Settings"
        );
    }

    #[test]
    fn captures_are_throttled_and_a_refusal_is_remembered() {
        let mut throttle = Throttle::default();
        let start = Instant::now();
        assert!(throttle.due(start));
        assert!(!throttle.stale());
        assert!(!throttle.due(start + Duration::from_millis(10)));
        assert!(throttle.stale());
        assert!(!throttle.due(start + CAPTURE_GAP - Duration::from_millis(1)));
        assert!(throttle.due(start + CAPTURE_GAP));
        assert!(!throttle.stale());
    }
}
