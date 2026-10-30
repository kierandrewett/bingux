//! Bounded edit distance for typo tolerance.

/// The typo budget of a word of `chars` characters.
pub fn budget(chars: usize, one: usize, two: usize) -> u8 {
    if chars >= two {
        2
    } else if chars >= one {
        1
    } else {
        0
    }
}

/// Rows reused between calls, so scanning a dictionary does not allocate.
#[derive(Default)]
pub struct Scratch {
    prev2: Vec<usize>,
    prev: Vec<usize>,
    cur: Vec<usize>,
}

impl Scratch {
    fn rows(&mut self, len: usize) -> (&mut Vec<usize>, &mut Vec<usize>, &mut Vec<usize>) {
        for v in [&mut self.prev2, &mut self.prev, &mut self.cur] {
            v.clear();
            v.resize(len, 0);
        }
        (&mut self.prev2, &mut self.prev, &mut self.cur)
    }
}

/// Whether `a` and `b` could be within `max` edits, judged from the common
/// prefix and suffix only: a cheap test that never rejects a true match.
pub fn could_match(a: &[char], b: &[char], max: u8) -> bool {
    let p = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let s = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    p + s + 2 * max as usize >= a.len().max(b.len())
}

/// Whether the bag distance of `a` and `b` (how many characters one has that
/// the other lacks) is within `max`. It never exceeds the edit distance, so a
/// word it rejects cannot match. Non-ASCII text is not judged.
pub fn bag_ok(a: &[char], b: &[char], max: u8) -> bool {
    let mut counts = [0i8; 128];
    for &c in a {
        if !c.is_ascii() {
            return true;
        }
        counts[c as usize] = counts[c as usize].saturating_add(1);
    }
    for &c in b {
        if !c.is_ascii() {
            return true;
        }
        counts[c as usize] = counts[c as usize].saturating_sub(1);
    }
    let (mut pos, mut neg) = (0usize, 0usize);
    for &n in &counts {
        if n > 0 {
            pos += n as usize;
        } else if n < 0 {
            neg += (-n) as usize;
        }
    }
    pos.max(neg) <= max as usize
}

/// Optimal-string-alignment distance (insert, delete, substitute, swap of
/// neighbours each cost 1) between `a` and `b`, or `None` when it exceeds `max`.
pub fn distance(a: &[char], b: &[char], max: u8, scratch: &mut Scratch) -> Option<u8> {
    let max = max as usize;
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let (n, m) = (a.len(), b.len());
    let (prev2, prev, cur) = scratch.rows(m + 1);
    for (j, v) in prev.iter_mut().enumerate() {
        *v = j;
    }
    for i in 1..=n {
        cur[0] = i;
        let mut row_min = cur[0];
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(prev2[j - 2] + 1);
            }
            cur[j] = v;
            row_min = row_min.min(v);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(prev2, prev);
        std::mem::swap(prev, cur);
    }
    let d = prev[m];
    (d <= max).then_some(d as u8)
}

/// The smallest distance between `word` and any prefix of `term`, or `None`
/// beyond `max`. This is what makes the last word of a query typo tolerant
/// while it is still being typed.
pub fn prefix_distance(
    word: &[char],
    term: &[char],
    max: u8,
    scratch: &mut Scratch,
) -> Option<u8> {
    let max = max as usize;
    let n = word.len();
    // The prefix of term can be at most n + max long and at least n - max.
    if term.len() + max < n {
        return None;
    }
    let limit = term.len().min(n + max);
    // dp[i][j]: distance between word[..i] and term[..j]; we need min over j
    // of dp[n][j]. Compute column by column over term.
    let (prev2, prev, cur) = scratch.rows(n + 1);
    for (i, v) in prev.iter_mut().enumerate() {
        *v = i;
    }
    let mut best = if n <= max { Some(prev[n]) } else { None };
    for j in 1..=limit {
        cur[0] = j;
        let mut col_min = cur[0];
        for i in 1..=n {
            let cost = usize::from(word[i - 1] != term[j - 1]);
            let mut v = (prev[i] + 1).min(cur[i - 1] + 1).min(prev[i - 1] + cost);
            if i > 1 && j > 1 && word[i - 1] == term[j - 2] && word[i - 2] == term[j - 1] {
                v = v.min(prev2[i - 2] + 1);
            }
            cur[i] = v;
            col_min = col_min.min(v);
        }
        if cur[n] <= max {
            best = Some(best.map_or(cur[n], |b| b.min(cur[n])));
        }
        if col_min > max {
            break;
        }
        std::mem::swap(prev2, prev);
        std::mem::swap(prev, cur);
    }
    best.map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn thresholds() {
        assert_eq!(budget(3, 4, 8), 0);
        assert_eq!(budget(4, 4, 8), 1);
        assert_eq!(budget(7, 4, 8), 1);
        assert_eq!(budget(8, 4, 8), 2);
    }

    #[test]
    fn distance_counts_swaps_as_one() {
        assert_eq!(distance(&c("proxy"), &c("porxy"), 1, &mut Scratch::default()), Some(1));
        assert_eq!(distance(&c("proxy"), &c("prxy"), 1, &mut Scratch::default()), Some(1));
        assert_eq!(distance(&c("proxy"), &c("pxorb"), 2, &mut Scratch::default()), None);
        assert_eq!(distance(&c("same"), &c("same"), 0, &mut Scratch::default()), Some(0));
    }

    #[test]
    fn prefix_distance_finds_the_best_prefix() {
        assert_eq!(prefix_distance(&c("bluet"), &c("bluetooth"), 1, &mut Scratch::default()), Some(0));
        assert_eq!(prefix_distance(&c("blutoo"), &c("bluetooth"), 1, &mut Scratch::default()), Some(1));
        assert_eq!(prefix_distance(&c("xyz"), &c("bluetooth"), 1, &mut Scratch::default()), None);
    }
}
