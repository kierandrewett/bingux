//! A list's sections, apart from anything drawn: which header goes before
//! which row, and where the cursor goes over rows and headers.

/// One line of a list: a section's header, or the row at a place in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Entry {
    /// The header of the section at this place in the list's section titles.
    Header(usize),
    /// The row at this place in the rows.
    Item(usize),
}

/// The lines of a list of `len` rows whose row `at` belongs to the section
/// `section(at)`, and the section titles. A header goes before the first row
/// of each run of rows with the same section; rows with none have no header.
pub(super) fn flatten(
    len: usize,
    section: impl Fn(usize) -> Option<String>,
) -> (Vec<Entry>, Vec<String>) {
    let mut entries = Vec::with_capacity(len + 16);
    let mut titles: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for at in 0..len {
        let next = section(at);
        if next != current {
            if let Some(title) = &next {
                titles.push(title.clone());
                entries.push(Entry::Header(titles.len() - 1));
            }
            current = next;
        }
        entries.push(Entry::Item(at));
    }
    (entries, titles)
}

/// The nearest row at or after `from` going `delta` (1 or -1) over the lines,
/// skipping headers; `None` when there is none that way.
pub(super) fn next_item(entries: &[Entry], from: usize, delta: i32) -> Option<usize> {
    let mut at = from as i32;
    loop {
        at += delta;
        if at < 0 || at as usize >= entries.len() {
            return None;
        }
        if matches!(entries[at as usize], Entry::Item(_)) {
            return Some(at as usize);
        }
    }
}

/// The first or last row among the lines.
pub(super) fn end_item(entries: &[Entry], last: bool) -> Option<usize> {
    let mut lines = entries.iter().enumerate();
    if last {
        lines
            .rev()
            .find(|(_, entry)| matches!(entry, Entry::Item(_)))
            .map(|(at, _)| at)
    } else {
        lines
            .find(|(_, entry)| matches!(entry, Entry::Item(_)))
            .map(|(at, _)| at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_goes_before_each_run_of_a_section() {
        let sections = ["A", "A", "B", "B", "A"];
        let (entries, titles) = flatten(5, |at| Some(sections[at].to_string()));
        assert_eq!(titles, ["A", "B", "A"]);
        assert_eq!(
            entries,
            [
                Entry::Header(0),
                Entry::Item(0),
                Entry::Item(1),
                Entry::Header(1),
                Entry::Item(2),
                Entry::Item(3),
                Entry::Header(2),
                Entry::Item(4),
            ]
        );
    }

    #[test]
    fn rows_with_no_section_have_no_header() {
        let (entries, titles) = flatten(3, |at| (at == 2).then(|| "Later".to_string()));
        assert_eq!(titles, ["Later"]);
        assert_eq!(
            entries,
            [
                Entry::Item(0),
                Entry::Item(1),
                Entry::Header(0),
                Entry::Item(2)
            ]
        );
    }

    #[test]
    fn the_cursor_skips_headers() {
        let (entries, _) = flatten(4, |at| Some(if at < 2 { "A" } else { "B" }.to_string()));
        // Header, 0, 1, Header, 2, 3
        assert_eq!(next_item(&entries, 2, 1), Some(4));
        assert_eq!(next_item(&entries, 4, -1), Some(2));
        assert_eq!(next_item(&entries, 5, 1), None);
        assert_eq!(next_item(&entries, 1, -1), None);
        assert_eq!(end_item(&entries, false), Some(1));
        assert_eq!(end_item(&entries, true), Some(5));
    }
}
