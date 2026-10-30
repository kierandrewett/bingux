//! The route table: an application's navigation tree, declared as data.
//!
//! A table maps path patterns to pages. Each route carries its title, icon,
//! kind and options, so Back, the header, the history menu, the window title,
//! the sidebar and search all read the same declaration. See
//! `design/navigation.md` for the model and `docs/bingux-ui.md` for use.

use std::sync::Arc;

use freya::prelude::*;

use super::location::Location;
use super::pattern::{Bindings, Pattern};
use crate::icons::Icon;

/// What a route is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    /// The app's root location. On a compact window it is the screen that
    /// lists the roots; on a wide window the sidebar is that list and the
    /// first root is shown.
    Index,
    /// A sidebar destination.
    Root,
    /// A subpage, pushed over the page it was opened from.
    Page,
    /// A sibling view of one page: replaces its sibling instead of stacking.
    Tab,
    /// A modal sheet over the entry beneath. Back dismisses it first.
    Sheet,
    /// A modal dialog over the entry beneath. Back dismisses it first.
    Dialog,
}

impl RouteKind {
    /// Whether a route of this kind is drawn over the entry beneath it.
    pub fn is_modal(self) -> bool {
        matches!(self, RouteKind::Sheet | RouteKind::Dialog)
    }
}

/// How a move between two locations is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    /// The new page replaces the old at once.
    None,
    /// The new page slides in from the right over the old.
    Push,
    /// The page slides out to the right, revealing the one below.
    Pop,
    /// A cross-fade: a switch between roots.
    Fade,
}

/// How a table's stack behaves. See `design/navigation.md`, section 3.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Settings: `navigate` synthesises the ancestors of a deep link, Back
    /// leaves a page for the one it was opened from, there is no forward.
    Hierarchical,
    /// A file view: `navigate` only pushes, Back is the browser's Back and
    /// there is a forward stack. Moves are not animated.
    Linear,
}

/// The values a location bound to its route.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Params {
    scheme: String,
    path: Bindings,
    query: Vec<(String, String)>,
    fragment: Option<String>,
}

impl Params {
    /// The scheme of the location: a table that answers to several (a file
    /// view's `file:` and `recent:`) can tell them apart.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// The path parameter `name`, or `None` when the pattern has none.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.path
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The path parameter `name`; empty when the pattern has none. A pattern
    /// that names the parameter always binds it, so this is for page code.
    pub fn str(&self, name: &str) -> &str {
        self.get(name).unwrap_or_default()
    }

    /// The integer path parameter `name` (a `:name:int` segment).
    pub fn int(&self, name: &str) -> Option<i64> {
        self.get(name)?.parse().ok()
    }

    /// Query key `key`.
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// The fragment of the location.
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// Every path binding, in pattern order.
    pub fn bindings(&self) -> &[(String, String)] {
        &self.path
    }

    /// Params for tests and for building a location by hand.
    pub fn from_path(path: &[(&str, &str)]) -> Self {
        Self {
            path: path
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            ..Self::default()
        }
    }
}

/// How a route names itself.
#[derive(Clone)]
pub enum Title {
    Static(&'static str),
    /// The value of a path parameter: a Wi-Fi network's name is its segment.
    Param(&'static str),
    /// Worked out from the params, usually from a cache.
    With(Arc<dyn Fn(&Params) -> String + Send + Sync>),
    /// The last path segment, for routes with nothing better (a folder).
    LastSegment,
}

/// A query key a route declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuerySpec {
    pub key: &'static str,
    /// The value that is dropped from the canonical location.
    pub default: &'static str,
    /// A change of this key pushes a history entry instead of replacing.
    pub pushes: bool,
}

type PageFn = Arc<dyn Fn(&Params) -> Element + Send + Sync>;
type UpFn = Arc<dyn Fn(&Params) -> Option<Location> + Send + Sync>;
type RewriteFn = Arc<dyn Fn(&Location) -> Location + Send + Sync>;

/// One route: data, plus the function that makes its page.
#[derive(Clone)]
pub struct RouteDef {
    pattern: Pattern,
    pub kind: RouteKind,
    title: Title,
    pub icon: Option<Icon>,
    /// The sidebar section a root is listed under.
    pub group: &'static str,
    /// Extra words search matches, space separated.
    pub keywords: &'static str,
    up: Option<UpFn>,
    pub transition: Option<Transition>,
    pub ephemeral: bool,
    /// Whether a root is listed in the sidebar.
    pub listed: bool,
    pub query: Vec<QuerySpec>,
    page: PageFn,
}

impl RouteDef {
    /// The stable id of the route: its pattern text. Shown by the inspector.
    pub fn id(&self) -> &str {
        self.pattern.text()
    }

    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }
}

/// A location resolved to its route.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// Index of the route in the table.
    pub route: usize,
    pub params: Params,
}

/// A table of routes for one application scheme.
#[derive(Clone)]
pub struct RouteTable {
    scheme: String,
    aliases: Vec<String>,
    extra: Vec<String>,
    rooted: bool,
    pub(crate) mode: Mode,
    routes: Vec<RouteDef>,
    rewrite: Option<RewriteFn>,
    not_found: Option<PageFn>,
}

impl RouteTable {
    /// An empty table for `scheme`.
    pub fn new(scheme: &str) -> Self {
        Self {
            scheme: scheme.to_ascii_lowercase(),
            aliases: Vec::new(),
            extra: Vec::new(),
            rooted: false,
            mode: Mode::Hierarchical,
            routes: Vec::new(),
            rewrite: None,
            not_found: None,
        }
    }

    /// Accept `scheme` as another name for this table's scheme: a location
    /// with it is canonicalised to the table's.
    pub fn alias_scheme(mut self, scheme: &str) -> Self {
        self.aliases.push(scheme.to_ascii_lowercase());
        self
    }

    /// Also resolve locations of `scheme` against this table's routes, without
    /// rewriting them: a file view's `recent:///` is not a `file:` URI but is
    /// a place in the same history.
    pub fn also_scheme(mut self, scheme: &str) -> Self {
        self.extra.push(scheme.to_ascii_lowercase());
        self
    }

    /// Write locations in the rooted form, `scheme:///path`, as file URIs are.
    pub fn rooted(mut self) -> Self {
        self.rooted = true;
        self
    }

    /// The stack mode; see [`Mode`].
    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Rewrites a location before it is resolved: where old addresses of the
    /// app now live. It runs on every deep link.
    pub fn rewrite(mut self, rewrite: impl Fn(&Location) -> Location + Send + Sync + 'static) -> Self {
        self.rewrite = Some(Arc::new(rewrite));
        self
    }

    /// The page shown for a location no route owns. The default offers Up and
    /// the app root.
    pub fn not_found(
        mut self,
        page: impl Fn(&Params) -> Element + Send + Sync + 'static,
    ) -> Self {
        self.not_found = Some(Arc::new(page));
        self
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn stack_mode(&self) -> Mode {
        self.mode
    }

    /// The root location of the app.
    pub fn home(&self) -> Location {
        if self.rooted {
            Location::rooted(&self.scheme)
        } else {
            Location::new(&self.scheme)
        }
    }

    // ---- declaring routes -------------------------------------------------

    fn add(mut self, pattern: &str, kind: RouteKind, title: Title) -> Self {
        let pattern = Pattern::new(pattern);
        assert!(
            !self.routes.iter().any(|route| route.pattern == pattern),
            "route `{}` is declared twice",
            pattern.text()
        );
        self.routes.push(RouteDef {
            pattern,
            kind,
            title,
            icon: None,
            group: "",
            keywords: "",
            up: None,
            transition: None,
            ephemeral: false,
            listed: true,
            query: Vec::new(),
            page: Arc::new(|_| rect().into()),
        });
        self
    }

    /// The route that lists the roots, at the app's own location.
    pub fn index(self, title: &'static str, page: impl Fn(&Params) -> Element + Send + Sync + 'static) -> Self {
        self.add("", RouteKind::Index, Title::Static(title)).page(page)
    }

    /// A sidebar destination at `pattern`.
    pub fn root(self, pattern: &str) -> Self {
        self.add(pattern, RouteKind::Root, Title::LastSegment)
    }

    /// A subpage at `pattern`.
    pub fn route(self, pattern: &str) -> Self {
        self.add(pattern, RouteKind::Page, Title::LastSegment)
    }

    /// A tab: a sibling view of one page.
    pub fn tab(self, pattern: &str) -> Self {
        self.add(pattern, RouteKind::Tab, Title::LastSegment)
    }

    /// A modal sheet at `pattern`.
    pub fn sheet(self, pattern: &str) -> Self {
        self.add(pattern, RouteKind::Sheet, Title::LastSegment)
    }

    /// A modal dialog at `pattern`.
    pub fn dialog(self, pattern: &str) -> Self {
        self.add(pattern, RouteKind::Dialog, Title::LastSegment)
    }

    fn last(&mut self) -> &mut RouteDef {
        self.routes
            .last_mut()
            .expect("declare a route before setting its options")
    }

    /// The last route's title.
    pub fn title(mut self, title: &'static str) -> Self {
        self.last().title = Title::Static(title);
        self
    }

    /// The last route is titled by path parameter `name`.
    pub fn title_param(mut self, name: &'static str) -> Self {
        self.last().title = Title::Param(name);
        self
    }

    /// The last route is titled by `title`, worked out from its params.
    pub fn title_with(mut self, title: impl Fn(&Params) -> String + Send + Sync + 'static) -> Self {
        self.last().title = Title::With(Arc::new(title));
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.last().icon = Some(icon);
        self
    }

    /// The sidebar section of the last (root) route.
    pub fn group(mut self, group: &'static str) -> Self {
        self.last().group = group;
        self
    }

    /// Extra search words for the last route.
    pub fn keywords(mut self, keywords: &'static str) -> Self {
        self.last().keywords = keywords;
        self
    }

    /// Where Up goes from the last route, when that is not its path parent
    /// (a virtual parent). `None` falls back to the path.
    pub fn virtual_parent(mut self, up: impl Fn(&Params) -> Option<Location> + Send + Sync + 'static) -> Self {
        self.last().up = Some(Arc::new(up));
        self
    }

    /// How a move *to* the last route is drawn, overriding the depth rule.
    pub fn transition(mut self, transition: Transition) -> Self {
        self.last().transition = Some(transition);
        self
    }

    /// The last route is replaced, not stacked, when something else is
    /// navigated to from it: Back skips it.
    pub fn ephemeral(mut self) -> Self {
        self.last().ephemeral = true;
        self
    }

    /// The last (root) route is a destination but is not listed in the
    /// sidebar: the signed-in person's page, opened from the profile.
    pub fn unlisted(mut self) -> Self {
        self.last().listed = false;
        self
    }

    /// Declare query key `key` of the last route with its `default`, which is
    /// dropped from the canonical location. A change replaces the top entry.
    pub fn query(mut self, key: &'static str, default: &'static str) -> Self {
        self.last().query.push(QuerySpec {
            key,
            default,
            pushes: false,
        });
        self
    }

    /// As [`RouteTable::query`], but a change pushes a history entry.
    pub fn query_pushes(mut self, key: &'static str, default: &'static str) -> Self {
        self.last().query.push(QuerySpec {
            key,
            default,
            pushes: true,
        });
        self
    }

    /// The page constructor of the last route.
    pub fn page(mut self, page: impl Fn(&Params) -> Element + Send + Sync + 'static) -> Self {
        self.last().page = Arc::new(page);
        self
    }

    // ---- reading the table ------------------------------------------------

    pub fn routes(&self) -> &[RouteDef] {
        &self.routes
    }

    pub fn route_def(&self, index: usize) -> &RouteDef {
        &self.routes[index]
    }

    /// The destinations the sidebar lists: the roots that are not unlisted.
    pub fn sidebar(&self) -> impl Iterator<Item = (usize, &RouteDef)> {
        self.roots().filter(|(_, route)| route.listed)
    }

    /// The sidebar destinations in declaration order.
    pub fn roots(&self) -> impl Iterator<Item = (usize, &RouteDef)> {
        self.routes
            .iter()
            .enumerate()
            .filter(|(_, route)| route.kind == RouteKind::Root)
    }

    /// The location of the first root, which a wide window shows at the index.
    pub fn default_root(&self) -> Option<usize> {
        self.sidebar().next().map(|(index, _)| index)
    }

    /// The location a route with only literal segments names, such as a
    /// sidebar root: `settings://network`. `None` for a pattern with
    /// parameters.
    pub fn pattern_location(&self, route: usize) -> Option<Location> {
        let mut location = self.home();
        for part in self.routes[route].pattern.parts() {
            match part {
                super::pattern::Part::Literal(text) => location = location.join(text.clone()),
                _ => return None,
            }
        }
        Some(location)
    }

    /// `location` in the table's own form: its scheme (an alias becomes the
    /// scheme), the rewrite applied, the rooted form, declared default query
    /// values dropped, and the fragment kept.
    pub fn canonical(&self, location: &Location) -> Location {
        let mut found = location.clone();
        if self.aliases.iter().any(|alias| alias == found.scheme()) {
            let mut moved = Location::new(&self.scheme);
            if found.is_rooted() {
                moved = Location::rooted(&self.scheme);
            }
            moved = moved.join_path_of(&found);
            for (key, value) in found.query_pairs() {
                moved = moved.with_query(key.clone(), value.clone());
            }
            found = match found.fragment() {
                Some(fragment) => moved.with_fragment(fragment),
                None => moved,
            };
        }
        if found.scheme() != self.scheme {
            return found;
        }
        if let Some(rewrite) = &self.rewrite {
            let fragment = found.fragment().map(str::to_string);
            found = rewrite(&found);
            if found.fragment().is_none()
                && let Some(fragment) = fragment
            {
                found = found.with_fragment(fragment);
            }
        }
        found = self.with_form(found);
        if let Some(resolved) = self.resolve(&found) {
            for spec in &self.routes[resolved.route].query {
                if found.query(spec.key) == Some(spec.default) {
                    found = found.without_query_key(spec.key);
                }
            }
        }
        found
    }

    /// `location` rewritten to the table's rooted or host form.
    fn with_form(&self, location: Location) -> Location {
        if location.is_rooted() == self.rooted {
            return location;
        }
        let mut formed = self.home();
        formed = formed.join_path_of(&location);
        for (key, value) in location.query_pairs() {
            formed = formed.with_query(key.clone(), value.clone());
        }
        match location.fragment() {
            Some(fragment) => formed.with_fragment(fragment),
            None => formed,
        }
    }

    /// The route that owns `location` and the values it bound, or `None`
    /// when no route does or the scheme is not this table's.
    pub fn resolve(&self, location: &Location) -> Option<Resolved> {
        if location.scheme() != self.scheme && !self.extra.iter().any(|extra| extra == location.scheme()) {
            return None;
        }
        let mut best: Option<(Vec<u8>, usize, Bindings)> = None;
        for (index, route) in self.routes.iter().enumerate() {
            let Some(bound) = route.pattern.matches(location.segments()) else {
                continue;
            };
            let score = route.pattern.score();
            if best.as_ref().is_none_or(|(top, _, _)| score > *top) {
                best = Some((score, index, bound));
            }
        }
        let (_, route, path) = best?;
        Some(Resolved {
            route,
            params: Params {
                scheme: location.scheme().to_string(),
                path,
                query: location.query_pairs().to_vec(),
                fragment: location.fragment().map(str::to_string),
            },
        })
    }

    /// The route kind of `location`, `Page` when it resolves to nothing.
    pub fn kind(&self, location: &Location) -> RouteKind {
        self.resolve(location)
            .map_or(RouteKind::Page, |found| self.routes[found.route].kind)
    }

    /// The title of `location`.
    pub fn title_of(&self, location: &Location) -> String {
        let Some(found) = self.resolve(location) else {
            return "Not found".to_string();
        };
        let route = &self.routes[found.route];
        match &route.title {
            Title::Static(text) => (*text).to_string(),
            Title::Param(name) => found.params.str(name).to_string(),
            Title::With(title) => title(&found.params),
            Title::LastSegment => location
                .last_segment()
                .map_or_else(|| self.scheme.clone(), str::to_string),
        }
    }

    /// The icon of `location`'s route.
    pub fn icon_of(&self, location: &Location) -> Option<Icon> {
        self.resolve(location)
            .and_then(|found| self.routes[found.route].icon)
    }

    /// Where Up goes from `location`: the route's own answer, else the
    /// nearest ancestor along the path that a route owns, else the app root.
    /// `None` at the app root.
    pub fn up(&self, location: &Location) -> Option<Location> {
        if let Some(found) = self.resolve(location) {
            let route = &self.routes[found.route];
            if route.kind == RouteKind::Index {
                return None;
            }
            if let Some(up) = &route.up
                && let Some(target) = up(&found.params)
            {
                return Some(self.canonical(&target));
            }
        }
        let mut ancestor = location.up();
        while let Some(candidate) = ancestor {
            if self.resolve(&candidate).is_some() {
                return Some(candidate);
            }
            ancestor = candidate.up();
        }
        // No route above: the app's root, when it is a route of its own.
        let home = self.home();
        (self.resolve(&home).is_some() && !location.same_path(&home)).then_some(home)
    }

    /// The chain of locations from the app root to `location`, following
    /// [`RouteTable::up`]: what a deep link's stack is made of. The first is
    /// the index when the table has one; the last is `location`.
    pub fn chain(&self, location: &Location) -> Vec<Location> {
        let mut chain = vec![location.clone()];
        let mut current = location.clone();
        // The depth bound keeps a cyclic `up` override from looping forever.
        for _ in 0..64 {
            let Some(parent) = self.up(&current) else { break };
            if chain.iter().any(|seen| seen.same_path(&parent)) {
                break;
            }
            chain.push(parent.clone());
            current = parent;
        }
        chain.reverse();
        chain
    }

    /// The root route of `location`: its first ancestor (or itself) of kind
    /// `Root`.
    pub fn root_of(&self, location: &Location) -> Option<usize> {
        self.chain(location)
            .iter()
            .filter_map(|place| self.resolve(place))
            .find(|found| self.routes[found.route].kind == RouteKind::Root)
            .map(|found| found.route)
    }

    /// The element for `location`'s page: its route's page, or the
    /// not-found page. The page is built inside an error boundary, so a
    /// page that panics while it renders (a hook out of order, an `unwrap`)
    /// shows an error card with Retry in place of its content and leaves the
    /// window, the sidebar and the other pages running.
    pub fn render(&self, location: &Location) -> Element {
        let guarded = |page: PageFn, params: Params| -> Element {
            crate::resilience::error_boundary_with(self.title_of(location), move || page(&params))
                // One boundary per page: leaving a failed page and coming back
                // starts clean. A change of query or fragment rebuilds the page
                // in place, as before.
                .key(location.path_text())
                .built_from(location.to_string())
                .into()
        };
        match self.resolve(location) {
            Some(found) => guarded(self.routes[found.route].page.clone(), found.params),
            None => match &self.not_found {
                Some(page) => guarded(
                    page.clone(),
                    Params {
                        scheme: location.scheme().to_string(),
                        path: Vec::new(),
                        query: location.query_pairs().to_vec(),
                        fragment: location.fragment().map(str::to_string),
                    },
                ),
                None => super::router::not_found_page(location.clone()),
            },
        }
    }

    /// How moving from `from` to `to` is drawn. See `design/navigation.md`:
    /// the same path never slides; a deeper path pushes, a shallower one pops,
    /// a sibling pushes, another root fades. A `Linear` table never animates.
    pub fn classify(&self, from: &Location, to: &Location) -> Transition {
        if self.mode == Mode::Linear || from.same_path(to) {
            return Transition::None;
        }
        if let Some(found) = self.resolve(to)
            && let Some(transition) = self.routes[found.route].transition
            && to.depth() >= from.depth()
        {
            return transition;
        }
        let (from_depth, to_depth) = (from.depth(), to.depth());
        let different_root = from_depth > 0
            && to_depth > 0
            && self.root_of(from).is_some()
            && self.root_of(from) != self.root_of(to);
        if different_root {
            Transition::Fade
        } else if to_depth < from_depth {
            Transition::Pop
        } else {
            Transition::Push
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn loc(text: &str) -> Location {
        Location::parse(text).unwrap_or_else(|| panic!("{text}"))
    }

    pub(crate) fn settings() -> RouteTable {
        RouteTable::new("settings")
            .index("Settings", |_| rect().into())
            .root("network").title("Network").icon(Icon::Network).group("Connections")
            .root("sound").title("Sound").keywords("audio volume")
            .root("display").title("Displays")
            .route("network/wifi").title("Wi-Fi")
            .route("network/wifi/network/:ssid").title_param("ssid")
            .route("network/wifi/saved").title("Saved networks")
            .route("network/wifi/saved/:ssid").title_param("ssid")
            .route("network/proxy").title("Proxy")
            .route("network/ethernet/:device/profile/:uuid").title_with(|p| format!("Profile {}", p.str("uuid")))
            .route("network/ethernet/:device/profile").title("Profiles")
            .route("network/ethernet/:device").title_param("device")
            .route("sound/applications").title("Application volumes").ephemeral()
            .route("display/:index:int").title_with(|p| format!("Display {}", p.int("index").unwrap_or(0) + 1))
            .route("display/default").title("Default display")
            .tab("sound/devices").title("Devices")
            .query("view", "list")
    }

    #[test]
    fn a_location_resolves_to_its_route_and_params() {
        let table = settings();
        let found = table.resolve(&loc("settings://network/wifi/network/Home%20Net")).unwrap();
        assert_eq!(table.route_def(found.route).id(), "network/wifi/network/:ssid");
        assert_eq!(found.params.str("ssid"), "Home Net");
        let found = table
            .resolve(&loc("settings://network/ethernet/enp42s0/profile/abc?x=1#f"))
            .unwrap();
        assert_eq!(found.params.str("device"), "enp42s0");
        assert_eq!(found.params.str("uuid"), "abc");
        assert_eq!(found.params.query("x"), Some("1"));
        assert_eq!(found.params.fragment(), Some("f"));
        assert!(table.resolve(&loc("settings://nowhere")).is_none());
        assert!(table.resolve(&loc("other://network")).is_none());
    }

    #[test]
    fn literals_beat_parameters_and_typed_parameters_check() {
        let table = settings();
        let default = table.resolve(&loc("settings://display/default")).unwrap();
        assert_eq!(table.route_def(default.route).id(), "display/default");
        let numbered = table.resolve(&loc("settings://display/2")).unwrap();
        assert_eq!(numbered.params.int("index"), Some(2));
        assert!(table.resolve(&loc("settings://display/second")).is_none());
        // A profile list and a profile are different routes of different depth.
        let list = table.resolve(&loc("settings://network/ethernet/e0/profile")).unwrap();
        assert_eq!(table.route_def(list.route).id(), "network/ethernet/:device/profile");
    }

    #[test]
    fn titles_come_from_the_route() {
        let table = settings();
        assert_eq!(table.title_of(&loc("settings://")), "Settings");
        assert_eq!(table.title_of(&loc("settings://network/wifi/network/Home")), "Home");
        assert_eq!(table.title_of(&loc("settings://network/ethernet/e0/profile/u1")), "Profile u1");
        assert_eq!(table.title_of(&loc("settings://display/0")), "Display 1");
        assert_eq!(table.title_of(&loc("settings://nowhere")), "Not found");
        assert_eq!(table.icon_of(&loc("settings://network")), Some(Icon::Network));
    }

    #[test]
    fn up_is_the_nearest_ancestor_a_route_owns() {
        let table = settings();
        assert_eq!(table.up(&loc("settings://network/wifi/network/Home")), Some(loc("settings://network/wifi")));
        assert_eq!(table.up(&loc("settings://network/wifi")), Some(loc("settings://network")));
        assert_eq!(table.up(&loc("settings://network")), Some(loc("settings://")));
        assert_eq!(table.up(&loc("settings://")), None);
        // `network/ethernet` has no route: the profile list's ancestor is the
        // device page, and the device page's is the root page.
        assert_eq!(
            table.up(&loc("settings://network/ethernet/e0/profile")),
            Some(loc("settings://network/ethernet/e0"))
        );
        assert_eq!(table.up(&loc("settings://network/ethernet/e0")), Some(loc("settings://network")));
        // A location nothing owns still goes up to something that exists.
        assert_eq!(table.up(&loc("settings://nowhere/deep")), Some(loc("settings://")));
    }

    #[test]
    fn a_route_can_override_up_with_a_virtual_parent() {
        let table = settings()
            .route("sound/profile/:name")
            .title_param("name")
            .virtual_parent(|p| Location::parse(&format!("settings://sound/applications?from={}", p.str("name"))));
        assert_eq!(
            table.up(&loc("settings://sound/profile/Music")),
            Some(loc("settings://sound/applications?from=Music"))
        );
    }

    #[test]
    fn chain_is_what_a_deep_link_stack_is_made_of() {
        let table = settings();
        assert_eq!(
            table.chain(&loc("settings://network/wifi/saved/Home")),
            vec![
                loc("settings://"),
                loc("settings://network"),
                loc("settings://network/wifi"),
                loc("settings://network/wifi/saved"),
                loc("settings://network/wifi/saved/Home"),
            ]
        );
        assert_eq!(table.chain(&loc("settings://")), vec![loc("settings://")]);
    }

    #[test]
    fn a_cyclic_up_override_cannot_loop() {
        let table = RouteTable::new("a")
            .route("x").virtual_parent(|_| Location::parse("a://y"))
            .route("y").virtual_parent(|_| Location::parse("a://x"));
        let chain = table.chain(&loc("a://x"));
        assert!(chain.len() <= 2, "{chain:?}");
    }

    #[test]
    fn the_root_of_a_location_is_its_sidebar_page() {
        let table = settings();
        let network = table.resolve(&loc("settings://network")).unwrap().route;
        assert_eq!(table.root_of(&loc("settings://network/wifi/saved/Home")), Some(network));
        assert_eq!(table.root_of(&loc("settings://network")), Some(network));
        assert_eq!(table.root_of(&loc("settings://")), None);
        let roots: Vec<_> = table.roots().map(|(_, route)| route.id().to_string()).collect();
        assert_eq!(roots, ["network", "sound", "display"]);
    }

    #[test]
    fn transitions_follow_depth_and_root() {
        let table = settings();
        let t = |a: &str, b: &str| table.classify(&loc(a), &loc(b));
        // Deeper: push. Shallower: pop. Sibling: push.
        assert_eq!(t("settings://network", "settings://network/wifi"), Transition::Push);
        assert_eq!(t("settings://network/wifi", "settings://network"), Transition::Pop);
        assert_eq!(t("settings://network/wifi", "settings://network/proxy"), Transition::Push);
        // The list to a page pushes; a page to the list pops.
        assert_eq!(t("settings://", "settings://network"), Transition::Push);
        assert_eq!(t("settings://network", "settings://"), Transition::Pop);
        // Another root fades.
        assert_eq!(t("settings://network", "settings://sound"), Transition::Fade);
        assert_eq!(t("settings://network/wifi", "settings://sound/applications"), Transition::Fade);
        // Query and fragment changes never slide.
        assert_eq!(t("settings://network?a=1", "settings://network?a=2"), Transition::None);
        assert_eq!(t("settings://network", "settings://network#x"), Transition::None);
    }

    #[test]
    fn a_linear_table_never_animates() {
        let table = settings().mode(Mode::Linear);
        assert_eq!(table.classify(&loc("settings://network"), &loc("settings://network/wifi")), Transition::None);
    }

    #[test]
    fn a_route_can_choose_its_own_transition() {
        let table = settings().route("network/sheet").transition(Transition::Fade);
        assert_eq!(table.classify(&loc("settings://network"), &loc("settings://network/sheet")), Transition::Fade);
    }

    #[test]
    fn canonical_drops_declared_defaults_and_follows_aliases() {
        let table = settings().alias_scheme("bingux-settings").rewrite(|location| {
            if location.segments().first().map(String::as_str) == Some("wifi") {
                let mut moved = Location::new(location.scheme()).join("network");
                for segment in location.segments() {
                    moved = moved.join(segment.clone());
                }
                return moved;
            }
            location.clone()
        });
        assert_eq!(table.canonical(&loc("settings://sound/devices?view=list")), loc("settings://sound/devices"));
        assert_eq!(table.canonical(&loc("settings://sound/devices?view=grid")), loc("settings://sound/devices?view=grid"));
        assert_eq!(table.canonical(&loc("bingux-settings://network/proxy#m")), loc("settings://network/proxy#m"));
        assert_eq!(table.canonical(&loc("settings://wifi/saved")), loc("settings://network/wifi/saved"));
        // Someone else's scheme is left alone.
        assert_eq!(table.canonical(&loc("other://x")), loc("other://x"));
    }

    #[test]
    fn the_rooted_form_is_normalised() {
        let table = RouteTable::new("file")
            .rooted()
            .alias_scheme("bingux-files")
            .root("*path");
        assert_eq!(table.canonical(&loc("bingux-files://home/me")), loc("file:///home/me"));
        assert_eq!(table.canonical(&loc("file:///home/me")), loc("file:///home/me"));
        assert_eq!(table.home().to_string(), "file:///");
        let found = table.resolve(&loc("file:///home/me/Documents")).unwrap();
        assert_eq!(found.params.str("path"), "home/me/Documents");
    }

    #[test]
    #[should_panic(expected = "declared twice")]
    fn a_route_cannot_be_declared_twice() {
        let _ = RouteTable::new("a").route("x").route("x");
    }
}
