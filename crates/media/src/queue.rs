//! The play queue: what plays, in which order, and which item is playing.
//!
//! The queue is this crate's, not mpv's: mpv's playlist is kept equal to it
//! by the operations each change returns ([`Op`]), so mpv can play the next
//! item without a gap (it opens it before the current one ends) and the
//! window, the shell's media controls and mpv agree on the order.
//!
//! Shuffle works the way a music player's does: the item playing stays, the
//! ones after it are shuffled, and turning shuffle off puts the items back in
//! the order they were added (the items played meanwhile stay where they are).

use std::path::PathBuf;

/// What kind of thing an item is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Audio,
    Video,
}

/// One thing to play.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    /// Unique in this process; survives reordering.
    pub id: u64,
    pub path: PathBuf,
    pub kind: Kind,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track: Option<u32>,
    /// Seconds, when known before playing.
    pub duration: Option<f64>,
    /// A picture of the album or a frame of the film, as a file.
    pub artwork: Option<PathBuf>,
}

impl Item {
    /// An item for a file, titled after the file until its tags are read.
    pub fn for_path(path: PathBuf, kind: Kind) -> Item {
        let title = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Item {
            id: next_id(),
            path,
            kind,
            title,
            ..Item::default()
        }
    }
}

/// A new item id.
pub fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// What repeats when the queue runs out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Repeat {
    /// Stop after the last item.
    #[default]
    Off,
    /// Start the queue again.
    All,
    /// Play the current item again and again.
    One,
}

impl Repeat {
    /// The next mode of the repeat button: off, all, one, off.
    pub fn next(self) -> Repeat {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

/// A change to make to mpv's playlist, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Empty the playlist and play these, starting at the index.
    Replace(Vec<PathBuf>, usize),
    /// Add to the end.
    Append(PathBuf),
    /// mpv's `playlist-move from to`: the entry at `from` goes before the
    /// entry now at `to` (`to` may be the length, the end).
    Move(usize, usize),
    Remove(usize),
    PlayIndex(usize),
    /// Nothing to play: stop and empty the playlist.
    Stop,
}

/// The queue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Queue {
    items: Vec<Item>,
    current: Option<usize>,
    shuffle: bool,
    repeat: Repeat,
    /// The ids in the order they were added, while shuffle is on.
    added_order: Vec<u64>,
}

impl Queue {
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn current_item(&self) -> Option<&Item> {
        self.current.and_then(|index| self.items.get(index))
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    /// The items after the current one.
    pub fn upcoming(&self) -> &[Item] {
        match self.current {
            Some(index) => &self.items[(index + 1).min(self.items.len())..],
            None => &self.items,
        }
    }

    /// Plays `items` from `start`, replacing the queue. With shuffle on, the
    /// items after the start are shuffled.
    pub fn replace(&mut self, items: Vec<Item>, start: usize, seed: u64) -> Vec<Op> {
        if items.is_empty() {
            return self.clear();
        }
        let start = start.min(items.len() - 1);
        self.added_order = items.iter().map(|item| item.id).collect();
        self.items = items;
        self.current = Some(start);
        if self.shuffle {
            // The chosen item plays first, then the rest in a shuffled order.
            let chosen = self.items.remove(start);
            self.items.insert(0, chosen);
            self.current = Some(0);
            shuffle_slice(&mut self.items[1..], seed);
        }
        let paths = self.items.iter().map(|item| item.path.clone()).collect();
        vec![Op::Replace(paths, self.current.unwrap_or(0))]
    }

    /// Adds items at the end, or right after the current item (`next`).
    pub fn add(&mut self, items: Vec<Item>, next: bool) -> Vec<Op> {
        if items.is_empty() {
            return Vec::new();
        }
        if self.items.is_empty() {
            return self.replace(items, 0, 0);
        }
        let mut ops = Vec::new();
        let mut at = match (next, self.current) {
            (true, Some(current)) => current + 1,
            _ => self.items.len(),
        };
        for item in items {
            self.added_order.push(item.id);
            ops.push(Op::Append(item.path.clone()));
            let end = self.items.len();
            if at < end {
                ops.push(Op::Move(end, at));
            }
            self.items.insert(at, item);
            at += 1;
        }
        ops
    }

    /// Moves the item at `from` so that it ends at `to` (both indexes before
    /// the move; the `reorder` contract of bingux-ui's sortable list).
    pub fn reorder(&mut self, from: usize, to: usize) -> Vec<Op> {
        let len = self.items.len();
        if from >= len || to >= len || from == to {
            return Vec::new();
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);
        self.current = self.current.map(|current| moved_index(current, from, to));
        // mpv puts the entry before the one now at the target: past `to` when
        // moving down.
        let target = if from < to { to + 1 } else { to };
        vec![Op::Move(from, target)]
    }

    /// Takes the item at `index` out. Removing the playing item plays the next.
    pub fn remove(&mut self, index: usize) -> Vec<Op> {
        if index >= self.items.len() {
            return Vec::new();
        }
        let removed = self.items.remove(index);
        self.added_order.retain(|id| *id != removed.id);
        if self.items.is_empty() {
            self.current = None;
            return vec![Op::Stop];
        }
        self.current = match self.current {
            Some(current) if current > index => Some(current - 1),
            Some(current) if current == index => Some(current.min(self.items.len() - 1)),
            other => other,
        };
        vec![Op::Remove(index)]
    }

    /// Empties the queue.
    pub fn clear(&mut self) -> Vec<Op> {
        self.items.clear();
        self.added_order.clear();
        self.current = None;
        vec![Op::Stop]
    }

    /// Removes everything after the current item.
    pub fn clear_upcoming(&mut self) -> Vec<Op> {
        let Some(current) = self.current else {
            return self.clear();
        };
        let mut ops = Vec::new();
        while self.items.len() > current + 1 {
            let last = self.items.len() - 1;
            let removed = self.items.remove(last);
            self.added_order.retain(|id| *id != removed.id);
            ops.push(Op::Remove(last));
        }
        ops
    }

    /// Replaces the item with the same id (tags learnt while playing).
    pub(crate) fn update_item(&mut self, item: Item) {
        if let Some(slot) = self.items.iter_mut().find(|slot| slot.id == item.id) {
            *slot = item;
        }
    }

    /// Plays the item at `index`.
    pub fn play(&mut self, index: usize) -> Vec<Op> {
        if index >= self.items.len() {
            return Vec::new();
        }
        self.current = Some(index);
        vec![Op::PlayIndex(index)]
    }

    /// mpv moved to another entry by itself (the previous one ended).
    pub fn follow(&mut self, index: Option<usize>) {
        self.current = index.filter(|index| *index < self.items.len());
    }

    /// Turns shuffle on (the upcoming items in a random order) or off (back
    /// in the order they were added).
    pub fn set_shuffle(&mut self, on: bool, seed: u64) -> Vec<Op> {
        if on == self.shuffle {
            return Vec::new();
        }
        self.shuffle = on;
        let start = self.current.map_or(0, |current| current + 1);
        if start >= self.items.len() {
            return Vec::new();
        }
        let mut wanted: Vec<u64> = self.items[start..].iter().map(|item| item.id).collect();
        if on {
            shuffle_slice(&mut wanted, seed);
        } else {
            let rank = |id: &u64| {
                self.added_order
                    .iter()
                    .position(|added| added == id)
                    .unwrap_or(usize::MAX)
            };
            wanted.sort_by_key(rank);
        }
        self.arrange(start, &wanted)
    }

    /// Puts the items from `start` in the order of `ids`, one mpv move per
    /// item that is not already in place.
    fn arrange(&mut self, start: usize, ids: &[u64]) -> Vec<Op> {
        let mut ops = Vec::new();
        for (offset, id) in ids.iter().enumerate() {
            let target = start + offset;
            let Some(found) = self.items[target..].iter().position(|item| item.id == *id) else {
                continue;
            };
            let from = target + found;
            if from != target {
                let item = self.items.remove(from);
                self.items.insert(target, item);
                ops.push(Op::Move(from, target));
            }
        }
        ops
    }
}

/// Where the item at `index` is after the item at `from` moved to `to`.
fn moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < index && index <= to {
        index - 1
    } else if to <= index && index < from {
        index + 1
    } else {
        index
    }
}

/// Fisher-Yates with a small xorshift generator: the order only needs to look
/// random, and a seed makes a test repeatable.
fn shuffle_slice<T>(items: &mut [T], seed: u64) {
    let mut state = seed | 1;
    for index in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let other = (state % (index as u64 + 1)) as usize;
        items.swap(index, other);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(count: usize) -> Vec<Item> {
        (0..count)
            .map(|index| Item {
                id: index as u64 + 100,
                path: PathBuf::from(format!("/m/{index}.flac")),
                ..Item::default()
            })
            .collect()
    }

    /// Applies the operations to a list of paths the way mpv does.
    fn mpv(ops: &[Op], playlist: &mut Vec<PathBuf>) {
        for op in ops {
            match op {
                Op::Replace(paths, _) => *playlist = paths.clone(),
                Op::Append(path) => playlist.push(path.clone()),
                Op::Move(from, to) => {
                    let entry = playlist.remove(*from);
                    let to = if *from < *to { to - 1 } else { *to };
                    playlist.insert(to, entry);
                }
                Op::Remove(index) => {
                    playlist.remove(*index);
                }
                Op::PlayIndex(_) => {}
                Op::Stop => playlist.clear(),
            }
        }
    }

    fn paths(queue: &Queue) -> Vec<PathBuf> {
        queue.items().iter().map(|item| item.path.clone()).collect()
    }

    #[test]
    fn mpv_follows_every_change() {
        let mut queue = Queue::default();
        let mut playlist = Vec::new();
        mpv(&queue.replace(items(6), 2, 7), &mut playlist);
        assert_eq!(playlist, paths(&queue));
        mpv(&queue.reorder(0, 4), &mut playlist);
        assert_eq!(playlist, paths(&queue));
        mpv(&queue.reorder(5, 1), &mut playlist);
        assert_eq!(playlist, paths(&queue));
        let mut more = items(2);
        more[0].id = 900;
        more[1].id = 901;
        mpv(&queue.add(more, true), &mut playlist);
        assert_eq!(playlist, paths(&queue));
        mpv(&queue.set_shuffle(true, 99), &mut playlist);
        assert_eq!(playlist, paths(&queue));
        mpv(&queue.set_shuffle(false, 0), &mut playlist);
        assert_eq!(playlist, paths(&queue));
        mpv(&queue.remove(3), &mut playlist);
        assert_eq!(playlist, paths(&queue));
        mpv(&queue.clear_upcoming(), &mut playlist);
        assert_eq!(playlist, paths(&queue));
    }

    #[test]
    fn the_playing_item_stays_current_when_others_move() {
        let mut queue = Queue::default();
        queue.replace(items(5), 2, 0);
        let playing = queue.current_item().unwrap().id;
        queue.reorder(0, 4);
        assert_eq!(queue.current_item().unwrap().id, playing);
        queue.reorder(4, 0);
        assert_eq!(queue.current_item().unwrap().id, playing);
        queue.remove(0);
        assert_eq!(queue.current_item().unwrap().id, playing);
    }

    #[test]
    fn play_next_goes_after_the_current_item() {
        let mut queue = Queue::default();
        queue.replace(items(3), 0, 0);
        let mut next = items(1);
        next[0].id = 7;
        queue.add(next, true);
        assert_eq!(queue.items()[1].id, 7);
    }

    #[test]
    fn shuffle_keeps_the_current_item_and_off_restores_the_order() {
        let mut queue = Queue::default();
        queue.replace(items(8), 3, 0);
        let before: Vec<u64> = queue.items().iter().map(|item| item.id).collect();
        queue.set_shuffle(true, 12345);
        assert_eq!(queue.current(), Some(3));
        assert_eq!(&queue.items()[..4], &items(8)[..4]);
        let shuffled: Vec<u64> = queue.items().iter().map(|item| item.id).collect();
        assert_ne!(shuffled, before);
        queue.set_shuffle(false, 0);
        let after: Vec<u64> = queue.items().iter().map(|item| item.id).collect();
        assert_eq!(after, before);
    }

    #[test]
    fn removing_the_last_item_stops() {
        let mut queue = Queue::default();
        queue.replace(items(1), 0, 0);
        assert_eq!(queue.remove(0), vec![Op::Stop]);
        assert_eq!(queue.current(), None);
    }
}
