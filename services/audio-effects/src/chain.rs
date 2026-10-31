//! The stack as the engine runs it: which stages exist, how they are linked
//! and what changed between two presets.
//!
//! Everything here is a pure function of data, so the rules (what restarts a
//! stage, what is only a control change, what is only a new set of links) are
//! tested without a sound server. `service` carries the results out.

use std::collections::BTreeMap;

use crate::catalog::Catalog;
use crate::effects::{self, Source};
use crate::graph::{self, Role};
use crate::preset::{Direction, PluginSlot, Preset};

/// What a running stage was started with, and has been set to since.
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub id: String,
    pub kind: String,
    pub controls: BTreeMap<String, f64>,
    pub path: Option<String>,
}

impl Running {
    pub fn of(slot: &PluginSlot) -> Self {
        Self {
            id: slot.id.clone(),
            kind: slot.kind.clone(),
            controls: slot.controls.clone(),
            path: slot.path.clone(),
        }
    }
}

/// One control to set in a running stage.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamChange {
    pub id: String,
    pub symbol: String,
    pub value: f64,
}

/// What to do to get from the running stages to a preset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Stages to stop: effects taken out, or whose kind or file changed.
    pub stop: Vec<String>,
    /// Stages to start, in stack order.
    pub start: Vec<String>,
    /// Controls to set in stages that keep running.
    pub params: Vec<ParamChange>,
    /// The effects that sit in the audio path, in order: the ones that are on.
    pub order: Vec<String>,
}

/// Whether a stage must be started afresh to take `new`: a different effect, or
/// a different impulse response file, which is not a control.
fn needs_restart(old: &Running, new: &PluginSlot) -> bool {
    old.kind != new.kind || old.path != new.path
}

/// The plan for taking the stages in `running` to `preset`. An effect the
/// system cannot run (`catalog` has no plugin for it) is left out.
pub fn plan(running: &[Running], preset: &Preset, catalog: &Catalog) -> Plan {
    let mut plan = Plan::default();
    for old in running {
        match preset.slot(&old.id) {
            Some(slot) if !needs_restart(old, slot) => {}
            _ => plan.stop.push(old.id.clone()),
        }
    }
    for slot in &preset.plugins {
        let known =
            effects::find(&slot.kind).is_some_and(|effect| catalog.plugin_of(effect).is_some());
        if !known {
            continue;
        }
        let kept = running
            .iter()
            .find(|old| old.id == slot.id)
            .filter(|old| !needs_restart(old, slot));
        match kept {
            None => plan.start.push(slot.id.clone()),
            Some(old) => plan.params.extend(changes(old, slot, catalog)),
        }
        if slot.enabled {
            plan.order.push(slot.id.clone());
        }
    }
    plan
}

/// The controls of `slot` that differ from what `old` has.
fn changes(old: &Running, slot: &PluginSlot, catalog: &Catalog) -> Vec<ParamChange> {
    let plugin = effects::find(&slot.kind).and_then(|effect| catalog.plugin_of(effect));
    let mut found = Vec::new();
    for (symbol, value) in &slot.controls {
        if old.controls.get(symbol) != Some(value)
            && plugin.is_none_or(|plugin| plugin.control(symbol).is_some())
        {
            found.push(ParamChange {
                id: slot.id.clone(),
                symbol: symbol.clone(),
                value: *value,
            });
        }
    }
    // A control no longer in the preset goes back to the plugin's default.
    for symbol in old
        .controls
        .keys()
        .filter(|symbol| !slot.controls.contains_key(*symbol))
    {
        if let Some(control) = plugin.and_then(|plugin| plugin.control(symbol)) {
            found.push(ParamChange {
                id: slot.id.clone(),
                symbol: symbol.clone(),
                value: control.default,
            });
        }
    }
    found
}

/// A link between two ports, as `node:port` names.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Link {
    pub output: String,
    pub input: String,
}

/// The links of a chain with `order` between the head and the tail, left and
/// right channel each.
pub fn links(direction: Direction, order: &[String]) -> Vec<Link> {
    let mut roles = vec![Role::Head];
    roles.extend(order.iter().cloned().map(Role::Effect));
    roles.push(Role::Tail);
    let mut links = Vec::new();
    for pair in roles.windows(2) {
        let from = graph::output_ports(direction, &pair[0]);
        let to = graph::input_ports(direction, &pair[1]);
        for channel in 0..2 {
            links.push(Link {
                output: from[channel].clone(),
                input: to[channel].clone(),
            });
        }
    }
    links
}

/// The links to remove and to make to get from `current` to `wanted`.
pub fn diff_links(current: &[Link], wanted: &[Link]) -> (Vec<Link>, Vec<Link>) {
    let remove = current
        .iter()
        .filter(|link| !wanted.contains(link))
        .cloned()
        .collect();
    let add = wanted
        .iter()
        .filter(|link| !current.contains(link))
        .cloned()
        .collect();
    (remove, add)
}

/// Whether an effect's stage needs a file the preset names: the convolver.
pub fn needs_file(kind: &str) -> bool {
    effects::find(kind).is_some_and(|effect| matches!(effect.source, Source::Convolver))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AudioPort, Backend, Control, PluginInfo, Unit};

    fn control(symbol: &str, default: f64) -> Control {
        Control {
            symbol: symbol.into(),
            name: symbol.into(),
            min: 0.0,
            max: 100.0,
            default,
            log: false,
            integer: false,
            toggle: false,
            choices: Vec::new(),
            unit: Unit::None,
            hidden: false,
        }
    }

    fn catalog() -> Catalog {
        let mut catalog = Catalog::default();
        for key in ["compressor", "limiter", "gate", "equalizer"] {
            let effect = effects::find(key).unwrap();
            catalog.plugins.push(PluginInfo {
                id: effect.source.plugin_id(),
                name: key.into(),
                maker: String::new(),
                backend: Backend::Lv2,
                library: None,
                label: String::new(),
                inputs: vec![AudioPort {
                    symbol: "in_l".into(),
                    name: "l".into(),
                    optional: false,
                    side_chain: false,
                }],
                outputs: Vec::new(),
                controls: vec![control("a", 5.0), control("b", 7.0)],
                meters: Vec::new(),
                latency: None,
            });
        }
        catalog
    }

    fn preset(ids: &[(&str, &str, bool)]) -> Preset {
        let mut preset = Preset::new("t", Direction::Output);
        for (id, kind, enabled) in ids {
            let mut slot = PluginSlot::new(*id, *kind);
            slot.enabled = *enabled;
            preset.plugins.push(slot);
        }
        preset
    }

    fn running(preset: &Preset) -> Vec<Running> {
        preset.plugins.iter().map(Running::of).collect()
    }

    #[test]
    fn the_first_plan_starts_every_effect_in_order() {
        let preset = preset(&[
            ("compressor-1", "compressor", true),
            ("limiter-1", "limiter", true),
        ]);
        let plan = plan(&[], &preset, &catalog());
        assert_eq!(plan.start, ["compressor-1", "limiter-1"]);
        assert_eq!(plan.order, ["compressor-1", "limiter-1"]);
        assert!(plan.stop.is_empty() && plan.params.is_empty());
    }

    #[test]
    fn reordering_changes_only_the_links() {
        let before = preset(&[("a-1", "compressor", true), ("b-1", "limiter", true)]);
        let mut after = before.clone();
        after.move_slot(0, 1);
        let plan = plan(&running(&before), &after, &catalog());
        assert!(
            plan.start.is_empty() && plan.stop.is_empty() && plan.params.is_empty(),
            "no stage restarts"
        );
        assert_eq!(plan.order, ["b-1", "a-1"]);
        let (remove, add) = diff_links(
            &links(Direction::Output, &["a-1".into(), "b-1".into()]),
            &links(Direction::Output, &plan.order),
        );
        assert_eq!(
            remove.len(),
            6,
            "head to a, a to b, b to tail, both channels"
        );
        assert_eq!(add.len(), 6);
    }

    #[test]
    fn switching_an_effect_off_keeps_its_stage_and_drops_its_links() {
        let before = preset(&[("a-1", "compressor", true), ("b-1", "limiter", true)]);
        let mut after = before.clone();
        after.slot_mut("a-1").unwrap().enabled = false;
        let plan = plan(&running(&before), &after, &catalog());
        assert!(
            plan.stop.is_empty(),
            "an effect that is off keeps its state"
        );
        assert_eq!(plan.order, ["b-1"]);
        let (remove, add) = diff_links(
            &links(Direction::Output, &["a-1".into(), "b-1".into()]),
            &links(Direction::Output, &plan.order),
        );
        assert_eq!(
            (remove.len(), add.len()),
            (4, 2),
            "head to b is new; head to a, a to b and b to tail... only a's links go"
        );
    }

    #[test]
    fn adding_and_removing_start_and_stop_one_stage() {
        let before = preset(&[("a-1", "compressor", true), ("b-1", "limiter", true)]);
        let mut after = before.clone();
        after.remove("a-1");
        after.plugins.push(PluginSlot::new("g-1", "gate"));
        let plan = plan(&running(&before), &after, &catalog());
        assert_eq!(plan.stop, ["a-1"]);
        assert_eq!(plan.start, ["g-1"]);
        assert_eq!(plan.order, ["b-1", "g-1"]);
    }

    #[test]
    fn a_control_change_is_set_live_and_a_kind_change_restarts() {
        let before = preset(&[("a-1", "compressor", true)]);
        let mut after = before.clone();
        after
            .slot_mut("a-1")
            .unwrap()
            .controls
            .insert("a".into(), 12.0);
        after
            .slot_mut("a-1")
            .unwrap()
            .controls
            .insert("nope".into(), 1.0);
        let plan1 = plan(&running(&before), &after, &catalog());
        assert_eq!(
            plan1.params,
            [ParamChange {
                id: "a-1".into(),
                symbol: "a".into(),
                value: 12.0
            }],
            "an unknown control is ignored"
        );
        assert!(plan1.start.is_empty() && plan1.stop.is_empty());
        let mut other = before.clone();
        other.slot_mut("a-1").unwrap().kind = "limiter".into();
        let plan2 = plan(&running(&before), &other, &catalog());
        assert_eq!(
            (plan2.stop.as_slice(), plan2.start.as_slice()),
            (
                ["a-1".to_string()].as_slice(),
                ["a-1".to_string()].as_slice()
            )
        );
    }

    #[test]
    fn a_control_that_leaves_the_preset_goes_back_to_the_default() {
        let mut before = preset(&[("a-1", "compressor", true)]);
        before
            .slot_mut("a-1")
            .unwrap()
            .controls
            .insert("b".into(), 50.0);
        let after = preset(&[("a-1", "compressor", true)]);
        let plan = plan(&running(&before), &after, &catalog());
        assert_eq!(
            plan.params,
            [ParamChange {
                id: "a-1".into(),
                symbol: "b".into(),
                value: 7.0
            }]
        );
    }

    #[test]
    fn an_effect_with_no_plugin_is_left_out() {
        let preset = preset(&[("a-1", "compressor", true), ("r-1", "reverb", true)]);
        let plan = plan(&[], &preset, &catalog());
        assert_eq!(plan.start, ["a-1"], "the reverb is not in the catalog");
        assert_eq!(plan.order, ["a-1"]);
    }

    #[test]
    fn an_empty_stack_links_the_head_to_the_tail() {
        let links = links(Direction::Output, &[]);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].output, "bingux_effects_output.out:output_FL");
        assert_eq!(links[0].input, "bingux_fx.output.tail:input_FL");
    }
}
