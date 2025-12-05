//! Loader for agent cards from markdown files with YAML frontmatter

use crate::agent::types::*;
use anyhow::{anyhow, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Loader for agent cards from markdown files with YAML frontmatter
#[derive(Debug)]
pub struct AgentCardLoader {
    /// Cache for loaded agent cards
    cache: HashMap<String, AgentCard>,
}

impl AgentCardLoader {
    /// Create a new agent card loader
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Load all agent cards from a directory
    pub fn load_from_directory(&mut self, dir_path: &str) -> Result<Vec<AgentCard>> {
        let path = Path::new(dir_path);
        if !path.exists() {
            return Err(anyhow!("Directory does not exist: {}", dir_path));
        }

        let mut cards = Vec::new();

        // Look for .md files in the directory
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_file() && file_path.extension().is_some_and(|ext| ext == "md") {
                match self.load_from_file(file_path.to_str().unwrap()) {
                    Ok(card) => cards.push(card),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to load agent card from {:?}: {}",
                            file_path, e
                        );
                    }
                }
            }
        }

        Ok(cards)
    }

    /// Load a single agent card from a markdown file
    pub fn load_from_file(&mut self, file_path: &str) -> Result<AgentCard> {
        // Check cache first
        if let Some(card) = self.cache.get(file_path) {
            return Ok(card.clone());
        }

        let content = fs::read_to_string(file_path)?;
        let card = self.parse_markdown(&content, file_path)?;

        // Cache the result
        self.cache.insert(file_path.to_string(), card.clone());
        Ok(card)
    }

    /// Parse agent card from markdown content with YAML frontmatter
    fn parse_markdown(&self, content: &str, file_path: &str) -> Result<AgentCard> {
        // Split content by the frontmatter delimiters
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() < 3 {
            return Err(anyhow!(
                "Invalid agent card format: missing YAML frontmatter in {}",
                file_path
            ));
        }

        let frontmatter_str = parts[1].trim();
        let markdown_content = parts[2].trim();

        // Parse YAML frontmatter
        let frontmatter: AgentCardFrontmatter = serde_yaml::from_str(frontmatter_str)
            .map_err(|e| anyhow!("Failed to parse YAML frontmatter in {}: {}", file_path, e))?;

        // Convert frontmatter to full agent card
        let card_id = uuid::Uuid::new_v4();
        let now = chrono::Utc::now();

        let card = AgentCard {
            id: card_id,
            name: frontmatter.name.clone(),
            version: frontmatter.version,
            description: frontmatter.description,
            capabilities: frontmatter
                .capabilities
                .into_iter()
                .map(|cap| Capability {
                    name: cap.name,
                    description: cap.description,
                    required_tools: cap.required_tools.unwrap_or_default(),
                })
                .collect(),
            required_tools: frontmatter.required_tools,
            optional_tools: frontmatter.optional_tools.unwrap_or_default(),
            tags: frontmatter.tags,
            system_prompt: self.extract_system_prompt(markdown_content)?,
            skills: frontmatter
                .skills
                .into_iter()
                .map(|skill| Skill {
                    name: skill.name,
                    category: skill.category,
                })
                .collect(),
            config: frontmatter.config.unwrap_or_default(),
            dependencies: frontmatter.dependencies.unwrap_or_default(),
            created_at: now,
            updated_at: now,
            status: frontmatter.status.unwrap_or(AgentCardStatus::Active),
            author: frontmatter.author,
            documentation_url: frontmatter.documentation_url,
        };

        // Validate the agent card
        self.validate(&card)?;

        Ok(card)
    }

    /// Extract system prompt from markdown content
    fn extract_system_prompt(&self, markdown_content: &str) -> Result<String> {
        // Look for a system prompt section in the markdown
        let lines: Vec<&str> = markdown_content.lines().collect();
        let mut in_system_prompt = false;
        let mut prompt_lines = Vec::new();

        for line in lines {
            let trimmed = line.trim();

            if trimmed.starts_with("# System Prompt") || trimmed.starts_with("## System Prompt") {
                in_system_prompt = true;
                continue;
            }

            if in_system_prompt {
                if trimmed.starts_with('#') && !trimmed.starts_with("##") {
                    // End of system prompt section
                    break;
                }
                prompt_lines.push(line);
            }
        }

        if prompt_lines.is_empty() {
            // If no explicit system prompt section, use the entire markdown content
            Ok(markdown_content.trim().to_string())
        } else {
            Ok(prompt_lines.join("\n").trim().to_string())
        }
    }

    /// Validate agent card
    fn validate(&self, card: &AgentCard) -> Result<()> {
        if card.name.is_empty() {
            return Err(anyhow!("Agent card name cannot be empty"));
        }

        if card.description.is_empty() {
            return Err(anyhow!("Agent card description cannot be empty"));
        }

        if card.system_prompt.is_empty() {
            return Err(anyhow!("Agent card system prompt cannot be empty"));
        }

        // Validate semantic version
        if !self.is_valid_semver(&card.version) {
            return Err(anyhow!("Invalid version format: {}", card.version));
        }

        Ok(())
    }

    /// Check if a string is a valid semantic version
    fn is_valid_semver(&self, version: &str) -> bool {
        let semver_regex =
            Regex::new(r"^\d+\.\d+\.\d+(-[a-zA-Z0-9\-\.]+)?(\+[a-zA-Z0-9\-\.]+)?$").unwrap();
        semver_regex.is_match(version)
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> usize {
        self.cache.len()
    }
}

impl Default for AgentCardLoader {
    fn default() -> Self {
        Self::new()
    }
}
