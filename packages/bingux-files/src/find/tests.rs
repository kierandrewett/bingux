//! Tests of the file search: the query language, the matcher, the walk, the
//! engine with its index, and the recent searches.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::civil::{Clock, Date, Moment, civil_from_days, days_from_civil};
use super::engine::{Event, Plan, run};
use super::history::{MAX_RECENT, Recents};
use super::index::NameIndex;
use super::matcher::{Matcher, Tier};
use super::query::*;
use super::walk::{WalkOptions, Walked, walk};
use crate::entry::{Entry, Kind as EntryKind};
use crate::filter::Filter;
use crate::listing::Cancel;

// ---------------------------------------------------------------------------
// The query language
// ---------------------------------------------------------------------------

fn parse(text: &str) -> Query {
    Query::parse(text)
}

#[test]
fn plain_words_and_phrases() {
    let query = parse(r#"annual "tax return" -draft"#);
    let terms: Vec<(&str, bool, bool)> = query
        .terms
        .iter()
        .map(|term| (term.text(), term.is_phrase(), term.is_excluded()))
        .collect();
    assert_eq!(terms, [("annual", false, false), ("tax return", true, false), ("draft", false, true)]);
    assert_eq!(query.filters, Filters::default());
    assert_eq!(query.plain_text(), "annual tax return");
    assert!(parse("   ").is_blank());
    assert!(parse("").terms.is_empty());
}

#[test]
fn every_operator_reads() {
    let (query, problems) = Query::parse_checked(
        "report name:*.tar.gz ext:PNG,.jpg type:images,Code kind:video size:>10MB modified:2024-05 \
         in:\"My Documents\" hidden:only case:yes match:starts sort:date",
    );
    assert!(problems.is_empty(), "{problems:?}");
    let f = &query.filters;
    assert_eq!(f.globs, ["*.tar.gz"]);
    assert_eq!(f.exts, ["png", "jpg"]);
    assert_eq!(f.kinds, [Kind::Image, Kind::Video, Kind::Code]);
    assert_eq!(f.size, Some(SizeCond::Gt(10_000_000)));
    assert_eq!(f.modified, Some(Modified::On(Moment { year: 2024, month: Some(5), day: None })));
    assert_eq!(f.within.as_deref(), Some("My Documents"));
    assert_eq!(f.hidden, Hidden::Only);
    assert!(f.case_sensitive);
    assert_eq!(f.name_mode, NameMode::Starts);
    assert_eq!(f.sort, SortBy::Modified);
    assert_eq!(query.plain_text(), "report");
    assert_eq!(f.count(), 8);
    assert_eq!(
        f.active(),
        [
            FilterKey::Kind,
            FilterKey::Modified,
            FilterKey::Size,
            FilterKey::Ext,
            FilterKey::Name,
            FilterKey::Within,
            FilterKey::Hidden,
            FilterKey::Case
        ]
    );
}

#[test]
fn sizes_dates_and_hidden_in_all_their_forms() {
    let size = |text: &str| parse(&format!("size:{text}")).filters.size;
    assert_eq!(size(">=1.5GB"), Some(SizeCond::Ge(1_500_000_000)));
    assert_eq!(size("<1KiB"), Some(SizeCond::Lt(1024)));
    assert_eq!(size("<=300"), Some(SizeCond::Le(300)));
    assert_eq!(size("=0"), Some(SizeCond::Eq(0)));
    assert_eq!(size("5k"), Some(SizeCond::Eq(5_000)));
    assert_eq!(size("1MB..1GB"), Some(SizeCond::Between(1_000_000, 1_000_000_000)));
    assert_eq!(size("..2m"), Some(SizeCond::Le(2_000_000)));
    assert_eq!(size("2m.."), Some(SizeCond::Ge(2_000_000)));
    for bad in ["big", ">", "1GB..1MB", "..", "1.2.3MB", "12XB", "-5"] {
        assert_eq!(size(bad), None, "{bad}");
    }
    let when = |text: &str| parse(&format!("modified:{text}")).filters.modified;
    assert_eq!(when("today"), Some(Modified::Today));
    assert_eq!(when("YESTERDAY"), Some(Modified::Yesterday));
    assert_eq!(when("week"), Some(Modified::Days(7)));
    assert_eq!(when("7d"), Some(Modified::Days(7)));
    assert_eq!(when("month"), Some(Modified::Days(30)));
    assert_eq!(when("45days"), Some(Modified::Days(45)));
    assert_eq!(when("year"), Some(Modified::ThisYear));
    let day = Moment { year: 2024, month: Some(2), day: Some(29) };
    assert_eq!(when("2024-02-29"), Some(Modified::On(day)));
    assert_eq!(when(">2024-02-29"), Some(Modified::Since(day)));
    assert_eq!(when("<2024-02-29"), Some(Modified::Before(day)));
    assert_eq!(
        when("2024-01..2024-02-29"),
        Some(Modified::Between(Moment { year: 2024, month: Some(1), day: None }, day))
    );
    for bad in ["2023-02-29", "2024-13", "24", "0d", "soon", "2024-02-30", "2024-03..2024-01"] {
        assert_eq!(when(bad), None, "{bad}");
    }
    assert_eq!(parse("hidden:yes").filters.hidden, Hidden::Include);
    assert_eq!(parse("hidden:no").filters.hidden, Hidden::Exclude);
    assert_eq!(parse("regex:yes").filters.name_mode, NameMode::Regex);
    assert_eq!(parse("regex:no").filters.name_mode, NameMode::Contains);
}

#[test]
fn what_cannot_be_read_stays_a_phrase_and_is_reported() {
    let (query, problems) = Query::parse_checked("size:big type:cats hidden:maybe ok");
    assert_eq!(
        problems.iter().map(|p| (p.key, p.value.as_str())).collect::<Vec<_>>(),
        [("size", "big"), ("type", "cats"), ("hidden", "maybe")]
    );
    let words: Vec<&str> = query.terms.iter().map(Term::text).collect();
    assert_eq!(words, ["size:big", "type:cats", "hidden:maybe", "ok"]);
    assert!(query.terms[0].is_phrase());
    assert_eq!(query.filters, Filters::default());
    // A key still being typed is neither a word nor a problem.
    let (query, problems) = Query::parse_checked("type: ext:");
    assert!(problems.is_empty() && query.terms.is_empty());
    // An unknown key is an ordinary word.
    assert_eq!(parse("foo:bar").terms[0].text(), "foo:bar");
    // Several type and extension tokens add up; the last of the others wins.
    let query = parse("type:image type:video ext:png ext:jpg size:>1k size:<5k");
    assert_eq!(query.filters.kinds, [Kind::Image, Kind::Video]);
    assert_eq!(query.filters.exts, ["png", "jpg"]);
    assert_eq!(query.filters.size, Some(SizeCond::Lt(5_000)));
}

#[test]
fn filters_clear_one_at_a_time_and_all_at_once() {
    let mut filters = parse("type:image size:>1MB modified:today match:regex sort:name").filters;
    filters.clear(FilterKey::Size);
    assert_eq!(filters.size, None);
    assert_eq!(filters.count(), 3);
    filters.clear(FilterKey::Name);
    assert_eq!(filters.name_mode, NameMode::Contains);
    filters.clear_all();
    assert_eq!(filters.count(), 0);
    assert_eq!(filters.sort, SortBy::Name, "the sort is not a filter");
}

/// A small deterministic generator, so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

const WORDS: &[&str] = &[
    "report", "a", "ünï", "日本", "my file", "-lead", "size:big", "type:image", "a:b", "x\"y", "  pad ", "名前 好き", "5\"",
    "name:", "-", "--", "ext:png", "hidden:", "in:x y", "tab\tsep", "RePoRt", "a.b", "..", "",
];

fn random_value(rng: &mut Rng) -> String {
    rng.pick(&["Photos", "a b", "~/x", "../up", "/abs/path", "été", "x.y", "dir/sub dir"]).to_string()
}

fn random_moment(rng: &mut Rng) -> Moment {
    let year = 1970 + rng.below(100) as i32;
    match rng.below(3) {
        0 => Moment { year, month: None, day: None },
        1 => Moment { year, month: Some(1 + rng.below(12) as u8), day: None },
        _ => Moment { year, month: Some(1 + rng.below(12) as u8), day: Some(1 + rng.below(28) as u8) },
    }
}

fn random_query(rng: &mut Rng) -> Query {
    let mut query = Query::default();
    for _ in 0..rng.below(5) {
        if let Some(mut term) = Term::new(rng.pick(WORDS)) {
            if rng.chance(30) {
                term = Term::phrase(term.text()).unwrap_or(term);
            }
            if rng.chance(25) {
                term = term.excluded();
            }
            query.terms.push(term);
        }
    }
    let f = &mut query.filters;
    for _ in 0..rng.below(3) {
        f.globs.push(random_value(rng));
    }
    for _ in 0..rng.below(4) {
        let ext = rng.pick(&["png", "jpg", "tar", "gz", "rs", "ünï"]).to_string();
        if !f.exts.contains(&ext) {
            f.exts.push(ext);
        }
    }
    for kind in Kind::ALL {
        if rng.chance(25) {
            f.kinds.push(kind);
        }
    }
    let bytes = |rng: &mut Rng| match rng.below(4) {
        0 => rng.next() % 5000,
        1 => (rng.next() % 900) * 1_000,
        2 => (rng.next() % 900) * 1_000_000,
        _ => (rng.next() % 9) * 1_000_000_000_000,
    };
    f.size = match rng.below(7) {
        0 => Some(SizeCond::Gt(bytes(rng))),
        1 => Some(SizeCond::Ge(bytes(rng))),
        2 => Some(SizeCond::Lt(bytes(rng))),
        3 => Some(SizeCond::Le(bytes(rng))),
        4 => Some(SizeCond::Eq(bytes(rng))),
        5 => {
            let (a, b) = (bytes(rng), bytes(rng));
            Some(SizeCond::Between(a.min(b), a.max(b)))
        }
        _ => None,
    };
    f.modified = match rng.below(9) {
        0 => Some(Modified::Today),
        1 => Some(Modified::Yesterday),
        2 => Some(Modified::Days(1 + rng.below(400) as u32)),
        3 => Some(Modified::ThisYear),
        4 => Some(Modified::On(random_moment(rng))),
        5 => Some(Modified::Since(random_moment(rng))),
        6 => Some(Modified::Before(random_moment(rng))),
        7 => {
            let (a, b) = (random_moment(rng), random_moment(rng));
            let (from, to) = if a.start() <= b.start() { (a, b) } else { (b, a) };
            Some(Modified::Between(from, to))
        }
        _ => None,
    };
    if rng.chance(30) {
        f.within = Some(random_value(rng));
    }
    f.hidden = *rng.pick(&[Hidden::Follow, Hidden::Exclude, Hidden::Include, Hidden::Only]);
    f.case_sensitive = rng.chance(30);
    f.name_mode = *rng.pick(&[NameMode::Contains, NameMode::Starts, NameMode::Exact, NameMode::Regex]);
    f.sort = *rng.pick(&[SortBy::Relevance, SortBy::Name, SortBy::Modified, SortBy::Size]);
    query
}

#[test]
fn a_query_prints_as_text_that_reads_back_the_same() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..20_000 {
        let query = random_query(&mut rng);
        let text = query.to_string();
        let (back, problems) = Query::parse_checked(&text);
        assert!(problems.is_empty(), "round {round}: {text:?} -> {problems:?}");
        assert_eq!(back, query, "round {round}: {text:?}");
    }
}

#[test]
fn whatever_is_typed_reads_without_panic_and_settles() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let pieces = [
        " ", "  ", "\t", "\"", "-", ":", "..", ">", "<", "=", ",", "name", "ext", "type", "kind", "size", "modified", "date",
        "in", "hidden", "case", "match", "regex", "sort", "yes", "no", "only", "png", "10MB", "2024", "2024-05-17", "today",
        "week", "image", "é", "日本", "*", "?", "[", "\\", "0", "9999999999999999999999",
    ];
    for round in 0..30_000 {
        let mut text = String::new();
        for _ in 0..rng.below(14) {
            text.push_str(rng.pick(&pieces));
        }
        let first = Query::parse(&text);
        let printed = first.to_string();
        let second = Query::parse(&printed);
        assert_eq!(second, first, "round {round}: {text:?} printed {printed:?}");
        assert_eq!(second.to_string(), printed, "round {round}: {text:?}");
    }
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

#[test]
fn the_calendar_counts_days() {
    for days in (-800_000..800_000).step_by(997) {
        assert_eq!(days_from_civil(civil_from_days(days)), days);
    }
    assert_eq!(days_from_civil(Date { year: 1970, month: 1, day: 1 }), 0);
    assert_eq!(days_from_civil(Date { year: 2000, month: 3, day: 1 }), 11_017);
    assert_eq!(civil_from_days(19_782), Date { year: 2024, month: 2, day: 29 });
    let year = Moment::parse("2024").unwrap();
    assert_eq!(year.end(), Date { year: 2025, month: 1, day: 1 });
    let december = Moment::parse("2024-12").unwrap();
    assert_eq!(december.end(), Date { year: 2025, month: 1, day: 1 });
    let leap_day = Moment::parse("2024-02-29").unwrap();
    assert_eq!(leap_day.end(), Date { year: 2024, month: 3, day: 1 });
    assert_eq!(Moment::parse("2024-2"), None);
    assert_eq!(Moment::parse("2024-00"), None);
}

#[test]
fn today_is_the_persons_day_in_their_zone() {
    // 2024-05-17 23:30 UTC is already the 18th at UTC+2.
    let now = days_from_civil(Date { year: 2024, month: 5, day: 17 }) * 86_400 + 23 * 3600 + 1800;
    let clock = Clock::fixed(now, 2 * 3600);
    assert_eq!(clock.today(), Date { year: 2024, month: 5, day: 18 });
    let midnight = clock.midnight(Date { year: 2024, month: 5, day: 18 });
    assert_eq!(midnight, days_from_civil(Date { year: 2024, month: 5, day: 18 }) * 86_400 - 2 * 3600);
    assert_eq!(Clock::fixed(now, 0).today(), Date { year: 2024, month: 5, day: 17 });
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

fn matcher(text: &str) -> Matcher {
    Matcher::new(&parse(text)).expect("a query that can run")
}

fn tier(query: &str, name: &str) -> Option<Tier> {
    matcher(query).name_hit(name).map(|hit| hit.tier)
}

#[test]
fn ranks_matches_from_exact_to_fuzzy() {
    assert_eq!(tier("report", "Report"), Some(Tier::Exact));
    assert_eq!(tier("rep", "report.pdf"), Some(Tier::Prefix));
    assert_eq!(tier("pdf", "annual-report.pdf"), Some(Tier::Word));
    assert_eq!(tier("port", "report.pdf"), Some(Tier::Substring));
    assert_eq!(tier("2024 report", "report for 2024.txt"), Some(Tier::Words));
    assert_eq!(tier("rpt", "my_report.txt"), Some(Tier::Fuzzy));
    assert_eq!(tier("zzz", "report.pdf"), None);
    // Two letters never go fuzzy; far-apart letters do not match.
    assert_eq!(tier("rt", "report.pdf"), Some(Tier::Substring));
    assert_eq!(tier("rp", "report.pdf"), None);
    assert_eq!(tier("rxz", "report.txt"), None);
    assert_eq!(tier("abc", "a------------------b------------------c"), None);
    assert_eq!(tier("", "anything"), Some(Tier::Any));
}

#[test]
fn a_phrase_needs_its_words_side_by_side_and_is_never_fuzzy() {
    assert_eq!(tier("\"tax return\"", "my tax return 2024.pdf"), Some(Tier::Word));
    assert_eq!(tier("\"tax return\"", "return of tax.pdf"), None);
    assert_eq!(tier("\"rpt\"", "my_report.txt"), None);
    assert_eq!(tier("\"tax return\" 2024", "tax return 2024.pdf"), Some(Tier::Words));
    assert_eq!(tier("\"tax return\" 2025", "tax return 2024.pdf"), None);
}

#[test]
fn excluded_words_extensions_patterns_and_modes() {
    assert!(tier("report -draft", "report final.pdf").is_some());
    assert_eq!(tier("report -draft", "report draft.pdf"), None);
    assert_eq!(tier("-draft", "report final.pdf"), Some(Tier::Any));
    assert_eq!(tier("ext:pdf", "a.PDF"), Some(Tier::Any));
    assert_eq!(tier("ext:pdf", "pdf"), None);
    assert_eq!(tier("ext:gz", "a.tar.gz"), Some(Tier::Any));
    assert_eq!(tier("name:*.tar.gz", "a.tar.gz"), Some(Tier::Any));
    assert_eq!(tier("name:*.tar.gz", "a.zip"), None);
    assert_eq!(tier("name:IMG_????", "img_0001"), Some(Tier::Any));
    assert_eq!(tier("name:holiday", "my-Holiday-2024"), Some(Tier::Any));
    assert_eq!(tier("name:IMG_???? case:yes", "IMG_0001"), Some(Tier::Any));
    assert_eq!(tier("name:IMG_???? case:yes", "img_0001"), None, "case:yes holds for patterns too");
    assert_eq!(tier("name:Holiday case:yes", "my-holiday"), None);
    assert_eq!(tier("rep match:starts", "report.pdf"), Some(Tier::Prefix));
    assert_eq!(tier("rep match:starts", "my report"), None);
    assert_eq!(tier("report match:exact", "Report"), Some(Tier::Exact));
    assert_eq!(tier("report match:exact", "report.pdf"), None);
    assert_eq!(tier("Report case:yes", "Report"), Some(Tier::Exact));
    assert_eq!(tier("Report case:yes", "report"), None);
    assert_eq!(tier(r"^rep.*\d{4}$ match:regex", "report 2024"), Some(Tier::Exact));
    assert_eq!(tier(r"\d{4} match:regex", "report 2024.txt"), Some(Tier::Substring));
    assert_eq!(tier(r"^rep match:regex", "report 2024.txt"), Some(Tier::Prefix));
    assert_eq!(tier(r"\d{5} match:regex", "report 2024"), None);
    assert!(Matcher::new(&parse("( match:regex")).is_err());
}

#[test]
fn highlights_by_byte_on_character_boundaries() {
    let name = "mon école.txt";
    let ranges = matcher("ÉCOLE").highlight(name);
    assert_eq!(ranges.len(), 1);
    assert_eq!(&name[ranges[0].clone()], "école");
    let fuzzy = matcher("rpt").highlight("my_report.txt");
    let picked: Vec<&str> = fuzzy.iter().map(|r| &"my_report.txt"[r.clone()]).collect();
    assert_eq!(picked.concat(), "rpt");
    assert!(matcher("zz").highlight("a").is_empty());
    let regex = matcher(r"o+r match:regex").highlight("my_foor.txt");
    assert_eq!(&"my_foor.txt"[regex[0].clone()], "oor");
}

fn entry(name: &str) -> Entry {
    Entry::named(PathBuf::from("/x").join(name), name.to_string(), EntryKind::File, false, false)
}

fn sized(name: &str, size: u64, modified: i64) -> Entry {
    let mut entry = entry(name);
    entry.size = size;
    entry.modified = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(modified as u64));
    entry.sized = true;
    entry
}

#[test]
fn entries_pass_type_size_date_and_hidden() {
    let day = |y, m, d| days_from_civil(Date { year: y, month: m, day: d }) * 86_400;
    let clock = Clock::fixed(day(2024, 5, 17) + 12 * 3600, 0);
    let with = |text: &str| Matcher::with_clock(&parse(text), clock).unwrap();

    let png = sized("a.png", 2_000_000, day(2024, 5, 17) + 3600);
    let old_txt = sized("b.txt", 10, day(2023, 1, 1));
    assert!(with("type:image").accepts(&png));
    assert!(!with("type:image").accepts(&old_txt));
    assert!(with("type:document").accepts(&old_txt), "plain text is a document");
    assert!(with("type:code,image").accepts(&png));
    assert!(with("size:>1MB").accepts(&png) && !with("size:>1MB").accepts(&old_txt));
    assert!(with("size:<1KB").accepts(&old_txt));
    assert!(with("size:10").accepts(&old_txt));
    assert!(with("modified:today").accepts(&png) && !with("modified:today").accepts(&old_txt));
    assert!(with("modified:2023").accepts(&old_txt) && !with("modified:2023").accepts(&png));
    assert!(with("modified:year").accepts(&png) && !with("modified:year").accepts(&old_txt));
    assert!(with("modified:>2024-01-01").accepts(&png));
    assert!(with("modified:<2024").accepts(&old_txt));
    assert!(with("modified:week").accepts(&png));
    assert!(!with("modified:yesterday").accepts(&png));
    // Midnight is the boundary, in the person's zone.
    let late = sized("c", 1, day(2024, 5, 16) + 86_399);
    assert!(with("modified:yesterday").accepts(&late));
    assert!(!with("modified:today").accepts(&late));
    let zoned = Matcher::with_clock(&parse("modified:today"), Clock::fixed(day(2024, 5, 17) + 12 * 3600, 3600)).unwrap();
    let before_local_midnight = sized("d", 1, day(2024, 5, 17) - 1800);
    assert!(zoned.accepts(&before_local_midnight), "00:30 in UTC+1 is today");
    // A folder has no size.
    let folder = Entry::named(PathBuf::from("/x/f"), "f".into(), EntryKind::Directory, false, false);
    assert!(!with("size:<1KB").accepts(&folder));
    assert!(with("type:folder").accepts(&folder));
    let mut hidden = entry(".h");
    hidden.hidden = true;
    assert!(with("hidden:only").accepts(&hidden) && !with("hidden:only").accepts(&png));
    assert!(!with("hidden:no").accepts(&hidden) && with("hidden:no").accepts(&png));
    assert!(with("hidden:yes").accepts(&hidden));
    // No date at all fails a date filter.
    let mut undated = entry("u");
    undated.modified = None;
    assert!(!with("modified:today").accepts(&undated));
}

#[test]
fn pre_accepts_only_rules_out_what_the_name_settles() {
    let m = matcher("type:image");
    assert!(m.pre_accepts("a.png", false, false));
    assert!(!m.pre_accepts("a.txt", false, false));
    assert!(!m.pre_accepts("dir", true, false));
    assert!(m.pre_accepts("looks-like-text", false, true), "a link is decided by its target");
    assert!(matcher("type:folder").pre_accepts("dir", true, false));
    assert!(!matcher("size:>1").pre_accepts("dir", true, false));
}

#[test]
fn relevance_orders_by_tier_then_penalty_then_depth() {
    let root = Path::new("/x");
    let m = matcher("notes");
    let mut entries: Vec<Entry> = ["z-notes.txt", "znotes.txt", "notes.txt", "my_n_o_t_e.txt", "notes", "a notes b", "n-o-t-e-s.md"]
        .into_iter()
        .map(entry)
        .collect();
    entries.push(Entry::named(PathBuf::from("/x/deep/er/notes.txt"), "notes.txt".into(), EntryKind::File, false, false));
    entries.sort_by_cached_key(|e| m.rank(e, root));
    let order: Vec<&str> = entries.iter().map(|e| e.path.to_str().unwrap()).collect();
    assert_eq!(
        order,
        [
            "/x/notes",
            "/x/notes.txt",
            "/x/deep/er/notes.txt",
            "/x/z-notes.txt",
            "/x/a notes b",
            "/x/znotes.txt",
            "/x/n-o-t-e-s.md",
            "/x/my_n_o_t_e.txt",
        ]
    );
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

fn tree(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bingux-find-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn walked(root: &Path, text: &str, options: &WalkOptions) -> (Vec<String>, super::walk::Summary) {
    let matcher = matcher(text);
    let mut found = Vec::new();
    let summary = walk(&[root.to_path_buf()], &matcher, options, &HashSet::new(), &Cancel::new(), &mut |event| {
        if let Walked::Hits(entries) = event {
            found.extend(entries.into_iter().map(|e| super::relative_folder(&e.path, root) + "|" + &e.name));
        }
        true
    });
    found.sort();
    (found, summary)
}

#[test]
fn walks_subfolders_and_skips_what_it_should() {
    let root = tree("walk");
    for dir in ["a/b", ".secret", "node_modules/pkg", ".git", "target"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "needle.txt",
        "a/needle-a.txt",
        "a/b/needle-b.txt",
        ".secret/needle-s.txt",
        ".needle-hidden",
        "node_modules/pkg/needle-n.js",
        "target/needle-t.o",
        ".git/needle-g",
        "a/other.txt",
    ] {
        fs::write(root.join(file), b"x").unwrap();
    }
    // A link up the tree is reported when it matches but never entered.
    symlink(&root, root.join("a/needle-loop")).unwrap();

    let (found, summary) = walked(&root, "needle", &WalkOptions::default());
    assert_eq!(
        found,
        [
            "a/b|needle-b.txt",
            "a|needle-a.txt",
            "a|needle-loop",
            "node_modules/pkg|needle-n.js",
            "target|needle-t.o",
            "|needle.txt",
        ]
    );
    assert!(!summary.truncated && !summary.cancelled);
    assert_eq!(summary.hits, 6);

    let (found, _) = walked(&root, "needle", &WalkOptions { show_hidden: true, ..WalkOptions::default() });
    assert!(found.contains(&".secret|needle-s.txt".to_string()));
    assert!(found.contains(&"|.needle-hidden".to_string()));
    assert!(found.contains(&".git|needle-g".to_string()));

    let quiet = WalkOptions { include_noise: false, ..WalkOptions::default() };
    let (found, _) = walked(&root, "needle", &quiet);
    assert!(!found.iter().any(|f| f.contains("needle-n.js") || f.contains("needle-t.o")));
    assert!(found.contains(&"a/b|needle-b.txt".to_string()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_walk_applies_every_filter_before_it_counts() {
    let root = tree("filters");
    fs::create_dir_all(root.join("sub/find-folder")).unwrap();
    fs::write(root.join("a.txt"), vec![0; 10]).unwrap();
    fs::write(root.join("b.png"), vec![0; 5000]).unwrap();
    fs::write(root.join("sub/c.png"), vec![0; 10]).unwrap();
    fs::write(root.join("sub/d.txt"), vec![0; 9000]).unwrap();
    let all = WalkOptions::default();
    assert_eq!(walked(&root, "type:image", &all).0, ["sub|c.png", "|b.png"]);
    assert_eq!(walked(&root, "type:folder", &all).0, ["sub|find-folder", "|sub"]);
    assert_eq!(walked(&root, "ext:txt size:>1KB", &all).0, ["sub|d.txt"]);
    assert_eq!(walked(&root, "modified:today type:image", &all).0.len(), 2);
    assert_eq!(walked(&root, "modified:2001", &all).0, Vec::<String>::new());
    assert_eq!(walked(&root, "-png type:image", &all).0, Vec::<String>::new());
    // A chooser's filter and the limit: the files it rejects do not count.
    let only_png = WalkOptions { filter: Some(Filter::parse("Pictures", "png")), max_hits: 3, ..WalkOptions::default() };
    let (found, summary) = walked(&root, "", &only_png);
    assert_eq!(found, ["sub|find-folder", "|b.png", "|sub"]);
    assert!(summary.truncated, "{summary:?}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn does_not_cross_into_another_filesystem() {
    use std::os::unix::fs::MetadataExt;
    // `/dev/shm` is a tmpfs inside the devtmpfs `/dev`: a different device,
    // so a walk of `/dev` must not enter it.
    let (Ok(outer), Ok(inner)) = (fs::metadata("/dev"), fs::metadata("/dev/shm")) else {
        return;
    };
    if outer.dev() == inner.dev() {
        return;
    }
    let name = format!("bingux-needle-{}", std::process::id());
    let file = Path::new("/dev/shm").join(&name);
    if fs::write(&file, b"x").is_err() {
        return;
    }
    let (across, _) = walked(Path::new("/dev"), &name, &WalkOptions::default());
    let (inside, _) = walked(Path::new("/dev/shm"), &name, &WalkOptions::default());
    let _ = fs::remove_file(&file);
    assert!(across.is_empty(), "{across:?}");
    assert_eq!(inside.len(), 1);
}

#[test]
fn streams_stops_when_cancelled_and_when_the_limit_is_reached() {
    let root = tree("cancel");
    for dir in 0..20 {
        fs::create_dir_all(root.join(format!("d{dir}"))).unwrap();
        for file in 0..10 {
            fs::write(root.join(format!("d{dir}/hit-{file}")), b"x").unwrap();
        }
    }
    let m = matcher("hit");
    let roots = [root.clone()];
    let none = HashSet::new();

    let limited = WalkOptions { max_hits: 25, ..WalkOptions::default() };
    let mut count = 0;
    let summary = walk(&roots, &m, &limited, &none, &Cancel::new(), &mut |w| {
        if let Walked::Hits(entries) = w {
            count += entries.len();
        }
        true
    });
    assert!(summary.truncated);
    assert_eq!(count, 25);

    // The caller cancels while results stream: nothing more is read.
    let cancel = Cancel::new();
    let mut batches = 0;
    let stopper = cancel.clone();
    let summary = walk(&roots, &m, &WalkOptions::default(), &none, &cancel, &mut |w| {
        if matches!(w, Walked::Hits(_)) {
            batches += 1;
            stopper.cancel();
        }
        true
    });
    assert!(summary.cancelled);
    assert_eq!(batches, 1);
    assert!(summary.hits < 200);

    // A flag set before the walk starts means no work.
    let cancel = Cancel::new();
    cancel.cancel();
    let summary = walk(&roots, &m, &WalkOptions::default(), &none, &cancel, &mut |_| true);
    assert!(summary.cancelled);
    assert_eq!(summary.hits, 0);

    // `emit` returning false stops it too.
    let summary = walk(&roots, &m, &WalkOptions::default(), &none, &Cancel::new(), &mut |_| false);
    assert!(summary.cancelled);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn several_roots_are_walked_in_turn_and_never_twice() {
    let root = tree("roots");
    for dir in ["one", "two", "one/inner"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in ["one/needle-1", "two/needle-2", "one/inner/needle-3"] {
        fs::write(root.join(file), b"x").unwrap();
    }
    let m = matcher("needle");
    let mut found = Vec::new();
    // `one/inner` lies under `one`: its files must not come twice.
    let roots = [root.join("one"), root.join("two"), root.join("one/inner")];
    let summary = walk(&roots, &m, &WalkOptions::default(), &HashSet::new(), &Cancel::new(), &mut |w| {
        if let Walked::Hits(entries) = w {
            found.extend(entries.into_iter().map(|e| e.name));
        }
        true
    });
    found.sort();
    assert_eq!(found, ["needle-1", "needle-2", "needle-3"]);
    assert_eq!(summary.hits, 3);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn relative_folders_read_below_the_root() {
    let root = Path::new("/h/me");
    assert_eq!(super::relative_folder(Path::new("/h/me/a.txt"), root), "");
    assert_eq!(super::relative_folder(Path::new("/h/me/Photos/2024/a.jpg"), root), "Photos/2024");
    assert_eq!(super::relative_folder(Path::new("/elsewhere/a.jpg"), root), "/elsewhere");
}

// ---------------------------------------------------------------------------
// The engine and the index
// ---------------------------------------------------------------------------

fn plan(root: &Path, text: &str, index: Option<&std::sync::Arc<NameIndex>>) -> Plan {
    Plan {
        query: parse(text),
        roots: vec![root.to_path_buf()],
        home: root.to_path_buf(),
        view_hidden: false,
        include_noise: true,
        chooser_filter: None,
        max_hits: 10_000,
        index: index.cloned(),
    }
}

/// Everything the engine found, in order, and how it ended.
fn found_by(plan: &Plan) -> (Vec<PathBuf>, super::engine::Summary) {
    let mut found = Vec::new();
    let summary = run(plan, &Cancel::new(), &mut |event| {
        if let Event::Found(entries) = event {
            found.extend(entries.into_iter().map(|e| e.path));
        }
        true
    });
    (found, summary)
}

fn wait_until_ready(index: &NameIndex) {
    let start = Instant::now();
    while !index.ready() {
        assert!(start.elapsed() < Duration::from_secs(20), "the crawl did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn make_library(root: &Path) {
    for dir in ["Documents/Taxes", "Pictures/Holiday", "Music", ".hidden-dir", "node_modules/pkg"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "Documents/report-2024.pdf",
        "Documents/Taxes/annual report.txt",
        "Documents/myreport.txt",
        "Documents/reprot.txt",
        "Pictures/Holiday/beach.png",
        "Pictures/Holiday/report-card.png",
        "Music/song.mp3",
        ".hidden-dir/report-secret.txt",
        "node_modules/pkg/report-noise.js",
        "report",
    ] {
        fs::write(root.join(file), b"x").unwrap();
    }
}

#[test]
fn without_an_index_the_engine_is_the_walk() {
    let root = tree("engine-walk");
    make_library(&root);
    let (found, summary) = found_by(&plan(&root, "report", None));
    let names: HashSet<String> = found.iter().map(|p| p.strip_prefix(&root).unwrap().display().to_string()).collect();
    assert!(names.contains("Documents/report-2024.pdf"));
    assert!(names.contains("Documents/myreport.txt"), "the walk finds the middle of a word");
    assert!(names.contains("report"));
    assert!(!names.iter().any(|n| n.starts_with(".hidden-dir")), "hidden stays hidden");
    assert_eq!(summary.indexed, 0);
    assert_eq!(summary.hits, found.len());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_index_answers_first_nothing_comes_twice_and_the_walk_adds_the_rest() {
    let root = tree("engine-index");
    make_library(&root);
    let index = NameIndex::new(&root);
    index.refresh();
    wait_until_ready(&index);
    assert!(index.len() >= 9, "{}", index.len());

    let mut events = Vec::new();
    let mut found: Vec<PathBuf> = Vec::new();
    let summary = run(&plan(&root, "report", Some(&index)), &Cancel::new(), &mut |event| {
        if let Event::Found(entries) = &event {
            found.extend(entries.iter().map(|e| e.path.clone()));
        }
        events.push(event);
        true
    });
    // No path twice.
    let unique: HashSet<&PathBuf> = found.iter().collect();
    assert_eq!(unique.len(), found.len(), "{found:?}");
    assert!(summary.indexed >= 4, "{summary:?}");
    // The first batch is the index's, and it is in relevance order: the
    // exact name first.
    let Some(Event::Found(first)) = events.first() else {
        panic!("no results: {events:?}");
    };
    assert_eq!(first[0].path, root.join("report"));
    // What only the walk finds (the middle of a word, noise folders) comes
    // after the index's answer.
    let after = |name: &str| found.iter().position(|p| p == &root.join(name)).unwrap_or_else(|| panic!("{name} missing: {found:?}"));
    assert!(after("report") < after("Documents/myreport.txt"));
    assert!(after("Documents/report-2024.pdf") < after("Documents/myreport.txt"));
    assert!(after("node_modules/pkg/report-noise.js") > summary.indexed - 1);
    // The index also offers near misses the walk cannot: `reprot` is a typo of `report`.
    assert!(found.contains(&root.join("Documents/reprot.txt")), "a near miss: {found:?}");
    assert_eq!(summary.hits, found.len());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_index_is_scoped_and_filtered_like_the_walk() {
    let root = tree("engine-scope");
    make_library(&root);
    let index = NameIndex::new(&root);
    index.refresh();
    wait_until_ready(&index);
    let scoped = Plan { roots: vec![root.join("Pictures")], ..plan(&root, "report", Some(&index)) };
    let (found, _) = found_by(&scoped);
    assert_eq!(found, [root.join("Pictures/Holiday/report-card.png")]);
    let (found, _) = found_by(&plan(&root, "type:image", Some(&index)));
    let names: HashSet<&str> = found.iter().filter_map(|p| p.file_name()?.to_str()).collect();
    assert_eq!(names, HashSet::from(["beach.png", "report-card.png"]));
    let (found, _) = found_by(&plan(&root, "ext:txt in:Documents", Some(&index)));
    assert_eq!(found.len(), 3, "{found:?}");
    let (found, summary) = found_by(&plan(&root, "in:Nowhere", Some(&index)));
    assert!(found.is_empty() && summary.missing.as_deref() == Some("Nowhere"));
    let (found, summary) = found_by(&plan(&root, "( match:regex", None));
    assert!(found.is_empty() && summary.invalid.is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_file_made_after_the_crawl_is_found_by_the_walk() {
    let root = tree("engine-fresh");
    make_library(&root);
    let index = NameIndex::new(&root);
    index.refresh();
    wait_until_ready(&index);
    fs::write(root.join("Documents/report-fresh.txt"), b"x").unwrap();
    fs::remove_file(root.join("Documents/report-2024.pdf")).unwrap();
    let (found, _) = found_by(&plan(&root, "report", Some(&index)));
    assert!(found.contains(&root.join("Documents/report-fresh.txt")));
    assert!(!found.contains(&root.join("Documents/report-2024.pdf")), "a name that has gone is dropped");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_search_stops_when_cancelled_between_batches_and_at_the_limit() {
    let root = tree("engine-cancel");
    for dir in 0..30 {
        fs::create_dir_all(root.join(format!("d{dir}"))).unwrap();
        for file in 0..20 {
            fs::write(root.join(format!("d{dir}/hit-{file}")), b"x").unwrap();
        }
    }
    let cancel = Cancel::new();
    let stopper = cancel.clone();
    let mut batches = 0;
    let summary = run(&plan(&root, "hit", None), &cancel, &mut |event| {
        if matches!(event, Event::Found(_)) {
            batches += 1;
            stopper.cancel();
        }
        true
    });
    assert!(summary.cancelled && batches == 1 && summary.hits < 600, "{summary:?}");

    let limited = Plan { max_hits: 40, ..plan(&root, "hit", None) };
    let (found, summary) = found_by(&limited);
    assert_eq!(found.len(), 40);
    assert!(summary.truncated);

    // The limit counts the index's answer too.
    let index = NameIndex::new(&root);
    index.refresh();
    wait_until_ready(&index);
    let limited = Plan { max_hits: 40, ..plan(&root, "hit", Some(&index)) };
    let (found, summary) = found_by(&limited);
    assert_eq!(found.len(), 40, "{summary:?}");
    assert!(summary.truncated);

    // A flag set before the search means no work; `emit` returning false stops it.
    let cancel = Cancel::new();
    cancel.cancel();
    let summary = run(&plan(&root, "hit", Some(&index)), &cancel, &mut |_| true);
    assert!(summary.cancelled && summary.hits == 0);
    let summary = run(&plan(&root, "hit", None), &Cancel::new(), &mut |_| false);
    assert!(summary.cancelled);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_index_only_covers_what_is_under_its_root() {
    let index = NameIndex::new(Path::new("/home/me"));
    assert!(index.covers(Path::new("/home/me")));
    assert!(index.covers(Path::new("/home/me/Pictures")));
    assert!(!index.covers(Path::new("/home/meagain")));
    assert!(!index.covers(Path::new("/srv")));
    assert!(index.is_empty() && !index.ready() && !index.crawling());
}

#[test]
fn scopes_name_their_roots() {
    use super::scope::Scope;
    let (here, home) = (Path::new("/home/me/Docs"), Path::new("/home/me"));
    let drives = [PathBuf::from("/run/media/me/USB"), PathBuf::from("/home/me/mnt")];
    assert_eq!(Scope::Folder.roots(here, home, &drives), [here]);
    assert_eq!(Scope::Subfolders.roots(here, home, &drives), [here]);
    assert_eq!(Scope::Home.roots(here, home, &drives), [home]);
    assert_eq!(Scope::Place(PathBuf::from("/home/me/Pictures")).roots(here, home, &drives), [PathBuf::from("/home/me/Pictures")]);
    assert_eq!(Scope::Everywhere.roots(here, home, &drives), [PathBuf::from("/home/me"), PathBuf::from("/run/media/me/USB")]);
    assert!(!Scope::Folder.walks() && Scope::Home.walks());
    assert_eq!(Scope::Everywhere.base(here, home), home);
}

// ---------------------------------------------------------------------------
// Recent searches
// ---------------------------------------------------------------------------

#[test]
fn recent_searches_are_newest_first_bounded_and_kept_in_a_file() {
    let mut recents = Recents::default();
    assert!(!recents.push("  "));
    assert!(!recents.push("a\nb"));
    assert!(recents.push("report"));
    assert!(!recents.push("report"), "the same search again changes nothing");
    assert!(recents.push("type:image"));
    assert!(recents.push("report"));
    assert_eq!(recents.items(), ["report", "type:image"]);
    for n in 0..20 {
        recents.push(&format!("q{n}"));
    }
    assert_eq!(recents.items().len(), MAX_RECENT);
    assert_eq!(recents.items()[0], "q19");
    recents.remove("q19");
    assert_eq!(recents.items()[0], "q18");

    let file = std::env::temp_dir().join(format!("bingux-recents-{}/x/recent-searches", std::process::id()));
    recents.save_to(&file).unwrap();
    assert_eq!(Recents::load_from(&file), recents);
    assert_eq!(Recents::load_from(Path::new("/nonexistent/recents")), Recents::default());
    recents.clear();
    assert!(recents.is_empty());
    let _ = fs::remove_dir_all(file.parent().unwrap().parent().unwrap());
}

#[test]
fn a_key_still_being_typed_is_not_searched_for() {
    assert_eq!(split_pending("report type:imag"), ("report ", "type:imag"));
    assert_eq!(split_pending("report type:image "), ("report type:image ", ""));
    assert_eq!(split_pending("report"), ("report", ""));
    assert_eq!(split_pending("size:"), ("", "size:"));
    assert_eq!(split_pending("foo:bar"), ("foo:bar", ""), "an unknown key is a word");
    assert_eq!(split_pending(""), ("", ""));
    assert_eq!(split_pending("a in:\"My Doc"), ("a ", "in:\"My Doc"));
    assert_eq!(split_pending("a \"two wor"), ("a \"two wor", ""), "an open phrase is searched as typed");
}
