//! Skill storage operations for SurrealDB

use crate::error::{SkillError, SkillResult};
use crate::types::{Skill, SkillScope, SkillSource};
use crucible_surrealdb::adapters::SurrealClientHandle;
use serde_json::json;
use tracing::debug;

#[cfg(feature = "embeddings")]
use crate::embedding::SkillSearchResult;

/// Storage operations for skills
pub struct SkillStore {
    client: SurrealClientHandle,
}

impl SkillStore {
    /// Create a new SkillStore from a SurrealDB client handle
    pub fn new(client: SurrealClientHandle) -> Self {
        Self { client }
    }

    /// Insert or update a skill
    pub async fn upsert(&self, skill: &Skill) -> SkillResult<()> {
        let id = skill_record_id(&skill.name, skill.source.scope);

        let sql = r#"
            UPSERT type::thing("skills", $id) CONTENT {
                name: $name,
                description: $description,
                scope: $scope,
                source_path: $source_path,
                source_agent: $source_agent,
                content_hash: $content_hash,
                body: $body,
                license: $license,
                compatibility: $compatibility,
                allowed_tools: $allowed_tools,
                metadata: $metadata,
                indexed_at: $indexed_at,
                updated_at: time::now()
            }
        "#;

        let params = json!({
            "id": id,
            "name": skill.name,
            "description": skill.description,
            "scope": skill.source.scope.to_string(),
            "source_path": skill.source.path.to_string_lossy(),
            "source_agent": skill.source.agent,
            "content_hash": skill.source.content_hash,
            "body": skill.body,
            "license": skill.license,
            "compatibility": skill.compatibility,
            "allowed_tools": skill.allowed_tools,
            "metadata": serde_json::to_value(&skill.metadata).unwrap_or(json!({})),
            "indexed_at": skill.indexed_at.to_rfc3339(),
        });

        self.client
            .inner()
            .query(sql, &[params])
            .await
            .map_err(|e| SkillError::DiscoveryError(e.to_string()))?;

        debug!("Upserted skill: {}", skill.name);
        Ok(())
    }

    /// Get skill by name (returns highest priority)
    pub async fn get_by_name(&self, name: &str) -> SkillResult<Option<Skill>> {
        let sql = r#"
            SELECT * FROM skills
            WHERE name = $name
            ORDER BY
                CASE scope
                    WHEN "kiln" THEN 3
                    WHEN "workspace" THEN 2
                    WHEN "personal" THEN 1
                END DESC
            LIMIT 1
        "#;

        let params = json!({ "name": name });
        let result = self
            .client
            .inner()
            .query(sql, &[params])
            .await
            .map_err(|e| SkillError::DiscoveryError(e.to_string()))?;

        // Convert the first record if it exists
        if let Some(record) = result.records.first() {
            let skill =
                record_to_skill(record).map_err(|e| SkillError::DiscoveryError(e.to_string()))?;
            Ok(Some(skill))
        } else {
            Ok(None)
        }
    }

    /// List all skills for a scope
    pub async fn list_by_scope(&self, scope: SkillScope) -> SkillResult<Vec<Skill>> {
        let sql = "SELECT * FROM skills WHERE scope = $scope";
        let params = json!({ "scope": scope.to_string() });

        let result = self
            .client
            .inner()
            .query(sql, &[params])
            .await
            .map_err(|e| SkillError::DiscoveryError(e.to_string()))?;

        result
            .records
            .iter()
            .map(|record| {
                record_to_skill(record).map_err(|e| SkillError::DiscoveryError(e.to_string()))
            })
            .collect()
    }

    /// List all skills (all scopes)
    pub async fn list_all(&self) -> SkillResult<Vec<Skill>> {
        let sql = "SELECT * FROM skills ORDER BY name";
        let result = self
            .client
            .inner()
            .query(sql, &[])
            .await
            .map_err(|e| SkillError::DiscoveryError(e.to_string()))?;

        result
            .records
            .iter()
            .map(|record| {
                record_to_skill(record).map_err(|e| SkillError::DiscoveryError(e.to_string()))
            })
            .collect()
    }

    /// Delete skill by name and scope
    pub async fn delete(&self, name: &str, scope: SkillScope) -> SkillResult<()> {
        let id = skill_record_id(name, scope);
        let sql = "DELETE type::thing(\"skills\", $id)";
        let params = json!({ "id": id });

        self.client
            .inner()
            .query(sql, &[params])
            .await
            .map_err(|e| SkillError::DiscoveryError(e.to_string()))?;

        Ok(())
    }

    /// Store embedding for a skill (stores in separate skills_embeddings table)
    #[cfg(feature = "embeddings")]
    pub async fn store_embedding(
        &self,
        skill: &Skill,
        embedding: &[f32],
        model: &str,
    ) -> SkillResult<()> {
        let skill_id = skill_record_id(&skill.name, skill.source.scope);

        let sql = r#"
            UPSERT type::thing("skills_embeddings", $skill_id) CONTENT {
                skill_id: type::thing("skills", $skill_id),
                name: $name,
                description: $description,
                scope: $scope,
                embedding: $embedding,
                model: $model,
                created_at: time::now()
            }
        "#;

        let params = json!({
            "skill_id": skill_id,
            "name": skill.name,
            "description": skill.description,
            "scope": skill.source.scope.to_string(),
            "embedding": embedding,
            "model": model,
        });

        self.client
            .inner()
            .query(sql, &[params])
            .await
            .map_err(|e| SkillError::DiscoveryError(format!("Failed to store embedding: {}", e)))?;

        debug!("Stored embedding for skill: {}", skill.name);
        Ok(())
    }

    /// Search skills by embedding similarity
    #[cfg(feature = "embeddings")]
    pub async fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> SkillResult<Vec<SkillSearchResult>> {
        let sql = format!(
            r#"
            SELECT
                name,
                description,
                scope,
                skill_id,
                vector::distance::cosine(embedding, $vector) AS distance
            FROM skills_embeddings
            ORDER BY distance ASC
            LIMIT {limit}
            "#,
            limit = limit
        );

        let params = json!({ "vector": query_embedding });

        let result = self
            .client
            .inner()
            .query(&sql, &[params])
            .await
            .map_err(|e| SkillError::DiscoveryError(format!("Skill search failed: {}", e)))?;

        let mut results = Vec::new();
        for record in result.records {
            let name = record
                .data
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let description = record
                .data
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let scope = record
                .data
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("personal")
                .to_string();

            let distance = record
                .data
                .get("distance")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0) as f32;

            let source_path = format!("{}:{}", scope, name);

            results.push(SkillSearchResult::new(
                name,
                description,
                scope,
                source_path,
                distance,
            ));
        }

        Ok(results)
    }
}

fn skill_record_id(name: &str, scope: SkillScope) -> String {
    format!("{}_{}", scope, name.replace('-', "_"))
}

// Implement embedding traits when embeddings feature is enabled
#[cfg(feature = "embeddings")]
impl crate::embedding::SkillEmbeddingStore for SkillStore {
    async fn store_skill_embedding(
        &self,
        skill: &Skill,
        embedding: &[f32],
        model: &str,
    ) -> SkillResult<()> {
        self.store_embedding(skill, embedding, model).await
    }
}

#[cfg(feature = "embeddings")]
impl crate::embedding::SkillSearchStore for SkillStore {
    async fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> SkillResult<Vec<SkillSearchResult>> {
        self.search_by_embedding(query_embedding, limit).await
    }
}

/// Convert a database Record to a Skill
fn record_to_skill(record: &crucible_core::database::Record) -> anyhow::Result<Skill> {
    use anyhow::anyhow;

    let name = record
        .data
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing name field"))?
        .to_string();

    let description = record
        .data
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing description field"))?
        .to_string();

    let scope_str = record
        .data
        .get("scope")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing scope field"))?;

    let scope = match scope_str {
        "kiln" => SkillScope::Kiln,
        "workspace" => SkillScope::Workspace,
        _ => SkillScope::Personal,
    };

    let source_path = record
        .data
        .get("source_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing source_path field"))?
        .to_string();

    let source_agent = record
        .data
        .get("source_agent")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let content_hash = record
        .data
        .get("content_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing content_hash field"))?
        .to_string();

    let body = record
        .data
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing body field"))?
        .to_string();

    let license = record
        .data
        .get("license")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let compatibility = record
        .data
        .get("compatibility")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let allowed_tools = record
        .data
        .get("allowed_tools")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });

    let metadata = record
        .data
        .get("metadata")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let indexed_at = record
        .data
        .get("indexed_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    Ok(Skill {
        name,
        description,
        body,
        license,
        compatibility,
        allowed_tools,
        metadata,
        source: SkillSource {
            agent: source_agent,
            scope,
            path: std::path::PathBuf::from(source_path),
            content_hash,
        },
        indexed_at,
    })
}
