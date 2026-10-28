//! Benchmark: build time, memory and query latency at 1k..1M short documents.
//!
//! `cargo run --release --example bench -- [sizes...]` (default 1000 10000
//! 100000 1000000). Documents are 2-4 word titles and 2 keywords drawn from a
//! Zipf-distributed vocabulary of pseudo words, plus a category.

use std::time::Instant;

use bingux_search::*;

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
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

const ONSETS: &[&str] = &["b", "c", "d", "f", "g", "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w", "br", "cl", "st", "tr", "sh", "ch"];
const VOWELS: &[&str] = &["a", "e", "i", "o", "u", "ai", "ou", "io"];
const CODAS: &[&str] = &["", "", "n", "r", "s", "t", "l", "x", "th", "nd"];

fn vocabulary(n: usize, rng: &mut Rng) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let syll = 1 + rng.below(3);
        let mut w = String::new();
        for _ in 0..syll {
            w.push_str(ONSETS[rng.below(ONSETS.len())]);
            w.push_str(VOWELS[rng.below(VOWELS.len())]);
            w.push_str(CODAS[rng.below(CODAS.len())]);
        }
        if w.len() >= 3 && seen.insert(w.clone()) {
            out.push(w);
        }
    }
    out
}

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4)
}

struct Zipf {
    cdf: Vec<f64>,
}
impl Zipf {
    fn new(n: usize) -> Self {
        let mut cdf = Vec::with_capacity(n);
        let mut acc = 0.0;
        for i in 1..=n {
            acc += 1.0 / (i as f64).powf(1.0);
            cdf.push(acc);
        }
        let total = acc;
        cdf.iter_mut().for_each(|c| *c /= total);
        Zipf { cdf }
    }
    fn sample(&self, rng: &mut Rng) -> usize {
        let u = rng.unit();
        self.cdf.partition_point(|&c| c < u).min(self.cdf.len() - 1)
    }
}

#[repr(C)]
struct Timespec {
    sec: i64,
    nsec: i64,
}

unsafe extern "C" {
    fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32;
}

/// CPU time of this thread in nanoseconds: unaffected by waiting for a core on
/// a busy machine.
fn cpu_ns() -> u64 {
    let mut ts = Timespec { sec: 0, nsec: 0 };
    // SAFETY: `ts` is a valid out pointer; CLOCK_THREAD_CPUTIME_ID is 3 on Linux.
    unsafe { clock_gettime(3, &mut ts) };
    ts.sec as u64 * 1_000_000_000 + ts.nsec as u64
}

fn pct(v: &mut [u64], p: f64) -> u64 {
    v.sort_unstable();
    v[((v.len() as f64 - 1.0) * p) as usize]
}

fn main() {
    let sizes: Vec<usize> = {
        let args: Vec<usize> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
        if args.is_empty() { vec![1_000, 10_000, 100_000, 1_000_000] } else { args }
    };
    println!(
        "{:>9} {:>8} {:>7} {:>7} | {:<22} {:>8} {:>8} | {:>8} {:>8} {:>8}",
        "docs", "build s", "est MiB", "rss MiB", "query", "cpu p50", "cpu p99", "wall p50", "wall p99", "wall max"
    );
    for &n in &sizes {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ n as u64);
        let vocab = vocabulary((n / 2).clamp(500, 400_000), &mut rng);
        let zipf = Zipf::new(vocab.len());
        let cats = ["Network", "Sound", "Display", "Power", "Users", "Apps", "Devices", "Keyboard", "Mouse", "Time"];
        let rss0 = rss_kib();
        let schema = Schema::new()
            .searchable("title", 10.0)
            .searchable("keywords", 5.0)
            .filterable(["category"]);
        let idx = Index::with_limits("bench", schema, Limits { max_documents: 4_000_000, max_index_bytes: 8 << 30, ..Limits::default() }).unwrap();
        let mut words_of: Vec<Vec<usize>> = Vec::with_capacity(n.min(200_000));
        let t = Instant::now();
        let make_doc = |i: usize, rng: &mut Rng, keep: &mut Vec<Vec<usize>>| -> Document {
            let nt = 2 + rng.below(3);
            let tw: Vec<usize> = (0..nt).map(|_| zipf.sample(rng)).collect();
            let kw: Vec<usize> = (0..2).map(|_| zipf.sample(rng)).collect();
            let title: Vec<&str> = tw.iter().map(|&w| vocab[w].as_str()).collect();
            let keywords: Vec<&str> = kw.iter().map(|&w| vocab[w].as_str()).collect();
            if keep.len() < 200_000 {
                keep.push(tw.clone());
            }
            Document::new(format!("d{i}"))
                .title(&title.join(" "))
                .keywords(&keywords.join(" "))
                .category(cats[rng.below(cats.len())])
        };
        let mut kept = Vec::new();
        let mut r2 = Rng(7 + n as u64);
        let docs = (0..n).map(|i| make_doc(i, &mut r2, &mut kept));
        idx.upsert(docs).unwrap();
        words_of.extend(kept);
        let build = t.elapsed().as_secs_f64();
        let rss = rss_kib().saturating_sub(rss0) / 1024;
        let est = idx.memory_usage() / (1 << 20);

        let mut qrng = Rng(42);
        let cancel = Cancel::new();
        let mut bench = |label: &str, mk: &mut dyn FnMut(&mut Rng) -> Search| {
            let runs = if n >= 1_000_000 { 200 } else { 600 };
            let mut lat = Vec::with_capacity(runs);
            let mut cpu = Vec::with_capacity(runs);
            for _ in 0..20 {
                let s = mk(&mut qrng);
                let _ = idx.search(&s, &cancel);
            }
            for _ in 0..runs {
                let s = mk(&mut qrng);
                let c0 = cpu_ns();
                let t = Instant::now();
                let r = idx.search(&s, &cancel).unwrap();
                lat.push(t.elapsed().as_micros() as u64);
                cpu.push((cpu_ns() - c0) / 1000);
                std::hint::black_box(r);
            }
            let max = *lat.iter().max().unwrap();
            let p50 = pct(&mut lat, 0.5);
            let p99 = pct(&mut lat, 0.99);
            let c50 = pct(&mut cpu, 0.5);
            let c99 = pct(&mut cpu, 0.99);
            println!("{n:>9} {build:>8.2} {est:>7} {rss:>7} | {label:<22} {c50:>6}µs {c99:>6}µs | {p50:>6}µs {p99:>6}µs {max:>6}µs");
        };
        let pick = |rng: &mut Rng, words_of: &[Vec<usize>]| -> Vec<usize> { words_of[rng.below(words_of.len())].clone() };
        let wo = &words_of;
        let v = &vocab;
        bench("single word", &mut |r| {
            let d = pick(r, wo);
            Search::new(&format!("{} ", v[d[0]])).limit(8)
        });
        bench("prefix (3 chars)", &mut |r| {
            let d = pick(r, wo);
            let w = &v[d[0]];
            Search::new(&w.chars().take(3).collect::<String>()).limit(8)
        });
        bench("prefix (1 char)", &mut |r| {
            let d = pick(r, wo);
            Search::new(&v[d[0]].chars().take(1).collect::<String>()).limit(8)
        });
        bench("typo (swap)", &mut |r| {
            let d = pick(r, wo);
            let mut c: Vec<char> = v[d[0]].chars().collect();
            if c.len() >= 4 {
                c.swap(1, 2);
            }
            Search::new(&format!("{} ", c.iter().collect::<String>())).limit(8)
        });
        bench("two words", &mut |r| {
            let d = pick(r, wo);
            let b = d.get(1).copied().unwrap_or(d[0]);
            Search::new(&format!("{} {}", v[d[0]], v[b])).limit(8)
        });
        // Words that occur in at most ~0.1% of the documents: what a person
        // typing something specific looks for.
        let rare: Vec<&Vec<usize>> = words_of.iter().filter(|d| d.len() >= 2 && d.iter().all(|&w| w > vocab.len() / 20)).take(5000).collect();
        if !rare.is_empty() {
            bench("two rare words, typed", &mut |r| {
                let d = rare[r.below(rare.len())];
                let last: String = v[d[1]].chars().take(3).collect();
                Search::new(&format!("{} {}", v[d[0]], last)).limit(8)
            });
        }
        bench("three words, typed", &mut |r| {
            let d = pick(r, wo);
            let (b, c) = (d.get(1).copied().unwrap_or(d[0]), d.get(2).copied().unwrap_or(d[0]));
            let last: String = v[c].chars().take(2).collect();
            Search::new(&format!("{} {} {}", v[d[0]], v[b], last)).limit(8)
        });
        bench("filtered word", &mut |r| {
            let d = pick(r, wo);
            Search::new(&format!("{} ", v[d[0]])).filter("category = Network").limit(8)
        });
        bench("filter only (match all)", &mut |_| Search::new("").filter("category = Sound").limit(8));
        bench("single + facet", &mut |r| {
            let d = pick(r, wo);
            Search::new(&format!("{} ", v[d[0]])).facet("category").limit(8)
        });
        println!();
        drop(idx);
    }
}
