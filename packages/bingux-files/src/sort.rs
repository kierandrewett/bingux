//! Sorting entries.

use std::cmp::Ordering;

use crate::entry::Entry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    /// The type's description, then the name.
    Kind,
}

/// How a listing is ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub descending: bool,
    /// Folders come before files whatever the key, as in GNOME Files.
    pub folders_first: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort {
            key: SortKey::Name,
            descending: false,
            folders_first: true,
        }
    }
}

impl Sort {
    /// Sort by `key`, ascending, folders first.
    pub fn by(key: SortKey) -> Self {
        Sort {
            key,
            ..Sort::default()
        }
    }

    /// The order after the user chooses `key` again: the same key flips the
    /// direction, a new key starts ascending (newest and largest first would
    /// surprise; the person flips it).
    pub fn toggled(self, key: SortKey) -> Self {
        if self.key == key {
            Sort {
                descending: !self.descending,
                ..self
            }
        } else {
            Sort {
                key,
                descending: false,
                ..self
            }
        }
    }

    pub fn compare(&self, a: &Entry, b: &Entry) -> Ordering {
        if self.folders_first && a.is_dir() != b.is_dir() {
            return if a.is_dir() {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let by_key = match self.key {
            SortKey::Name => natural_cmp(&a.name, &b.name),
            SortKey::Size => a
                .size
                .cmp(&b.size)
                .then_with(|| natural_cmp(&a.name, &b.name)),
            SortKey::Modified => a
                .modified
                .cmp(&b.modified)
                .then_with(|| natural_cmp(&a.name, &b.name)),
            SortKey::Kind => a
                .mime
                .description
                .cmp(b.mime.description)
                .then_with(|| natural_cmp(&a.name, &b.name)),
        };
        if self.descending {
            by_key.reverse()
        } else {
            by_key
        }
    }

    /// Put `shown`, indices into `entries`, in this order.
    ///
    /// Same order as [`Sort::compare`], without comparing text pair by pair:
    /// every name becomes a byte string whose plain byte order is the natural
    /// order ([`name_key`]), all of them in one buffer, so sorting 100,000
    /// names allocates twice. Ties on the key fall back to the exact name,
    /// then to the position, so the order is total and `shown` may be sorted
    /// with the faster unstable sort.
    pub fn order(&self, entries: &[Entry], shown: &mut [usize]) {
        struct Item {
            index: usize,
            /// 0 for folders when folders come first, else 1.
            group: u8,
            primary: i128,
            /// The first eight bytes of the key, so most comparisons never
            /// touch the buffer.
            head: u64,
            key: (u32, u32),
        }
        let mut arena: Vec<u8> = Vec::with_capacity(shown.len() * 16);
        let mut ranks: Vec<(&'static str, u32)> = Vec::new();
        if self.key == SortKey::Kind {
            let mut names: Vec<&'static str> = shown
                .iter()
                .map(|&index| entries[index].mime.description)
                .collect();
            names.sort_unstable();
            names.dedup();
            ranks = names.into_iter().zip(0..).collect();
        }
        let mut items: Vec<Item> = Vec::with_capacity(shown.len());
        for &index in shown.iter() {
            let entry = &entries[index];
            let start = arena.len() as u32;
            name_key(&entry.name, &mut arena);
            let primary = match self.key {
                SortKey::Name => 0,
                SortKey::Size => i128::from(entry.size),
                SortKey::Modified => entry.modified.map_or(i128::MIN, |time| {
                    match time.duration_since(std::time::UNIX_EPOCH) {
                        Ok(after) => after.as_nanos() as i128,
                        Err(before) => -(before.duration().as_nanos() as i128),
                    }
                }),
                SortKey::Kind => ranks
                    .binary_search_by(|(name, _)| (*name).cmp(entry.mime.description))
                    .map_or(0, |at| i128::from(ranks[at].1)),
            };
            let mut head = [0u8; 8];
            let key = &arena[start as usize..];
            let take = key.len().min(8);
            head[..take].copy_from_slice(&key[..take]);
            items.push(Item {
                index,
                group: u8::from(!(self.folders_first && entry.is_dir())),
                primary,
                head: u64::from_be_bytes(head),
                key: (start, arena.len() as u32),
            });
        }
        let arena = &arena[..];
        let descending = self.descending;
        items.sort_unstable_by(|a, b| {
            a.group.cmp(&b.group).then_with(|| {
                let by_key = a
                    .primary
                    .cmp(&b.primary)
                    .then_with(|| a.head.cmp(&b.head))
                    .then_with(|| {
                        arena[a.key.0 as usize..a.key.1 as usize]
                            .cmp(&arena[b.key.0 as usize..b.key.1 as usize])
                    })
                    .then_with(|| entries[a.index].name.cmp(&entries[b.index].name))
                    .then_with(|| a.index.cmp(&b.index));
                if descending { by_key.reverse() } else { by_key }
            })
        });
        for (slot, item) in shown.iter_mut().zip(&items) {
            *slot = item.index;
        }
    }
}

/// Append to `out` a byte string whose byte order is [`natural_cmp`]'s order
/// of names (ties aside: names whose keys are equal differ only in case or
/// leading zeros, and the caller breaks those on the exact name).
///
/// Letters are lower-cased; a run of digits becomes the marker `0x30`, its
/// length as two bytes, and the digits without leading zeros, so longer
/// numbers are bigger and equal-length ones compare digit by digit.
pub fn name_key(name: &str, out: &mut Vec<u8>) {
    let bytes = name.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_digit() {
            let mut end = at;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            let mut start = at;
            while start + 1 < end && bytes[start] == b'0' {
                start += 1;
            }
            // `0` alone is the number zero; all-zero runs keep one digit.
            let digits = &bytes[start..end];
            let digits = if digits == b"0" { &digits[..0] } else { digits };
            out.push(0x30);
            out.extend_from_slice(&(digits.len().min(u16::MAX as usize) as u16).to_be_bytes());
            out.extend_from_slice(digits);
            at = end;
        } else if byte < 0x80 {
            out.push(byte.to_ascii_lowercase());
            at += 1;
        } else {
            // One UTF-8 character: lower-case it, as `natural_cmp` does.
            let rest = &name[at..];
            let c = rest.chars().next().unwrap_or('\u{fffd}');
            let mut buffer = [0u8; 4];
            for lower in c.to_lowercase() {
                out.extend_from_slice(lower.encode_utf8(&mut buffer).as_bytes());
            }
            at += c.len_utf8();
        }
    }
}

/// Name order for people: case-insensitive, and runs of digits compare as
/// numbers, so `file2` comes before `file10`. Ties break on the exact text so
/// the order is total and stable.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let number = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        chars.next();
                    }
                    digits
                };
                let one = number(&mut left);
                let two = number(&mut right);
                let one_trimmed = one.trim_start_matches('0');
                let two_trimmed = two.trim_start_matches('0');
                let order = one_trimmed
                    .len()
                    .cmp(&two_trimmed.len())
                    .then_with(|| one_trimmed.cmp(two_trimmed));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                left.next();
                right.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_compare_as_numbers() {
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("File2", "file2"), Ordering::Less);
        assert_eq!(natural_cmp("a", "B"), Ordering::Less);
        assert_eq!(natural_cmp("img007", "img7"), Ordering::Less);
    }

    fn entry(name: &str, dir: bool, size: u64) -> Entry {
        let kind = if dir { crate::Kind::Directory } else { crate::Kind::File };
        let mut entry = Entry::named(std::path::PathBuf::from("/x").join(name), name.into(), kind, false, false);
        entry.size = size;
        entry.modified = Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(size * 7 % 13));
        entry
    }

    #[test]
    fn keys_order_like_natural_cmp() {
        let names = [
            "a", "A", "b", "B", "file2", "file10", "File2", "file02", "file2a", "file2 a", "img7", "img007",
            "img10x", "9", "10", "0", "00", "007", "x-1", "x_1", "x1", ".hidden", "é", "É", "z", "ß", "日本",
            "a b", "a-b", "a1b2", "a1b10", "a01b2", "", "1a", "a1", "~", "[x]",
        ];
        let key = |name: &str| {
            let mut out = Vec::new();
            name_key(name, &mut out);
            out
        };
        for a in names {
            for b in names {
                let by_key = key(a).cmp(&key(b)).then_with(|| a.cmp(b));
                assert_eq!(by_key, natural_cmp(a, b), "{a:?} against {b:?}");
            }
        }
    }

    #[test]
    fn order_matches_compare_for_every_key() {
        let names = ["b.txt", "a10.txt", "a2.txt", "Folder", "folder2", "Zed", "c.png", "C.png", "d", "e1", "e01"];
        let entries: Vec<Entry> = names
            .iter()
            .enumerate()
            .map(|(i, name)| entry(name, name.starts_with(['F', 'f']), (i as u64 * 5) % 4))
            .collect();
        for key in [SortKey::Name, SortKey::Size, SortKey::Modified, SortKey::Kind] {
            for descending in [false, true] {
                for folders_first in [false, true] {
                    let sort = Sort { key, descending, folders_first };
                    let mut slow: Vec<usize> = (0..entries.len()).collect();
                    slow.sort_by(|a, b| sort.compare(&entries[*a], &entries[*b]));
                    let mut fast: Vec<usize> = (0..entries.len()).collect();
                    sort.order(&entries, &mut fast);
                    let show = |order: &[usize]| order.iter().map(|i| names[*i]).collect::<Vec<_>>();
                    assert_eq!(show(&fast), show(&slow), "{sort:?}");
                }
            }
        }
    }
}
