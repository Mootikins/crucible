use crate::agent::types::*;
use std::collections::HashMap;

/// Matcher for finding agents based on capabilities, skills, and other criteria
#[derive(Debug)]
pub struct CapabilityMatcher {
    /// Weights for different matching criteria
    weights: MatchingWeights,
}

#[derive(Debug, Clone)]
pub struct MatchingWeights {
    /// Weight for exact capability matches
    pub capability_match: u32,
    /// Weight for skill matches
    pub skill_match: u32,
    /// Weight for tag matches
    pub tag_match: u32,
    /// Weight for tool availability matches
    pub tool_match: u32,
    /// Weight for text search matches
    pub text_match: u32,
    /// Weight for skill level matches
    pub skill_level_match: u32,
}

impl Default for MatchingWeights {
    fn default() -> Self {
        Self {
            capability_match: 30,
            skill_match: 25,
            tag_match: 15,
            tool_match: 20,
            text_match: 10,
            skill_level_match: 15,
        }
    }
}

impl Default for CapabilityMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityMatcher {
    /// Create a new capability matcher with default weights
    pub fn new() -> Self {
        Self {
            weights: MatchingWeights::default(),
        }
    }

    /// Create a new capability matcher with custom weights
    pub fn with_weights(weights: MatchingWeights) -> Self {
        Self { weights }
    }

    /// Find agents matching the given query
    pub fn find_matching_agents(
        &self,
        agents: &HashMap<String, AgentDefinition>,
        query: &AgentQuery,
    ) -> Vec<AgentMatch> {
        let mut matches = Vec::new();

        for agent in agents.values() {
            if let Some(agent_match) = self.match_agent(agent, query) {
                matches.push(agent_match);
            }
        }

        // Sort by score (highest first)
        matches.sort_by(|a, b| b.score.cmp(&a.score));
        matches
    }

    /// Match a single agent against the query
    fn match_agent(&self, agent: &AgentDefinition, query: &AgentQuery) -> Option<AgentMatch> {
        // Check status filter
        if let Some(required_status) = &query.status {
            if &agent.status != required_status {
                return None;
            }
        }

        let mut score = 0u32;
        let mut matched_criteria = Vec::new();
        let mut missing_requirements = Vec::new();

        // Check capabilities
        let capability_matches = self.count_capability_matches(agent, &query.capabilities);
        if capability_matches > 0 {
            score += (capability_matches as u32) * self.weights.capability_match;
            matched_criteria.push(format!("{} capabilities", capability_matches));
        } else if !query.capabilities.is_empty() {
            missing_requirements.extend(query.capabilities.clone());
        }

        // Check skills
        let skill_matches = self.count_skill_matches(agent, &query.skills);
        if skill_matches > 0 {
            score += (skill_matches as u32) * self.weights.skill_match;
            matched_criteria.push(format!("{} skills", skill_matches));
        } else if !query.skills.is_empty() {
            missing_requirements.extend(query.skills.clone());
        }

        // Check tags
        let tag_matches = self.count_tag_matches(agent, &query.tags);
        if tag_matches > 0 {
            score += (tag_matches as u32) * self.weights.tag_match;
            matched_criteria.push(format!("{} tags", tag_matches));
        }

        // Check required tools
        let tool_matches = self.count_tool_matches(agent, &query.required_tools);
        if tool_matches > 0 {
            score += (tool_matches as u32) * self.weights.tool_match;
            matched_criteria.push(format!("{} tools", tool_matches));
        } else if !query.required_tools.is_empty() {
            missing_requirements.extend(query.required_tools.clone());
        }

        // Check skill level
        if let Some(min_level) = &query.min_skill_level {
            let level_matches = self.count_skill_level_matches(agent, min_level);
            if level_matches > 0 {
                score += (level_matches as u32) * self.weights.skill_level_match;
                matched_criteria.push(format!("{} skills at required level", level_matches));
            }
        }

        // Check text search
        if let Some(search_text) = &query.text_search {
            if let Some(text_score) = self.check_text_match(agent, search_text) {
                score += text_score * self.weights.text_match;
                matched_criteria.push("text search match".to_string());
            }
        }

        // Only return a match if we have some positive score
        if score > 0 {
            Some(AgentMatch {
                agent: agent.clone(),
                score,
                matched_criteria,
                missing_requirements,
            })
        } else {
            None
        }
    }

    /// Count how many capabilities match
    fn count_capability_matches(&self, agent: &AgentDefinition, required_caps: &[String]) -> usize {
        required_caps
            .iter()
            .filter(|req_cap| agent.capabilities.iter().any(|cap| cap.name == **req_cap))
            .count()
    }

    /// Count how many skills match
    fn count_skill_matches(&self, agent: &AgentDefinition, required_skills: &[String]) -> usize {
        required_skills
            .iter()
            .filter(|req_skill| agent.skills.iter().any(|skill| skill.name == **req_skill))
            .count()
    }

    /// Count how many tags match
    fn count_tag_matches(&self, agent: &AgentDefinition, required_tags: &[String]) -> usize {
        required_tags
            .iter()
            .filter(|req_tag| agent.tags.contains(*req_tag))
            .count()
    }

    /// Count how many required tools are available
    fn count_tool_matches(&self, agent: &AgentDefinition, required_tools: &[String]) -> usize {
        required_tools
            .iter()
            .filter(|tool| agent.required_tools.contains(*tool))
            .count()
    }

    /// Count skills at or above the required level
    fn count_skill_level_matches(&self, agent: &AgentDefinition, min_level: &SkillLevel) -> usize {
        let level_value = self.skill_level_to_value(min_level);
        agent
            .capabilities
            .iter()
            .filter(|cap| self.skill_level_to_value(&cap.skill_level) >= level_value)
            .count()
    }

    /// Check text search in name and description
    fn check_text_match(&self, agent: &AgentDefinition, search_text: &str) -> Option<u32> {
        let search_lower = search_text.to_lowercase();
        let mut score = 0u32;

        // Check name match (higher weight)
        if agent.name.to_lowercase().contains(&search_lower) {
            score += 10;
        }

        // Check description match
        if agent.description.to_lowercase().contains(&search_lower) {
            score += 5;
        }

        // Check capabilities match
        for cap in &agent.capabilities {
            if cap.name.to_lowercase().contains(&search_lower)
                || cap.description.to_lowercase().contains(&search_lower)
            {
                score += 3;
            }
        }

        // Check tags match
        for tag in &agent.tags {
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

    /// Convert skill level to numeric value for comparison
    fn skill_level_to_value(&self, level: &SkillLevel) -> u8 {
        match level {
            SkillLevel::Beginner => 1,
            SkillLevel::Intermediate => 2,
            SkillLevel::Advanced => 3,
            SkillLevel::Expert => 4,
        }
    }

    /// Find agents that can work together (have compatible tools and capabilities)
    pub fn find_compatible_agents(
        &self,
        agents: &HashMap<String, AgentDefinition>,
        primary_agent: &str,
    ) -> Vec<AgentMatch> {
        let primary = match agents.get(primary_agent) {
            Some(agent) => agent,
            None => return Vec::new(),
        };
        let mut compatible = Vec::new();

        for (name, agent) in agents {
            if name == primary_agent {
                continue;
            }

            let mut score = 0u32;
            let mut matched_criteria = Vec::new();

            // Check for tool compatibility
            let shared_tools = primary
                .required_tools
                .iter()
                .filter(|tool| agent.required_tools.contains(*tool))
                .count();
            if shared_tools > 0 {
                score += shared_tools as u32 * 10;
                matched_criteria.push(format!("{} shared tools", shared_tools));
            }

            // Check for complementary capabilities
            let complementary_caps = agent
                .capabilities
                .iter()
                .filter(|cap| {
                    !primary
                        .capabilities
                        .iter()
                        .any(|p_cap| p_cap.name == cap.name)
                })
                .count();
            if complementary_caps > 0 {
                score += complementary_caps as u32 * 5;
                matched_criteria.push(format!("{} complementary capabilities", complementary_caps));
            }

            // Check for complementary skills
            let complementary_skills = agent
                .skills
                .iter()
                .filter(|skill| {
                    !primary
                        .skills
                        .iter()
                        .any(|p_skill| p_skill.name == skill.name)
                })
                .count();
            if complementary_skills > 0 {
                score += complementary_skills as u32 * 3;
                matched_criteria.push(format!("{} complementary skills", complementary_skills));
            }

            if score > 0 {
                compatible.push(AgentMatch {
                    agent: agent.clone(),
                    score,
                    matched_criteria,
                    missing_requirements: Vec::new(),
                });
            }
        }

        compatible.sort_by(|a, b| b.score.cmp(&a.score));
        compatible
    }

    /// Suggest agents for a specific task based on task requirements
    pub fn suggest_agents_for_task(
        &self,
        agents: &HashMap<String, AgentDefinition>,
        task_description: &str,
        required_capabilities: &[String],
        preferred_skills: &[String],
    ) -> Vec<AgentMatch> {
        let query = AgentQuery {
            capabilities: required_capabilities.to_vec(),
            skills: preferred_skills.to_vec(),
            tags: Vec::new(),
            required_tools: Vec::new(),
            min_skill_level: Some(SkillLevel::Intermediate),
            status: Some(AgentStatus::Active),
            text_search: Some(task_description.to_string()),
        };

        self.find_matching_agents(agents, &query)
    }
}
