//! The per-index settings: what is searched, filtered, sorted and shown, how
//! results are ranked, and whether other processes may see the index.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::tokenizer;

/// A field that is tokenised and searched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchableField {
    pub name: String,
    pub weight: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RankingRule {
    Words,
    Typo,
    Proximity,
    Attribute,
    Exactness,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypoSettings {
    pub enabled: bool,
    pub min_word_len_one_typo: usize,
    pub min_word_len_two_typos: usize,
    /// Words that must match exactly.
    pub disabled_words: BTreeSet<String>,
}

impl Default for TypoSettings {
    fn default() -> Self {
        TypoSettings {
            enabled: true,
            min_word_len_one_typo: 4,
            min_word_len_two_typos: 8,
            disabled_words: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub searchable: Vec<SearchableField>,
    pub filterable: Vec<String>,
    pub sortable: Vec<String>,
    /// Fields copied into a hit's `_source`. `None` copies all.
    pub displayed: Option<Vec<String>>,
    pub ranking_rules: Vec<RankingRule>,
    /// Query word (folded) to the words and phrases it also means.
    pub synonyms: BTreeMap<String, Vec<String>>,
    pub stop_words: BTreeSet<String>,
    pub typo: TypoSettings,
    /// Whether other processes may search this index (design/search.md 8.4).
    pub external: bool,
    /// A short human title, shown by `ListIndexes`.
    pub title: String,
}

impl Default for Schema {
    fn default() -> Self {
        Schema {
            searchable: Vec::new(),
            filterable: Vec::new(),
            sortable: Vec::new(),
            displayed: None,
            ranking_rules: vec![
                RankingRule::Words,
                RankingRule::Typo,
                RankingRule::Proximity,
                RankingRule::Attribute,
                RankingRule::Exactness,
            ],
            synonyms: BTreeMap::new(),
            stop_words: BTreeSet::new(),
            typo: TypoSettings::default(),
            external: false,
            title: String::new(),
        }
    }
}

/// The most searchable fields an index can have.
pub const MAX_SEARCHABLE: usize = 64;

impl Schema {
    pub fn new() -> Self {
        Schema::default()
    }

    pub fn searchable(mut self, name: &str, weight: f32) -> Self {
        self.searchable.push(SearchableField {
            name: name.to_string(),
            weight,
        });
        self
    }

    pub fn filterable<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.filterable.extend(fields.into_iter().map(Into::into));
        self
    }

    pub fn sortable<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.sortable.extend(fields.into_iter().map(Into::into));
        self
    }

    pub fn displayed<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.displayed = Some(fields.into_iter().map(Into::into).collect());
        self
    }

    pub fn ranking_rules(mut self, rules: Vec<RankingRule>) -> Self {
        self.ranking_rules = rules;
        self
    }

    /// `word` also means each of `means` (one way).
    pub fn synonym<I, S>(mut self, word: &str, means: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.synonyms
            .entry(tokenizer::fold(word))
            .or_default()
            .extend(means.into_iter().map(Into::into));
        self
    }

    /// Every word of the group means every other.
    pub fn synonym_group(mut self, group: &[&str]) -> Self {
        for word in group {
            let others: Vec<String> = group
                .iter()
                .filter(|o| *o != word)
                .map(|o| o.to_string())
                .collect();
            self = self.synonym(word, others);
        }
        self
    }

    pub fn stop_words<I, S>(mut self, words: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.stop_words
            .extend(words.into_iter().map(|w| tokenizer::fold(w.as_ref())));
        self
    }

    pub fn typo(mut self, typo: TypoSettings) -> Self {
        self.typo = typo;
        self
    }

    pub fn external(mut self, external: bool) -> Self {
        self.external = external;
        self
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn validate(&self) -> Result<()> {
        if self.searchable.is_empty() {
            return Err(Error::Schema("a schema needs a searchable field".into()));
        }
        if self.searchable.len() > MAX_SEARCHABLE {
            return Err(Error::Schema(format!(
                "at most {MAX_SEARCHABLE} searchable fields"
            )));
        }
        let mut seen = BTreeSet::new();
        for f in &self.searchable {
            if !seen.insert(&f.name) {
                return Err(Error::Schema(format!("duplicate searchable `{}`", f.name)));
            }
            if !(f.weight.is_finite() && f.weight > 0.0) {
                return Err(Error::Schema(format!(
                    "weight of `{}` must be positive",
                    f.name
                )));
            }
        }
        Ok(())
    }
}
