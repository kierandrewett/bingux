//! The rules of Rename for several items, with no window: what each name
//! becomes for a find and replace, a change of case and a number, and why the
//! Rename button cannot run. The sheet ([`super::batch_rename`]) draws these.
//!
//! A rule changes the part of a name before its extension (`Holiday 1` of
//! `Holiday 1.jpg`, the whole name of a folder), so `.jpg` stays `.jpg`
//! whatever is replaced. The steps run in a fixed order: find and replace,
//! then the case, then the number.

use std::collections::HashMap;

/// How the letters of a name change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CaseChange {
    #[default]
    Keep,
    Lower,
    Upper,
    /// The first letter of each word in capitals, the rest small.
    Title,
}

/// Where a name's number goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Numbering {
    #[default]
    Off,
    /// `Holiday 1.jpg`.
    After,
    /// `1 Holiday.jpg`.
    Before,
}

/// What a person set in the sheet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rule {
    pub find: String,
    pub replace: String,
    pub match_case: bool,
    pub case: CaseChange,
    pub numbering: Numbering,
    /// The first number. Numbers are padded with zeros to the width of the
    /// last, so they sort in order (`01` to `12`).
    pub start: u32,
}

/// Why one name cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    Empty,
    Slash,
    /// `.` or `..`.
    Dots,
    /// Another item of the batch would have the same name.
    Twice,
    /// An item that is not being renamed has the name already.
    Taken,
}

impl Problem {
    /// What the row says.
    pub fn text(&self) -> String {
        match self {
            Problem::Empty => crate::tr!("ui-batch-rename-problem-empty"),
            Problem::Slash => crate::tr!("ui-batch-rename-problem-slash"),
            Problem::Dots => crate::tr!("ui-batch-rename-problem-dots"),
            Problem::Twice => crate::tr!("ui-batch-rename-problem-twice"),
            Problem::Taken => crate::tr!("ui-batch-rename-problem-taken"),
        }
    }
}

/// One item's old and new name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Planned {
    pub from: String,
    pub to: String,
    pub problem: Option<Problem>,
}

impl Planned {
    pub fn changes(&self) -> bool {
        self.from != self.to
    }
}

/// The part of `name` a rule changes and the extension it keeps: `.jpg` of a
/// file, nothing of a folder or of a name that starts with its only dot.
fn split(name: &str, folder: bool) -> (&str, &str) {
    if folder {
        return (name, "");
    }
    match name.rfind('.') {
        Some(at) if at > 0 => (&name[..at], &name[at..]),
        _ => (name, ""),
    }
}

/// `text` with every `find` replaced, ignoring case unless `match_case`.
fn replace_all(text: &str, find: &str, replace: &str, match_case: bool) -> String {
    if find.is_empty() {
        return text.to_string();
    }
    if match_case {
        return text.replace(find, replace);
    }
    let wanted: Vec<char> = find.chars().flat_map(char::to_lowercase).collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let mut lowered = Vec::with_capacity(wanted.len());
        let mut end = at;
        while end < chars.len() && lowered.len() < wanted.len() {
            lowered.extend(chars[end].1.to_lowercase());
            end += 1;
        }
        if lowered == wanted {
            out.push_str(replace);
            at = end;
        } else {
            out.push(chars[at].1);
            at += 1;
        }
    }
    out
}

fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = c.is_whitespace() || matches!(c, '-' | '_' | '(' | '[');
        }
    }
    out
}

/// What `name` (a folder when `folder`) becomes as item `index` of the batch,
/// whose last number has `width` digits.
pub(crate) fn rename_one(
    name: &str,
    folder: bool,
    rule: &Rule,
    index: usize,
    width: usize,
) -> String {
    let (stem, extension) = split(name, folder);
    let replaced = replace_all(stem, &rule.find, &rule.replace, rule.match_case);
    let cased = match rule.case {
        CaseChange::Keep => replaced,
        CaseChange::Lower => replaced.to_lowercase(),
        CaseChange::Upper => replaced.to_uppercase(),
        CaseChange::Title => title_case(&replaced),
    };
    let number = rule.start as usize + index;
    let numbered = match rule.numbering {
        Numbering::Off => cased,
        Numbering::After if cased.is_empty() => format!("{number:0width$}"),
        Numbering::After => format!("{cased} {number:0width$}"),
        Numbering::Before if cased.is_empty() => format!("{number:0width$}"),
        Numbering::Before => format!("{number:0width$} {cased}"),
    };
    format!("{numbered}{extension}")
}

/// Every item's new name, in the order given, with what is wrong with it.
/// `items` are each name and whether it is a folder; `taken` says whether a
/// name belongs to something in the folder that is not being renamed.
pub(crate) fn plan(
    items: &[(String, bool)],
    rule: &Rule,
    taken: impl Fn(&str) -> bool,
) -> Vec<Planned> {
    let last = rule.start as usize + items.len().saturating_sub(1);
    let width = last.to_string().len();
    let mut planned: Vec<Planned> = items
        .iter()
        .enumerate()
        .map(|(index, (name, folder))| Planned {
            from: name.clone(),
            to: rename_one(name, *folder, rule, index, width),
            problem: None,
        })
        .collect();
    let mut count: HashMap<String, usize> = HashMap::new();
    for item in &planned {
        *count.entry(item.to.clone()).or_default() += 1;
    }
    let olds: Vec<String> = planned.iter().map(|item| item.from.clone()).collect();
    for item in &mut planned {
        item.problem = if item.to.trim().is_empty() {
            Some(Problem::Empty)
        } else if item.to.contains('/') {
            Some(Problem::Slash)
        } else if item.to == "." || item.to == ".." {
            Some(Problem::Dots)
        } else if count.get(&item.to).copied().unwrap_or(0) > 1 {
            Some(Problem::Twice)
        } else if item.changes() && !olds.contains(&item.to) && taken(&item.to) {
            Some(Problem::Taken)
        } else {
            None
        };
    }
    planned
}

/// Why Rename cannot run for `planned`, or `None` when it can.
pub(crate) fn blocked(planned: &[Planned]) -> Option<String> {
    if let Some(problem) = planned.iter().find_map(|item| item.problem.as_ref()) {
        return Some(problem.text());
    }
    if !planned.iter().any(Planned::changes) {
        return Some(crate::tr!("ui-batch-rename-nothing-changes"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<(String, bool)> {
        names.iter().map(|name| (name.to_string(), false)).collect()
    }

    fn to(planned: &[Planned]) -> Vec<&str> {
        planned.iter().map(|item| item.to.as_str()).collect()
    }

    #[test]
    fn find_and_replace_leaves_the_extension_alone() {
        let rule = Rule {
            find: "jpg".into(),
            replace: "photo".into(),
            ..Rule::default()
        };
        let planned = plan(&files(&["IMG jpg.jpg", "a.JPG"]), &rule, |_| false);
        assert_eq!(to(&planned), ["IMG photo.jpg", "a.JPG"]);
    }

    #[test]
    fn find_ignores_case_unless_asked() {
        let mut rule = Rule {
            find: "img_".into(),
            replace: "Holiday ".into(),
            ..Rule::default()
        };
        let names = files(&["IMG_0001.jpg", "img_0002.jpg"]);
        assert_eq!(
            to(&plan(&names, &rule, |_| false)),
            ["Holiday 0001.jpg", "Holiday 0002.jpg"]
        );
        rule.match_case = true;
        assert_eq!(
            to(&plan(&names, &rule, |_| false)),
            ["IMG_0001.jpg", "Holiday 0002.jpg"]
        );
    }

    #[test]
    fn case_changes_the_name_not_the_extension() {
        let names = files(&["my holiday-photo.JPG"]);
        let with = |case| {
            to(&plan(
                &names,
                &Rule {
                    case,
                    ..Rule::default()
                },
                |_| false,
            ))[0]
                .to_string()
        };
        assert_eq!(with(CaseChange::Upper), "MY HOLIDAY-PHOTO.JPG");
        assert_eq!(with(CaseChange::Lower), "my holiday-photo.JPG");
        assert_eq!(with(CaseChange::Title), "My Holiday-Photo.JPG");
    }

    #[test]
    fn numbers_are_padded_to_the_last_one() {
        let names: Vec<(String, bool)> = (0..12).map(|n| (format!("p{n}.png"), false)).collect();
        let rule = Rule {
            find: "p".into(),
            numbering: Numbering::After,
            start: 1,
            ..Rule::default()
        };
        let planned = plan(&names, &rule, |_| false);
        assert_eq!(planned[0].to, "0 01.png");
        assert_eq!(planned[11].to, "11 12.png");
        let rule = Rule {
            numbering: Numbering::Before,
            start: 7,
            ..Rule::default()
        };
        assert_eq!(
            to(&plan(&files(&["a.txt", "b.txt"]), &rule, |_| false)),
            ["7 a.txt", "8 b.txt"]
        );
    }

    #[test]
    fn a_folder_has_no_extension() {
        let rule = Rule {
            case: CaseChange::Upper,
            ..Rule::default()
        };
        let planned = plan(&[("v1.2 notes".to_string(), true)], &rule, |_| false);
        assert_eq!(planned[0].to, "V1.2 NOTES");
    }

    #[test]
    fn clashes_and_bad_names_block_the_rename() {
        let rule = Rule {
            find: "a".into(),
            replace: "b".into(),
            ..Rule::default()
        };
        // Two items would end as one name.
        let planned = plan(&files(&["a.txt", "b.txt"]), &rule, |_| false);
        assert_eq!(planned[0].problem, Some(Problem::Twice));
        assert!(blocked(&planned).is_some());
        // A name that something else in the folder has.
        let planned = plan(&files(&["a.txt"]), &rule, |name| name == "b.txt");
        assert_eq!(planned[0].problem, Some(Problem::Taken));
        // Swapping two names of the batch is not a clash with the folder.
        let swap = Rule {
            find: "1".into(),
            replace: "2".into(),
            ..Rule::default()
        };
        let planned = plan(&files(&["x1", "y2"]), &swap, |name| name == "y2");
        assert_eq!(planned[0].problem, None);
        let slash = Rule {
            find: "a".into(),
            replace: "/".into(),
            ..Rule::default()
        };
        assert_eq!(
            plan(&files(&["a.txt"]), &slash, |_| false)[0].problem,
            Some(Problem::Slash)
        );
        let empty = Rule {
            find: "a".into(),
            ..Rule::default()
        };
        assert_eq!(
            plan(&files(&["a"]), &empty, |_| false)[0].problem,
            Some(Problem::Empty)
        );
        // Nothing set: nothing to do.
        assert_eq!(
            blocked(&plan(&files(&["a.txt"]), &Rule::default(), |_| false)).as_deref(),
            Some("Nothing would change.")
        );
    }
}
