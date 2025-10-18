use crate::agent::types::*;
use anyhow::{anyhow, Result};
use regex::Regex;
use std::fs;
use std::path::Path;
use std::collections::HashMap;

/// Loader for agent definitions from markdown files with YAML frontmatter
#[derive(Debug)]
pub struct AgentLoader {
    /// Cache for loaded agents
    cache: HashMap<String, AgentDefinition>,
}

impl AgentLoader {
    /// Create a new agent loader
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Load all agent definitions from a directory
    pub fn load_from_directory(&mut self, dir_path: &str) -> Result<Vec<AgentDefinition>> {
        let path = Path::new(dir_path);
        if !path.exists() {
            return Err(anyhow!("Directory does not exist: {}", dir_path));
        }

        let mut agents = Vec::new();

        // Look for .md files in the directory
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_file() && file_path.extension().is_some_and(|ext| ext == "md") {
                match self.load_from_file(file_path.to_str().unwrap()) {
                    Ok(agent) => agents.push(agent),
                    Err(e) => {
                        eprintln!("Warning: Failed to load agent from {:?}: {}", file_path, e);
                    }
                }
            }
        }

        Ok(agents)
    }

    /// Load a single agent from a markdown file
    pub fn load_from_file(&mut self, file_path: &str) -> Result<AgentDefinition> {
        // Check cache first
        if let Some(agent) = self.cache.get(file_path) {
            return Ok(agent.clone());
        }

        let content = fs::read_to_string(file_path)?;
        let agent = self.parse_markdown_agent(&content, file_path)?;

        // Cache the result
        self.cache.insert(file_path.to_string(), agent.clone());
        Ok(agent)
    }

    /// Parse agent definition from markdown content with YAML frontmatter
    fn parse_markdown_agent(&self, content: &str, file_path: &str) -> Result<AgentDefinition> {
        // Split content by the frontmatter delimiters
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() < 3 {
            return Err(anyhow!("Invalid agent file format: missing YAML frontmatter in {}", file_path));
        }

        let frontmatter_str = parts[1].trim();
        let markdown_content = parts[2].trim();

        // Parse YAML frontmatter
        let frontmatter: AgentFrontmatter = serde_yaml::from_str(frontmatter_str)
            .map_err(|e| anyhow!("Failed to parse YAML frontmatter in {}: {}", file_path, e))?;

        // Convert frontmatter to full agent definition
        let agent_id = uuid::Uuid::new_v4();
        let now = chrono::Utc::now();

        let agent = AgentDefinition {
            id: agent_id,
            name: frontmatter.name.clone(),
            version: frontmatter.version,
            description: frontmatter.description,
            capabilities: frontmatter.capabilities.into_iter().map(|cap| Capability {
                name: cap.name,
                description: cap.description,
                skill_level: cap.skill_level,
                required_tools: cap.required_tools.unwrap_or_default(),
            }).collect(),
            required_tools: frontmatter.required_tools,
            optional_tools: frontmatter.optional_tools.unwrap_or_default(),
            tags: frontmatter.tags,
            personality: Personality {
                tone: frontmatter.personality.tone,
                style: frontmatter.personality.style,
                verbosity: frontmatter.personality.verbosity,
                traits: frontmatter.personality.traits,
                preferences: frontmatter.personality.preferences.unwrap_or_default(),
            },
            system_prompt: self.extract_system_prompt(markdown_content)?,
            skills: frontmatter.skills.into_iter().map(|skill| Skill {
                name: skill.name,
                category: skill.category,
                proficiency: skill.proficiency,
                experience_years: skill.experience_years.unwrap_or(0.0),
                certifications: skill.certifications.unwrap_or_default(),
            }).collect(),
            config: frontmatter.config.unwrap_or_default(),
            dependencies: frontmatter.dependencies.unwrap_or_default(),
            created_at: now,
            updated_at: now,
            status: frontmatter.status.unwrap_or(AgentStatus::Active),
            author: frontmatter.author,
            documentation_url: frontmatter.documentation_url,
        };

        // Validate the agent definition
        self.validate_agent(&agent)?;

        Ok(agent)
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
                if trimmed.starts_with("#") && !trimmed.starts_with("##") {
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

    /// Validate agent definition
    fn validate_agent(&self, agent: &AgentDefinition) -> Result<()> {
        if agent.name.is_empty() {
            return Err(anyhow!("Agent name cannot be empty"));
        }

        if agent.description.is_empty() {
            return Err(anyhow!("Agent description cannot be empty"));
        }

        if agent.system_prompt.is_empty() {
            return Err(anyhow!("Agent system prompt cannot be empty"));
        }

        // Validate semantic version
        if !self.is_valid_semver(&agent.version) {
            return Err(anyhow!("Invalid version format: {}", agent.version));
        }

        // Validate skill proficiency (1-10)
        for skill in &agent.skills {
            if skill.proficiency == 0 || skill.proficiency > 10 {
                return Err(anyhow!("Skill proficiency must be between 1 and 10: {} = {}",
                                  skill.name, skill.proficiency));
            }
        }

        Ok(())
    }

    /// Check if a string is a valid semantic version
    fn is_valid_semver(&self, version: &str) -> bool {
        let semver_regex = Regex::new(r"^\d+\.\d+\.\d+(-[a-zA-Z0-9\-\.]+)?(\+[a-zA-Z0-9\-\.]+)?$").unwrap();
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

impl Default for AgentLoader {
    fn default() -> Self {
        Self::new()
    }
}