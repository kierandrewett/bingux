//! Pure readiness aggregation for one hosted session generation.
//!
//! The bridge feeds this state machine with facts (Gnoblin ready, output
//! configured, lines from `ready.sock`, Gnoblin's layer-surface list) and gets
//! back the `report` requests it must send to bsm. Nothing here reads a clock
//! or does I/O, so the evidence rules are unit-tested exhaustively.
//!
//! Rules (see `docs/bsm/bridge.md`):
//!
//! * A line whose generation differs from this session's is ignored.
//! * `gnoblin_ready` is sent once, when Gnoblin reports its runtime handshake.
//! * `output_configured` is sent once per output, only after the virtual
//!   monitor exists, the DisplayConfig layout is applied and the bsm configure
//!   is acknowledged. `a` counts configured outputs, `b` known outputs.
//! * `bingux_launched` needs `launched` from every required component.
//! * A surface counts as mapped on an output only when Bingux said so **and**
//!   Gnoblin's layer list holds a mapped layer surface with the role's
//!   namespace on that output's monitor.
//! * `desktop_ready` needs `ready` from every required component, every
//!   required surface confirmed on every output, and is sent once.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// Surface role a Bingux window can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Wallpaper,
    Bar,
    Dock,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Wallpaper, Role::Bar, Role::Dock];

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "wallpaper" => Some(Role::Wallpaper),
            "bar" => Some(Role::Bar),
            "dock" => Some(Role::Dock),
            _ => None,
        }
    }

    /// `zwlr_layer_surface_v1` namespace Bingux uses for this role.
    pub fn namespace(self) -> &'static str {
        match self {
            Role::Wallpaper => "bingux-wallpaper",
            Role::Bar => "bingux-top-bar",
            Role::Dock => "bingux-dock",
        }
    }
}

/// One line on `ready.sock`, as sent by the Bingux shell processes.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ReadyLine {
    pub generation: String,
    /// `wallpaper`, `desktop`, or absent for a single combined reporter.
    #[serde(default)]
    pub component: Option<String>,
    pub event: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub mapped: Option<bool>,
    /// On `launched`: roles this component maps on every output.
    #[serde(default)]
    pub required: Option<Vec<String>>,
}

/// A layer surface as listed by Gnoblin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSurface {
    pub namespace: String,
    /// Connector of the monitor the surface is on.
    pub connector: String,
    pub mapped: bool,
}

/// A report to send to bsm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    GnoblinReady,
    OutputConfigured { output: String, a: u32, b: u32 },
    BinguxLaunched,
    SurfacesMapped { output: String, a: u32, b: u32 },
    DesktopReady { a: u32, b: u32 },
}

const COMPONENT_ALL: &str = "all";
const COMPONENT_WALLPAPER: &str = "wallpaper";
const COMPONENT_DESKTOP: &str = "desktop";

fn default_roles(component: &str) -> Vec<Role> {
    match component {
        COMPONENT_WALLPAPER => vec![Role::Wallpaper],
        COMPONENT_DESKTOP => vec![Role::Bar, Role::Dock],
        _ => Role::ALL.to_vec(),
    }
}

#[derive(Debug, Default)]
struct Component {
    launched: bool,
    ready: bool,
    roles: Vec<Role>,
}

#[derive(Debug)]
pub struct Readiness {
    generation: String,
    gnoblin_ready: bool,
    gnoblin_reported: bool,
    outputs: BTreeSet<String>,
    configured: BTreeSet<String>,
    configured_reported: BTreeSet<String>,
    /// bsm output name -> Gnoblin connector.
    connectors: BTreeMap<String, String>,
    components: BTreeMap<String, Component>,
    /// (output as named by Bingux, role) -> mapped.
    surfaces: BTreeMap<(String, Role), bool>,
    layers: Vec<LayerSurface>,
    launched_reported: bool,
    surfaces_reported: BTreeMap<String, (u32, u32)>,
    desktop_reported: bool,
    rejected: u64,
}

impl Readiness {
    pub fn new(generation: &str) -> Self {
        Self {
            generation: generation.to_owned(),
            gnoblin_ready: false,
            gnoblin_reported: false,
            outputs: BTreeSet::new(),
            configured: BTreeSet::new(),
            configured_reported: BTreeSet::new(),
            connectors: BTreeMap::new(),
            components: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            layers: Vec::new(),
            launched_reported: false,
            surfaces_reported: BTreeMap::new(),
            desktop_reported: false,
            rejected: 0,
        }
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    /// Lines ignored because of a wrong generation or malformed content.
    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    pub fn set_gnoblin_ready(&mut self, ready: bool) -> Vec<Report> {
        self.gnoblin_ready = ready;
        self.evaluate()
    }

    pub fn output_added(&mut self, name: &str) -> Vec<Report> {
        self.outputs.insert(name.to_owned());
        self.evaluate()
    }

    pub fn output_removed(&mut self, name: &str) -> Vec<Report> {
        self.outputs.remove(name);
        self.configured.remove(name);
        self.configured_reported.remove(name);
        self.connectors.remove(name);
        self.surfaces_reported.remove(name);
        self.evaluate()
    }

    /// The virtual monitor of `output` exists, has connector `connector`, the
    /// layout is applied and bsm's configure is acknowledged.
    pub fn output_configured(&mut self, output: &str, connector: &str) -> Vec<Report> {
        self.outputs.insert(output.to_owned());
        self.configured.insert(output.to_owned());
        self.connectors.insert(output.to_owned(), connector.to_owned());
        self.evaluate()
    }

    /// The output must be configured again (size changed, monitor rebuilt).
    pub fn output_unconfigured(&mut self, output: &str) -> Vec<Report> {
        self.configured.remove(output);
        self.configured_reported.remove(output);
        self.surfaces_reported.remove(output);
        self.evaluate()
    }

    pub fn set_layers(&mut self, layers: Vec<LayerSurface>) -> Vec<Report> {
        self.layers = layers;
        self.evaluate()
    }

    pub fn ready_line(&mut self, line: &ReadyLine) -> Vec<Report> {
        if line.generation != self.generation {
            self.rejected += 1;
            return Vec::new();
        }
        let component = line
            .component
            .clone()
            .unwrap_or_else(|| COMPONENT_ALL.to_owned());
        match line.event.as_str() {
            "launched" => {
                let roles = match &line.required {
                    Some(list) => list.iter().filter_map(|r| Role::parse(r)).collect(),
                    None => default_roles(&component),
                };
                let entry = self.components.entry(component).or_default();
                entry.launched = true;
                entry.roles = roles;
            }
            "surface" => {
                let (Some(output), Some(role), Some(mapped)) =
                    (line.output.as_ref(), line.role.as_deref().and_then(Role::parse), line.mapped)
                else {
                    self.rejected += 1;
                    return Vec::new();
                };
                self.surfaces.insert((output.clone(), role), mapped);
                // A surface line implies the reporter exists even if its
                // `launched` line was lost; launched still has to arrive.
                if !mapped {
                    // A hidden surface invalidates an earlier `ready`.
                    if let Some(c) = self.components.get_mut(&component) {
                        c.ready = false;
                    }
                }
            }
            "ready" => {
                let entry = self.components.entry(component.clone()).or_default();
                if entry.roles.is_empty() {
                    entry.roles = default_roles(&component);
                }
                entry.ready = true;
            }
            _ => {
                self.rejected += 1;
                return Vec::new();
            }
        }
        self.evaluate()
    }

    fn required_components(&self) -> Vec<&'static str> {
        if self.components.contains_key(COMPONENT_ALL) {
            vec![COMPONENT_ALL]
        } else {
            vec![COMPONENT_WALLPAPER, COMPONENT_DESKTOP]
        }
    }

    fn required_roles(&self) -> BTreeSet<Role> {
        let mut roles = BTreeSet::new();
        for name in self.required_components() {
            match self.components.get(name) {
                Some(c) if !c.roles.is_empty() => roles.extend(c.roles.iter().copied()),
                _ => roles.extend(default_roles(name)),
            }
        }
        roles
    }

    /// Is `role` mapped on `output`, by Bingux's word and by Gnoblin's list?
    fn confirmed(&self, output: &str, role: Role) -> bool {
        let connector = self.connectors.get(output).map(String::as_str);
        let said = |name: &str| self.surfaces.get(&(name.to_owned(), role)).copied().unwrap_or(false);
        let bingux_says = said(output) || connector.is_some_and(said);
        if !bingux_says {
            return false;
        }
        let Some(connector) = connector else { return false };
        self.layers
            .iter()
            .any(|l| l.mapped && l.namespace == role.namespace() && l.connector == connector)
    }

    fn evaluate(&mut self) -> Vec<Report> {
        let mut out = Vec::new();

        if self.gnoblin_ready && !self.gnoblin_reported {
            self.gnoblin_reported = true;
            out.push(Report::GnoblinReady);
        }

        // Outputs are reported only once Gnoblin is ready: bsm counts them
        // after the GnoblinReady milestone.
        if self.gnoblin_reported {
            let total = self.outputs.len() as u32;
            let done = self.configured.len() as u32;
            let newly: Vec<String> = self
                .configured
                .iter()
                .filter(|o| !self.configured_reported.contains(*o))
                .cloned()
                .collect();
            for output in newly {
                self.configured_reported.insert(output.clone());
                out.push(Report::OutputConfigured { output, a: done, b: total });
            }
        }

        let required_components = self.required_components();
        let launched = required_components
            .iter()
            .all(|c| self.components.get(*c).is_some_and(|c| c.launched));
        if launched && !self.launched_reported {
            self.launched_reported = true;
            out.push(Report::BinguxLaunched);
        }

        let all_configured =
            !self.outputs.is_empty() && self.outputs.iter().all(|o| self.configured.contains(o));

        if self.launched_reported && self.gnoblin_reported {
            let roles = self.required_roles();
            let mut all_mapped = true;
            let outputs: Vec<String> = self.configured.iter().cloned().collect();
            for output in &outputs {
                let b = roles.len() as u32;
                let a = roles.iter().filter(|r| self.confirmed(output, **r)).count() as u32;
                if a != b {
                    all_mapped = false;
                }
                if self.surfaces_reported.get(output) != Some(&(a, b)) {
                    self.surfaces_reported.insert(output.clone(), (a, b));
                    out.push(Report::SurfacesMapped { output: output.clone(), a, b });
                }
            }

            let all_ready = required_components
                .iter()
                .all(|c| self.components.get(*c).is_some_and(|c| c.ready));
            if all_ready
                && all_mapped
                && all_configured
                && !roles.is_empty()
                && !outputs.is_empty()
                && !self.desktop_reported
            {
                self.desktop_reported = true;
                let n = outputs.len() as u32;
                out.push(Report::DesktopReady { a: n, b: n });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(component: Option<&str>, event: &str) -> ReadyLine {
        ReadyLine {
            generation: "7".into(),
            component: component.map(str::to_owned),
            event: event.into(),
            output: None,
            role: None,
            mapped: None,
            required: None,
        }
    }

    fn surface(component: &str, output: &str, role: &str, mapped: bool) -> ReadyLine {
        ReadyLine {
            output: Some(output.into()),
            role: Some(role.into()),
            mapped: Some(mapped),
            ..line(Some(component), "surface")
        }
    }

    fn layer(role: Role, connector: &str) -> LayerSurface {
        LayerSurface { namespace: role.namespace().into(), connector: connector.into(), mapped: true }
    }

    fn all_layers(connector: &str) -> Vec<LayerSurface> {
        Role::ALL.iter().map(|r| layer(*r, connector)).collect()
    }

    fn configured_session() -> Readiness {
        let mut r = Readiness::new("7");
        r.output_added("HDMI-A-1");
        assert_eq!(r.set_gnoblin_ready(true), vec![Report::GnoblinReady]);
        let reports = r.output_configured("HDMI-A-1", "Meta-0");
        assert_eq!(reports, vec![Report::OutputConfigured { output: "HDMI-A-1".into(), a: 1, b: 1 }]);
        r
    }

    fn drive_to_ready(r: &mut Readiness) -> Vec<Report> {
        let mut all = Vec::new();
        all.extend(r.ready_line(&line(Some("wallpaper"), "launched")));
        all.extend(r.ready_line(&line(Some("desktop"), "launched")));
        for (c, role) in [("wallpaper", "wallpaper"), ("desktop", "bar"), ("desktop", "dock")] {
            all.extend(r.ready_line(&surface(c, "Meta-0", role, true)));
        }
        all.extend(r.set_layers(all_layers("Meta-0")));
        all.extend(r.ready_line(&line(Some("wallpaper"), "ready")));
        all.extend(r.ready_line(&line(Some("desktop"), "ready")));
        all
    }

    #[test]
    fn happy_path_orders_reports() {
        let mut r = configured_session();
        let reports = drive_to_ready(&mut r);
        assert!(reports.contains(&Report::BinguxLaunched));
        assert_eq!(reports.last(), Some(&Report::DesktopReady { a: 1, b: 1 }));
        assert!(reports.contains(&Report::SurfacesMapped { output: "HDMI-A-1".into(), a: 3, b: 3 }));
        let launched = reports.iter().position(|r| *r == Report::BinguxLaunched).unwrap();
        let mapped = reports
            .iter()
            .position(|r| matches!(r, Report::SurfacesMapped { a: 3, b: 3, .. }))
            .unwrap();
        assert!(launched < mapped);
    }

    #[test]
    fn wrong_generation_changes_nothing() {
        let mut r = configured_session();
        let mut stale = line(Some("wallpaper"), "launched");
        stale.generation = "6".into();
        assert!(r.ready_line(&stale).is_empty());
        assert_eq!(r.rejected(), 1);
        let reports = drive_to_ready(&mut r);
        assert_eq!(reports.last(), Some(&Report::DesktopReady { a: 1, b: 1 }));
    }

    #[test]
    fn bingux_claim_without_gnoblin_layer_is_not_mapped() {
        let mut r = configured_session();
        r.ready_line(&line(Some("wallpaper"), "launched"));
        r.ready_line(&line(Some("desktop"), "launched"));
        for (c, role) in [("wallpaper", "wallpaper"), ("desktop", "bar"), ("desktop", "dock")] {
            r.ready_line(&surface(c, "Meta-0", role, true));
        }
        r.ready_line(&line(Some("wallpaper"), "ready"));
        let reports = r.ready_line(&line(Some("desktop"), "ready"));
        assert!(!reports.contains(&Report::DesktopReady { a: 1, b: 1 }), "{reports:?}");
        // Gnoblin sees only two of the three surfaces.
        let reports = r.set_layers(vec![layer(Role::Wallpaper, "Meta-0"), layer(Role::Bar, "Meta-0")]);
        assert!(!reports.contains(&Report::DesktopReady { a: 1, b: 1 }));
        let reports = r.set_layers(all_layers("Meta-0"));
        assert_eq!(reports.last(), Some(&Report::DesktopReady { a: 1, b: 1 }));
    }

    #[test]
    fn unmapped_gnoblin_layer_does_not_count() {
        let mut r = configured_session();
        drive_to_ready(&mut r);
        let mut r = configured_session();
        r.ready_line(&line(None, "launched"));
        r.ready_line(&surface("all", "Meta-0", "wallpaper", true));
        let mut layers = all_layers("Meta-0");
        layers[0].mapped = false;
        let reports = r.set_layers(layers);
        assert!(reports.iter().all(|x| !matches!(x, Report::SurfacesMapped { a: 1, .. })));
    }

    #[test]
    fn layer_on_other_connector_does_not_count() {
        let mut r = configured_session();
        r.ready_line(&line(None, "launched"));
        for role in ["wallpaper", "bar", "dock"] {
            r.ready_line(&surface("all", "Meta-0", role, true));
        }
        let reports = r.set_layers(all_layers("Meta-1"));
        assert!(reports.iter().all(|x| !matches!(x, Report::SurfacesMapped { a: 1.., .. })), "{reports:?}");
        r.ready_line(&line(None, "ready"));
        assert!(!r.set_layers(all_layers("Meta-1")).contains(&Report::DesktopReady { a: 1, b: 1 }));
    }

    #[test]
    fn ready_before_surfaces_waits_for_them() {
        let mut r = configured_session();
        r.ready_line(&line(Some("wallpaper"), "launched"));
        r.ready_line(&line(Some("desktop"), "launched"));
        r.ready_line(&line(Some("wallpaper"), "ready"));
        r.ready_line(&line(Some("desktop"), "ready"));
        for (c, role) in [("wallpaper", "wallpaper"), ("desktop", "bar")] {
            r.ready_line(&surface(c, "Meta-0", role, true));
        }
        assert!(!r.set_layers(all_layers("Meta-0")).contains(&Report::DesktopReady { a: 1, b: 1 }));
        let reports = r.ready_line(&surface("desktop", "Meta-0", "dock", true));
        assert_eq!(reports.last(), Some(&Report::DesktopReady { a: 1, b: 1 }));
    }

    #[test]
    fn one_component_missing_blocks_launch() {
        let mut r = configured_session();
        let reports = r.ready_line(&line(Some("wallpaper"), "launched"));
        assert!(!reports.contains(&Report::BinguxLaunched));
        let reports = r.ready_line(&line(Some("desktop"), "launched"));
        assert!(reports.contains(&Report::BinguxLaunched));
    }

    #[test]
    fn desktop_ready_is_sent_once() {
        let mut r = configured_session();
        let first = drive_to_ready(&mut r);
        assert_eq!(first.iter().filter(|x| matches!(x, Report::DesktopReady { .. })).count(), 1);
        assert!(!r.ready_line(&line(Some("desktop"), "ready")).iter().any(|x| matches!(x, Report::DesktopReady { .. })));
    }

    #[test]
    fn gnoblin_ready_required_before_output_reports() {
        let mut r = Readiness::new("7");
        r.output_added("DP-1");
        assert!(r.output_configured("DP-1", "Meta-0").is_empty());
        let reports = r.set_gnoblin_ready(true);
        assert_eq!(
            reports,
            vec![
                Report::GnoblinReady,
                Report::OutputConfigured { output: "DP-1".into(), a: 1, b: 1 }
            ]
        );
    }

    #[test]
    fn two_outputs_count_and_gate() {
        let mut r = Readiness::new("7");
        r.output_added("A");
        r.output_added("B");
        r.set_gnoblin_ready(true);
        let first = r.output_configured("A", "Meta-0");
        assert_eq!(first, vec![Report::OutputConfigured { output: "A".into(), a: 1, b: 2 }]);
        let second = r.output_configured("B", "Meta-1");
        assert_eq!(second, vec![Report::OutputConfigured { output: "B".into(), a: 2, b: 2 }]);
        r.ready_line(&line(None, "launched"));
        for conn in ["Meta-0", "Meta-1"] {
            for role in ["wallpaper", "bar", "dock"] {
                r.ready_line(&surface("all", conn, role, true));
            }
        }
        r.ready_line(&line(None, "ready"));
        let mut layers = all_layers("Meta-0");
        let reports = r.set_layers(layers.clone());
        assert!(!reports.contains(&Report::DesktopReady { a: 2, b: 2 }));
        layers.extend(all_layers("Meta-1"));
        let reports = r.set_layers(layers);
        assert_eq!(reports.last(), Some(&Report::DesktopReady { a: 2, b: 2 }));
    }

    #[test]
    fn unmapping_a_surface_invalidates_ready_before_report() {
        let mut r = configured_session();
        r.ready_line(&line(Some("wallpaper"), "launched"));
        r.ready_line(&line(Some("desktop"), "launched"));
        r.ready_line(&line(Some("wallpaper"), "ready"));
        r.ready_line(&line(Some("desktop"), "ready"));
        r.ready_line(&surface("desktop", "Meta-0", "bar", true));
        r.ready_line(&surface("desktop", "Meta-0", "bar", false));
        r.ready_line(&surface("wallpaper", "Meta-0", "wallpaper", true));
        r.ready_line(&surface("desktop", "Meta-0", "dock", true));
        // desktop's ready was invalidated by the unmap, so no report yet
        let reports = r.set_layers(all_layers("Meta-0"));
        assert!(!reports.contains(&Report::DesktopReady { a: 1, b: 1 }), "{reports:?}");
    }

    #[test]
    fn output_removed_stops_blocking() {
        let mut r = Readiness::new("7");
        r.output_added("A");
        r.output_added("B");
        r.set_gnoblin_ready(true);
        r.output_configured("A", "Meta-0");
        r.ready_line(&line(None, "launched"));
        for role in ["wallpaper", "bar", "dock"] {
            r.ready_line(&surface("all", "Meta-0", role, true));
        }
        r.ready_line(&line(None, "ready"));
        r.set_layers(all_layers("Meta-0"));
        let reports = r.output_removed("B");
        assert!(reports.contains(&Report::DesktopReady { a: 1, b: 1 }), "{reports:?}");
    }

    #[test]
    fn malformed_surface_line_is_rejected() {
        let mut r = configured_session();
        let mut bad = line(Some("desktop"), "surface");
        bad.role = Some("popup".into());
        assert!(r.ready_line(&bad).is_empty());
        assert_eq!(r.rejected(), 1);
        assert!(r.ready_line(&line(Some("desktop"), "explode")).is_empty());
        assert_eq!(r.rejected(), 2);
    }

    #[test]
    fn json_line_parses() {
        let l: ReadyLine = serde_json::from_str(
            r#"{"generation":"7","component":"desktop","event":"surface","output":"Meta-0","role":"bar","mapped":true}"#,
        )
        .unwrap();
        assert_eq!(l.role.as_deref(), Some("bar"));
        assert!(serde_json::from_str::<ReadyLine>(r#"{"event":"ready"}"#).is_err());
    }
}
