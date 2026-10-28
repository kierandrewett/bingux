//! Behavioural tests of the engine through its public API.

use std::collections::BTreeSet;

use bingux_search::*;
use serde_json::{Value, json};

fn cancel() -> Cancel {
    Cancel::new()
}

fn ids(r: &SearchResponse) -> Vec<&str> {
    r.hits.iter().map(|h| h.id.as_str()).collect()
}

fn settings_schema() -> Schema {
    Schema::new()
        .searchable("title", 10.0)
        .searchable("keywords", 5.0)
        .searchable("breadcrumb", 3.0)
        .searchable("body", 1.0)
        .filterable(["category", "tags", "price", "pinned"])
        .sortable(["price", "name"])
}

fn doc(id: &str, title: &str) -> Document {
    Document::new(id).title(title)
}

fn run(idx: &Index, s: Search) -> SearchResponse {
    idx.search(&s, &cancel()).unwrap()
}

// ---------------------------------------------------------------- ranking

#[test]
fn heavier_fields_rank_first() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([
        doc("body", "Other").body("proxy settings are here"),
        doc("keywords", "Other two").keywords("proxy"),
        doc("title", "Proxy"),
    ])
    .unwrap();
    let r = run(&idx, Search::new("proxy"));
    assert_eq!(ids(&r), ["title", "keywords", "body"]);
    assert!(r.hits[0].score > r.hits[1].score && r.hits[1].score > r.hits[2].score);
}

#[test]
fn exact_beats_prefix_beats_typo() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([
        doc("typo", "Prxoy list"),
        doc("prefix", "Proxylist"),
        doc("exact", "Proxy"),
    ])
    .unwrap();
    let r = run(&idx, Search::new("proxy"));
    assert_eq!(ids(&r), ["exact", "prefix", "typo"]);
}

#[test]
fn more_words_matched_rank_higher_and_all_strategy_requires_every_word() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([
        doc("one", "Wi-Fi"),
        doc("two", "Wi-Fi password"),
        doc("none", "Bluetooth"),
    ])
    .unwrap();
    let r = run(&idx, Search::new("wifi password").strategy(Strategy::Any));
    assert_eq!(ids(&r), ["two", "one"]);
    let r = run(&idx, Search::new("wifi password").strategy(Strategy::All));
    assert_eq!(ids(&r), ["two"]);
    // `Last` falls back to the first word when too few documents match all.
    let r = run(&idx, Search::new("wifi password"));
    assert_eq!(ids(&r), ["two", "one"]);
}

#[test]
fn adjacent_words_beat_distant_words_and_order_matters() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([
        doc("far", "network card and some other words then proxy"),
        doc("reversed", "proxy network"),
        doc("near", "network proxy"),
    ])
    .unwrap();
    let r = run(&idx, Search::new("network proxy").strategy(Strategy::All));
    assert_eq!(ids(&r), ["near", "reversed", "far"]);
}

#[test]
fn ranking_rules_are_configurable() {
    // Without the typo rule an exact word no longer beats a typo by that rule;
    // the attribute rule decides.
    let schema = settings_schema().ranking_rules(vec![RankingRule::Attribute]);
    let idx = Index::new("t", schema).unwrap();
    idx.upsert([
        doc("body_exact", "Zzz").body("proxy"),
        doc("title_typo", "Prxoy"),
    ])
    .unwrap();
    let r = run(&idx, Search::new("proxy"));
    assert_eq!(r.hits[0].id, "title_typo");
}

#[test]
fn score_is_between_zero_and_one_and_a_perfect_match_is_near_one() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([doc("a", "Proxy")]).unwrap();
    let r = run(&idx, Search::new("proxy"));
    let s = r.hits[0].score;
    assert!((0.0..1.0).contains(&s));
    assert!(s > 0.9, "perfect match scored {s}");
}

// ---------------------------------------------------------------- typos

#[test]
fn typo_thresholds_follow_word_length_and_first_letter_is_exact() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([
        doc("cat", "cat"),
        doc("mouse", "mouse"),
        doc("keyboard", "keyboard layout"),
        doc("accessibility", "accessibility"),
    ])
    .unwrap();
    // 3 letters: no typo.
    assert!(run(&idx, Search::new("cut ")).hits.is_empty());
    // 5 letters: one typo.
    assert_eq!(ids(&run(&idx, Search::new("moose "))), ["mouse"]);
    // ...but not two.
    assert!(run(&idx, Search::new("mxxse ")).hits.is_empty());
    // 8+ letters: two typos.
    assert_eq!(ids(&run(&idx, Search::new("kayboard "))), ["keyboard"]);
    assert_eq!(ids(&run(&idx, Search::new("accesibilty "))), ["accessibility"]);
    // First letter must be right.
    assert!(run(&idx, Search::new("nouse ")).hits.is_empty());
    // A swap is one typo.
    assert_eq!(ids(&run(&idx, Search::new("muose "))), ["mouse"]);
}

#[test]
fn the_last_word_is_a_prefix_while_typing_but_not_after_a_space() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([doc("bt", "Bluetooth")]).unwrap();
    assert_eq!(ids(&run(&idx, Search::new("blue"))), ["bt"]);
    assert!(run(&idx, Search::new("blue ")).hits.is_empty());
    // A prefix with a typo still finds it.
    assert_eq!(ids(&run(&idx, Search::new("blutoo"))), ["bt"]);
}

#[test]
fn typo_tolerance_can_be_turned_off() {
    let schema = settings_schema().typo(TypoSettings {
        enabled: false,
        ..TypoSettings::default()
    });
    let idx = Index::new("t", schema).unwrap();
    idx.upsert([doc("a", "keyboard")]).unwrap();
    assert!(run(&idx, Search::new("kayboard ")).hits.is_empty());
    assert_eq!(ids(&run(&idx, Search::new("keyb"))), ["a"]);
}

// ---------------------------------------------------------------- text

#[test]
fn compounds_match_joined_split_and_camel_forms() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([
        doc("hyphen", "Wi-Fi"),
        doc("snake", "show_seconds"),
        doc("camel", "fooBar"),
    ])
    .unwrap();
    for q in ["wifi", "wi-fi", "wi fi"] {
        assert_eq!(ids(&run(&idx, Search::new(q))), ["hyphen"], "{q}");
    }
    assert_eq!(ids(&run(&idx, Search::new("seconds"))), ["snake"]);
    assert_eq!(ids(&run(&idx, Search::new("show seconds"))), ["snake"]);
    assert_eq!(ids(&run(&idx, Search::new("bar"))), ["camel"]);
}

#[test]
fn diacritics_and_case_are_folded_both_ways() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([doc("a", "Résumé Éditeur"), doc("b", "plain resume")]).unwrap();
    let r = run(&idx, Search::new("RESUME"));
    assert_eq!(r.hits.len(), 2);
    let r = run(&idx, Search::new("éditeur"));
    assert_eq!(ids(&r), ["a"]);
}

#[test]
fn phrases_need_adjacent_words_in_one_field() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([
        doc("adjacent", "Show the seconds").keywords("clock"),
        doc("phrase", "Show seconds"),
        doc("split", "Show").keywords("seconds"),
    ])
    .unwrap();
    let r = run(&idx, Search::new("\"show seconds\""));
    assert_eq!(ids(&r), ["phrase"]);
    let r = run(&idx, Search::new("show seconds").strategy(Strategy::All));
    assert_eq!(r.hits.len(), 3);
}

#[test]
fn hard_separators_stop_phrases_across_breadcrumb_segments() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([Document::new("a").title("x").breadcrumb(["Network", "Proxy"])]).unwrap();
    assert!(run(&idx, Search::new("\"network proxy\"")).hits.is_empty());
    assert_eq!(run(&idx, Search::new("network proxy").strategy(Strategy::All)).hits.len(), 1);
}

#[test]
fn synonyms_and_stop_words() {
    let schema = settings_schema()
        .synonym_group(&["wifi", "wireless"])
        .synonym("lan", ["local area network"])
        .stop_words(["the", "of"]);
    let idx = Index::new("t", schema).unwrap();
    idx.upsert([
        doc("w", "Wireless"),
        doc("n", "Local area network"),
        doc("t", "Tweak the settings of the desktop"),
    ])
    .unwrap();
    assert_eq!(ids(&run(&idx, Search::new("wifi"))), ["w"]);
    assert_eq!(ids(&run(&idx, Search::new("lan"))), ["n"]);
    // Stop words are not searched: a query of them matches everything.
    assert_eq!(run(&idx, Search::new("the of")).hits.len(), 3);
    assert_eq!(ids(&run(&idx, Search::new("tweak desktop").strategy(Strategy::All))), ["t"]);
}

#[test]
fn empty_query_matches_everything_in_insertion_order() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([doc("b", "x"), doc("a", "y"), doc("c", "z")]).unwrap();
    let r = run(&idx, Search::new(""));
    assert_eq!(ids(&r), ["b", "a", "c"]);
    assert_eq!(r.total, 3);
    assert_eq!(r.hits[0].score, 1.0);
}

// ---------------------------------------------------------------- filter / sort / facets

fn catalog() -> Index {
    let idx = Index::new("t", settings_schema()).unwrap();
    let rows = [
        ("a", "Alpha", "Network", 5, vec!["x", "y"], true),
        ("b", "Beta", "Network", 10, vec!["y"], false),
        ("c", "Gamma", "Sound", 7, vec!["z"], true),
        ("d", "Delta", "sound", 20, vec![], false),
    ];
    idx.upsert(rows.iter().map(|(id, t, c, p, tags, pin)| {
        Document::new(*id)
            .title(t)
            .category(c)
            .field("price", *p)
            .field("name", *t)
            .field("tags", json!(tags))
            .field("pinned", *pin)
    }))
    .unwrap();
    idx
}

#[test]
fn filters_equality_ranges_in_and_boolean_logic() {
    let idx = catalog();
    let f = |expr: &str| -> Vec<String> {
        let r = run(&idx, Search::new("").filter(expr));
        let mut v: Vec<String> = r.hits.iter().map(|h| h.id.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(f("category = 'Network'"), ["a", "b"]);
    assert_eq!(f("category = sound"), ["c", "d"], "string equality ignores case");
    assert_eq!(f("category != Network"), ["c", "d"]);
    assert_eq!(f("price > 5 AND price <= 10"), ["b", "c"]);
    assert_eq!(f("price 7 TO 20"), ["b", "c", "d"]);
    assert_eq!(f("tags IN [x, z]"), ["a", "c"]);
    assert_eq!(f("tags EXISTS"), ["a", "b", "c"]);
    assert_eq!(f("NOT tags EXISTS"), ["d"]);
    assert_eq!(f("pinned = true OR price = 20"), ["a", "c", "d"]);
    assert_eq!(f("(category = Network OR category = Sound) AND NOT pinned = true"), ["b", "d"]);
}

#[test]
fn filtering_an_attribute_that_is_not_filterable_is_an_error_naming_it() {
    let idx = catalog();
    let e = idx
        .search(&Search::new("").filter("title = x"), &cancel())
        .unwrap_err();
    assert!(e.to_string().contains("title"), "{e}");
    assert_eq!(e.kind(), "illegal_argument_exception");
}

#[test]
fn filters_apply_together_with_text() {
    let idx = catalog();
    let r = run(&idx, Search::new("a").filter("category = Network"));
    // `a` is a prefix of Alpha only among Network documents... Beta has no `a*`.
    assert_eq!(ids(&r), ["a"]);
}

#[test]
fn sorting_by_a_field_numbers_and_strings_with_missing_last() {
    let idx = catalog();
    idx.upsert([Document::new("e").title("Epsilon")]).unwrap();
    let r = run(&idx, Search::new("").sort("price:asc"));
    assert_eq!(ids(&r), ["a", "c", "b", "d", "e"]);
    let r = run(&idx, Search::new("").sort("price:desc"));
    assert_eq!(ids(&r), ["d", "b", "c", "a", "e"]);
    let r = run(&idx, Search::new("").sort("name:desc"));
    assert_eq!(ids(&r)[..4], ["c", "d", "b", "a"]);
    assert!(idx.search(&Search::new("").sort("category:asc"), &cancel()).is_err());
}

#[test]
fn facets_count_values_among_the_matches() {
    let idx = catalog();
    let r = run(&idx, Search::new("").facet("category").facet("tags"));
    let cat = &r.aggregations["category"];
    let counts: Vec<(String, usize)> = cat
        .buckets
        .iter()
        .map(|b| (b.key.as_str().unwrap().to_string(), b.doc_count))
        .collect();
    assert_eq!(counts[0].1, 2);
    assert_eq!(counts.iter().map(|c| c.1).sum::<usize>(), 4);
    let tags = &r.aggregations["tags"];
    assert_eq!(tags.buckets[0].key, json!("y"));
    // Facets follow the filter.
    let r = run(&idx, Search::new("").filter("pinned = true").facet("category"));
    let total: usize = r.aggregations["category"].buckets.iter().map(|b| b.doc_count).sum();
    assert_eq!(total, 2);
}

#[test]
fn offset_limit_and_total() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert((0..25).map(|i| doc(&format!("d{i:02}"), &format!("item {i}")))).unwrap();
    let r = run(&idx, Search::new("item").limit(10).offset(20));
    assert_eq!(r.hits.len(), 5);
    assert_eq!(r.total, 25);
    assert_eq!(r.relation, Relation::Eq);
    let mut s = Search::new("item").limit(5);
    s.track_total_hits = Some(10);
    let r = run(&idx, s);
    assert_eq!((r.total, r.relation), (10, Relation::Gte));
    assert!(idx.search(&Search::new("").limit(5000), &cancel()).is_err());
}

#[test]
fn cursor_pagination_visits_every_document_once() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert((0..23).map(|i| doc(&format!("d{i:02}"), if i % 2 == 0 { "item" } else { "item extra" }))).unwrap();
    let mut seen = Vec::new();
    let mut after: Option<Vec<Value>> = None;
    loop {
        let mut s = Search::new("item").limit(5).sort("_score:desc").sort("_id:asc");
        if let Some(a) = after.take() {
            s = s.search_after(a);
        }
        let r = run(&idx, s);
        if r.hits.is_empty() {
            break;
        }
        seen.extend(r.hits.iter().map(|h| h.id.clone()));
        after = r.hits.last().unwrap().sort.clone();
    }
    let unique: BTreeSet<_> = seen.iter().cloned().collect();
    assert_eq!(seen.len(), 23);
    assert_eq!(unique.len(), 23);
}

// ---------------------------------------------------------------- highlights

#[test]
fn highlight_ranges_are_byte_offsets_on_multibyte_text() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([doc("a", "Ünïcode Café résumé").body("日本語 の proxy")]).unwrap();
    let r = run(&idx, Search::new("cafe").highlight(&["title"]));
    let h = &r.hits[0];
    let title = h.str_field("title").unwrap();
    let ranges = h.ranges("title");
    assert_eq!(ranges.len(), 1);
    assert_eq!(&title[ranges[0].clone()], "Café");
    assert_eq!(h.highlight["title"], ["Ünïcode <em>Café</em> résumé"]);
    // CJK per character.
    let r = run(&idx, Search::new("語").highlight(&["body"]));
    let h = &r.hits[0];
    let body = h.str_field("body").unwrap();
    assert_eq!(&body[h.ranges("body")[0].clone()], "語");
}

#[test]
fn a_prefix_match_marks_only_the_typed_part_and_typos_mark_the_word() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([doc("a", "Proxy server")]).unwrap();
    let r = run(&idx, Search::new("pro").highlight(&["title"]));
    assert_eq!(r.hits[0].highlight["title"], ["<em>Pro</em>xy server"]);
    let r = run(&idx, Search::new("sevrer ").highlight(&["title"]));
    assert_eq!(r.hits[0].highlight["title"], ["Proxy <em>server</em>"]);
    let r = run(&idx, Search::new("pro").highlight(&[]));
    assert!(r.hits[0].highlight.contains_key("title"));
}

#[test]
fn highlight_a_compound_and_array_values() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([Document::new("a").title("Wi-Fi").breadcrumb(["Network", "Wi-Fi"])]).unwrap();
    let r = run(&idx, Search::new("wifi").highlight(&["title", "breadcrumb"]));
    let h = &r.hits[0];
    assert_eq!(h.highlight["title"], ["<em>Wi-Fi</em>"]);
    assert_eq!(h.ranges_at("breadcrumb", 1), vec![0..5]);
    assert!(h.ranges_at("breadcrumb", 0).is_empty());
}

#[test]
fn fragment_size_crops_around_the_match() {
    let idx = Index::new("t", settings_schema()).unwrap();
    let body = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron";
    idx.upsert([doc("a", "x").body(body)]).unwrap();
    let mut s = Search::new("kappa").highlight(&["body"]);
    s.highlight.as_mut().unwrap().fragment_size = 30;
    let r = run(&idx, s);
    let f = &r.hits[0].highlight["body"][0];
    assert!(f.contains("<em>kappa</em>") && f.starts_with('…') && f.ends_with('…'), "{f}");
    assert!(f.chars().count() < 60);
}

// ---------------------------------------------------------------- source, displayed

#[test]
fn displayed_and_source_includes_shape_the_hit() {
    let schema = settings_schema().displayed(["title", "uri"]);
    let idx = Index::new("t", schema).unwrap();
    idx.upsert([doc("a", "Proxy").uri("bingux-settings://network/proxy").keywords("secret")]).unwrap();
    let r = run(&idx, Search::new("proxy"));
    assert_eq!(r.hits[0].source, json!({"title": "Proxy", "uri": "bingux-settings://network/proxy"}));
    assert_eq!(r.hits[0].uri(), Some("bingux-settings://network/proxy"));
    let r = run(&idx, Search::new("proxy").source_only(&["title"]));
    assert_eq!(r.hits[0].source, json!({"title": "Proxy"}));
}

// ---------------------------------------------------------------- incremental

#[test]
fn update_replaces_and_delete_removes() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert([doc("a", "Proxy"), doc("b", "Mouse")]).unwrap();
    idx.upsert([doc("a", "Keyboard")]).unwrap();
    assert!(run(&idx, Search::new("proxy")).hits.is_empty());
    assert_eq!(ids(&run(&idx, Search::new("keyboard"))), ["a"]);
    assert_eq!(idx.len(), 2);
    assert_eq!(idx.delete(["b", "zzz"]), 1);
    assert!(run(&idx, Search::new("mouse")).hits.is_empty());
    assert_eq!(idx.len(), 1);
    assert_eq!(idx.get("a").unwrap().fields["title"], json!("Keyboard"));
    assert!(idx.get("b").is_none());
}

#[test]
fn compaction_keeps_results_and_filters_intact() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert((0..200).map(|i| {
        Document::new(format!("d{i}")).title(&format!("item number{}", i % 7)).category(if i % 2 == 0 { "even" } else { "odd" }).field("price", i)
    }))
    .unwrap();
    idx.delete((0..200).filter(|i| i % 3 == 0).map(|i| format!("d{i}")));
    let before = run(&idx, Search::new("number3").filter("category = even AND price > 50").limit(1000));
    let g = idx.generation();
    idx.compact_now();
    let after = run(&idx, Search::new("number3").filter("category = even AND price > 50").limit(1000));
    assert_eq!(idx.generation(), g, "compaction is not a content change");
    let mut a: Vec<_> = before.hits.iter().map(|h| h.id.clone()).collect();
    let mut b: Vec<_> = after.hits.iter().map(|h| h.id.clone()).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b);
    assert!(!a.is_empty());
    assert_eq!(idx.len(), 200 - 67);
}

#[test]
fn limits_stop_an_upsert_without_corrupting_the_index() {
    let idx = Index::with_limits(
        "t",
        settings_schema(),
        Limits { max_documents: 3, ..Limits::default() },
    )
    .unwrap();
    let e = idx.upsert((0..5).map(|i| doc(&format!("d{i}"), "x"))).unwrap_err();
    assert!(matches!(e, Error::LimitExceeded(_)));
    assert_eq!(idx.len(), 3);
    // Replacing an existing document is still allowed at the limit.
    idx.upsert([doc("d0", "changed")]).unwrap();
    let big = Document::new("big").body(&"x ".repeat(40_000));
    assert!(matches!(
        Index::new("t", settings_schema()).unwrap().upsert([big]),
        Err(Error::LimitExceeded(_))
    ));
    assert!(idx.memory_usage() > 0);
}

// ---------------------------------------------------------------- cancellation, timeout

#[test]
fn a_cancelled_search_returns_cancelled_and_a_timeout_returns_partial() {
    let idx = Index::new("t", settings_schema()).unwrap();
    idx.upsert((0..20_000).map(|i| doc(&format!("d{i}"), &format!("alpha beta{}", i % 50)))).unwrap();
    let c = cancel();
    c.cancel();
    assert!(matches!(idx.search(&Search::new("alpha"), &c), Err(Error::Cancelled)));
    // A deadline that has already passed returns what there is, flagged.
    let r = idx
        .search(&Search::new("alpha").timeout(std::time::Duration::ZERO), &cancel())
        .unwrap();
    assert!(r.timed_out);
    // Cancelling from another thread while it runs ends it.
    let c = cancel();
    let c2 = c.clone();
    let t = std::thread::spawn(move || c2.cancel());
    t.join().unwrap();
    assert!(idx.search(&Search::new("alpha"), &c).is_err());
}

// ---------------------------------------------------------------- external

#[test]
fn external_only_hides_private_documents_and_private_indexes() {
    let idx = Index::new("t", settings_schema().external(true)).unwrap();
    idx.upsert([doc("pub", "Proxy"), doc("priv", "Proxy secret").external(false)]).unwrap();
    let mut s = Search::new("proxy");
    s.external_only = true;
    let r = run(&idx, s);
    assert_eq!(ids(&r), ["pub"]);
    assert_eq!(run(&idx, Search::new("proxy")).hits.len(), 2);
    assert!(idx.get_external("priv").is_none());
    assert!(idx.get_external("pub").is_some());

    let private = Index::new("p", settings_schema()).unwrap();
    private.upsert([doc("a", "Proxy").external(true)]).unwrap();
    let mut s = Search::new("proxy");
    s.external_only = true;
    assert!(matches!(private.search(&s, &cancel()), Err(Error::UnknownIndex(_))));
    assert!(private.get_external("a").is_none(), "a document cannot opt in");
}

// ---------------------------------------------------------------- response shape / DSL

#[test]
fn response_has_the_opensearch_field_names() {
    let idx = catalog();
    let v = run(&idx, Search::new("alpha").highlight(&[]).facet("category")).to_json();
    for key in ["took", "timed_out", "_shards", "hits", "aggregations"] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    for key in ["total", "max_score", "hits"] {
        assert!(v["hits"].get(key).is_some(), "missing hits.{key}");
    }
    assert!(v["hits"]["total"]["value"].is_u64());
    assert!(matches!(v["hits"]["total"]["relation"].as_str(), Some("eq" | "gte")));
    let hit = &v["hits"]["hits"][0];
    for key in ["_index", "_id", "_score", "_source", "highlight"] {
        assert!(hit.get(key).is_some(), "missing hit.{key}");
    }
    for key in ["total", "successful", "skipped", "failed"] {
        assert!(v["_shards"][key].is_u64());
    }
    let agg = &v["aggregations"]["category"];
    assert!(agg["buckets"][0]["key"].is_string() && agg["buckets"][0]["doc_count"].is_u64());
    assert!(agg.get("doc_count_error_upper_bound").is_some() && agg.get("sum_other_doc_count").is_some());
    // And it parses back.
    let back = SearchResponse::from_json(&v).unwrap();
    assert_eq!(back.hits[0].id, "a");
}

fn os(idx: &Index, body: Value) -> Vec<String> {
    let s = Search::from_json(&body).unwrap();
    let mut v: Vec<String> = idx.search(&s, &cancel()).unwrap().hits.iter().map(|h| h.id.clone()).collect();
    v.sort();
    v
}

#[test]
fn opensearch_query_subset() {
    let idx = catalog();
    assert_eq!(os(&idx, json!({"query": {"match_all": {}}})), ["a", "b", "c", "d"]);
    assert_eq!(os(&idx, json!({"query": {"match": {"title": "alpha"}}})), ["a"]);
    assert_eq!(os(&idx, json!({"query": {"match": {"title": {"query": "alpha beta", "operator": "or"}}}})), ["a", "b"]);
    assert!(os(&idx, json!({"query": {"match": {"title": {"query": "alpha beta", "operator": "and"}}}})).is_empty());
    assert_eq!(os(&idx, json!({"query": {"multi_match": {"query": "gamma", "fields": ["title^3", "keywords"]}}})), ["c"]);
    assert_eq!(os(&idx, json!({"query": {"match_phrase": {"title": "delta"}}})), ["d"]);
    assert_eq!(os(&idx, json!({"query": {"prefix": {"title": "gam"}}})), ["c"]);
    assert_eq!(os(&idx, json!({"query": {"term": {"category": "network"}}})), ["a", "b"]);
    assert_eq!(os(&idx, json!({"query": {"terms": {"tags": ["x", "z"]}}})), ["a", "c"]);
    assert_eq!(os(&idx, json!({"query": {"range": {"price": {"gte": 7, "lt": 20}}}})), ["b", "c"]);
    assert_eq!(os(&idx, json!({"query": {"exists": {"field": "tags"}}})), ["a", "b", "c"]);
    assert_eq!(
        os(&idx, json!({"query": {"bool": {
            "must": [{"match": {"title": "alpha beta gamma delta"}}],
            "filter": [{"term": {"category": "network"}}],
            "must_not": [{"term": {"pinned": true}}]}}})),
        ["b"]
    );
    assert_eq!(
        os(&idx, json!({"query": {"bool": {"should": [{"match": {"title": "alpha"}}, {"match": {"title": "delta"}}]}}})),
        ["a", "d"]
    );
    assert_eq!(
        os(&idx, json!({"query": {"bool": {
            "should": [{"match": {"title": "alpha"}}, {"match": {"title": "delta"}}],
            "must": [{"match": {"title": "alpha delta"}}], "minimum_should_match": 1}}})),
        ["a", "d"]
    );
}

#[test]
fn opensearch_paging_sorting_source_highlight_aggs() {
    let idx = catalog();
    let s = Search::from_json(&json!({
        "query": {"match_all": {}}, "from": 1, "size": 2,
        "sort": [{"price": "desc"}], "_source": ["title"],
        "aggs": {"by_cat": {"terms": {"field": "category", "size": 1}}},
        "track_total_hits": true
    })).unwrap();
    let r = idx.search(&s, &cancel()).unwrap();
    assert_eq!(ids(&r), ["b", "c"]);
    assert_eq!(r.hits[0].source, json!({"title": "Beta"}));
    assert_eq!(r.hits[0].sort.as_ref().unwrap(), &vec![json!(10.0)]);
    assert_eq!(r.aggregations["by_cat"].buckets.len(), 1);
    assert!(r.aggregations["by_cat"].sum_other_doc_count > 0 || r.aggregations["by_cat"].buckets[0].doc_count == 4);

    let s = Search::from_json(&json!({
        "query": {"match": {"title": "alpha"}},
        "highlight": {"fields": {"title": {}}, "pre_tags": ["["], "post_tags": ["]"]}
    })).unwrap();
    let r = idx.search(&s, &cancel()).unwrap();
    assert_eq!(r.hits[0].highlight["title"], ["[Alpha]"]);
}

#[test]
fn unsupported_opensearch_features_are_rejected_naming_the_key() {
    for (body, needle) in [
        (json!({"query": {"fuzzy": {"title": "x"}}}), "fuzzy"),
        (json!({"query": {"nested": {"path": "a"}}}), "nested"),
        (json!({"script_fields": {}}), "script_fields"),
        (json!({"aggs": {"x": {"avg": {"field": "price"}}}}), "avg"),
        (json!({"query": {"bool": {"weird": []}}}), "weird"),
        (json!({"query": {"match_all": {}, "term": {}}}), "exactly one"),
    ] {
        let e = Search::from_json(&body).unwrap_err();
        assert_eq!(e.kind(), "parsing_exception", "{body}");
        assert!(e.to_string().contains(needle), "{e} should name {needle}");
        let j = e.to_json();
        assert_eq!(j["error"]["type"], "parsing_exception");
        assert_eq!(j["status"], 400);
    }
}

#[test]
fn meilisearch_shorthand() {
    let idx = catalog();
    let s = Search::from_json(&json!({
        "q": "a", "filter": ["category = Network", ["price > 1", "price < 3"]],
        "facets": ["category"], "limit": 3, "attributesToHighlight": ["title"],
        "matchingStrategy": "all"
    })).unwrap();
    // The filter array ANDs its strings; the nested array ORs.
    let r = idx.search(&s, &cancel()).unwrap();
    assert_eq!(r.hits.len(), 1);
    assert_eq!(r.hits[0].id, "a");
    assert!(r.aggregations.contains_key("category"));
    assert!(Search::from_json(&json!({"q": "a", "nonsense": 1})).is_err());
}

#[test]
fn filter_parse_errors_report_a_reason() {
    let idx = catalog();
    let e = idx.search(&Search::new("").filter("price >"), &cancel()).unwrap_err();
    assert_eq!(e.to_json()["status"], 400);
}

// ---------------------------------------------------------------- service

#[test]
fn service_merges_indexes_and_reports_changes() {
    let svc = SearchService::new();
    let a = svc.create_index("a", settings_schema()).unwrap();
    let b = svc.create_index("b", settings_schema()).unwrap();
    let rx = svc.watch();
    a.upsert([doc("1", "Proxylist")]).unwrap();
    b.upsert([doc("2", "Proxy")]).unwrap();
    let c1 = rx.recv().unwrap();
    assert_eq!(c1.index, "a");
    assert_eq!(rx.recv().unwrap().index, "b");
    let r = svc.search(&Search::new("proxy"), &cancel()).unwrap();
    assert_eq!(r.total, 2);
    assert_eq!(r.hits[0].index.as_ref(), "b", "the better match comes first across indexes");
    let r = svc.search(&Search::new("proxy").index("a"), &cancel()).unwrap();
    assert_eq!(r.hits.len(), 1);
    assert!(matches!(
        svc.search(&Search::new("x").index("nope"), &cancel()),
        Err(Error::UnknownIndex(_))
    ));
    // Same schema: the index is reused with its documents.
    let again = svc.create_index("a", settings_schema()).unwrap();
    assert_eq!(again.len(), 1);
}

struct Fixed;
impl Searchable for Fixed {
    fn name(&self) -> &str {
        "fixed"
    }
    fn schema(&self) -> Schema {
        settings_schema()
    }
    fn documents(&self) -> Box<dyn Iterator<Item = Document> + Send + '_> {
        Box::new((0..5).map(|i| doc(&format!("{i}"), "Thing")))
    }
}

#[test]
fn a_searchable_loads_into_its_index() {
    let svc = SearchService::new();
    assert_eq!(svc.load(&Fixed).unwrap(), 5);
    assert_eq!(svc.index("fixed").unwrap().len(), 5);
    assert_eq!(svc.load(&Fixed).unwrap(), 5, "a reload replaces");
    assert_eq!(svc.index("fixed").unwrap().len(), 5);
}
