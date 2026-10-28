use std::{
    collections::VecDeque,
    hash::{
        Hash,
        Hasher,
    },
    rc::Rc,
};

use rustc_hash::{
    FxHashMap,
    FxHasher,
};
use smallvec::SmallVec;

/// Cache of values shared between users, where each user keeps its last `USER_CAPACITY`
/// used values alive. A value is dropped once no user uses it, unless `RETAIN` is
/// above 0 (Bingux patch): then the `RETAIN` values that lost their last user most
/// recently are kept, and come back to life when a user asks for them again. A page
/// that was left and is opened again finds its text laid out as it left it, instead of
/// shaping every label again on the frame that mounts it.
pub struct LRUCache<V, ID: Hash, const USER_CAPACITY: usize = 2, const RETAIN: usize = 0> {
    map: FxHashMap<u64, (i32, Rc<V>)>,
    pub users: FxHashMap<ID, SmallVec<[u64; 2]>>,
    /// Values with no user, newest last in `retired`; at most `RETAIN`.
    retained: FxHashMap<u64, Rc<V>>,
    retired: VecDeque<u64>,
}

impl<V, ID: Hash, const USER_CAPACITY: usize, const RETAIN: usize> Default
    for LRUCache<V, ID, USER_CAPACITY, RETAIN>
{
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
            users: FxHashMap::default(),
            retained: FxHashMap::default(),
            retired: VecDeque::new(),
        }
    }
}

impl<V, ID: Hash + Eq, const USER_CAPACITY: usize, const RETAIN: usize>
    LRUCache<V, ID, USER_CAPACITY, RETAIN>
{
    /// Takes the value of `hash` out of use: it is dropped, or retained for
    /// whoever asks for it next when this cache retains. Takes the fields it
    /// changes apart, so a caller that holds a borrow of `users` can call it.
    fn retire(
        map: &mut FxHashMap<u64, (i32, Rc<V>)>,
        retained: &mut FxHashMap<u64, Rc<V>>,
        retired: &mut VecDeque<u64>,
        hash: u64,
    ) {
        let Some((_, value)) = map.remove(&hash) else {
            return;
        };
        if RETAIN == 0 {
            return;
        }
        if retained.insert(hash, value).is_none() {
            retired.push_back(hash);
        }
        while retained.len() > RETAIN {
            let Some(oldest) = retired.pop_front() else {
                break;
            };
            retained.remove(&oldest);
        }
    }

    fn hash_of<H: Hash>(hash_value: &H) -> u64 {
        let mut hasher = FxHasher::default();
        hash_value.hash(&mut hasher);
        hasher.finish()
    }

    /// Marks `hash_value` as the last value used by `id` and returns it if cached.
    /// When not cached it returns `None` and leaves a free slot for the [LRUCache::insert] that follows.
    pub fn utilize<H: Hash>(&mut self, id: ID, hash_value: &H) -> Option<Rc<V>> {
        let hash = Self::hash_of(hash_value);
        if RETAIN > 0 && !self.map.contains_key(&hash) {
            if let Some(value) = self.retained.remove(&hash) {
                self.map.insert(hash, (0, value));
            }
        }
        let entry = self.map.get_mut(&hash);
        let cached_value = entry.as_ref().map(|(_, value)| value.clone());

        let hashes = self.users.entry(id).or_default();

        if let Some(position) = hashes.iter().position(|used| *used == hash) {
            hashes.remove(position);
            hashes.push(hash);

            return cached_value;
        }

        if let Some((users, _)) = entry {
            *users += 1;
            hashes.push(hash);
        }

        let keep = if cached_value.is_some() {
            USER_CAPACITY
        } else {
            USER_CAPACITY.saturating_sub(1)
        };
        let extra = hashes.len().saturating_sub(keep);

        for old_hash in hashes.drain(0..extra) {
            let Some((users, _)) = self.map.get_mut(&old_hash) else {
                continue;
            };

            *users -= 1;

            if *users == 0 {
                Self::retire(&mut self.map, &mut self.retained, &mut self.retired, old_hash);
            }
        }

        cached_value
    }

    /// Saves `value` for `hash_value` and marks it as used by `id`.
    pub fn insert<H: Hash>(&mut self, id: ID, hash_value: &H, value: V) -> Rc<V> {
        let hash = Self::hash_of(hash_value);
        let value = Rc::new(value);
        if RETAIN > 0 {
            self.retained.remove(&hash);
        }

        let (users, _) = self.map.entry(hash).or_insert_with(|| (0, value.clone()));
        let user_hashes = self.users.entry(id).or_default();

        if !user_hashes.contains(&hash) {
            user_hashes.push(hash);
            *users += 1;
        }

        value
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn users_len(&self) -> usize {
        self.users.len()
    }

    pub fn remove(&mut self, id: &ID) {
        let Some(hashes) = self.users.remove(id) else {
            return;
        };

        for hash in hashes {
            let Some((users, _)) = self.map.get_mut(&hash) else {
                continue;
            };

            *users -= 1;

            if *users == 0 {
                Self::retire(&mut self.map, &mut self.retained, &mut self.retired, hash);
            }
        }
    }

    pub fn reset(&mut self) {
        self.map.clear();
        self.users.clear();
        self.retained.clear();
        self.retired.clear();
    }

    /// How many values with no user are being kept; see `RETAIN`.
    pub fn retained_len(&self) -> usize {
        self.retained.len()
    }
}

impl<V, ID: Hash, const USER_CAPACITY: usize, const RETAIN: usize> std::fmt::Debug
    for LRUCache<V, ID, USER_CAPACITY, RETAIN>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LRUCache")
            .field("cached_values", &self.map.len())
            .field("retained_values", &self.retained.len())
            .field("cached_users", &self.users.len())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use std::rc::Rc;

    use crate::lru_cache::LRUCache;

    type Cache = LRUCache<i32, u64>;

    #[test]
    fn lru_cache() {
        let mut cache = Cache::default();

        cache
            .utilize(1, &50)
            .unwrap_or_else(|| cache.insert(1, &50, 500));
        assert_eq!(cache.utilize(1, &50), Some(Rc::new(500)));

        cache
            .utilize(1, &60)
            .unwrap_or_else(|| cache.insert(1, &60, 600));
        assert_eq!(cache.utilize(1, &60), Some(Rc::new(600)));
        assert_eq!(cache.utilize(1, &50), Some(Rc::new(500)));

        cache
            .utilize(1, &70)
            .unwrap_or_else(|| cache.insert(1, &70, 700));
        assert!(!cache.map.contains_key(&Cache::hash_of(&60)));
        assert!(cache.map.contains_key(&Cache::hash_of(&50)));

        cache.remove(&1);
        assert!(!cache.map.contains_key(&Cache::hash_of(&50)));
        assert!(!cache.map.contains_key(&Cache::hash_of(&70)));

        cache
            .utilize(1, &70)
            .unwrap_or_else(|| cache.insert(1, &70, 700));
        assert!(cache.utilize(2, &70).is_some());

        cache.remove(&1);
        assert!(cache.map.contains_key(&Cache::hash_of(&70)));
        cache.remove(&2);
        assert!(!cache.map.contains_key(&Cache::hash_of(&70)));
    }

    #[test]
    fn a_retaining_cache_gives_back_what_lost_its_last_user() {
        let mut cache = LRUCache::<i32, u64, 2, 2>::default();

        cache.utilize(1, &10).unwrap_or_else(|| cache.insert(1, &10, 100));
        cache.remove(&1);
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.retained_len(), 1);

        // Another node asks for it: it is back, and used again.
        assert_eq!(cache.utilize(2, &10), Some(Rc::new(100)));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.retained_len(), 0);
        cache.remove(&2);
        assert_eq!(cache.retained_len(), 1);
    }

    #[test]
    fn a_retaining_cache_keeps_the_newest_values_only() {
        let mut cache = LRUCache::<i32, u64, 2, 2>::default();

        for (id, hash) in [(1, 10), (2, 20), (3, 30)] {
            cache.utilize(id, &hash).unwrap_or_else(|| cache.insert(id, &hash, hash * 10));
            cache.remove(&id);
        }

        assert_eq!(cache.retained_len(), 2);
        assert!(cache.utilize(9, &10).is_none(), "the oldest was let go");
        assert_eq!(cache.utilize(9, &30), Some(Rc::new(300)));

        cache.reset();
        assert_eq!(cache.retained_len(), 0);
        assert!(cache.utilize(9, &30).is_none());
    }

    #[test]
    fn keeps_both_hashes_when_another_user_had_one() {
        let mut cache = Cache::default();

        cache
            .utilize(1, &10)
            .unwrap_or_else(|| cache.insert(1, &10, 100));
        cache
            .utilize(2, &20)
            .unwrap_or_else(|| cache.insert(2, &20, 200));
        cache
            .utilize(1, &20)
            .unwrap_or_else(|| cache.insert(1, &20, 200));

        assert_eq!(cache.users[&1].len(), 2);
        assert!(cache.map.contains_key(&Cache::hash_of(&10)));
        assert!(cache.map.contains_key(&Cache::hash_of(&20)));
    }

    #[test]
    fn only_keeps_two_hashes_per_user() {
        let mut cache = Cache::default();

        for hash in [10, 20, 30, 40] {
            let id = 1000 + hash as u64;
            cache
                .utilize(id, &hash)
                .unwrap_or_else(|| cache.insert(id, &hash, hash * 10));
        }

        for hash in [10, 20, 30, 40] {
            cache
                .utilize(1, &hash)
                .unwrap_or_else(|| cache.insert(1, &hash, hash * 10));
        }

        assert_eq!(cache.users[&1].len(), 2);
    }

    #[test]
    fn using_a_hash_again_keeps_it() {
        let mut cache = Cache::default();

        for hash in [10, 20, 10, 30] {
            cache
                .utilize(1, &hash)
                .unwrap_or_else(|| cache.insert(1, &hash, hash * 10));
        }

        assert!(cache.map.contains_key(&Cache::hash_of(&10)));
        assert!(!cache.map.contains_key(&Cache::hash_of(&20)));
        assert!(cache.map.contains_key(&Cache::hash_of(&30)));
    }
}
