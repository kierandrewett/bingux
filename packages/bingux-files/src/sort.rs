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
}
