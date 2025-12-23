//! Skill embedding generation and search

#![cfg(feature = "embeddings")]

use crate::error::{SkillError, SkillResult};
use crate::types::Skill;
use crucible_core::traits::provider::CanEmbed;
use tracing::debug;

/// Embed a skill's description and store it
pub async fn embed_skill<P: CanEmbed>(
    client: &impl SkillEmbeddingStore,
    skill: &Skill,
    provider: &P,
) -> SkillResult<()> {
    // Embed the description (concise, designed for matching)
    let embedding = provider
        .embed(&skill.description)
        .await
        .map_err(|e| SkillError::DiscoveryError(format!("Embedding failed: {}", e)))?;

    let vector = embedding.embedding;
    let model = embedding.model;
    let dimensions = vector.len();

    // Store embedding
    client.store_skill_embedding(skill, &vector, &model).await?;

    debug!("Embedded skill: {} ({} dims)", skill.name, dimensions);
    Ok(())
}

/// Trait for storing skill embeddings (implemented by SkillStore when storage feature enabled)
pub trait SkillEmbeddingStore: Send + Sync {
    fn store_skill_embedding(
        &self,
        skill: &Skill,
        embedding: &[f32],
        model: &str,
    ) -> impl std::future::Future<Output = SkillResult<()>> + Send;
}

/// Search skills by semantic similarity
pub async fn search_skills_semantic<P: CanEmbed>(
    client: &impl SkillSearchStore,
    query: &str,
    provider: &P,
    limit: usize,
) -> SkillResult<Vec<SkillSearchResult>> {
    let embedding = provider
        .embed(query)
        .await
        .map_err(|e| SkillError::DiscoveryError(format!("Embedding failed: {}", e)))?;

    client.search_by_embedding(&embedding.embedding, limit).await
}

/// Trait for searching skills by embedding
pub trait SkillSearchStore: Send + Sync {
    fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> impl std::future::Future<Output = SkillResult<Vec<SkillSearchResult>>> + Send;
}

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
    pub fn new(name: String, description: String, scope: String, source_path: String, distance: f32) -> Self {
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
