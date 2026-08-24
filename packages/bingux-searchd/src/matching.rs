pub fn score(query: &str, candidate: &str) -> Option<f64> {
    let query = query.to_ascii_lowercase();
    let candidate = candidate.to_ascii_lowercase();
    score_normalized(&query, &candidate)
}

pub fn score_normalized(query: &str, candidate: &str) -> Option<f64> {
    let query = query.trim();
    if query.is_empty() {
        return Some(0.0);
    }

    let mut total = 0.0;
    let mut terms = 0usize;

    for term in query.split_whitespace() {
        total += term_score(term, candidate)?;
        terms += 1;
    }

    Some(total / terms as f64)
}

fn term_score(term: &str, candidate: &str) -> Option<f64> {
    if term == candidate {
        return Some(1.0);
    }

    if let Some(position) = candidate.find(term) {
        let position_penalty = position as f64 / candidate.len().max(1) as f64 * 0.15;
        let length_penalty = (candidate.len().saturating_sub(term.len())) as f64
            / candidate.len().max(1) as f64
            * 0.1;
        return Some((0.95 - position_penalty - length_penalty).max(0.5));
    }

    let mut expected_characters = term.chars();
    let mut expected = expected_characters.next()?;
    let mut previous = None;
    let mut gaps = 0usize;

    for (index, character) in candidate.char_indices() {
        if character != expected {
            continue;
        }

        if let Some(previous) = previous {
            gaps += index.saturating_sub(previous + character.len_utf8());
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
    use super::score;

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
