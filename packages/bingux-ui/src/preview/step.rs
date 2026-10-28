//! Stepping from one previewed file to the next with the arrow keys.

/// A step between neighbours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Previous,
    Next,
    Up,
    Down,
}

/// The row after stepping from `from` among `len` rows laid out `columns`
/// to a line (a list has one). `None` at an edge: the preview stays on the
/// last file instead of wrapping round.
pub fn neighbour(from: usize, len: usize, step: Step, columns: usize) -> Option<usize> {
    let columns = columns.max(1);
    let to = match step {
        Step::Previous => from.checked_sub(1)?,
        Step::Next => from + 1,
        Step::Up => from.checked_sub(columns)?,
        Step::Down => from + columns,
    };
    (from < len && to < len).then_some(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_stops_at_the_edges() {
        assert_eq!(neighbour(0, 5, Step::Next, 1), Some(1));
        assert_eq!(neighbour(0, 5, Step::Previous, 1), None);
        assert_eq!(neighbour(4, 5, Step::Next, 1), None);
        // A grid of three columns: down from 1 is 4, and 4 has nothing below.
        assert_eq!(neighbour(1, 5, Step::Down, 3), Some(4));
        assert_eq!(neighbour(4, 5, Step::Down, 3), None);
        assert_eq!(neighbour(4, 5, Step::Up, 3), Some(1));
        assert_eq!(neighbour(1, 5, Step::Up, 3), None);
        // A list has one column, so up and down step by one.
        assert_eq!(neighbour(2, 5, Step::Down, 0), Some(3));
        assert_eq!(neighbour(9, 5, Step::Previous, 1), None, "from past the end");
    }
}
