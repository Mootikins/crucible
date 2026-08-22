use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

pub struct FuzzyMatcher {
    matcher: Matcher,
}

impl FuzzyMatcher {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    pub fn match_items(&mut self, query: &str, items: &[impl AsRef<str>]) -> Vec<(usize, u32)> {
        if query.is_empty() {
            return items
                .iter()
                .enumerate()
                .map(|(idx, _)| (idx, 100u32))
                .collect();
        }

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

        let matched_items = pattern.match_list(items, &mut self.matcher);

        let mut matches: Vec<(usize, u32)> = items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                matched_items
                    .iter()
                    .find(|(matched_item, _)| matched_item.as_ref() == item.as_ref())
                    .map(|(_, score)| (idx, *score))
            })
            .collect();

        matches.sort_by_key(|m| std::cmp::Reverse(m.1));
        matches
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Levenshtein distance between two strings, counted in chars.
///
/// The "did you mean" hints use it. Fifteen lines beat a dependency.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut cur = vec![0usize; b_chars.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b_chars.len()]
}

#[cfg(test)]
mod levenshtein_tests {
    use super::levenshtein;

    #[test]
    fn levenshtein_measures_what_it_claims() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("quit", "qut"), 1);
        assert_eq!(levenshtein("quit", "quiit"), 1);
        assert_eq!(levenshtein("quit", "qxit"), 1);
        assert_eq!(levenshtein("abc", "xyz"), 3);
        assert_eq!(levenshtein("pre_toolcall", "pre_tool_call"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn levenshtein_counts_chars_not_bytes() {
        assert_eq!(levenshtein("é", "e"), 1);
    }
}
