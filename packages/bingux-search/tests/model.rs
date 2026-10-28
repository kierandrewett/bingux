//! Property test: the index against a naive reference engine, under random
//! upserts, deletes and compactions.

use std::collections::{BTreeMap, BTreeSet};

use bingux_search::tokenizer::{tokenize, tokenize_query};
use bingux_search::{Cancel, Document, Index, Schema, Search};
use proptest::prelude::*;

use bingux_search::Strategy as Match;

const VOCAB: &[&str] = &[
    "proxy", "proxies", "network", "networks", "wifi", "wi-fi", "bluetooth", "blutooth",
    "password", "pass", "keyboard", "kayboard", "mouse", "Ünïcode", "unicode", "show_seconds",
    "fooBar", "802.1x", "日本語", "cat", "car", "accessibility", "accesibility", "the",
];

#[derive(Clone, Debug)]
enum Op {
    Upsert(u8, Vec<usize>, Vec<usize>),
    Delete(u8),
    Compact,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => (0u8..24, prop::collection::vec(0..VOCAB.len(), 0..4), prop::collection::vec(0..VOCAB.len(), 0..3))
            .prop_map(|(i, t, k)| Op::Upsert(i, t, k)),
        3 => (0u8..24).prop_map(Op::Delete),
        1 => Just(Op::Compact),
    ]
}

fn text(words: &[usize]) -> String {
    words.iter().map(|&i| VOCAB[i]).collect::<Vec<_>>().join(" ")
}

fn schema() -> Schema {
    Schema::new().searchable("title", 10.0).searchable("keywords", 5.0)
}

// ---- reference engine: sets of terms per document

fn osa(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        d[i][0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let c = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + c);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}

fn budget(n: usize) -> usize {
    if n >= 8 { 2 } else if n >= 4 { 1 } else { 0 }
}

fn word_matches(word: &str, prefix: bool, term: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    let t: Vec<char> = term.chars().collect();
    if term == word || (prefix && term.starts_with(word)) {
        return true;
    }
    let b = budget(w.len());
    if b == 0 || w[0] != t[0] {
        return false;
    }
    if prefix {
        (0..=t.len()).any(|k| osa(&w, &t[..k]) <= b)
    } else {
        osa(&w, &t) <= b
    }
}

struct Model {
    docs: BTreeMap<String, BTreeSet<String>>,
}

impl Model {
    fn matching(&self, query: &str) -> BTreeSet<String> {
        let words: Vec<String> = tokenize_query(query).into_iter().map(|t| t.text).collect();
        if words.is_empty() {
            return self.docs.keys().cloned().collect();
        }
        let trailing = query.ends_with(char::is_whitespace);
        self.docs
            .iter()
            .filter(|(_, terms)| {
                words.iter().enumerate().all(|(i, w)| {
                    let prefix = i == words.len() - 1 && !trailing;
                    terms.iter().any(|t| word_matches(w, prefix, t))
                })
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

fn terms_of(title: &str, keywords: &str) -> BTreeSet<String> {
    tokenize(title)
        .into_iter()
        .chain(tokenize(keywords))
        .map(|t| t.text)
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    #[test]
    fn index_agrees_with_the_reference_engine(ops in prop::collection::vec(op(), 1..40),
                                              queries in prop::collection::vec((0..VOCAB.len(), 0usize..6, any::<bool>()), 1..8)) {
        let idx = Index::new("m", schema()).unwrap();
        let mut model = Model { docs: BTreeMap::new() };
        for op in &ops {
            match op {
                Op::Upsert(i, t, k) => {
                    let id = format!("d{i}");
                    let (title, kw) = (text(t), text(k));
                    idx.upsert([Document::new(&id).title(&title).keywords(&kw)]).unwrap();
                    model.docs.insert(id, terms_of(&title, &kw));
                }
                Op::Delete(i) => {
                    let id = format!("d{i}");
                    idx.delete([&id]);
                    model.docs.remove(&id);
                }
                Op::Compact => idx.compact_now(),
            }
        }
        prop_assert_eq!(idx.len(), model.docs.len());
        for (w, cut, trailing) in queries {
            let word = VOCAB[w];
            // Truncate to simulate typing, then maybe a trailing space.
            let chars: Vec<char> = word.chars().collect();
            let keep = chars.len().saturating_sub(cut).max(1);
            let mut q: String = chars[..keep].iter().collect();
            if trailing { q.push(' '); }
            let mut s = Search::new(&q).strategy(Match::All).limit(1000);
            s.track_total_hits = None;
            let got = idx.search(&s, &Cancel::new()).unwrap();
            let got_ids: BTreeSet<String> = got.hits.iter().map(|h| h.id.clone()).collect();
            let want = model.matching(&q);
            prop_assert_eq!(&got_ids, &want, "query {:?} after {:?}", q, ops);
            prop_assert_eq!(got.total as usize, want.len());
        }
        // Two-word queries.
        let q = format!("{} {}", VOCAB[0], VOCAB[1]);
        let mut s = Search::new(&q).strategy(Match::All).limit(1000);
        s.track_total_hits = None;
        let got: BTreeSet<String> = idx.search(&s, &Cancel::new()).unwrap().hits.into_iter().map(|h| h.id).collect();
        prop_assert_eq!(got, model.matching(&q));
    }

    #[test]
    fn filters_and_facets_agree_with_a_scan(rows in prop::collection::vec((0u8..30, 0u8..4, 0i32..50), 1..40),
                                            deletes in prop::collection::vec(0u8..30, 0..10),
                                            lo in 0i32..50, hi in 0i32..50) {
        let schema = schema().filterable(["cat", "n"]);
        let idx = Index::new("m", schema).unwrap();
        let mut model: BTreeMap<String, (u8, i32)> = BTreeMap::new();
        for (id, cat, n) in &rows {
            idx.upsert([Document::new(format!("d{id}")).title("x").field("cat", format!("c{cat}")).field("n", *n)]).unwrap();
            model.insert(format!("d{id}"), (*cat, *n));
        }
        for d in &deletes {
            idx.delete([format!("d{d}")]);
            model.remove(&format!("d{d}"));
        }
        let expr = format!("(cat = c1 OR cat = c2) AND n {lo} TO {hi}");
        let mut s = Search::new("").filter(&expr).facet("cat").limit(1000);
        s.track_total_hits = None;
        let r = idx.search(&s, &Cancel::new()).unwrap();
        let want: BTreeSet<String> = model.iter()
            .filter(|(_, (c, n))| (*c == 1 || *c == 2) && *n >= lo && *n <= hi)
            .map(|(id, _)| id.clone()).collect();
        let got: BTreeSet<String> = r.hits.iter().map(|h| h.id.clone()).collect();
        prop_assert_eq!(got, want.clone());
        let facet_total: usize = r.aggregations["cat"].buckets.iter().map(|b| b.doc_count).sum();
        prop_assert_eq!(facet_total, want.len());
    }
}
