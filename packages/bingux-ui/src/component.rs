//! Component names and published props for the inspector.
//!
//! Freya keeps no record of which component produced an element, so the
//! inspector would show every container as `rect`. A component names its root
//! element with [`ComponentName::component`]; the inspector reads the name
//! back from the element's accessibility node (`class_name`) and shows it in
//! the Elements tree and the breadcrumb. Any application component can call it,
//! not only the ones in this crate.
//!
//! ```ignore
//! rect().component("SidebarRow").child(label)
//! ```
//!
//! A component can also publish the call that built it, so the inspector shows
//! `Row::new("Airplane mode").subtitle("Off")` with each value as a field of
//! the right type instead of a pile of layout numbers:
//!
//! ```ignore
//! rect()
//!     .component("Row")
//!     .published(|| {
//!         Published::call("Row::new")
//!             .arg(Prop::text("title", title))
//!             .method(Prop::text("subtitle", subtitle))
//!     })
//!     .child(label)
//! ```
//!
//! Neither changes layout or style. The name is stored as the element's
//! accessibility class name and the published call as its author id, which
//! the inspector reads back.
//!
//! A published value can be changed live from the inspector when the component
//! opts in with [`prop_override`]: it asks for the inspector's value of a prop
//! before using the one it was built with.

use std::cell::RefCell;
use std::collections::HashMap;

use freya::prelude::*;
use freya_core::accessibility::id::AccessibilityId;

/// Adds [`component`](ComponentName::component) to every element builder that
/// carries accessibility data.
pub trait ComponentName: AccessibilityExt + Sized {
    /// Records `name` as the component that built this element.
    fn component(self, name: &'static str) -> Self {
        self.a11y_builder(move |builder| builder.set_class_name(name))
    }

    /// Publishes the call that built this element. `build` is the call, written
    /// where the component already holds its values; the inspector reads it back.
    fn published(self, build: impl FnOnce() -> Published) -> Self {
        let call = build().encode();
        self.a11y_builder(move |builder| builder.set_author_id(call))
    }
}

impl<T: AccessibilityExt> ComponentName for T {}

/// How a published prop is edited, and what it may hold.
#[derive(Clone, Debug, PartialEq)]
pub enum PropKind {
    /// Free text.
    Text,
    /// On or off.
    Flag,
    /// A number between `min` and `max`, stepped by `step`.
    Number { min: f64, max: f64, step: f64 },
    /// One of a fixed set of values, written as they appear in source.
    Choice(Vec<String>),
}

/// One value a component was built with: its name, its current value as text
/// and the kind of value it is, which tells the inspector what editor to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub name: String,
    pub value: String,
    pub kind: PropKind,
    /// Whether the component reads [`prop_override`] for this prop, so the
    /// inspector can change it live.
    pub live: bool,
}

impl Prop {
    pub fn text(name: &str, value: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            value: value.into(),
            kind: PropKind::Text,
            live: false,
        }
    }

    pub fn flag(name: &str, value: bool) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            kind: PropKind::Flag,
            live: false,
        }
    }

    pub fn number(name: &str, value: f64, min: f64, max: f64, step: f64) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            kind: PropKind::Number { min, max, step },
            live: false,
        }
    }

    /// A value from a closed set; `options` are the values as written in source
    /// (`Align::Center`), and `value` is one of them.
    pub fn choice(name: &str, value: impl Into<String>, options: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            value: value.into(),
            kind: PropKind::Choice(options.iter().map(|option| option.to_string()).collect()),
            live: false,
        }
    }

    /// Marks the prop as one the component reads [`prop_override`] for, so the
    /// inspector can change it live. Text props need no mark when a label shows
    /// the text: the inspector edits the label.
    pub fn live(mut self) -> Self {
        self.live = true;
        self
    }

    /// The value as Rust source.
    pub fn source(&self) -> String {
        match self.kind {
            PropKind::Text => format!("{:?}", self.value),
            PropKind::Number { .. } => self.value.clone(),
            PropKind::Flag | PropKind::Choice(_) => self.value.clone(),
        }
    }
}

/// The call that built a component: the constructor with its arguments, then
/// the builder methods that were set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Published {
    /// The constructor path, such as `Row::new` or `switch`.
    pub call: String,
    /// The constructor's arguments, in order.
    pub args: Vec<Prop>,
    /// The builder methods called after it; `name` is the method.
    pub methods: Vec<Prop>,
}

impl Published {
    pub fn call(path: &str) -> Self {
        Self {
            call: path.to_string(),
            ..Self::default()
        }
    }

    pub fn arg(mut self, prop: Prop) -> Self {
        self.args.push(prop);
        self
    }

    pub fn method(mut self, prop: Prop) -> Self {
        self.methods.push(prop);
        self
    }

    pub fn maybe_method(self, prop: Option<Prop>) -> Self {
        match prop {
            Some(prop) => self.method(prop),
            None => self,
        }
    }

    /// Every prop, constructor arguments first.
    pub fn props(&self) -> impl Iterator<Item = &Prop> {
        self.args.iter().chain(&self.methods)
    }

    /// The call as Rust source: `Row::new("Wi-Fi").subtitle("On")`.
    pub fn source(&self) -> String {
        let args: Vec<String> = self.args.iter().map(Prop::source).collect();
        let mut out = format!("{}({})", self.call, args.join(", "));
        for method in &self.methods {
            out.push_str(&format!(".{}({})", method.name, method.source()));
        }
        out
    }

    // The wire form: records split by RS, fields by US, choice options by GS.
    // `a` or `m` marks an argument or a method.
    fn encode(&self) -> String {
        let mut out = format!("bingux.call{RS}{}", self.call);
        for (role, prop) in self
            .args
            .iter()
            .map(|prop| ('a', prop))
            .chain(self.methods.iter().map(|prop| ('m', prop)))
        {
            let kind = match &prop.kind {
                PropKind::Text => "t".to_string(),
                PropKind::Flag => "f".to_string(),
                PropKind::Number { min, max, step } => format!("n:{min}:{max}:{step}"),
                PropKind::Choice(options) => format!("c:{}", options.join(&GS.to_string())),
            };
            out.push_str(&format!(
                "{RS}{role}{US}{}{US}{}{US}{kind}{US}{}",
                prop.name,
                prop.value,
                if prop.live { "1" } else { "0" }
            ));
        }
        out
    }

    /// Reads a call back from an element's author id; `None` for any other id.
    pub fn decode(id: &str) -> Option<Self> {
        let mut records = id.split(RS);
        if records.next()? != "bingux.call" {
            return None;
        }
        let mut call = Published::call(records.next()?);
        for record in records {
            let mut fields = record.split(US);
            let role = fields.next()?;
            let name = fields.next()?.to_string();
            let value = fields.next()?.to_string();
            let kind = fields.next()?;
            let kind = match kind.split_once(':') {
                None if kind == "f" => PropKind::Flag,
                None => PropKind::Text,
                Some(("n", rest)) => {
                    let mut parts = rest
                        .split(':')
                        .map(|part| part.parse::<f64>().unwrap_or(0.0));
                    PropKind::Number {
                        min: parts.next().unwrap_or(0.0),
                        max: parts.next().unwrap_or(0.0),
                        step: parts.next().unwrap_or(1.0),
                    }
                }
                Some(("c", options)) => PropKind::Choice(
                    options
                        .split(GS)
                        .filter(|option| !option.is_empty())
                        .map(str::to_string)
                        .collect(),
                ),
                Some(_) => PropKind::Text,
            };
            let live = fields.next() == Some("1");
            let prop = Prop {
                name,
                value,
                kind,
                live,
            };
            if role == "a" {
                call.args.push(prop);
            } else {
                call.methods.push(prop);
            }
        }
        Some(call)
    }
}

const RS: char = '\u{1e}';
const US: char = '\u{1f}';
const GS: char = '\u{1d}';

thread_local! {
    /// The values the inspector set live, by component and prop name.
    static OVERRIDES: RefCell<HashMap<(u64, String), String>> = RefCell::new(HashMap::new());
    /// Bumped whenever an override changes, so a component that asked for one
    /// renders again.
    static EPOCH: State<u64> = State::create_global(0);
}

/// The value the inspector set for prop `name` of the component with
/// accessibility id `id`, if any. Reading it subscribes the component, so it
/// renders again when the inspector changes or clears the value. A component
/// that supports live editing calls it for each prop it opts in.
pub fn prop_override(id: AccessibilityId, name: &str) -> Option<String> {
    EPOCH.with(|epoch| {
        let _ = *epoch.read();
    });
    OVERRIDES.with(|map| map.borrow().get(&(id.0, name.to_string())).cloned())
}

/// Sets prop `name` of the component with accessibility id `id` to `value`.
/// The inspector calls this; the component picks it up with [`prop_override`].
pub fn set_prop_override(id: AccessibilityId, name: &str, value: String) {
    OVERRIDES.with(|map| map.borrow_mut().insert((id.0, name.to_string()), value));
    bump();
}

/// Clears one prop's override, or every override of the component when `name`
/// is `None`.
pub fn clear_prop_override(id: AccessibilityId, name: Option<&str>) {
    OVERRIDES.with(|map| {
        map.borrow_mut()
            .retain(|(key, prop), _| !(*key == id.0 && name.is_none_or(|name| name == prop)));
    });
    bump();
}

/// The override for `name`, read without subscribing: the inspector's own read.
pub fn peek_prop_override(id: AccessibilityId, name: &str) -> Option<String> {
    OVERRIDES.with(|map| map.borrow().get(&(id.0, name.to_string())).cloned())
}

fn bump() {
    EPOCH.with(|epoch| {
        let mut epoch = *epoch;
        let next = *epoch.peek() + 1;
        epoch.set(next);
    });
}
