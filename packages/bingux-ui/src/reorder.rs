//! The arithmetic of reordering a list by hand: where a dragged item would
//! land, how far every other item slides to make room, how a keyboard move
//! maps to a place, and the spring that carries each item to its place.
//!
//! Nothing here draws or reads the window; [`sortable`](crate::controls::sortable)
//! feeds it measured heights and pointer distances and draws what it says.
//! Everything works along one axis, so a horizontal strip can use it as the
//! vertical list does. Distances are in pixels and are measured *in the
//! content*: how far the pointer has moved since the pick-up, less how far the
//! page has scrolled since, so a list that is auto-scrolled under a held
//! pointer needs no special case.
//!
//! # The `on_reorder(from, to)` contract
//!
//! A list that can be reordered never reorders itself. When the person drops
//! an item, or moves it with the keyboard, the control calls `on_reorder`
//! once with the place the item was in (`from`) and the place it now belongs
//! at (`to`), both indexes into the list *before* the move. `to` is where the
//! item ends up: the caller removes the item at `from` and inserts it at `to`
//! (`let item = list.remove(from); list.insert(to, item);`). It is never
//! called with `from == to`, nor with an index past the end, and it is not
//! called when the move is cancelled with Escape. The control keeps the
//! animation running until the caller has had the call; a caller that
//! rebuilds the items in the new order loses nothing, because every item is
//! drawn at its new place from the first frame after the call.

/// How far from the edge of a scroll container, in pixels, a held item starts
/// scrolling it.
pub const EDGE: f32 = 56.0;
/// The scroll speed at the edge itself, in pixels per second.
pub const EDGE_SPEED: f32 = 720.0;
/// How stiff the spring is, as its natural frequency in radians per second. A
/// critically damped spring with this frequency is within 1% of its target
/// after `4.6 / OMEGA` seconds, about 120 ms.
pub const OMEGA: f32 = 38.0;
/// A spring is at rest within this many pixels of its target, and slower than
/// [`REST_SPEED`] pixels per second.
const REST_DISTANCE: f32 = 0.25;
const REST_SPEED: f32 = 12.0;
/// A lift (0 resting, 1 lifted) is at rest within this of its target.
const REST_LIFT: f32 = 0.01;

/// The extents of a column of items, one after the other with the same gap
/// between each pair. Item `i` starts at [`Stack::top`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stack {
    sizes: Vec<f32>,
    gap: f32,
}

impl Stack {
    pub fn new(sizes: Vec<f32>, gap: f32) -> Self {
        Self { sizes, gap }
    }

    pub fn len(&self) -> usize {
        self.sizes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sizes.is_empty()
    }

    pub fn size(&self, index: usize) -> f32 {
        self.sizes.get(index).copied().unwrap_or(0.0)
    }

    /// Where item `index` starts, from the start of the column.
    pub fn top(&self, index: usize) -> f32 {
        self.sizes
            .iter()
            .take(index)
            .map(|size| size + self.gap)
            .sum()
    }

    /// The middle of item `index`.
    pub fn centre(&self, index: usize) -> f32 {
        self.top(index) + self.size(index) / 2.0
    }

    /// The distance every item between two places moves when an item passes
    /// them: its size and one gap.
    pub fn step(&self, index: usize) -> f32 {
        self.size(index) + self.gap
    }

    /// The place item `from` takes when it has been dragged `delta` pixels
    /// from where it started. It moves past a neighbour once its leading edge
    /// (the bottom when it moves down, the top when it moves up) has crossed
    /// the neighbour's middle, which holds for items of any height and does
    /// not flicker between two places, because the test is against the
    /// neighbours' places before anything slid.
    pub fn landing(&self, from: usize, delta: f32) -> usize {
        if from >= self.len() {
            return from;
        }
        let top = self.top(from) + delta;
        let bottom = top + self.size(from);
        let mut to = from;
        if delta > 0.0 {
            while to + 1 < self.len() && self.centre(to + 1) < bottom {
                to += 1;
            }
        } else if delta < 0.0 {
            while to > 0 && self.centre(to - 1) > top {
                to -= 1;
            }
        }
        to
    }

    /// The gap, counted from 0 (before the first item) to `len` (after the
    /// last), that a pointer `at` pixels from the start of the column is in:
    /// the number of items whose middle is above it. For a list that shows a
    /// line where the item would go instead of moving the others.
    pub fn gap_at(&self, at: f32) -> usize {
        (0..self.len()).filter(|&index| self.centre(index) < at).count()
    }

    /// Where every item's place is `offset` from its own when item `from`
    /// is moved to place `to`. Items between the two places slide one step
    /// toward the place `from` leaves; item `from` itself is `offset` from
    /// its own place by the distance to its new one, which is where it comes
    /// to rest when dropped.
    pub fn offsets(&self, from: usize, to: usize) -> Vec<f32> {
        let mut offsets = vec![0.0; self.len()];
        if from >= self.len() || to >= self.len() || from == to {
            return offsets;
        }
        let step = self.step(from);
        if to > from {
            for index in from + 1..=to {
                offsets[index] = -step;
                offsets[from] += self.step(index);
            }
        } else {
            for index in to..from {
                offsets[index] = step;
                offsets[from] -= self.step(index);
            }
        }
        offsets
    }
}

/// Where item `from` ends up when it is dropped into `gap` (see
/// [`Stack::gap_at`]). Dropping in the gap just before or just after itself
/// leaves it where it is.
pub fn destination(from: usize, gap: usize) -> usize {
    if gap > from { gap - 1 } else { gap }
}

/// A key that moves an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    First,
    Last,
}

/// Where an item at `from` goes for `step` in a list of `count`, or `None`
/// when it is already there (the first item pressed Up).
pub fn key_target(count: usize, from: usize, step: Move) -> Option<usize> {
    if from >= count {
        return None;
    }
    let to = match step {
        Move::Up => from.checked_sub(1)?,
        Move::Down => from + 1,
        Move::First => 0,
        Move::Last => count - 1,
    };
    (to < count && to != from).then_some(to)
}

/// How fast to scroll, in pixels per second (negative is up), for a held item
/// whose pointer is at `y` in a container spanning `top` to `bottom`: zero
/// outside the [`EDGE`] bands, growing to [`EDGE_SPEED`] at the edge and
/// beyond it.
pub fn edge_speed(y: f32, top: f32, bottom: f32) -> f32 {
    if bottom - top < 2.0 * EDGE {
        return 0.0;
    }
    let near_top = (top + EDGE - y) / EDGE;
    let near_bottom = (y - (bottom - EDGE)) / EDGE;
    if near_top > 0.0 {
        -EDGE_SPEED * near_top.min(1.5)
    } else if near_bottom > 0.0 {
        EDGE_SPEED * near_bottom.min(1.5)
    } else {
        0.0
    }
}

/// One step of a critically damped spring: `x` with velocity `v`, pulled to
/// `target` for `dt` seconds. It is the exact solution, so a long frame
/// cannot make it overshoot or blow up.
pub fn spring(x: f32, v: f32, target: f32, dt: f32) -> (f32, f32) {
    let offset = x - target;
    let decay = (-OMEGA * dt).exp();
    let carried = v + OMEGA * offset;
    (
        target + (offset + carried * dt) * decay,
        (v - OMEGA * carried * dt) * decay,
    )
}

/// What a drag is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    /// An item is held and follows the pointer.
    Held,
    /// The item has been let go, or moved by key, and everything is sliding
    /// to rest. `commit` is whether the order changes when it is over.
    Settling { commit: bool },
}

/// How the places of the other items show a held item's destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indicate {
    /// The other items slide to make room.
    Slide,
    /// The others stay and a line shows the gap, for a long list.
    Line,
}

/// A drag, from pick-up to rest. The caller steps it once a frame with the
/// time that passed and draws [`Reorder::offset`] and [`Reorder::lift`] for
/// each item.
#[derive(Clone, Debug)]
pub struct Reorder {
    stack: Stack,
    indicate: Indicate,
    phase: Phase,
    from: usize,
    to: usize,
    /// The gap a line shows ([`Indicate::Line`]), counted as [`Stack::gap_at`].
    gap: usize,
    /// The held item's distance from its place, following the pointer.
    delta: f32,
    positions: Vec<f32>,
    speeds: Vec<f32>,
    lift: f32,
    lift_speed: f32,
    target_lift: f32,
}

impl Default for Reorder {
    fn default() -> Self {
        Self {
            stack: Stack::default(),
            indicate: Indicate::Slide,
            phase: Phase::Idle,
            from: 0,
            to: 0,
            gap: 0,
            delta: 0.0,
            positions: Vec::new(),
            speeds: Vec::new(),
            lift: 0.0,
            lift_speed: 0.0,
            target_lift: 0.0,
        }
    }
}

/// What a step of a drag finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Done {
    /// Everything is at rest and the order is unchanged.
    Rest,
    /// Everything is at rest and item `from` belongs at place `to`.
    Moved { from: usize, to: usize },
}

impl Reorder {
    pub fn new(indicate: Indicate) -> Self {
        Self {
            indicate,
            ..Self::default()
        }
    }

    pub fn indicate(&self) -> Indicate {
        self.indicate
    }

    /// Change how the destination shows. Takes effect at the next pick-up;
    /// ignored while something is held or sliding.
    pub fn set_indicate(&mut self, indicate: Indicate) {
        if self.phase == Phase::Idle {
            self.indicate = indicate;
        }
    }

    /// Where the held item starts, from the start of the column.
    pub fn top_of_held(&self) -> f32 {
        self.stack.top(self.from)
    }

    /// Whether anything is held or still sliding.
    pub fn is_busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Whether an item is held by the pointer.
    pub fn is_held(&self) -> bool {
        self.phase == Phase::Held
    }

    /// The held, or just-released, item.
    pub fn held(&self) -> Option<usize> {
        self.is_busy().then_some(self.from)
    }

    /// The place the held item would take if let go now.
    pub fn target(&self) -> usize {
        self.to
    }

    /// The held item is `delta` pixels from its place and the pointer is
    /// `pointer` pixels from the start of the column, for [`Indicate::Line`]:
    /// the line goes in the gap under the pointer and the item would land
    /// there. Returns whether the place it would take changed.
    pub fn drag_to_gap(&mut self, delta: f32, pointer: f32) -> bool {
        if self.phase != Phase::Held {
            return false;
        }
        self.delta = delta;
        self.gap = self.stack.gap_at(pointer);
        let to = destination(self.from, self.gap).min(self.stack.len().saturating_sub(1));
        let changed = to != self.to;
        self.to = to;
        changed
    }

    /// Where the line of [`Indicate::Line`] goes, from the start of the
    /// column, while an item is held and would move.
    pub fn line(&self) -> Option<f32> {
        (self.indicate == Indicate::Line && self.phase == Phase::Held && self.to != self.from)
            .then(|| self.stack.top(self.gap))
    }

    /// How far item `index` is drawn from its place: the held item follows
    /// the pointer, the others slide to make room.
    pub fn offset(&self, index: usize) -> f32 {
        if self.phase == Phase::Held && index == self.from {
            return self.delta;
        }
        self.positions.get(index).copied().unwrap_or(0.0)
    }

    /// How far the held item is lifted: 0 at rest, 1 held.
    pub fn lift(&self) -> f32 {
        self.lift
    }

    /// Pick up item `from` of a column measured as `stack`.
    pub fn pick(&mut self, stack: Stack, from: usize) {
        if from >= stack.len() {
            return;
        }
        self.positions = vec![0.0; stack.len()];
        self.speeds = vec![0.0; stack.len()];
        self.stack = stack;
        self.phase = Phase::Held;
        self.from = from;
        self.to = from;
        self.gap = from;
        self.delta = 0.0;
        self.lift = 0.0;
        self.lift_speed = 0.0;
        self.target_lift = 1.0;
    }

    /// The held item is `delta` pixels from its place. Returns whether the
    /// place it would take changed.
    pub fn drag(&mut self, delta: f32) -> bool {
        if self.phase != Phase::Held {
            return false;
        }
        self.delta = delta;
        let to = self.stack.landing(self.from, delta);
        let changed = to != self.to;
        self.to = to;
        changed
    }

    /// Let the held item go: it slides into its place and everything settles.
    pub fn drop(&mut self) {
        if self.phase != Phase::Held {
            return;
        }
        self.positions[self.from] = self.delta;
        self.speeds[self.from] = 0.0;
        self.phase = Phase::Settling {
            commit: self.to != self.from,
        };
        self.target_lift = 0.0;
        if self.indicate == Indicate::Line {
            // Nothing slid, so there is nothing to carry to a place: the
            // item is where the line was and the order is changed at once.
            self.positions[self.from] = 0.0;
        }
    }

    /// Take the held item back to where it started, as Escape does.
    pub fn cancel(&mut self) {
        if self.phase != Phase::Held {
            return;
        }
        self.positions[self.from] = self.delta;
        self.speeds[self.from] = 0.0;
        self.to = self.from;
        self.phase = Phase::Settling { commit: false };
        self.target_lift = 0.0;
    }

    /// Move item `from` to place `to` by key: the same slide as a drop,
    /// without a lift.
    pub fn nudge(&mut self, stack: Stack, from: usize, to: usize) {
        if from >= stack.len() || to >= stack.len() || from == to {
            return;
        }
        self.pick(stack, from);
        self.target_lift = 0.0;
        self.to = to;
        self.phase = Phase::Settling { commit: true };
    }

    /// Where item `index` is headed.
    fn target_offset(&self, index: usize) -> f32 {
        match self.indicate {
            Indicate::Line => 0.0,
            Indicate::Slide => {
                let settling_commit = matches!(self.phase, Phase::Settling { commit: true });
                let aimed = if matches!(self.phase, Phase::Held) || settling_commit {
                    self.to
                } else {
                    self.from
                };
                self.stack
                    .offsets(self.from, aimed)
                    .get(index)
                    .copied()
                    .unwrap_or(0.0)
            }
        }
    }

    /// Advance by `dt` seconds. `instant` skips the springs: everything is at
    /// its target at once, for reduced motion. Returns what finished, if
    /// anything did.
    pub fn step(&mut self, dt: f32, instant: bool) -> Option<Done> {
        if self.phase == Phase::Idle {
            return None;
        }
        let held = self.phase == Phase::Held;
        let mut resting = true;
        for index in 0..self.positions.len() {
            if held && index == self.from {
                continue;
            }
            let target = self.target_offset(index);
            if instant {
                self.positions[index] = target;
                self.speeds[index] = 0.0;
                continue;
            }
            let (x, v) = spring(self.positions[index], self.speeds[index], target, dt);
            self.positions[index] = x;
            self.speeds[index] = v;
            if (x - target).abs() > REST_DISTANCE || v.abs() > REST_SPEED {
                resting = false;
            }
        }
        if instant {
            self.lift = self.target_lift;
            self.lift_speed = 0.0;
        } else {
            let (lift, speed) = spring(self.lift, self.lift_speed, self.target_lift, dt);
            self.lift = lift;
            self.lift_speed = speed;
            if (lift - self.target_lift).abs() > REST_LIFT {
                resting = false;
            }
        }
        let Phase::Settling { commit } = self.phase else {
            return None;
        };
        if !resting {
            return None;
        }
        let done = if commit && self.to != self.from {
            Done::Moved {
                from: self.from,
                to: self.to,
            }
        } else {
            Done::Rest
        };
        *self = Self::new(self.indicate);
        Some(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three rows of different heights, 8 px apart: tops 0, 48 and 136.
    fn uneven() -> Stack {
        Stack::new(vec![40.0, 80.0, 40.0], 8.0)
    }

    fn even(count: usize) -> Stack {
        Stack::new(vec![56.0; count], 0.0)
    }

    fn settle(reorder: &mut Reorder) -> Done {
        for _ in 0..600 {
            if let Some(done) = reorder.step(1.0 / 60.0, false) {
                return done;
            }
        }
        panic!("a drag never came to rest");
    }

    #[test]
    fn tops_add_up_the_sizes_and_gaps() {
        let stack = uneven();
        assert_eq!(stack.top(0), 0.0);
        assert_eq!(stack.top(1), 48.0);
        assert_eq!(stack.top(2), 136.0);
        assert_eq!(stack.centre(1), 88.0);
    }

    #[test]
    fn an_item_stays_until_its_edge_crosses_a_neighbours_middle() {
        let stack = even(4);
        // Row 0's bottom edge is at 56; row 1's middle is at 84.
        assert_eq!(stack.landing(0, 0.0), 0);
        assert_eq!(stack.landing(0, 27.0), 0);
        assert_eq!(stack.landing(0, 29.0), 1);
        assert_eq!(stack.landing(0, 200.0), 3);
        assert_eq!(stack.landing(3, -29.0), 2);
        assert_eq!(stack.landing(3, -500.0), 0);
    }

    #[test]
    fn variable_heights_move_by_the_neighbours_own_middle() {
        let stack = uneven();
        // Row 0 is 40 high: its bottom edge is at 100 after 60 px, past the
        // middle of the 80 px row (88) but short of the last row's (156).
        assert_eq!(stack.landing(0, 60.0), 1);
        assert_eq!(stack.landing(0, 40.0), 0);
        assert_eq!(stack.landing(0, 117.0), 2);
        // Row 2 moving up passes the tall row's middle once its top is above 88.
        assert_eq!(stack.landing(2, -47.0), 2);
        assert_eq!(stack.landing(2, -49.0), 1);
        assert_eq!(stack.landing(2, -100.0), 1);
        assert_eq!(stack.landing(2, -120.0), 0);
    }

    #[test]
    fn the_landing_place_is_stable_along_a_drag() {
        // Dragging down and back never skips a place or lands twice.
        let stack = uneven();
        let mut last = 0;
        for delta in (0..=200).map(|n| n as f32) {
            let now = stack.landing(0, delta);
            assert!(now >= last && now - last <= 1, "jumped at {delta}");
            last = now;
        }
        assert_eq!(last, 2);
        for delta in (0..=200).rev().map(|n| n as f32) {
            let now = stack.landing(0, delta);
            assert!(last >= now && last - now <= 1, "jumped back at {delta}");
            last = now;
        }
        assert_eq!(last, 0);
    }

    #[test]
    fn a_pointer_is_in_the_gap_after_the_middles_above_it() {
        let stack = even(3);
        assert_eq!(stack.gap_at(-5.0), 0);
        assert_eq!(stack.gap_at(30.0), 1);
        assert_eq!(stack.gap_at(60.0), 1);
        assert_eq!(stack.gap_at(120.0), 2);
        assert_eq!(stack.gap_at(500.0), 3);
    }

    #[test]
    fn dropping_in_a_gap_next_to_itself_changes_nothing() {
        assert_eq!(destination(1, 1), 1);
        assert_eq!(destination(1, 2), 1);
        assert_eq!(destination(0, 2), 1);
        assert_eq!(destination(2, 0), 0);
        assert_eq!(destination(0, 3), 2);
    }

    #[test]
    fn items_between_slide_by_the_dragged_items_extent() {
        let stack = uneven();
        // Row 0 (40 + 8) to the end: rows 1 and 2 each move up by 48, and row
        // 0 itself goes down by their sizes and gaps.
        assert_eq!(stack.offsets(0, 2), vec![88.0 + 48.0, -48.0, -48.0]);
        // Row 2 (40 + 8) to the start: rows 0 and 1 move down by 48.
        assert_eq!(stack.offsets(2, 0), vec![48.0, 48.0, -(48.0 + 88.0)]);
        assert_eq!(stack.offsets(1, 1), vec![0.0; 3]);
        assert_eq!(stack.offsets(1, 7), vec![0.0; 3]);
    }

    #[test]
    fn a_slide_keeps_the_column_the_same_height() {
        // After the move the item that was at `from` starts where the item it
        // displaced did, and the whole column is as tall as before.
        let stack = uneven();
        for (from, to) in [(0, 1), (0, 2), (2, 0), (1, 2), (2, 1)] {
            let offsets = stack.offsets(from, to);
            let mut order: Vec<usize> = (0..3).collect();
            let item = order.remove(from);
            order.insert(to, item);
            let mut top = 0.0;
            for &moved in &order {
                let drawn = stack.top(moved) + offsets[moved];
                assert!((drawn - top).abs() < 0.001, "{from}->{to}: item {moved} at {drawn}, wanted {top}");
                top += stack.size(moved) + 8.0;
            }
        }
    }

    #[test]
    fn keys_move_one_place_and_stop_at_the_ends() {
        assert_eq!(key_target(4, 2, Move::Up), Some(1));
        assert_eq!(key_target(4, 2, Move::Down), Some(3));
        assert_eq!(key_target(4, 0, Move::Up), None);
        assert_eq!(key_target(4, 3, Move::Down), None);
        assert_eq!(key_target(4, 2, Move::First), Some(0));
        assert_eq!(key_target(4, 2, Move::Last), Some(3));
        assert_eq!(key_target(4, 0, Move::First), None);
        assert_eq!(key_target(4, 3, Move::Last), None);
        assert_eq!(key_target(0, 0, Move::Down), None);
        assert_eq!(key_target(1, 0, Move::Down), None);
        assert_eq!(key_target(3, 9, Move::Up), None);
    }

    #[test]
    fn the_edge_speed_grows_toward_the_edge() {
        assert_eq!(edge_speed(300.0, 0.0, 600.0), 0.0);
        assert_eq!(edge_speed(EDGE, 0.0, 600.0), 0.0);
        assert!(edge_speed(10.0, 0.0, 600.0) < 0.0);
        assert!(edge_speed(-20.0, 0.0, 600.0) < edge_speed(10.0, 0.0, 600.0));
        assert!(edge_speed(590.0, 0.0, 600.0) > 0.0);
        assert!(edge_speed(590.0, 0.0, 600.0) > edge_speed(560.0, 0.0, 600.0));
        // Past the edge it keeps a bounded speed.
        assert_eq!(edge_speed(5000.0, 0.0, 600.0), 1.5 * EDGE_SPEED);
        // A container too short for two bands never scrolls.
        assert_eq!(edge_speed(10.0, 0.0, 80.0), 0.0);
    }

    #[test]
    fn a_spring_arrives_without_overshoot() {
        let (mut x, mut v) = (0.0_f32, 0.0_f32);
        for _ in 0..120 {
            (x, v) = spring(x, v, 100.0, 1.0 / 60.0);
            assert!(x <= 100.0 + 0.001, "overshot to {x}");
        }
        assert!((x - 100.0).abs() < 0.5);
        // A frame that took a whole second does not blow up.
        let (x, _) = spring(0.0, 0.0, 100.0, 1.0);
        assert!((x - 100.0).abs() < 0.001);
    }

    #[test]
    fn a_held_item_follows_the_pointer_and_names_its_place() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.pick(even(4), 1);
        assert!(reorder.is_held());
        assert_eq!(reorder.held(), Some(1));
        assert!(!reorder.drag(10.0));
        assert_eq!(reorder.offset(1), 10.0);
        assert!(reorder.drag(60.0));
        assert_eq!(reorder.target(), 2);
        assert_eq!(reorder.offset(1), 60.0);
    }

    #[test]
    fn others_slide_out_of_the_way_a_frame_at_a_time() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.pick(even(4), 0);
        reorder.drag(70.0);
        assert_eq!(reorder.target(), 1);
        assert_eq!(reorder.step(1.0 / 60.0, false), None);
        let first = reorder.offset(1);
        assert!(first < 0.0 && first > -56.0, "row 1 is part-way: {first}");
        for _ in 0..30 {
            reorder.step(1.0 / 60.0, false);
        }
        assert!((reorder.offset(1) + 56.0).abs() < 0.5);
        assert_eq!(reorder.offset(2), 0.0);
        assert!(reorder.lift() > 0.99);
    }

    #[test]
    fn dropping_settles_into_place_and_then_reports_the_move() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.pick(even(4), 0);
        reorder.drag(130.0);
        for _ in 0..10 {
            reorder.step(1.0 / 60.0, false);
        }
        reorder.drop();
        assert!(!reorder.is_held() && reorder.is_busy());
        // It does not jump: the first frame after the drop is where it was.
        assert!((reorder.offset(0) - 130.0).abs() < 0.001);
        reorder.step(1.0 / 60.0, false);
        assert!(reorder.offset(0) < 130.0 && reorder.offset(0) > 112.0);
        // Row 0 lands at place 2: two rows below.
        assert_eq!(settle(&mut reorder), Done::Moved { from: 0, to: 2 });
        assert!(!reorder.is_busy());
        assert_eq!(reorder.offset(0), 0.0);
        assert_eq!(reorder.lift(), 0.0);
    }

    #[test]
    fn dropping_where_it_started_reports_nothing_moved() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.pick(even(3), 1);
        reorder.drag(12.0);
        reorder.drop();
        assert_eq!(settle(&mut reorder), Done::Rest);
    }

    #[test]
    fn escape_takes_the_item_home_and_changes_nothing() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.pick(even(4), 0);
        reorder.drag(130.0);
        for _ in 0..20 {
            reorder.step(1.0 / 60.0, false);
        }
        assert!(reorder.offset(1) < -50.0);
        reorder.cancel();
        assert_eq!(reorder.target(), 0);
        assert!((reorder.offset(0) - 130.0).abs() < 0.001, "it starts from where it is");
        assert_eq!(settle(&mut reorder), Done::Rest);
        assert_eq!(reorder.offset(1), 0.0);
    }

    #[test]
    fn a_key_move_is_the_same_slide_without_a_lift() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.nudge(even(4), 2, 1);
        assert!(!reorder.is_held() && reorder.is_busy());
        reorder.step(1.0 / 60.0, false);
        assert!(reorder.offset(1) > 0.0, "the item above slides down");
        assert!(reorder.offset(2) < 0.0, "the moved item slides up");
        assert_eq!(reorder.lift(), 0.0);
        assert_eq!(settle(&mut reorder), Done::Moved { from: 2, to: 1 });
    }

    #[test]
    fn a_key_move_that_goes_nowhere_does_nothing() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.nudge(even(4), 0, 0);
        reorder.nudge(even(4), 0, 4);
        reorder.nudge(even(4), 9, 1);
        assert!(!reorder.is_busy());
    }

    #[test]
    fn reduced_motion_is_everything_at_once() {
        let mut reorder = Reorder::new(Indicate::Slide);
        reorder.nudge(even(4), 0, 1);
        assert_eq!(reorder.step(1.0 / 60.0, true), Some(Done::Moved { from: 0, to: 1 }));
        assert!(!reorder.is_busy());

        reorder.pick(even(4), 0);
        reorder.drag(70.0);
        assert_eq!(reorder.step(1.0 / 60.0, true), None);
        assert_eq!(reorder.offset(1), -56.0, "no frames in between");
        assert_eq!(reorder.lift(), 1.0);
        reorder.cancel();
        assert_eq!(reorder.step(1.0 / 60.0, true), Some(Done::Rest));
    }

    #[test]
    fn a_line_list_leaves_the_others_where_they_are() {
        let mut reorder = Reorder::new(Indicate::Line);
        reorder.pick(even(6), 1);
        reorder.drag(150.0);
        // The pointer is 28 px into row 1, and 150 px have gone by.
        reorder.drag_to_gap(150.0, 56.0 + 28.0 + 150.0);
        assert_eq!(reorder.line(), Some(4.0 * 56.0));
        for _ in 0..20 {
            reorder.step(1.0 / 60.0, false);
        }
        assert_eq!(reorder.offset(2), 0.0);
        assert_eq!(reorder.offset(3), 0.0);
        assert_eq!(reorder.target(), 3);
        reorder.drop();
        assert_eq!(settle(&mut reorder), Done::Moved { from: 1, to: 3 });
    }
}
