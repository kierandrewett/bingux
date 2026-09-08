#[derive(Debug)]
struct Term {
    value: String,
    exact: bool,
    excluded: bool,
    extension: bool,
}

/// OR-separated conjunctions; AND binds more tightly than OR. Unfinished
/// quotes extend to the input end, so search remains useful while typing.
#[derive(Debug)]
pub struct SearchQuery {
    groups: Vec<Vec<Term>>,
}

impl SearchQuery {
    /// One literal retrieval seed per OR branch; final matching still evaluates
    /// the complete expression. Short/fuzzy-only queries use the local cache.
    pub fn index_seeds(&self) -> Vec<String> {
        let mut seeds: Vec<_> = self.groups.iter().filter_map(|group| {
            group.iter().filter(|term| !term.excluded && !term.extension)
                .flat_map(|term| term.value.split(|c: char| !c.is_alphanumeric()))
                .filter(|word| word.chars().count() >= 3)
                .max_by_key(|word| word.len()).map(str::to_owned)
        }).collect();
        seeds.sort();
        seeds.dedup();
        seeds
    }

    pub fn parse(input: &str) -> Self {
        let mut groups = Vec::new();
        let mut group = Vec::new();
        let mut chars = input.chars().peekable();
        while let Some(character) = chars.next() {
            if character.is_whitespace() {
                continue;
            }
            let excluded = character == '-' && chars.peek().is_some_and(|c| !c.is_whitespace());
            let first = if excluded {
                chars.next().unwrap()
            } else {
                character
            };
            let mut quoted = first == '"';
            let mut exact = quoted;
            let mut value = String::new();
            if !quoted {
                value.push(first);
            }
            while let Some(&next) = chars.peek() {
                if !quoted && next.is_whitespace() {
                    break;
                }
                chars.next();
                if next == '\\' && chars.peek().is_some_and(|c| *c == '"' || *c == '\\') {
                    value.push(chars.next().unwrap());
                } else if next == '"' {
                    quoted = !quoted;
                    exact = true;
                } else {
                    value.push(next);
                }
            }
            if value.is_empty() {
                continue;
            }
            if !excluded && !exact && value == "OR" {
                if !group.is_empty() {
                    groups.push(std::mem::take(&mut group));
                }
                continue;
            }
            if !excluded && !exact && value == "AND" {
                continue;
            }
            let value = value.to_lowercase();
            let extension = !exact && (value.starts_with("filetype:") || value.starts_with("ext:"));
            let value = if extension {
                value
                    .split_once(':')
                    .unwrap()
                    .1
                    .trim_start_matches('.')
                    .to_owned()
            } else {
                value
            };
            if value.is_empty() {
                continue;
            }
            group.push(Term {
                value,
                exact,
                excluded,
                extension,
            });
        }
        if !group.is_empty() {
            groups.push(group);
        }
        Self { groups }
    }

    /// Inputs are Unicode-lowercased once when the index is built.
    pub fn score_fields(&self, title: &str, searchable: &str) -> Option<f64> {
        if self.groups.is_empty() {
            return Some(0.0);
        }
        self.groups
            .iter()
            .filter_map(|group| {
                let mut total = 0.0;
                let mut count = 0;
                for term in group {
                    let literal_match = if term.extension {
                        title
                            .rsplit_once('.')
                            .is_some_and(|(_, ext)| ext == term.value)
                    } else {
                        searchable.contains(&term.value)
                    };
                    if term.excluded {
                        if literal_match {
                            return None;
                        }
                        continue;
                    }
                    let value = if term.extension {
                        if !literal_match {
                            return None;
                        }
                        1.0
                    } else if term.exact {
                        if !literal_match {
                            return None;
                        }
                        if title == term.value {
                            1.0
                        } else if title.contains(&term.value) {
                            0.94
                        } else {
                            0.7
                        }
                    } else {
                        let context_score = term_score(&term.value, searchable)? * 0.78;
                        term_score(&term.value, title)
                            .unwrap_or(0.0)
                            .max(context_score)
                    };
                    total += value;
                    count += 1;
                }
                Some(if count == 0 {
                    0.1
                } else {
                    total / f64::from(count)
                })
            })
            .max_by(f64::total_cmp)
    }
}

pub fn score(query: &str, candidate: &str) -> Option<f64> {
    let candidate = candidate.to_lowercase();
    SearchQuery::parse(query).score_fields(&candidate, &candidate)
}

pub fn score_normalized(query: &str, candidate: &str) -> Option<f64> {
    SearchQuery::parse(query).score_fields(candidate, candidate)
}

fn term_score(term: &str, candidate: &str) -> Option<f64> {
    if term == candidate {
        return Some(1.0);
    }

    if let Some(position) = candidate.find(term) {
        let boundary = position == 0
            || candidate[..position]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric());
        let position_penalty = position as f64 / candidate.len().max(1) as f64 * 0.08;
        return Some((if boundary { 0.94 } else { 0.83 }) - position_penalty);
    }

    let mut expected_characters = term.chars();
    let mut expected = expected_characters.next()?;
    let mut previous = None;
    let mut gaps = 0usize;
    let max_gaps = term.chars().count() * 2;

    for (index, character) in candidate.chars().enumerate() {
        if character != expected {
            continue;
        }

        if let Some(previous) = previous {
            gaps += index.saturating_sub(previous + 1);
            if gaps > max_gaps {
                return None;
            }
        }
        previous = Some(index);

        let Some(next) = expected_characters.next() else {
            let density = term.len() as f64 / (term.len() + gaps) as f64;
            return Some(0.2 + density * 0.35);
        };
        expected = next;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{SearchQuery, score};

    #[test]
    fn os_index_seeds_preserve_or_branches_without_operators_or_globs() {
        assert_eq!(SearchQuery::parse("\"annual report\" -draft OR firefox ext:pdf").index_seeds(), vec!["firefox", "report"]);
        assert!(SearchQuery::parse("-draft ext:pdf ab").index_seeds().is_empty());
        assert_eq!(SearchQuery::parse("notes* OR notes").index_seeds(), vec!["notes"]);
    }

    #[test]
    fn phrases_are_contiguous_and_case_insensitive() {
        assert!(score("\"Visual Studio\"", "VISUAL STUDIO Code").is_some());
        assert!(score("\"visual code\"", "Visual Studio Code").is_none());
        assert!(score("\"visual studio", "Visual Studio Code").is_some());
    }

    #[test]
    fn or_has_lower_precedence_than_and() {
        assert!(score("firefox OR visual AND code", "Firefox").is_some());
        assert!(score("firefox OR visual AND code", "Visual Studio Code").is_some());
        assert!(score("firefox OR visual AND code", "Visual Studio").is_none());
        assert!(score("\"OR\"", "Firefox").is_none());
        assert!(score("firefox OR", "Firefox").is_some());
        assert!(score("OR firefox", "Firefox").is_some());
    }

    #[test]
    fn exclusions_are_literal_not_fuzzy_and_apply_to_their_branch() {
        assert!(score("report -draft", "report final.pdf").is_some());
        assert!(score("report -draft", "report draft.pdf").is_none());
        assert!(score("report -\"old copy\"", "report old copy.pdf").is_none());
        assert!(score("report -dft", "report draft.pdf").is_some());
        assert!(score("-draft", "final.pdf").is_some());
        assert!(score("report -draft OR notes", "notes draft.txt").is_some());
    }

    #[test]
    fn extensions_are_exact_and_can_be_excluded() {
        assert!(score("report filetype:PDF", "report.pdf").is_some());
        assert!(score("report ext:pdf", "report.pdf.bak").is_none());
        assert!(score("report -ext:pdf", "report.pdf").is_none());
    }

    #[test]
    fn names_and_boundaries_outrank_path_context() {
        let query = SearchQuery::parse("report");
        assert!(
            query.score_fields("report.pdf", "report.pdf /home/report.pdf")
                > query.score_fields("notes.txt", "notes.txt /reports/notes.txt")
        );
        assert!(score("code", "Code Editor") > score("code", "decode"));
        assert!(score("ffx", "Firefox").is_some());
        assert!(score("abc", "a/very/long/path/b/another/very/long/path/c").is_none());
        assert!(score("CAFÉ", "Café").is_some());
    }

    #[test]
    fn ranks_exact_matches_above_substrings() {
        assert!(
            score("firefox", "firefox").expect("exact")
                > score("firefox", "mozilla firefox").expect("substring")
        );
    }

    #[test]
    fn matches_multiple_terms() {
        assert!(score("visual code", "Visual Studio Code").is_some());
    }

    #[test]
    fn rejects_non_matching_terms() {
        assert_eq!(score("firefox terminal", "Firefox"), None);
    }
}
