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

        matches.sort_by(|a, b| b.1.cmp(&a.1));
        matches
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}
