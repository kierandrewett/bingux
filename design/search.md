# Search in bingux-ui

Contributor design note. The usage docs are the "Search" section of
`docs/bingux-ui.md`; this file says why the pieces are the way they are, what
was compared, and what is deliberately not done. Update it when the design
changes.

## 1. What exists today (audit)

| Piece | Where | What it does | Why it is not the answer |
| ----- | ----- | ------------ | ------------------------ |
| Settings index | `packages/bingux-settings/src/settings_index.rs` | A `&[(PageId, title, keywords)]` table, ~330 rows. `matching(query)` lower-cases `title + keywords` once (`LazyLock`) and keeps rows that contain every whitespace-separated query word as a **substring**. `matching_pages` does the same for page titles. | Linear scan, substring only (no word boundaries, no typo tolerance, no ranking: index order), runs on the UI thread, private to Settings. The table has no breadcrumb, anchor or icon, so a hit can only open a page. |
| In-place filtering | `bingux-ui/src/search_highlight.rs`, `shell/sections.rs`, `shell/navigation.rs`, `settings/src/search_results.rs` | Pages not marked `index_only` render their real controls while the search has text; every `Row`/`Group` calls `use_match` and hides itself unless its text contains every query word; the sidebar lists the blocks that stayed visible (`sections::hits`, registered through `on_sized`). | It needs the whole page tree mounted, so it cannot work for anything outside the app (Spotlight) or for data that is not a row. Choosing a result only opens the page; no row is singled out. |
| Files name matcher | `packages/bingux-files/src/search.rs` | `Query::matches` gives a `Tier` (exact, prefix, word-start, substring, all words, fuzzy subsequence) over file names; `walk` is a cancellable BFS. | Right for "filter this folder" (names only, keeps the user's sort, no index). Stays as is. The file view's address-bar search may adopt `bingux_ui::search` later; **not migrated now**. |
| bingux-searchd | `packages/bingux-searchd` | The Spotlight daemon. The shell speaks protocol v1 over `$XDG_RUNTIME_DIR/bingux/search-v1.sock` (newline JSON, `query` / `activate` / `cancel`, records ≤ 64 KiB, socket 0600 in a 0700 directory). Built-in providers: applications (desktop files), files (bounded walker + optional plocate), calculation, weather, web, SQLite, AI chat. External providers are **subprocesses** declared by manifests (`bingux.search-provider`) speaking `hello` / `query` / `activate` on stdio, with `timeoutMs` ≤ 10 s, `priority` ≤ 1000, lazy/eager start and bounded backoff restarts. Results are `ProviderResult { resultId, kind, title, subtitle, icon, score }`; the daemon turns result ids into short-lived opaque activation ids. Ranking is `score` desc, then `0.001 × priority`, then stable fields; each local source contributes at most 5 results. | It already has the aggregator machinery (deadlines, cancellation, activation registry, limits). It has no notion of a running app's live data, of typed documents, facets or highlights, or of "the app is closed". It is extended, not replaced (section 8.6). |

The old Settings matcher is not slow (330 rows, tens of microseconds); the
problem is that it cannot express anything richer than "page + title" and cannot
leave the process.

## 2. Goals and non-goals

Goals

1. Any Bingux app registers *searchable documents* with bingux-ui once and gets
   in-app search for free; it does not write a matcher.
2. Documents look like Meilisearch documents (`id` + JSON fields), so a hit can
   be as thin as a title or as rich as an icon, breadcrumb, category, URI and
   highlighted ranges.
3. Responses are OpenSearch `_search` JSON (with a typed Rust view of the same
   data).
4. As-you-type: below 1 ms at 10k documents, below 10 ms at 1M short documents,
   never on the UI thread, superseded queries cancelled.
5. Spotlight (`bingux-searchd`) searches what an app *chose* to expose, whether
   the app is running or not, and can ask the app to open a hit.
6. UI-free engine: Spotlight and searchd link it without Freya.

Non-goals

- Full-text search over file contents; the Files app and searchd keep their own
  walkers. The engine is for *app-owned structured records* (settings, apps,
  contacts, notes, devices) up to about a million short documents.
- All of OpenSearch. A documented subset (section 6.1) is accepted; anything
  else is rejected with an OpenSearch-shaped error, never silently ignored.
- Several processes writing one index. One process owns an index.
- Language-specific stemming or dictionaries (section 4.1).

## 3. Document model

```
Document { id: String, fields: Map<String, serde_json::Value>, external: Option<bool> }
```

- `id`: unique per index; upserting an existing id replaces the document.
- `fields`: any JSON: text, number, bool, array, nested object. Attribute names
  in schemas and filters are dotted paths (`meta.owner`); arrays are flattened,
  as in Meilisearch. The whole object is stored as compact JSON and returned as
  `_source`.
- Conventional fields every app uses the same way (builder helpers exist; they
  are ordinary fields and nothing is special-cased in the engine): `title`,
  `breadcrumb` (array of strings), `keywords`, `body`, `category`, `icon`
  (icon name) and **`uri`**: where choosing the hit goes, a plain string in
  the app's own address scheme, e.g. `bingux-settings://network/proxy#method`.
  Hits navigate by handing the string to the app's router (`navigate(uri)`);
  an `Activate` from Spotlight delivers the same string. There is no opaque
  action object: the URI is the action.
- `external`: per-document override of the index's exposure (section 8.4). Not
  part of `_source`.

`Schema` (Meilisearch calls it settings) is per index:

| Setting | Meaning | Default |
| ------- | ------- | ------- |
| `searchable: [(field, weight)]` | Tokenised and searched fields. The weight scales BM25F and fixes the attribute rank (higher weight, better rank; ties keep declaration order). | at least one required |
| `filterable: [field]` | Usable in `filter`, `term`, `terms`, `range`, and as facets / `terms` aggregations. | none |
| `sortable: [field]` | Usable in `sort`. | none |
| `displayed: Option<[field]>` | Fields copied into `_source` of a hit. | all |
| `ranking_rules` | Ordered subset of `words, typo, proximity, attribute, exactness`. | all five, in that order |
| `synonyms`, `stop_words` | Query-side synonym groups; stop words are neither indexed nor searched. | empty |
| `typo` | `enabled`, `min_word_len_one_typo` (4), `min_word_len_two_typos` (8), words where it is off. | on |
| `external` | The index may be shown to other processes (section 8). | **false** |

Conventional weights: `title` 10, `keywords` 5, `breadcrumb` 3, `body` 1.

## 4. Index

### 4.1 Tokenizer

One tokenizer serves indexing, queries and highlight offsets, so the three
cannot disagree. It is language-agnostic: no stemming, no stop words unless the
schema lists some.

1. Words: Unicode word boundaries (UAX #29, `unicode-segmentation`). CJK
   ideographs come out one per token, so a Han query matches as a phrase of
   adjacent characters.
2. Sub-words: each word is split on `_` and `.`; apostrophes are removed
   (`don't` → `dont`); then at lower→Upper (`fooBar` → `foo`, `bar`) and at an
   acronym end of three or more capitals (`HTTPServer` → `http`, `server`).
   `IPv4` stays one token; `802.1x` is `802` `1x`.
3. Compounds: when a word was split by `-`, `_`, `.` or camel case, the joined
   form is also indexed at the position of the first part (`Wi-Fi` → `wi`@0,
   `fi`@1 and `wifi`@0). Typing `wifi`, `wi-fi` or `wi fi` all find it.
4. Folding: NFD, drop combining marks, lower-case (`Résumé` → `resume`). ASCII
   takes a no-allocation fast path. Folding is per character, so byte ranges
   map back to the original text for highlights.
5. Positions: one per token, `u16` per field (tokens past 65535 in a field are
   not indexed). Path and sentence separators (`/ \ › » > | ; , : . ! ?` between
   words, newline) add a gap of 8, so a phrase or proximity cannot straddle
   breadcrumb segments.

### 4.2 Structure

In memory, per index, behind one `RwLock`:

- **Dictionary**: `BTreeMap<Box<str>, TermId>`; a prefix is a range scan.
- **Postings** per term: parallel sorted `docs: Vec<u32>` and `occ: Vec<u32>`.
  `occ` packs one occurrence (7-bit field, 16-bit position) inline, or points
  into the list's `extra` array when a document has several. Internal doc ids
  only grow, so every list stays sorted by append and binary search works for
  intersections.
- **Documents**: id and compact JSON bytes, parsed only for returned hits.
  Per-field token counts (`u16`) feed BM25F.
- **Facets**: per filterable field, value → sorted doc list (strings
  case-folded for equality) and a numeric `BTreeMap`; per sortable field, a
  per-document column.
- **Liveness**: a bitset. Delete clears a bit; update is delete + append under
  a new internal id. Stale postings are skipped on read and compacted away
  (rewrite with id remapping) when more than a quarter of the ids are dead.
  Nothing is rebuilt per edit.

Writes are two-phase: tokenising a batch happens outside the lock; applying it
takes the write lock for at most a few hundred documents at a time, so a reader
never waits longer than that.

Memory is bounded by `Limits { max_documents, max_document_bytes,
max_index_bytes }` (defaults 2 000 000 / 64 KiB / 1 GiB); an upsert past a limit
returns `Err(LimitExceeded)` at the first document that would pass one; the
documents before it stay. `Index::memory_usage()`
reports an estimate from the lengths of the structures above.

### 4.3 Typo tolerance

For a query word of `n` chars: 0 typos below 4, 1 from 4, 2 from 8. The first
character must match exactly. Candidates are found by scanning the dictionary
range of that first character with a bounded Damerau-Levenshtein (a
transposition costs 1) that stops when the row minimum exceeds the budget. The
last word of a query that does not end with a space is also a *prefix*: a term
whose prefix is within the budget matches. At most 256 expansions per word are
kept (nearest first, then document frequency).

Rejected: a Levenshtein automaton over an FST. It needs an immutable dictionary
(we update incrementally), and the scan costs microseconds at the sizes in
section 11.

### 4.4 Ranking

Each candidate is scored into one `u64` key; ranking is a single integer
compare, and the `_score` returned is `key / 2^51`, a number in `[0, 1)` that
sorts exactly like the key. The key concatenates one bit field per ranking rule
in the configured order, then BM25F:

| Rule | Field in key | Better when |
| ---- | ------------ | ----------- |
| `words` | share of the query's words matched, 0..31 (5 bits) | more |
| `typo` | summed edit distance of matched words (6 bits) | fewer |
| `proximity` | minimal summed position gap between consecutive matched words, across fields 8 (8 bits) | smaller |
| `attribute` | summed rank of the best field each word matched in, then summed position (13 bits) | higher weight, earlier |
| `exactness` | share of the words matched by an identical term, not a prefix or typo, 0..31 (5 bits) | more |
| (tail) | BM25F quantised (14 bits) | higher |

This is Meilisearch's order with its documented equivalents. BM25F uses the
field weights and per-field length normalisation (k1 = 1.2, b = 0.75) as the
final tie-break and as the only discriminator when rules are removed. Remaining
ties keep insertion order (a document that was updated moves to the end). A
request `sort` takes precedence over all rules (OpenSearch semantics) and the key
is its tie-break.

Two bounds keep a query's cost independent of how common its words are. A
word scans at most 100 000 postings; a query that hits the bound (a one-letter
prefix in a million documents) returns what it saw and reports
`relation: "gte"`. A term with more than 10 000 postings skips the length
normalisation of BM25, which only breaks ties. The prefix expansions of words of
up to three letters are cached until the index changes.

`matching_strategy` (Meilisearch name) controls partial matches: `all` needs
every word, `last` (builder default) tries all words and drops words from the
end until enough hits exist, `any` matches any word. OpenSearch `match` maps to
`any`, or `all` with `"operator": "and"`.

## 5. Query language

Three ways in, one engine:

1. Builder: `Search::new("proxy").index("settings").filter("category = 'Network'").limit(8)`.
2. Meilisearch shorthand: `q`, `filter`, `facets`, `sort`, `limit`, `offset`,
   `attributesToHighlight`, `matchingStrategy`, as JSON.
3. OpenSearch body subset (section 6.1).

The text `q` is tokenised like a document. `"quoted text"` is a phrase: its
words must be adjacent in one field. A query ending in a space has no prefix
word.

Filter expressions (Meilisearch grammar, subset): `field = v`, `!=`, `<`, `<=`,
`>`, `>=`, `field a TO b`, `field IN [a, b]`, `field EXISTS`, `NOT`, `AND`,
`OR`, parentheses; strings in single or double quotes; equality on strings is
case-insensitive.

## 6. Response shape

```json
{
  "took": 1,
  "timed_out": false,
  "_shards": { "total": 1, "successful": 1, "skipped": 0, "failed": 0 },
  "hits": {
    "total": { "value": 3, "relation": "eq" },
    "max_score": 0.93,
    "hits": [{
      "_index": "settings", "_id": "network/proxy", "_score": 0.93,
      "_source": { "title": "Proxy", "breadcrumb": ["Network"], "icon": "network", "uri": "bingux-settings://network/proxy" },
      "highlight": { "title": ["<em>Pro</em>xy"] },
      "sort": [0.93, "network/proxy"]
    }]
  },
  "aggregations": { "by_category": { "doc_count_error_upper_bound": 0, "sum_other_doc_count": 0, "buckets": [{ "key": "Network", "doc_count": 2 }] } }
}
```

The typed Rust result (`SearchResponse`, `Hit`) carries the same data plus
`matches`: field → byte ranges (Meilisearch `_matchesPosition`), which a UI
needs to draw highlights without parsing `<em>`. `highlight` uses `<em>` /
`</em>` unless `pre_tags` / `post_tags` are given, and marks only the typed part
of a prefix match. `track_total_hits` defaults to 10 000, as in OpenSearch:
`relation` is `"gte"` when the true count is larger.

### 6.1 Request subset accepted

Supported: `query` as `match_all`, `match`, `multi_match` (fields with `^boost`,
`operator`), `match_phrase`, `match_phrase_prefix`, `prefix`, `term`, `terms`,
`range` (`gt gte lt lte`), `exists`, `bool` (`must`, `should`, `filter`,
`must_not`, `minimum_should_match` as an integer); `from`, `size` (≤ 1000,
`from + size` ≤ 10 000), `sort` (field with `asc`/`desc`, `_score`, `_id`),
`search_after`, `_source` (bool, includes/excludes list or object), `highlight`
(`fields`, `pre_tags`, `post_tags`, `number_of_fragments`, `fragment_size`),
`track_total_hits`, `timeout`, `aggs` with `terms` (`field`, `size`). Anything
else (`nested`, `script`, `fuzzy` as a clause, `function_score`, `knn`, other
aggregations) returns a `parsing_exception` error with the offending key. A
`bool` with text clauses scores the sum of its matching `must` and `should`
clauses, as OpenSearch does; a lone text clause keeps the exact rule order of
section 4.4.

## 7. Threading and lifecycle in bingux-ui

- `bingux_ui::search` re-exports the UI-free crate `bingux-search`
  (`packages/bingux-search`; deps `serde`, `serde_json`, `unicode-segmentation`,
  `unicode-normalization`). Spotlight and searchd depend on
  the crate directly.
- A process-wide `SearchService` holds indexes by name. An app registers a
  `Searchable` (name, schema, `documents()` iterator) or gets
  `service.index("settings")` and `upsert`s itself. Loading runs on the
  existing bingux-ui worker pool (`task.rs`), in write batches; a search issued
  before the load ends sees what is indexed so far and `SearchState::loading`
  stays true.
- `use_search(index, query)`: on a query change the previous job is dropped
  (skipped if still queued) and its cancel token set (a running search stops at
  the next 1 024 candidates), the new one is queued on the pool, and its result
  is written to a `State` from the UI thread when the future resolves. Freya
  polls the task once per event-loop turn, so the UI sees at most one write per
  frame however many keystrokes arrive. No debounce: the engine is fast enough
  that a delay would only add latency.
- No layout shift: `SearchState` keeps the previous hits while a newer query is
  running (`loading: true`) and starts empty and non-loading.
- Never run a query or an upsert in `render` or an event handler.

## 8. Spotlight: crossing the process boundary

### 8.1 What has to be possible

A. Spotlight finds a hit in a *running* app (live data).
B. Spotlight finds a hit in an app that is *closed*.
C. Choosing a hit opens its `uri` in the app, starting the app if needed.
D. Spotlight sees only what the app marked `external`; no other process can
   widen that.

### 8.2 What Bingux already uses for IPC (evidence)

| Job | Mechanism | Where |
| --- | --------- | ----- |
| Shell ↔ search daemon | Unix socket `$XDG_RUNTIME_DIR/bingux/search-v1.sock`, newline-delimited UTF-8 JSON, versioned records (`"v":1`), 64 KiB record cap, directory 0700, socket 0600, stale socket replaced; QML client `shell/bingux/SearchSocket.qml` | `bingux-searchd/src/server.rs`, `protocol.rs`; `docs/desktop-shell.md` ("The shell and daemon run as the profile user... does not listen on TCP") |
| Shell ↔ status daemon | Unix socket `$XDG_RUNTIME_DIR/bingux/metrics-v1.sock`, same framing | `bingux-statusd/README.md` |
| Daemon ↔ external search providers | Subprocess on stdio, same newline JSON, manifests of kind `bingux.search-provider` listed in `providerManifestPaths` | `bingux-searchd/src/external.rs`, `docs/search-presentation.md` |
| App single instance and URI hand-off | Unix socket `$XDG_RUNTIME_DIR/<app-id>.sock`; a second launch writes its address as one line and exits; the running copy opens it and raises its window | `bingux-ui/src/app.rs` (`socket_path`, `listen`, `start_forwarding`) |
| Launching an app with a URI | Desktop entries (`Exec=bingux-settings %u`, `MimeType=x-scheme-handler/bingux-settings`) launched through GIO (`shell/bingux/launch-application.py`); searchd already spawns programs for results (`Activation::Spawn`) | `packages/bingux-settings/bingux-settings.desktop`, `bingux-searchd/src/providers.rs` |
| Compositor test channel, notifications | D-Bus (Gnoblin test client, `org.freedesktop.Notifications`, GIO launch notifications) | `shell/bingux/shell_notify.py`, `docs/portable-testing.md` |
| systemd | user *services* only (`bingux-searchd.service`, `bingux-statusd.service`); no `.socket` units | `packaging/systemd/` |

No Bingux protocol between its own processes uses D-Bus; it is used where a
freedesktop service already speaks it. The Bingux convention for its own
protocols is a unix socket in `$XDG_RUNTIME_DIR/bingux/`, newline JSON, an
integer version in each record.

### 8.3 Options compared

| | Per-app unix socket, searchd framing, manifest + snapshot | D-Bus (zbus), one name per app | Apps push documents into searchd over `search-v1.sock` |
| - | - | - | - |
| Fits Bingux conventions | Yes: same directory, framing, permissions, versioning and error style as `search-v1.sock` and `metrics-v1.sock`; same per-app-socket pattern as the single-instance hand-off. | No other Bingux protocol uses it; adds `zbus` to searchd and a bus-name / XML / activation-file vocabulary the codebase does not otherwise have. | Transport yes, but it changes the closed v1 shell protocol or adds a third kind of client to it. |
| Discovery of running apps | `readdir` of `$XDG_RUNTIME_DIR/bingux/search/`; a dead socket fails to connect and is skipped. | Free (`ListNames`, `NameOwnerChanged`). | Not needed: apps connect to searchd. |
| Apps that are not running | A manifest (`bingux.search-app`, like the existing provider manifests) names the app and its command; the on-disk snapshot answers search; activation launches the app with the URI. | `.service` activation file; the bus starts the whole Freya app only to answer a search. | searchd keeps what was pushed; stale until the app next runs. |
| Live data | Yes, the app answers. | Yes. | Only what was pushed; every change costs a copy. |
| Latency | One local stream round trip, ≈ 0.05-0.1 ms + search. | ≈ 0.1-0.3 ms. | In-process in searchd, but searchd holds every app's documents in memory. |
| Security | 0700 directory, 0600 socket, same user; only `external` data is served. | Per-user bus; any same-user process can call it; same exposure. | Data leaves the app when pushed; any same-user peer could push into searchd. |
| Tooling | `socat - UNIX-CONNECT:...`, `nc -U` | `busctl`, `d-spy` | `socat` |
| New dependencies | none | `zbus` in searchd | none |

### 8.4 Decision

**A per-app unix socket, `$XDG_RUNTIME_DIR/bingux/search/<app-id>.sock`,
speaking searchd's framing (newline-delimited UTF-8 JSON, `"v":1`), with the
OpenSearch `_search` body and response as the payload; a `bingux.search-app`
manifest for apps that are not running; an on-disk snapshot index for search
while the app is closed; and activation by launching the app with the hit's
URI.** `bingux-searchd` becomes the aggregator client, as a new built-in source
next to applications and files; the shell protocol `search-v1` is not changed.

Why the socket and not D-Bus: it is what every other Bingux protocol is, so it
adds no dependency to searchd and no new vocabulary to maintain, and what
D-Bus would give is already provided by Bingux conventions: discovery by
directory (the app-id socket path), activation by the desktop entry and the
single-instance URI hand-off the apps already implement. Its weakness (a socket
exists only while the app runs) is exactly case B, which the snapshot covers.

systemd socket activation was considered and rejected: it would start the whole
app only to answer a read-only query that a snapshot answers in microseconds,
and Bingux has no `.socket` units to follow.

The framing code is shared: the bind / permission / record-reading helpers of
`bingux-searchd/src/server.rs` move into `bingux_search::ipc` and searchd
calls them, so there is one implementation.

### 8.5 Wire protocol (version 1)

Framing is `search-v1`'s: one JSON object per line. The record limit is 1 MiB
(a provider response carries rich hits; shell records keep their 64 KiB). The
server answers each request with exactly one record carrying the request's
`id`.

| Request `type` | Fields | Reply |
| -------------- | ------ | ----- |
| `hello` | | `{"v":1,"type":"hello","app":"bingux-settings","indexes":[{"name","title","documents","generation","updated"}]}`; **external indexes only** |
| `search` | `id`, `index`, `body` (an OpenSearch or Meilisearch-shaped object) | `{"v":1,"type":"result","id","generation","response":{...OpenSearch _search...}}` or `{"v":1,"type":"error","id","error":{"type","reason"},"status"}` |
| `activate` | `id`, `index`, `doc` | `{"v":1,"type":"activated","id"}` once the app's router has the `uri`; `error` if the document is gone |
| `subscribe` | | `{"v":1,"type":"subscribed"}`, then `{"v":1,"type":"changed","index","generation"}` events, at most one per 250 ms per index |

Only external indexes and, inside them, external documents are visible; an
unknown or non-external index answers `index_not_found_exception`, the same as
one that does not exist. `_source` is limited to the index's `displayed`
fields.

### 8.6 Permission and opt-in

Nothing is exposed by default.

1. The app declares per index `schema.external = true` in code. A document may
   opt out one by one (`Document.external = Some(false)`), never in.
2. The socket exists only while the app runs its provider
   (`Application::search_provider`), and a snapshot is written only for
   external indexes. An app with no external index opens no socket.
3. The `bingux.search-app` manifest is the packaging-level opt-in: searchd only
   queries apps that have one. The user can switch an app off with the existing
   `disabledProviders` list (provider id = app id) and weight it with
   `search.appWeights`.
4. Dynamic or private documents (Wi-Fi networks, Bluetooth devices) are in the
   in-app index but never `external`.

### 8.7 When the app is closed

`bingux_search::snapshot::save` writes `$XDG_STATE_HOME/bingux/search/<app-id>/<index>.idx`
(default `~/.local/state`): magic `BXSI`, format version, tokenizer version,
app version string, schema, the external documents (not the in-memory
structures: the reader rebuilds the index, which costs about 7 ms for the 200
documents of Settings and a few hundred ms for 10 000 on the loaded machine of
section 11, and searchd does it once, off the first keystroke). It goes to a temporary file
in the same directory, is fsynced and renamed, mode 0600, so a reader sees the
old or the new file, never a torn one. The app writes it after each
registration load and after a change settles (2 s debounce), only for external
indexes. A reader that finds another format or tokenizer version does not trust
the file. The app version string in the header means an updated app replaces
the snapshot of the old one on its next start ("rebuilt on update"), and
searchd ignores a snapshot whose app version differs from its manifest's, or
that is older than the program named by the manifest's `command` (the app was
updated and has not run since).

searchd's policy per app: connect to the socket with a deadline; if it answers,
use it; otherwise search the snapshot in-process (loaded once, reloaded when the
file's mtime changes). Activation spawns the manifest's `command` with the hit's
`uri`, which the single-instance hand-off turns into "open this address in the
running copy" or "start and open this address".

### 8.8 Hooking into searchd

`search-v1` is **not changed**: results keep `resultId / kind / title /
subtitle / icon / score`. The aggregator (`bingux_searchd::app_search`) is a
built-in source inside `LocalProviders::query`:

- it reads the manifests (`$XDG_DATA_DIRS/bingux/search-apps/*.json`), asks
  each app (socket, else snapshot) in parallel under a 40 ms deadline, merges by
  `_score × weight` (weight from `search.appWeights`, default 1.0), keeps at
  most 5 per app as local sources do, and maps each hit to a candidate: `title`
  = `_source.title`, `subtitle` = breadcrumb joined by ` › `, `icon` =
  `_source.icon`, `kind` = `action`, activation = spawn of the app with the
  `uri`;
- the same code returns the merged OpenSearch JSON (`aggregate()`) for a future
  rich Spotlight UI; v1 stays closed, a v2 can carry it.

The subprocess provider protocol is unchanged and remains the tool for things
that are not apps (web suggestions).

## 9. Migration of Settings

1. `settings_index.rs` becomes a builder of `Document`s (title, breadcrumb,
   keywords, `uri`, icon, category) registered in the `settings` index at
   startup; the ~330 rows keep their text. Page titles and sidebar keywords
   become documents too.
2. `search_results.rs` shows a ranked list of hits (icon, highlighted title,
   breadcrumb) from `use_search`. Choosing a hit does `navigate(uri)`; the
   fragment names the row to flash.
3. In-place filtering of live page trees (`search_highlight::section`,
   `use_match`) goes away for Settings; the helpers stay in bingux-ui for apps
   that still use them. The file view's address-bar search is **not** migrated
   now.
4. Dynamic documents (Wi-Fi networks, Bluetooth devices): only when already
   known to the page, `category: "Device"`, never `external`. Not done when the
   page has no cheap list.

## 10. Rejected alternatives

- **tantivy / meilisearch as a dependency**: tantivy 0.26 is in the cargo cache
  but pulls ~40 crates, is segment-based (commit latency, merge threads) and has
  no cheap per-keystroke prefix-typo story; Meilisearch is a server. A
  hand-rolled engine for ≤ 1M short documents owns the tokenizer used by
  highlights and has to meet the budgets in section 11.
- **SQLite FTS5** (already in searchd): no typo tolerance, no custom ranking,
  file I/O per keystroke.
- **`fst` for the dictionary**: immutable; incremental updates would need a
  second delta layer.
- **A debounce in `use_search`**: adds latency to cover work that is not slow.

## 11. Measured

`cargo run --release --example bench` in `packages/bingux-search`. Documents are
2-4 word titles and 2 keywords drawn from a Zipf-distributed vocabulary
(`n/2` pseudo words), plus a category; two searchable fields, one filterable.
Queries are built from words of real documents. **Latency is thread CPU time**
(`CLOCK_THREAD_CPUTIME_ID`), 600 samples (200 at 1M) after 20 warm-up runs,
single thread, no result cache. The machine (Ryzen 7 5800X, 16 threads) was at
load average 60 while this ran, so wall times were 1.2-3x larger and their tails
much larger; CPU time excludes waiting for a core but not cache and SMT
contention. Rerun on a quiet machine for wall numbers.

| docs | build | est. heap | RSS |
| ---: | ----: | --------: | --: |
| 1,000 | 0.03 s | 0 MiB | 0 MiB |
| 10,000 | 0.23 s | 2 MiB | 3 MiB |
| 100,000 | 2.92 s | 29 MiB | 37 MiB |
| 1,000,000 | 55.20 s | 281 MiB | 369 MiB |

| query | 1,000 p50 / p99 | 10,000 p50 / p99 | 100,000 p50 / p99 | 1,000,000 p50 / p99 |
| --- | ---: | ---: | ---: | ---: |
| single word | 19 µs / 45 µs | 40 µs / 211 µs | 202 µs / 1.9 ms | 547 µs / 15.9 ms |
| prefix (3 chars) | 17 µs / 41 µs | 42 µs / 197 µs | 325 µs / 2.1 ms | 3.4 ms / 10.0 ms |
| prefix (1 char) | 30 µs / 79 µs | 206 µs / 469 µs | 1.5 ms / 4.2 ms | 3.2 ms / 24.0 ms |
| typo (swap) | 19 µs / 44 µs | 60 µs / 184 µs | 719 µs / 2.3 ms | 5.9 ms / 25.9 ms |
| two words | 33 µs / 109 µs | 150 µs / 687 µs | 1.8 ms / 6.2 ms | 21.1 ms / 93.4 ms |
| two rare words, typed | 16 µs / 34 µs | 54 µs / 195 µs | 634 µs / 2.2 ms | 7.0 ms / 22.5 ms |
| three words, typed | 37 µs / 139 µs | 196 µs / 2.3 ms | 1.7 ms / 34.1 ms | 20.5 ms / 114.1 ms |
| filtered word | 9 µs / 19 µs | 22 µs / 126 µs | 71 µs / 1.7 ms | 487 µs / 30.6 ms |
| filter only (match all) | 8 µs / 17 µs | 9 µs / 12 µs | 20 µs / 28 µs | 167 µs / 250 µs |
| single + facet | 21 µs / 68 µs | 54 µs / 215 µs | 342 µs / 2.0 ms | 2.3 ms / 29.5 ms |

Reading it against the goals:

- **Under 1 ms at 10k documents**: met at p50 for every query type (largest:
  one-letter prefix, 206 µs). p99 passes 1 ms only for three-word typed queries
  (2.3 ms).
- **Under 10 ms at 1M short documents**: met at p50 for a single word (0.5 ms),
  a one-letter prefix (3.2 ms, from the cached expansion and the 100 000-posting
  scan bound), a three-letter prefix (3.4 ms), a typo (5.9 ms), a filtered word
  (0.5 ms), a facet (2.3 ms) and two rare words (7 ms). **Not met**: two or
  three words that are common in the corpus (21 ms and 20 ms). A random document
  of a Zipf corpus is mostly made of common words, so these queries intersect
  lists of tens of thousands of documents, and the cost is the candidates times
  the probe. **p99 at 1M is 10-30 ms for nearly everything** (the tail is cache
  misses and the machine's load; it was 3-5x worse in wall time).
- Build is 55 µs per document at 1M (single thread, tokenising included); memory
  is about 280 bytes per document (estimate) and 370 MiB resident for 1M.

## 12. Not done

- Dynamic documents for Settings (Wi-Fi networks, Bluetooth devices): the pages
  own that data in their own resources and there is no shared list to read;
  publishing one would also put names of devices in an index. Installed
  applications are indexed (private, loaded from the cached list).
- The file view's address-bar search is not migrated.
- Common-word multi-word queries at a million documents are above the 10 ms
  goal (section 11). A block-skipping list format or per-pair proximity indexes
  would fix it; neither is justified by an app that has such a corpus today.
- The snapshot stores documents, not the built index, so a reader pays the
  rebuild (section 8.7).
