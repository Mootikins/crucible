//! Matcher for finding agent cards based on tags and text search

use crate::agent::types::*;
use std::collections::HashMap;

/// Matcher for finding agent cards based on tags and text search
#[derive(Debug)]
pub struct AgentCardMatcher {
    /// Weights for different matching criteria
    weights: MatchingWeights,
}

#[derive(Debug, Clone)]
pub struct MatchingWeights {
    /// Weight for tag matches
    pub tag_match: u32,
    /// Weight for text search matches
    pub text_match: u32,
}

impl Default for MatchingWeights {
    fn default() -> Self {
        Self {
            tag_match: 20,
            text_match: 10,
        }
    }
}

impl Default for AgentCardMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentCardMatcher {
    /// Create a new agent card matcher with default weights
    pub fn new() -> Self {
        Self {
            weights: MatchingWeights::default(),
        }
    }

    /// Create a new agent card matcher with custom weights
    pub fn with_weights(weights: MatchingWeights) -> Self {
        Self { weights }
    }

    /// Find agent cards matching the given query
    pub fn find_matching(
        &self,
        cards: &HashMap<String, AgentCard>,
        query: &AgentCardQuery,
    ) -> Vec<AgentCardMatch> {
        let mut matches = Vec::new();

        for card in cards.values() {
            if let Some(card_match) = self.match_card(card, query) {
                matches.push(card_match);
            }
        }

        // Sort by score (highest first)
        matches.sort_by(|a, b| b.score.cmp(&a.score));
        matches
    }

    /// Match a single agent card against the query
    fn match_card(&self, card: &AgentCard, query: &AgentCardQuery) -> Option<AgentCardMatch> {
        let mut score = 0u32;
        let mut matched_criteria = Vec::new();

        // Check tags
        let tag_matches = self.count_tag_matches(card, &query.tags);
        if tag_matches > 0 {
            score += (tag_matches as u32) * self.weights.tag_match;
            matched_criteria.push(format!("{} tags", tag_matches));
        }

        // Check text search
        if let Some(search_text) = &query.text_search {
            if let Some(text_score) = self.check_text_match(card, search_text) {
                score += text_score * self.weights.text_match;
                matched_criteria.push("text search match".to_string());
            }
        }

        // Only return a match if we have some positive score
        if score > 0 {
            Some(AgentCardMatch {
                card: card.clone(),
                score,
                matched_criteria,
            })
        } else {
            None
        }
    }

    /// Count how many tags match
    fn count_tag_matches(&self, card: &AgentCard, required_tags: &[String]) -> usize {
        required_tags
            .iter()
            .filter(|req_tag| card.tags.contains(*req_tag))
            .count()
    }

    /// Check text search in name and description
    fn check_text_match(&self, card: &AgentCard, search_text: &str) -> Option<u32> {
        let search_lower = search_text.to_lowercase();
        let mut score = 0u32;

        // Check name match (higher weight)
        if card.name.to_lowercase().contains(&search_lower) {
            score += 10;
        }

        // Check description match
        if card.description.to_lowercase().contains(&search_lower) {
            score += 5;
        }

        // Check tags match
        for tag in &card.tags {
            if tag.to_lowercase().contains(&search_lower) {
                score += 2;
            }
        }

        if score > 0 {
            Some(score)
        } else {
            None
        }
    }
}
