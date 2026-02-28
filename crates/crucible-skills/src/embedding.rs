//! Skill search result


/// Skill search result
#[derive(Debug, Clone)]
pub struct SkillSearchResult {
    pub name: String,
    pub description: String,
    pub scope: String,
    pub source_path: String,
    pub distance: f32,
    pub relevance: f32,
}

impl SkillSearchResult {
    pub fn new(
        name: String,
        description: String,
        scope: String,
        source_path: String,
        distance: f32,
    ) -> Self {
        Self {
            name,
            description,
            scope,
            source_path,
            distance,
            relevance: 1.0 - distance,
        }
    }
}
