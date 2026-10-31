//! Navigation: the split view, the page head, the header bar and search.

use std::sync::{Arc, LazyLock};

use bingux_ui::nav::router::use_local_router;
use bingux_ui::nav::{Confirmation, PopGuard, use_page_title, use_pop_guard, use_retained_state};
use bingux_ui::prelude::*;
use bingux_ui::search_highlight;
use freya::prelude::*;

use super::common::rows;
use crate::model::Spec;

/// The sample window's own routes: three sidebar pages and a subpage. The
/// specimens run on their own router (`use_local_router`), so a Back press in
/// a specimen never moves the taxonomy.
static SAMPLE: LazyLock<Arc<RouteTable>> = LazyLock::new(|| {
    Arc::new(
        RouteTable::new("sample")
            .index("Sample", |_| rect().into())
            .root("network")
            .title("Network")
            .icon(Icon::Network)
            .group("Connections")
            .page(|_| SamplePage { name: "Network" }.into())
            .root("sound")
            .title("Sound")
            .icon(Icon::Sound)
            .group("Hardware")
            .page(|_| SamplePage { name: "Sound" }.into())
            .root("power")
            .title("Power")
            .icon(Icon::Power)
            .group("Hardware")
            .page(|_| SamplePage { name: "Power" }.into())
            .route("network/saved")
            .title("Saved networks")
            .page(|_| {
                SamplePage {
                    name: "Saved networks",
                }
                .into()
            })
            .route("network/new")
            .title("New network")
            .page(|_| EditPage.into()),
    )
});

/// A page of the sample window. Its second row opens the subpage.
#[derive(PartialEq)]
struct SamplePage {
    name: &'static str,
}

impl Component for SamplePage {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        page_column()
            .child(
                Group::new(self.name.to_string())
                    .description("Rows of the sample page.")
                    .row(row("First row").subtitle("With a subtitle"))
                    .row(row("Second row").value("Value"))
                    .row(row("Opens a subpage").navigates().on_press(move |_| {
                        router.push(&Location::new("sample").join("network").join("saved"));
                    })),
            )
            .into_element()
    }
}

/// The sample window, starting at `at` (`network`, `network/saved`).
#[derive(PartialEq)]
struct SampleWindow {
    at: &'static str,
    profile: bool,
    tool: bool,
    search: bool,
}

impl Component for SampleWindow {
    fn render(&self) -> impl IntoElement {
        let mut start = Location::new("sample");
        for segment in self.at.split('/') {
            start = start.join(segment);
        }
        use_local_router(SAMPLE.clone(), &start);
        let query = use_state(|| "wi".to_string());
        let mut split = NavigationSplitView::new("Sample");
        if self.profile {
            split = split.profile(Profile {
                name: "Kieran".to_string(),
                target: Location::new("sample").join("power"),
            });
        }
        if self.tool {
            split =
                split.header_trailing(icon_button(Icon::ViewMore, |_| {}).tooltip("View options"));
        }
        if self.search {
            let results: Element = SearchSample { query }.into();
            split = split.search(query, results, |_: ()| {});
        }
        split
    }
}

fn sample(at: &'static str, profile: bool, tool: bool, search: bool) -> Element {
    SampleWindow {
        at,
        profile,
        tool,
        search,
    }
    .into_element()
}

pub const SPLIT_VIEW: &[Spec] = &[
    Spec::live("Sidebar, pages and header", || {
        sample("network", false, false, false)
    })
    .page(460.0),
    Spec::live("A subpage with Back (and its history menu)", || {
        sample("network/saved", false, false, false)
    })
    .page(460.0),
    Spec::forced("With a profile", || sample("sound", true, false, false)).page(460.0),
    Spec::forced("A tool in the header", || {
        sample("power", false, true, false)
    })
    .page(460.0),
];

pub const PAGE_HEAD: &[Spec] = &[
    Spec::live("Subtitle and an action", || {
        page_column()
            .child(
                PageHead::new()
                    .subtitle("Connected to HomeNet")
                    .action("Scan", |_| {}),
            )
            .into_element()
    }),
    Spec::live("A master switch", || {
        let mut on = use_state(|| true);
        page_column()
            .child(
                PageHead::new()
                    .subtitle(if on() { "On" } else { "Off" })
                    .switch("Wi-Fi", on(), move |next| on.set(next)),
            )
            .into_element()
    }),
    Spec::forced("No subtitle", || {
        page_column().child(PageHead::new()).into_element()
    }),
];

pub const HEADER_BAR: &[Spec] = &[
    Spec::live(
        "Title, leading and trailing controls, window buttons",
        || {
            HeaderBar::new("Taxonomy")
                .leading(icon_button(Icon::GoPrevious, |_| {}).tooltip("Back"))
                .trailing(icon_button(Icon::ViewMore, |_| {}).tooltip("More"))
                .trailing(WindowControls)
                .into_element()
        },
    ),
    Spec::forced("Busy", || {
        HeaderBar::new("Saving")
            .busy(true)
            .trailing(WindowControls)
            .into_element()
    }),
    Spec::forced("Long title", || {
        HeaderBar::new("A title far too long for the bar it is centred across")
            .trailing(WindowControls)
            .into_element()
    }),
    Spec::forced("Title slot", || {
        HeaderBar::new("")
            .title_slot(segmented(
                vec![("a".into(), "Day".into()), ("b".into(), "Week".into())],
                "a",
                |_| {},
            ))
            .into_element()
    }),
];

const TOPICS: [(&str, &str); 4] = [
    ("Wi-Fi", "Connect to a wireless network"),
    ("Wi-Fi hotspot", "Share this connection"),
    ("Ethernet", "Wired connection"),
    ("VPN", "Private tunnels"),
];

pub const SEARCH: &[Spec] = &[
    Spec::live("Type in the sidebar field: matches are marked", || {
        sample("network", false, false, true)
    })
    .page(460.0),
    Spec::forced("Marked text helpers", || {
        let tokens = use_tokens();
        let query = use_state(|| "wi fi".to_string());
        search_highlight::provide(query);
        let matched = search_highlight::hits("wi fi", "Wi-Fi hotspot");
        rows(vec![
            row("hits(query, text)")
                .subtitle(matched.to_string())
                .into_element(),
            search_highlight::label(typography::BODY, &tokens, "Wi-Fi hotspot"),
            search_highlight::paragraph(
                typography::CAPTION,
                &tokens,
                "Share the Wi-Fi connection of this computer",
            ),
        ])
    }),
];

#[derive(PartialEq)]
struct SearchSample {
    query: State<String>,
}

impl Component for SearchSample {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        search_highlight::provide(self.query);
        let text = self.query.read().to_lowercase();
        let mut group = Group::new("Network".to_string());
        for (title, detail) in TOPICS {
            if search_highlight::hits(&text, &format!("{title} {detail}")) || text.trim().is_empty()
            {
                group = group.row(
                    rect()
                        .tw(&tokens, &[w_full(), flex_col(), gap(S1)])
                        .child(search_highlight::label(typography::BODY, &tokens, title))
                        .child(search_highlight::label(
                            typography::CAPTION,
                            &tokens,
                            detail,
                        )),
                );
            }
        }
        search_highlight::page_block(
            "Network",
            Icon::Network,
            page_column().child(group).into_element(),
        )
    }
}

pub const ADDRESS: &[Spec] = &[
    Spec::live("The window's location, and moving it", || {
        let location = use_location();
        let router = use_router();
        rows(vec![
            row("use_location()")
                .mono_value(location.to_string())
                .into_element(),
            row("Open Controls / Buttons")
                .navigates()
                .on_press(|_| navigate("bingux-taxonomy://controls/buttons"))
                .into_element(),
            row("Open Icons / Icon set")
                .navigates()
                .on_press(move |_| {
                    router.switch_root(
                        &Location::new("bingux-taxonomy")
                            .join("icons")
                            .join("icon-set"),
                    )
                })
                .into_element(),
            row("Open the root location (the list of pages on a phone)")
                .navigates()
                .on_press(|_| navigate("bingux-taxonomy://"))
                .into_element(),
        ])
    }),
    Spec::forced("Locations are values", || {
        let sample = Location::new("bingux-settings")
            .join("network")
            .join("wifi")
            .with_query("filter", "open")
            .with_fragment("method");
        rows(vec![
            row("Location::new(scheme).join(seg).with_query(k, v).with_fragment(f)")
                .mono_value(sample.to_string())
                .into_element(),
            row("up()")
                .mono_value(
                    sample
                        .up()
                        .map_or_else(String::new, |location| location.to_string()),
                )
                .into_element(),
            row("Location::parse(text)")
                .mono_value(format!(
                    "{:?}",
                    Location::parse("bingux-files://home/Documents")
                        .map(|location| location.segments().to_vec())
                ))
                .into_element(),
        ])
    }),
    Spec::forced("The table behind the sidebar", || {
        let table = crate::app::table();
        rows(vec![
            row("RouteTable::new(scheme).root(pattern).title(t).icon(i).group(g).page(f)")
                .subtitle(format!(
                    "{} routes, {} sidebar pages",
                    table.routes().len(),
                    table.roots().count()
                ))
                .into_element(),
            row("table.title_of(&location)")
                .mono_value(
                    table.title_of(
                        &Location::new("bingux-taxonomy")
                            .join("controls")
                            .join("buttons"),
                    ),
                )
                .into_element(),
        ])
    }),
];

/// A page that holds an unsaved name: Back asks before it drops it, the header
/// names the page after what it holds, and the name survives a visit to
/// another page and back.
#[derive(PartialEq)]
struct EditPage;

impl Component for EditPage {
    fn render(&self) -> impl IntoElement {
        let mut name = use_retained_state("name", String::new);
        let dirty = !name.read().is_empty();
        use_pop_guard(move || {
            if dirty {
                PopGuard::Confirm(Confirmation {
                    title: "Discard this network?".to_string(),
                    body: "The name you typed will be lost.".to_string(),
                    discard: "Discard".to_string(),
                })
            } else {
                PopGuard::Allow
            }
        });
        use_page_title(if dirty {
            "Unsaved network"
        } else {
            "New network"
        });
        page_column()
            .child(Group::new("Network".to_string()).row(text_row(
                "Name",
                name,
                move |next: String| name.set(next),
            )))
            .into_element()
    }
}

/// The back stack on view: Back and Forward with their history menus (right
/// click or hold), the stack as the router keeps it, and a page whose Back is
/// guarded.
#[derive(PartialEq)]
struct StackDemo;

impl Component for StackDemo {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let router = use_local_router(SAMPLE.clone(), &Location::new("sample").join("network"));
        let model = router.model();
        let mut lines = vstack(S1);
        for entry in model.back_entries() {
            lines = lines.child(caption(format!("back: {}", model.title(entry))));
        }
        lines = lines.child(styled_label(
            typography::EMPHASIS,
            format!("now: {}", model.title(model.current())),
        ));
        for entry in model.forward_entries() {
            lines = lines.child(caption(format!("forward: {}", model.title(entry))));
        }
        let table = router.table();
        vstack(S3)
            .child(
                hstack(S2)
                    .child(BackButton)
                    .child(ForwardButton)
                    .child(
                        action_button("Saved networks", move |_| {
                            router.push(&Location::new("sample").join("network").join("saved"));
                        })
                        .compact(),
                    )
                    .child(
                        action_button("New network", move |_| {
                            router.push(&Location::new("sample").join("network").join("new"));
                        })
                        .compact(),
                    ),
            )
            .child(lines)
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h(Dim::Box24)])
                    .child(table.render(&router.location())),
            )
            .into_element()
    }
}

pub const BACK_STACK: &[Spec] =
    &[
        Spec::live("Back, Forward, history menu, guard, retained state", || {
            StackDemo.into_element()
        })
        .page(520.0),
    ];

/// Six documents the search demo registers.
struct DemoWords;

impl bingux_ui::search::Searchable for DemoWords {
    fn name(&self) -> &str {
        "taxonomy-search"
    }

    fn schema(&self) -> bingux_ui::search::Schema {
        bingux_ui::search::Schema::new()
            .searchable("title", 10.0)
            .searchable("keywords", 5.0)
            .searchable("breadcrumb", 3.0)
    }

    fn documents(&self) -> Box<dyn Iterator<Item = bingux_ui::search::Document> + Send + '_> {
        use bingux_ui::search::Document;
        let rows: [(&str, &str, &str, &str); 6] = [
            ("proxy", "Proxy", "network", "Network"),
            (
                "wifi-password",
                "Wi-Fi password",
                "saved key passphrase",
                "Network",
            ),
            (
                "bluetooth",
                "Bluetooth",
                "devices pair headphones",
                "Devices",
            ),
            ("keyboard", "Keyboard layout", "input method", "Devices"),
            ("accent", "Accent colour", "theme", "Appearance"),
            (
                "night-light",
                "Night light",
                "colour temperature",
                "Displays",
            ),
        ];
        Box::new(rows.into_iter().map(|(id, title, words, page)| {
            Document::new(id)
                .title(title)
                .keywords(words)
                .breadcrumb([page])
        }))
    }
}

/// A search field over a small registered index: results appear as you type,
/// forgive typos and mark the match.
fn search_service_demo() -> Element {
    use_hook(|| {
        let _ = bingux_ui::search::register(DemoWords);
    });
    let query = use_state(|| "blutooth".to_string());
    let text = query.read().clone();
    let state = bingux_ui::search::use_search("taxonomy-search", &text);
    let mut group = Group::new(None).unfiltered();
    for hit in &state.hits {
        let mut r = row(hit.title().unwrap_or_default())
            .title_marks(hit.ranges("title"))
            .subtitle(hit.breadcrumb().join(" \u{203a} "));
        if let Some((_, ranges)) = hit.joined("breadcrumb", " \u{203a} ") {
            r = r.subtitle_marks(ranges);
        }
        group = group.row(r);
    }
    rect()
        .tw(&use_tokens(), &[flex_col(), gap(S2)])
        .child(search_field(query, |_| {}).fill_width())
        .maybe_child((!state.hits.is_empty()).then_some(group))
        .into()
}

pub const SEARCH_SERVICE: &[Spec] = &[Spec::live(
    "Type: a typo (`blutooth`) still finds Bluetooth",
    search_service_demo,
)];

pub const MASTER_DETAIL: &[Spec] = &[Spec::live("A list and its page", || {
    let selected = use_state(|| 0_usize);
    let names = ["Equalizer", "Compressor", "Limiter"];
    let rows_for = move |current: usize| -> Element {
        let mut selected = selected;
        Group::new(None::<String>)
            .rows(names.iter().enumerate().map(|(index, name)| {
                row(*name)
                    .selected(index == current)
                    .on_press(move |_| selected.set(index))
                    .into_element()
            }))
            .into_element()
    };
    let current = *selected.read();
    let detail = Group::titled(names[current])
        .row(row("Settings of this effect"))
        .into_element();
    master_detail(rows_for(current), detail, true, |()| {}).into_element()
})
.page(260.0)];
