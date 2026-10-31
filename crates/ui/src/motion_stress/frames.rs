//! Invariants over a run of frames: what must stay put, what may not change its elision, what may
//! not leave the window.

use std::collections::HashMap;

use torin::prelude::Area;

use super::Frame;
use crate::inspector::snapshot::Node;

impl Frame {
    /// Each node by a path that is the same in every frame while the structure is: the titles of
    /// its ancestors and its own, each with its ordinal among same-titled siblings.
    pub(crate) fn paths(&self) -> HashMap<String, &Node> {
        let mut keys: HashMap<u64, String> = HashMap::new();
        let mut out: HashMap<String, &Node> = HashMap::new();
        let mut seen: HashMap<(Option<u64>, String), usize> = HashMap::new();
        for node in self.nodes() {
            let title = node.title().to_owned();
            let ordinal = {
                let count = seen.entry((node.parent, title.clone())).or_insert(0);
                let ordinal = *count;
                *count += 1;
                ordinal
            };
            let parent = node
                .parent
                .and_then(|parent| keys.get(&parent).cloned())
                .unwrap_or_default();
            let key = format!("{parent}/{title}#{ordinal}");
            keys.insert(node.id, key.clone());
            out.insert(key, node);
        }
        out
    }
}

/// The border box of a node as four numbers.
fn box_of(node: &Node) -> Option<[f32; 4]> {
    let area: Area = node.area?;
    Some([area.min_x(), area.min_y(), area.width(), area.height()])
}

fn differs(a: [f32; 4], b: [f32; 4], eps: f32) -> bool {
    a.iter().zip(&b).any(|(a, b)| (a - b).abs() > eps)
}

/// The nodes that are not `allowed` and are not where they were in `frames[0]`, in any frame
/// after it. `allowed` is given a node's path. A node that is not in a frame is not compared in it.
pub(crate) fn unmoved(frames: &[Frame], eps: f32, allowed: impl Fn(&str) -> bool) -> Vec<String> {
    let Some(first) = frames.first() else {
        return Vec::new();
    };
    let before = first.paths();
    let mut out = Vec::new();
    let mut reported: Vec<String> = Vec::new();
    for (at, frame) in frames.iter().enumerate().skip(1) {
        let now = frame.paths();
        let mut keys: Vec<&String> = before.keys().collect();
        keys.sort();
        for key in keys {
            if allowed(key) || reported.contains(key) {
                continue;
            }
            let (Some(a), Some(b)) = (
                box_of(before[key]),
                now.get(key).and_then(|node| box_of(node)),
            ) else {
                continue;
            };
            if differs(a, b, eps) {
                reported.push(key.clone());
                out.push(format!("{key} moved at frame {at}: {a:.2?} -> {b:.2?}"));
            }
        }
    }
    out
}

/// A text whose elision state changed during the run: it was cut by an ellipsis in one frame and
/// whole in another, though it kept its string. Texts are paired by string and ordinal. `flips`
/// is how many changes a text may make (0 for a motion that does not resize, 1 for a resize that
/// crosses a threshold once).
pub(crate) fn elision_flips(frames: &[Frame], flips: usize) -> Vec<String> {
    let mut states: HashMap<(String, usize), Vec<bool>> = HashMap::new();
    for frame in frames {
        let mut ordinals: HashMap<String, usize> = HashMap::new();
        for look in frame.texts() {
            let ordinal = {
                let count = ordinals.entry(look.text.clone()).or_insert(0);
                let ordinal = *count;
                *count += 1;
                ordinal
            };
            states
                .entry((look.text, ordinal))
                .or_default()
                .push(look.elided);
        }
    }
    let mut out = Vec::new();
    let mut keys: Vec<_> = states.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let series = &states[&key];
        let changes = series.windows(2).filter(|pair| pair[0] != pair[1]).count();
        if changes > flips {
            out.push(format!(
                "the text {:?} changed its elision {changes} times: {}",
                key.0,
                series
                    .iter()
                    .map(|elided| if *elided { 'E' } else { '.' })
                    .collect::<String>()
            ));
        }
    }
    out
}

/// The nodes of `names` that are not wholly inside the window in some frame.
pub(crate) fn inside_window(frames: &[Frame], names: &[&str], slack: f32) -> Vec<String> {
    let mut out = Vec::new();
    for (at, frame) in frames.iter().enumerate() {
        for name in names {
            for node in frame.all(name) {
                let Some(area) = node.area else { continue };
                let (w, h) = frame.window;
                if area.min_x() < -slack
                    || area.min_y() < -slack
                    || area.max_x() > w + slack
                    || area.max_y() > h + slack
                {
                    out.push(format!(
                        "{name} left the window at frame {at}: {area:?} in {w}x{h}"
                    ));
                }
            }
        }
    }
    out
}

/// What differs between two frames' signatures, one line per difference, at most `limit`.
pub(crate) fn signature_diff(a: &Frame, b: &Frame, limit: usize) -> Vec<String> {
    let (sa, sb) = (a.signature(), b.signature());
    let mut out = Vec::new();
    if sa.len() != sb.len() {
        out.push(format!("{} nodes against {}", sa.len(), sb.len()));
    }
    for (at, (x, y)) in sa.iter().zip(&sb).enumerate() {
        if x != y {
            out.push(format!("node {at}: {x}  !=  {y}"));
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}
