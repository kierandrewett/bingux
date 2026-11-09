//! Cursors and selections: one or more ranges, each an anchor (where the
//! selection started) and a head (where the caret is). Positions are char
//! indices into the rope. A range whose anchor is its head is a caret.
//!
//! The ranges are kept sorted and never overlap: two that touch or overlap
//! after a move or an edit become one, which is what a person expects when
//! two carets run into each other.

/// One caret or selected range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
    /// The display column a vertical move aims for, so Down through a short
    /// line comes back to the same column on the next long one.
    pub goal: Option<usize>,
}

impl Range {
    pub fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
            goal: None,
        }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self {
            anchor,
            head,
            goal: None,
        }
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn len(&self) -> usize {
        self.end() - self.start()
    }

    /// The head moved to `head`; `extend` keeps the anchor, otherwise the
    /// range becomes a caret there.
    pub fn moved(self, head: usize, extend: bool) -> Self {
        if extend {
            Self {
                anchor: self.anchor,
                head,
                goal: None,
            }
        } else {
            Self::caret(head)
        }
    }

    fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

/// The carets and selections of a document. There is always at least one;
/// `primary` is the one the view follows and Escape keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    ranges: Vec<Range>,
    primary: usize,
}

impl Default for Selection {
    fn default() -> Self {
        Self::caret(0)
    }
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self {
            ranges: vec![Range::caret(at)],
            primary: 0,
        }
    }

    pub fn single(range: Range) -> Self {
        Self {
            ranges: vec![range],
            primary: 0,
        }
    }

    /// Several ranges; `primary` indexes `ranges`. They are sorted and merged.
    pub fn new(ranges: Vec<Range>, primary: usize) -> Self {
        let ranges = if ranges.is_empty() {
            vec![Range::caret(0)]
        } else {
            ranges
        };
        let primary = primary.min(ranges.len() - 1);
        let mut selection = Self { ranges, primary };
        selection.normalise();
        selection
    }

    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    pub fn primary(&self) -> Range {
        self.ranges[self.primary]
    }

    pub fn primary_index(&self) -> usize {
        self.primary
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// True when there is more than one caret or range.
    pub fn is_multi(&self) -> bool {
        self.ranges.len() > 1
    }

    /// True when any range selects text.
    pub fn has_selection(&self) -> bool {
        self.ranges.iter().any(|range| !range.is_empty())
    }

    /// Every range mapped by `f`, then sorted and merged again.
    pub fn map(&self, mut f: impl FnMut(Range) -> Range) -> Self {
        let ranges = self.ranges.iter().map(|range| f(*range)).collect();
        Self::new(ranges, self.primary)
    }

    /// Only the primary range: what Escape leaves.
    pub fn only_primary(&self) -> Self {
        Self::single(self.primary())
    }

    /// Adds `range` and makes it the primary one.
    pub fn with(&self, range: Range) -> Self {
        let mut ranges = self.ranges.clone();
        ranges.push(range);
        let primary = ranges.len() - 1;
        Self::new(ranges, primary)
    }

    /// Removes the range that holds `at`, unless it is the only one. Returns
    /// whether one was removed: an Alt+click on a caret takes it away.
    pub fn without_at(&self, at: usize) -> Option<Self> {
        if self.ranges.len() < 2 {
            return None;
        }
        let index = self
            .ranges
            .iter()
            .position(|range| range.start() <= at && at <= range.end())?;
        let mut ranges = self.ranges.clone();
        ranges.remove(index);
        let primary = if self.primary >= ranges.len() {
            ranges.len() - 1
        } else {
            self.primary
        };
        Some(Self::new(ranges, primary))
    }

    /// Clamps every position to `len` (the document's length in chars).
    pub fn clamped(&self, len: usize) -> Self {
        self.map(|range| Range {
            anchor: range.anchor.min(len),
            head: range.head.min(len),
            goal: range.goal,
        })
    }

    /// Sort and merge overlapping ranges, keeping track of the primary one.
    fn normalise(&mut self) {
        if self.ranges.len() < 2 {
            return;
        }
        let primary = self.ranges[self.primary];
        let mut indexed: Vec<(usize, Range)> = self.ranges.iter().copied().enumerate().collect();
        indexed.sort_by_key(|(_, range)| (range.start(), range.end()));
        let mut merged: Vec<Range> = Vec::with_capacity(indexed.len());
        let mut new_primary = 0;
        for (index, range) in indexed {
            let is_primary = index == self.primary;
            if let Some(last) = merged.last_mut() {
                let touches = if last.is_empty() || range.is_empty() {
                    range.start() <= last.end()
                } else {
                    range.start() < last.end()
                };
                if touches {
                    let start = last.start().min(range.start());
                    let end = last.end().max(range.end());
                    let reversed = if is_primary {
                        range.reversed()
                    } else {
                        last.reversed()
                    };
                    *last = if reversed {
                        Range::new(end, start)
                    } else {
                        Range::new(start, end)
                    };
                    if is_primary {
                        new_primary = merged.len() - 1;
                    }
                    continue;
                }
            }
            if is_primary {
                new_primary = merged.len();
            }
            merged.push(range);
        }
        let _ = primary;
        self.ranges = merged;
        self.primary = new_primary;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_ranges_merge_and_keep_primary() {
        let selection = Selection::new(
            vec![Range::new(0, 5), Range::new(3, 8), Range::caret(20)],
            1,
        );
        assert_eq!(selection.len(), 2);
        assert_eq!(selection.ranges()[0].start(), 0);
        assert_eq!(selection.ranges()[0].end(), 8);
        assert_eq!(selection.primary_index(), 0);
    }

    #[test]
    fn carets_at_one_place_merge() {
        let selection = Selection::new(vec![Range::caret(4), Range::caret(4), Range::caret(9)], 2);
        assert_eq!(selection.len(), 2);
        assert_eq!(selection.primary().head, 9);
    }

    #[test]
    fn adjacent_selections_stay_apart() {
        let selection = Selection::new(vec![Range::new(0, 3), Range::new(3, 6)], 0);
        assert_eq!(selection.len(), 2);
    }

    #[test]
    fn alt_click_removes_a_caret() {
        let selection = Selection::new(vec![Range::caret(1), Range::caret(9)], 1);
        let fewer = selection.without_at(9).unwrap();
        assert_eq!(fewer.len(), 1);
        assert_eq!(fewer.primary().head, 1);
        assert!(fewer.without_at(1).is_none());
    }
}
