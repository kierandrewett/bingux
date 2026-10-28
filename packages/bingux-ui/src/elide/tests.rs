use std::collections::HashSet;

use unicode_segmentation::UnicodeSegmentation;

use super::*;

/// Every grapheme is 8 px, the ellipsis too.
fn fixed(text: &str) -> f32 {
    text.graphemes(true).count() as f32 * 8.0
}

/// Letters differ in width, as in a proportional font. Marks are free.
fn proportional(text: &str) -> f32 {
    text.graphemes(true)
        .map(|cluster| match cluster.chars().next().unwrap_or(' ') {
            'i' | 'l' | 'j' | '.' | '/' | '\\' | ',' | '\'' | ' ' | '-' | 't' | 'f' | 'I' | '1' => 4.0,
            'm' | 'w' | 'M' | 'W' | '@' => 12.0,
            'A'..='Z' => 9.5,
            '…' => 9.0,
            c if c as u32 > 0x2E80 => 16.0,
            _ => 7.0,
        })
        .sum()
}

/// `text` in `graphemes` graphemes of room.
fn path(text: &str, graphemes: usize) -> String {
    elide_path(text, graphemes as f32 * 8.0, &fixed)
}

fn name(text: &str, graphemes: usize) -> String {
    elide_name(text, graphemes as f32 * 8.0, &fixed)
}

const DEEP: &str = "/home/kieran/Documents/Projects/bingux/design/final-report-2026.pdf";
const LONG_NAME: &str = "final-report-2026-revised-for-review.pdf";

// ---------------------------------------------------------------------------
// The ladder, rung by rung, on the paths of the Path label specimen
// ---------------------------------------------------------------------------

#[test]
fn a_path_that_fits_is_unchanged() {
    assert_eq!(path(DEEP, DEEP.chars().count()), DEEP);
    assert_eq!(path(DEEP, 500), DEEP);
    assert_eq!(path("/usr/bin/", 20), "/usr/bin/");
    assert_eq!(path("~/Documents/report.pdf", 22), "~/Documents/report.pdf");
    assert_eq!(elide_path_traced(DEEP, 1000.0, &fixed).1, ElideStep::Whole);
}

#[test]
fn step_2_collapses_the_middle_outwards_and_keeps_the_end() {
    assert_eq!(path(DEEP, 66), "/home/kieran/Documents/…/bingux/design/final-report-2026.pdf");
    assert_eq!(path(DEEP, 59), "/home/kieran/…/bingux/design/final-report-2026.pdf");
    assert_eq!(path(DEEP, 49), "/home/kieran/…/design/final-report-2026.pdf");
    for width in [66, 59, 49] {
        assert_eq!(elide_path_traced(DEEP, width as f32 * 8.0, &fixed).1, ElideStep::Collapsed);
    }
}

#[test]
fn step_3_abbreviates_the_longest_directory_first_then_collapses_further() {
    // `kieran` goes first, it is longer than `home`.
    assert_eq!(path(DEEP, 42), "/home/k/…/design/final-report-2026.pdf");
    assert_eq!(path(DEEP, 37), "/h/k/…/design/final-report-2026.pdf");
    // The directory that holds the file is never abbreviated; below this
    // width it is dropped, the abbreviated anchor stays.
    assert_eq!(path(DEEP, 34), "/h/…/design/final-report-2026.pdf");
    assert_eq!(path(DEEP, 32), "/h/…/final-report-2026.pdf");
    for width in [42, 37, 34, 32] {
        assert_eq!(elide_path_traced(DEEP, width as f32 * 8.0, &fixed).1, ElideStep::Abbreviated);
    }
}

#[test]
fn step_4_cuts_the_name_keeping_the_extension_then_the_directories_give_way() {
    assert_eq!(path(DEEP, 25), "/h/…/final-repor…2026.pdf");
    assert_eq!(path(DEEP, 21), "/h/…/final-r…2026.pdf");
    assert_eq!(path(DEEP, 18), "/h/…/fina…2026.pdf");
    assert_eq!(elide_path_traced(DEEP, 25.0 * 8.0, &fixed).1, ElideStep::NameCut);
    // Fewer than eight letters of the name beside `/h/…/`: the name alone.
    assert_eq!(path(DEEP, 17), "final-re…2026.pdf");
    assert_eq!(path(DEEP, 12), "fina…026.pdf");
    assert_eq!(path(DEEP, 6), "f….pdf");
    assert_eq!(elide_path_traced(DEEP, 17.0 * 8.0, &fixed).1, ElideStep::NameOnly);
}

#[test]
fn step_5_gives_up_the_extension_and_then_everything() {
    assert_eq!(path(DEEP, 5), "fi…df");
    assert_eq!(path(DEEP, 3), "f…f");
    assert_eq!(path(DEEP, 2), "f…");
    assert_eq!(elide_path_traced(DEEP, 5.0 * 8.0, &fixed).1, ElideStep::Middle);
    assert_eq!(path(DEEP, 1), "…");
    assert_eq!(path(DEEP, 0), "…");
    assert_eq!(elide_path_traced(DEEP, 0.0, &fixed).1, ElideStep::Least);
    assert_eq!(elide_path(DEEP, -5.0, &fixed), "…");
}

#[test]
fn a_compound_extension_stays_whole() {
    let tar = "~/backups/home-folder-of-kieran-2026-10-26.tar.gz";
    let cut = path(tar, 28);
    assert!(cut.ends_with(".tar.gz"), "{cut}");
    assert!(cut.contains('…'), "{cut}");
    assert_eq!(name("backup-of-the-home-folder-2026-10-26.tar.gz", 20), "backup-o…0-26.tar.gz");
}

#[test]
fn the_name_keeps_the_end_of_its_stem_beside_the_extension() {
    assert_eq!(name(LONG_NAME, 40), LONG_NAME);
    assert_eq!(name(LONG_NAME, 30), "final-report-2026-rev…view.pdf");
    assert_eq!(name(LONG_NAME, 20), "final-repor…view.pdf");
}

// ---------------------------------------------------------------------------
// Other shapes of path
// ---------------------------------------------------------------------------

#[test]
fn a_home_anchor_is_never_abbreviated() {
    let home = "~/Documents/Projects/bingux/design/final-report-2026.pdf";
    assert_eq!(path(home, 42), "~/Documents/…/design/final-report-2026.pdf");
    assert_eq!(path(home, 40), "~/D/…/design/final-report-2026.pdf");
    assert_eq!(path(home, 32), "~/…/design/final-report-2026.pdf");
    assert_eq!(path(home, 28), "~/…/final-report-2026.pdf");
}

#[test]
fn a_relative_path_has_no_root() {
    assert_eq!(path("src/controls/deep/path_label.rs", 24), "src/…/deep/path_label.rs");
    assert_eq!(path("src/controls/deep/path_label.rs", 22), "s/…/deep/path_label.rs");
    assert_eq!(path("src/controls/deep/path_label.rs", 20), "s/…/path_label.rs");
    assert_eq!(path("src/controls/deep/path_label.rs", 14), "path_label.rs");
}

#[test]
fn dot_directories_keep_their_dot() {
    assert_eq!(abbreviate(".config"), Some(".c"));
    assert_eq!(abbreviate("Documents"), Some("D"));
    assert_eq!(abbreviate("."), None);
    assert_eq!(abbreviate(".."), None);
    assert_eq!(abbreviate("a"), None);
    assert_eq!(abbreviate(".a"), None);
    assert_eq!(abbreviate("é\u{301}clair"), Some("é\u{301}"));
    let cut = path("/home/kieran/.config/bingux/settings/theme.toml", 36);
    assert!(cut.contains("/.c/") || cut.contains("/…/"), "{cut}");
}

#[test]
fn dots_in_a_path_are_directories_like_any_other() {
    let up = "../../shared/assets/images/photo-of-the-whole-family.jpg";
    let cut = path(up, 40);
    assert!(cut.starts_with("../"), "{cut}");
    assert!(cut.ends_with("photo-of-the-whole-family.jpg") || cut.contains('…'), "{cut}");
    for width in 0..60 {
        let out = path(up, width);
        assert!(!out.contains("/./") && !out.starts_with("./"), "{out}");
    }
}

#[test]
fn a_trailing_separator_stays() {
    assert_eq!(path("~/Documents/Work/Reports/", 200), "~/Documents/Work/Reports/");
    assert_eq!(path("~/Documents/Work/Reports/", 16), "~/…/Reports/");
    assert!(path("~/Documents/Work/Reports/", 9).ends_with('/'));
}

#[test]
fn a_scheme_and_host_are_one_segment() {
    let share = "smb://nas.local/share/a/b/report.pdf";
    assert_eq!(path(share, 30), "smb://nas.local/…/b/report.pdf");
    assert_eq!(path(share, 29), "smb://nas.local/…/report.pdf");
    assert_eq!(path("file:///home/kieran/x/y/z", 20), "file:///home/…/x/y/z");
    // A host is not abbreviated.
    assert_eq!(path(share, 24), "report.pdf");
    assert!(path(share, 28).starts_with("smb://nas.local/"));
}

#[test]
fn windows_paths_keep_their_backslashes_and_drive() {
    let win = "C:\\Users\\kieran\\Documents\\Projects\\final-report-2026.pdf";
    assert_eq!(path(win, 48), "C:\\Users\\…\\Projects\\final-report-2026.pdf");
    assert_eq!(path(win, 44), "C:\\Users\\…\\Projects\\final-report-2026.pdf");
    assert_eq!(path(win, 40), "C:\\U\\…\\Projects\\final-report-2026.pdf");
    assert_eq!(path(win, 36), "C:\\…\\Projects\\final-report-2026.pdf");
    for width in 0..70 {
        let out = path(win, width);
        assert!(!out.contains('/'), "{out}");
    }
    let unc = "\\\\nas\\share\\Projects\\Reports\\final-report-2026.pdf";
    let cut = path(unc, 34);
    assert!(cut.starts_with("\\\\"), "{cut}");
    assert!(cut.contains("\\…\\"), "{cut}");
}

#[test]
fn empty_root_and_single_segments() {
    assert_eq!(path("", 10), "");
    assert_eq!(path("/", 1), "/");
    assert_eq!(path("/", 0), "…");
    assert_eq!(path("~", 1), "~");
    assert_eq!(path("///", 1), "…");
    assert_eq!(path("readme.md", 100), "readme.md");
    assert_eq!(path("a-very-long-single-segment-name.txt", 20), "a-very-long…name.txt");
    assert_eq!(path("/a-very-long-single-segment-name.txt", 20), "/a-very-lon…name.txt");
}

#[test]
fn a_very_long_single_component_is_cut_in_the_middle() {
    let component = "x".repeat(300);
    let out = path(&component, 30);
    assert_eq!(out.graphemes(true).count(), 30);
    assert!(out.contains('…'));
}

#[test]
fn a_name_without_an_extension_is_cut_evenly() {
    assert_eq!(name("no-extension-at-all-just-a-long-name", 16), "no-exten…ng-name");
    assert_eq!(elide_middle("Wireless network at home", 14.0 * 8.0, &fixed), "Wireles…t home");
    assert_eq!(elide_middle("short", 100.0, &fixed), "short");
    assert_eq!(elide_middle("", 0.0, &fixed), "");
}

#[test]
fn dotfiles_and_odd_dots_have_no_extension() {
    assert_eq!(extension_start(".bashrc"), ".bashrc".len());
    assert_eq!(extension_start("trailing."), "trailing.".len());
    assert_eq!(extension_start("no-dot"), 6);
    assert_eq!(extension_start("Mr. Smith went to town"), "Mr. Smith went to town".len());
    assert_eq!(extension_start("file.averyveryverylongextension"), "file.averyveryverylongextension".len());
    assert_eq!(extension_start("a.b.tar.xz"), 3);
    assert_eq!(extension_start("a.tar"), 1);
    assert_eq!(extension_start("a.b.c"), 3);
}

#[test]
fn a_directory_name_has_no_extension() {
    // `v1.2` is a folder: only a file keeps what follows the dot.
    let cut = path("~/Projects/releases/bingux-ui-v1.2/", 14);
    assert!(cut.ends_with('/'), "{cut}");
    assert!(!cut.ends_with(".2/") || fixed(&cut) <= 14.0 * 8.0, "{cut}");
}

#[test]
fn a_name_without_room_for_its_extension_gives_it_up() {
    assert_eq!(name("x.pdf", 1), "…");
    assert_eq!(name("final-report-2026.pdf", 4), "fi…f");
    assert_eq!(name("report.pdf", 10), "report.pdf");
    assert_eq!(name("", 0), "");
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

const SAMPLES: &[&str] = &[
    DEEP,
    "~/Documents/report.pdf",
    "/home/kieran/Documents/Reports/final.pdf",
    "smb://nas.local/share/Videos/Holidays/2026/clip-final.mp4",
    "/mnt/Ünïcödé/日本語のフォルダ/ファイル名の長い書類.pdf",
    "relative/dir/with/many/segments/and-a-long-final-name.tar.gz",
    "a-single-segment-that-is-very-long.txt",
    "C:\\Users\\kieran\\Documents\\Projects\\final-report-2026.pdf",
    "~/Documents/Work/Reports/",
    "/",
    "///a//b//",
    "../../x/y/z.txt",
    "/home/kieran/.config/bingux/settings/theme-dark-and-light.toml",
    "/tmp/re\u{301}sume\u{301}/re\u{301}sume\u{301}-final-re\u{301}port.pdf",
    "/tmp/family-👨\u{200D}👩\u{200D}👧/family-👨\u{200D}👩\u{200D}👧-photos-of-the-whole-year.png",
    "/tmp/עברית/שם-קובץ-ארוך-מאוד.pdf",
];

fn measures() -> [(&'static str, &'static dyn Fn(&str) -> f32); 2] {
    [("fixed", &fixed), ("proportional", &proportional)]
}

#[test]
fn the_result_fits_unless_it_is_the_ellipsis() {
    for text in SAMPLES {
        for (label, measure) in measures() {
            let mut width = 0.0;
            while width < measure(text) + 16.0 {
                let out = elide_path(text, width, measure);
                assert!(!out.is_empty() || text.is_empty(), "{label}: `{text}` at {width}");
                assert!(
                    out == ELLIPSIS || measure(&out) <= width,
                    "{label}: `{text}` at {width} gave `{out}` ({})",
                    measure(&out)
                );
                width += 0.5;
            }
        }
    }
}

#[test]
fn a_wider_width_never_shows_less() {
    for text in SAMPLES {
        for (label, measure) in measures() {
            let mut previous_width = 0.0_f32;
            let mut previous_step = ElideStep::Least;
            let mut width = 0.0;
            while width < measure(text) + 16.0 {
                let (out, step) = elide_path_traced(text, width, measure);
                assert!(
                    out == ELLIPSIS || previous_width == f32::MAX || measure(&out) + 0.001 >= previous_width,
                    "{label}: `{text}` at {width} px gave `{out}` narrower than the result before"
                );
                assert!(
                    step <= previous_step,
                    "{label}: `{text}` at {width} px went from {previous_step:?} to {step:?}"
                );
                previous_width = if out == ELLIPSIS { f32::MAX } else { measure(&out) };
                previous_step = step;
                width += 0.5;
            }
        }
    }
}

#[test]
fn eliding_twice_changes_nothing() {
    for text in SAMPLES {
        for (label, measure) in measures() {
            let mut width = 0.0;
            while width < measure(text) + 16.0 {
                let once = elide_path(text, width, measure);
                let twice = elide_path(&once, width, measure);
                assert_eq!(once, twice, "{label}: `{text}` at {width}");
                width += 1.0;
            }
        }
    }
}

#[test]
fn the_same_width_always_gives_the_same_text() {
    for text in SAMPLES {
        let first = elide_path(text, 123.0, &proportional);
        for _ in 0..3 {
            assert_eq!(elide_path(text, 123.0, &proportional), first);
        }
    }
}

#[test]
fn names_and_middles_are_monotone_and_fit_too() {
    let names = [
        LONG_NAME,
        "backup-of-the-home-folder.tar.gz",
        "日本語のとても長いファイル名.pdf",
        "no-extension-at-all-just-a-long-name",
        "re\u{301}sume\u{301}-final-re\u{301}port.pdf",
    ];
    for text in names {
        for (label, measure) in measures() {
            let mut last = 0.0_f32;
            let mut last_middle = 0.0_f32;
            let mut width = 0.0;
            while width < measure(text) + 16.0 {
                let out = elide_name(text, width, measure);
                assert!(out == ELLIPSIS || measure(&out) <= width, "{label}: {text} at {width}: {out}");
                assert!(measure(&out) + 0.001 >= last, "{label}: {text} at {width}: {out}");
                last = measure(&out);
                let middle = elide_middle(text, width, measure);
                assert!(middle == ELLIPSIS || measure(&middle) <= width, "{label}: {text} at {width}: {middle}");
                assert!(measure(&middle) + 0.001 >= last_middle, "{label}: {text} at {width}: {middle}");
                last_middle = measure(&middle);
                assert_eq!(elide_name(&out, width, measure), out, "{label}: {text} at {width}");
                width += 0.5;
            }
        }
    }
}

/// The byte offsets of the grapheme boundaries of `text`.
fn boundaries(text: &str) -> HashSet<usize> {
    let mut at: HashSet<usize> = text.grapheme_indices(true).map(|(index, _)| index).collect();
    at.insert(text.len());
    at
}

/// Every piece of `out` between the ellipses is, separator by separator, the
/// start or the end of a segment of `original`, cut on a grapheme boundary of
/// that segment (or the whole segment).
fn assert_cuts_between_graphemes(original: &str, out: &str, context: &str) {
    let segments: Vec<&str> = original.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    for fragment in out.split(ELLIPSIS).flat_map(|piece| piece.split(['/', '\\'])) {
        if fragment.is_empty() || fragment.contains("://") || fragment.ends_with(':') {
            continue;
        }
        let aligned = segments.iter().any(|segment| {
            let allowed = boundaries(segment);
            (segment.starts_with(fragment) && allowed.contains(&fragment.len()))
                || (segment.ends_with(fragment)
                    && allowed.contains(&(segment.len() - fragment.len())))
        });
        assert!(
            aligned,
            "{context}: `{fragment}` of `{out}` is not cut on grapheme boundaries of `{original}`"
        );
    }
}

#[test]
fn a_cut_never_lands_inside_a_grapheme() {
    for text in SAMPLES {
        for (label, measure) in measures() {
            let mut width = 0.0;
            while width < measure(text) + 16.0 {
                let out = elide_path(text, width, measure);
                assert_cuts_between_graphemes(text, &out, &format!("{label} at {width}"));
                let out = elide_middle(text, width, measure);
                assert_cuts_between_graphemes(text, &out, &format!("{label} middle at {width}"));
                width += 0.5;
            }
        }
    }
}

#[test]
fn combining_marks_and_joined_emoji_stay_whole() {
    let accents = "re\u{301}sume\u{301}-final-re\u{301}port-of-the-whole-year.pdf";
    for graphemes in 6..accents.graphemes(true).count() {
        let cut = name(accents, graphemes);
        assert!(!cut.starts_with('\u{301}'), "{cut:?}");
        assert!(!cut.contains("…\u{301}"), "{cut:?}");
        assert_cuts_between_graphemes(accents, &cut, "accents");
    }
    let family = "family-👨\u{200D}👩\u{200D}👧-photos-of-the-whole-year.png";
    for graphemes in 8..family.graphemes(true).count() {
        let cut = name(family, graphemes);
        for (index, c) in cut.char_indices() {
            if c == '\u{200D}' {
                let before = cut[..index].chars().next_back();
                let after = cut[index + 3..].chars().next();
                assert!(
                    before.is_some_and(|b| b != '…') && after.is_some_and(|a| a != '…'),
                    "{cut:?}"
                );
            }
        }
        assert_cuts_between_graphemes(family, &cut, "family");
    }
    let flags = "🇬🇧🇫🇷🇩🇪🇮🇹-flags-of-europe.png";
    for graphemes in 4..flags.graphemes(true).count() {
        assert_cuts_between_graphemes(flags, &name(flags, graphemes), "flags");
    }
}

#[test]
fn cjk_names_are_cut_between_characters() {
    let text = "日本語のとても長いファイル名.pdf";
    for graphemes in 0..text.graphemes(true).count() {
        let cut = name(text, graphemes);
        assert!(!cut.is_empty());
        assert!(fixed(&cut) <= graphemes.max(1) as f32 * 8.0, "{cut} in {graphemes}");
    }
    assert_eq!(name(text, 10), "日本語…ル名.pdf");
    assert!(name(text, 10).ends_with(".pdf"));
}

// ---------------------------------------------------------------------------
// A proportional measure decides by width
// ---------------------------------------------------------------------------

#[test]
fn a_proportional_measure_decides_by_width_not_count() {
    let text = "/home/iii/lll/mmm/WWW/final.pdf";
    let narrow = elide_path(text, 120.0, &proportional);
    let by_count = elide_path(text, 120.0, &fixed);
    assert!(proportional(&narrow) <= 120.0);
    assert_ne!(narrow, by_count);
}

#[test]
fn a_measure_may_be_a_dyn_fn_or_a_struct() {
    struct Mono(f32);
    impl Measure for Mono {
        fn width(&self, text: &str) -> f32 {
            text.graphemes(true).count() as f32 * self.0
        }
    }
    let dynamic: &dyn Fn(&str) -> f32 = &fixed;
    assert_eq!(elide_path(DEEP, 200.0, dynamic), elide_path(DEEP, 200.0, &Mono(8.0)));
    assert!(Measure::width(&Mono(4.0), &elide_path(DEEP, 200.0, &Mono(4.0))) <= 200.0);
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

fn labels(paths: &[&str]) -> Vec<String> {
    distinguish(paths).iter().map(Distinct::label).collect()
}

#[test]
fn unique_names_are_shown_alone() {
    assert_eq!(labels(&["/a/one.txt", "/b/two.txt"]), ["one.txt", "two.txt"]);
    assert_eq!(labels(&[]), Vec::<String>::new());
    assert_eq!(labels(&["/a/one.txt"]), ["one.txt"]);
}

#[test]
fn duplicate_names_show_the_nearest_parent_that_tells_them_apart() {
    assert_eq!(
        labels(&["/work/app/src/index.ts", "/work/lib/src/index.ts", "/work/app/test/index.ts"]),
        ["index.ts — app/src", "index.ts — lib/src", "index.ts — test"]
    );
    assert_eq!(
        labels(&["/a/docs/readme.md", "/b/docs/readme.md", "/c/other/readme.md"]),
        ["readme.md — a/docs", "readme.md — b/docs", "readme.md — other"]
    );
}

#[test]
fn only_the_names_that_clash_grow() {
    assert_eq!(
        labels(&["/home/a/Documents", "/home/b/Documents", "/home/a/Music"]),
        ["Documents — a", "Documents — b", "Music"]
    );
}

#[test]
fn the_same_path_twice_has_the_same_label() {
    assert_eq!(labels(&["/a/x.txt", "/a/x.txt"]), ["x.txt", "x.txt"]);
    assert_eq!(labels(&["/a/x.txt", "/a/x.txt", "/b/x.txt"]), ["x.txt — a", "x.txt — a", "x.txt — b"]);
}

#[test]
fn a_path_that_cannot_be_told_apart_stops_growing() {
    let out = labels(&["/a/b", "a/b"]);
    assert_eq!(out.len(), 2);
}

#[test]
fn windows_paths_are_told_apart_with_backslashes() {
    assert_eq!(
        labels(&["C:\\one\\notes.txt", "C:\\two\\notes.txt"]),
        ["notes.txt — one", "notes.txt — two"]
    );
}

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

#[test]
fn right_to_left_text_is_isolated_and_the_rest_is_left_alone() {
    assert_eq!(isolate_ltr("/home/kieran/report.pdf"), "/home/kieran/report.pdf");
    assert!(!has_rtl("日本語/ファイル"));
    assert!(has_rtl("/tmp/עברית/file.pdf"));
    assert!(has_rtl("/tmp/ملف.pdf"));
    let isolated = isolate_ltr("/tmp/עברית/file.pdf");
    assert!(isolated.starts_with('\u{2066}') && isolated.ends_with('\u{2069}'));
    assert_eq!(isolated.trim_matches(|c| c == '\u{2066}' || c == '\u{2069}'), "/tmp/עברית/file.pdf");
}
