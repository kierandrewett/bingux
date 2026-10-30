//! Documents: an id and JSON fields, like a Meilisearch document.

use serde::{Deserialize, Serialize};
pub use serde_json::Value;

pub type Map = serde_json::Map<String, Value>;

/// One searchable record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Unique within the index. Upserting an existing id replaces it.
    pub id: String,
    pub fields: Map,
    /// Per-document override of the index's exposure to other processes. It
    /// can hide a document of an external index, never expose one of a private
    /// index. `None` follows the index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
}

impl Document {
    pub fn new(id: impl Into<String>) -> Self {
        Document {
            id: id.into(),
            fields: Map::new(),
            external: None,
        }
    }

    /// Sets any field.
    pub fn field(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.fields.insert(name.to_string(), value.into());
        self
    }

    pub fn title(self, title: &str) -> Self {
        self.field("title", title)
    }

    /// The path of the thing, outermost first: `["Network", "Proxy"]`.
    pub fn breadcrumb<I, S>(self, path: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let list: Vec<Value> = path.into_iter().map(|s| Value::String(s.into())).collect();
        self.field("breadcrumb", Value::Array(list))
    }

    /// Extra words a person might type, space separated.
    pub fn keywords(self, words: &str) -> Self {
        self.field("keywords", words)
    }

    pub fn body(self, text: &str) -> Self {
        self.field("body", text)
    }

    pub fn category(self, category: &str) -> Self {
        self.field("category", category)
    }

    pub fn icon(self, icon: &str) -> Self {
        self.field("icon", icon)
    }

    /// Where choosing the hit goes, in the app's own address scheme, such as
    /// `bingux-settings://network/proxy#method`.
    pub fn uri(self, uri: &str) -> Self {
        self.field("uri", uri)
    }

    pub fn external(mut self, external: bool) -> Self {
        self.external = Some(external);
        self
    }
}

/// Calls `f` for every scalar or leaf value at the dotted `path` of `fields`.
/// Arrays are flattened, objects are walked by path segments.
pub fn for_each_value<'a>(fields: &'a Map, path: &str, f: &mut impl FnMut(&'a Value)) {
    let mut segments = path.split('.');
    let Some(first) = segments.next() else { return };
    let Some(value) = fields.get(first) else { return };
    let rest: Vec<&str> = segments.collect();
    walk(value, &rest, f);
}

fn walk<'a>(value: &'a Value, rest: &[&str], f: &mut impl FnMut(&'a Value)) {
    match value {
        Value::Array(items) => {
            for item in items {
                walk(item, rest, f);
            }
        }
        Value::Object(map) => {
            if let Some((head, tail)) = rest.split_first()
                && let Some(next) = map.get(*head)
            {
                walk(next, tail, f);
            }
        }
        leaf => {
            if rest.is_empty() {
                f(leaf);
            }
        }
    }
}

/// The text of a leaf: strings as they are, numbers and booleans as written.
pub fn leaf_text(value: &Value) -> Option<std::borrow::Cow<'_, str>> {
    match value {
        Value::String(s) => Some(std::borrow::Cow::Borrowed(s)),
        Value::Number(n) => Some(std::borrow::Cow::Owned(n.to_string())),
        Value::Bool(b) => Some(std::borrow::Cow::Borrowed(if *b { "true" } else { "false" })),
        _ => None,
    }
}
