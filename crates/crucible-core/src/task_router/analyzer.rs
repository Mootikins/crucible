use anyhow::Result;
use chrono::Utc;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::types::*;

/// Task Analysis Engine - analyzes user requests and breaks them down into manageable subtasks
#[derive(Debug)]
pub struct TaskAnalyzer {
    /// Patterns for identifying different task types
    task_patterns: HashMap<String, Vec<Regex>>,
    /// Capability keyword mappings
    capability_keywords: HashMap<String, Vec<String>>,
    /// Complexity factors
    complexity_factors: ComplexityFactors,
}

/// Factors for determining task complexity
#[derive(Debug)]
struct ComplexityFactors {
    /// Keywords indicating high complexity
    high_complexity_keywords: Vec<String>,
    /// Keywords indicating multi-step processes
    multi_step_keywords: Vec<String>,
    /// Keywords indicating research needs
    research_keywords: Vec<String>,
    /// Keywords indicating code work
    code_keywords: Vec<String>,
    /// Keywords indicating collaboration
    collaboration_keywords: Vec<String>,
}

impl TaskAnalyzer {
    /// Create a new task analyzer
    pub fn new() -> Self {
        let mut analyzer = Self {
            task_patterns: HashMap::new(),
            capability_keywords: HashMap::new(),
            complexity_factors: ComplexityFactors::new(),
        };

        analyzer.initialize_patterns();
        analyzer
    }

    /// Initialize task patterns and capability mappings
    fn initialize_patterns(&mut self) {
        // Task type patterns
        self.task_patterns.insert("research".to_string(), vec![
            Regex::new(r"(?i)(research|investigate|analyze|study|explore|find|look up|search for)").unwrap(),
            Regex::new(r"(?i)(what is|how does|why does|when did|where can)").unwrap(),
        ]);

        self.task_patterns.insert("code_generation".to_string(), vec![
            Regex::new(r"(?i)(write|create|generate|implement|build|develop).{0,20}(code|function|class|script|program)").unwrap(),
            Regex::new(r"(?i)(code|function|class|script|program).{0,20}(write|create|generate)").unwrap(),
        ]);

        self.task_patterns.insert("analysis".to_string(), vec![
            Regex::new(r"(?i)(analyze|review|examine|evaluate|assess|audit)").unwrap(),
            Regex::new(r"(?i)(what's wrong|how to improve|find issues|debug|troubleshoot)").unwrap(),
        ]);

        self.task_patterns.insert("writing".to_string(), vec![
            Regex::new(r"(?i)(write|create|draft|compose|document|summarize)").unwrap(),
            Regex::new(r"(?i)(article|report|documentation|summary|email|message)").unwrap(),
        ]);

        self.task_patterns.insert("collaboration".to_string(), vec![
            Regex::new(r"(?i)(work together|collaborate|team|group|multiple agents|coordinate)").unwrap(),
            Regex::new(r"(?i)(help me with|assist with|support on)").unwrap(),
        ]);

        // Capability keywords
        self.capability_keywords.insert("coding".to_string(), vec![
            "code".to_string(), "programming".to_string(), "function".to_string(),
            "class".to_string(), "algorithm".to_string(), "debug".to_string(),
            "rust".to_string(), "javascript".to_string(), "python".to_string(),
        ]);

        self.capability_keywords.insert("research".to_string(), vec![
            "research".to_string(), "investigate".to_string(), "analyze".to_string(),
            "study".to_string(), "explore".to_string(), "find".to_string(),
        ]);

        self.capability_keywords.insert("writing".to_string(), vec![
            "write".to_string(), "document".to_string(), "summarize".to_string(),
            "draft".to_string(), "compose".to_string(), "article".to_string(),
        ]);

        self.capability_keywords.insert("data_analysis".to_string(), vec![
            "data".to_string(), "analyze".to_string(), "database".to_string(),
            "query".to_string(), "statistics".to_string(), "metrics".to_string(),
        ]);

        self.capability_keywords.insert("collaboration".to_string(), vec![
            "collaborate".to_string(), "coordinate".to_string(), "team".to_string(),
            "group".to_string(), "together".to_string(), "multiple".to_string(),
        ]);
    }

    /// Analyze a user request and break it down into subtasks
    pub async fn analyze_request(&self, request: &UserRequest) -> Result<TaskAnalysis> {
        // Step 1: Identify task types and complexity
        let task_types = self.identify_task_types(&request.content);
        let complexity = self.assess_complexity(&request.content, &task_types);

        // Step 2: Extract required capabilities
        let required_capabilities = self.extract_required_capabilities(&request.content);

        // Step 3: Generate subtasks
        let subtasks = self.generate_subtasks(request, &task_types, &required_capabilities, &complexity)?;

        // Step 4: Identify dependencies between subtasks
        let dependencies = self.identify_dependencies(&subtasks);

        // Step 5: Recommend execution strategy
        let execution_strategy = self.recommend_execution_strategy(&subtasks, &dependencies, &complexity);

        // Step 6: Estimate duration
        let estimated_duration = self.estimate_duration(&subtasks);

        let confidence = self.calculate_analysis_confidence(&subtasks, &complexity);

        Ok(TaskAnalysis {
            request_id: request.id,
            subtasks,
            required_capabilities,
            complexity,
            estimated_duration_minutes: estimated_duration,
            dependencies,
            execution_strategy,
            confidence,
            timestamp: Utc::now(),
        })
    }

    /// Identify the types of tasks in the request
    fn identify_task_types(&self, content: &str) -> Vec<String> {
        let mut task_types = HashSet::new();

        for (task_type, patterns) in &self.task_patterns {
            for pattern in patterns {
                if pattern.is_match(content) {
                    task_types.insert(task_type.clone());
                    break;
                }
            }
        }

        task_types.into_iter().collect()
    }

    /// Assess task complexity
    fn assess_complexity(&self, content: &str, task_types: &[String]) -> TaskComplexity {
        let content_lower = content.to_lowercase();
        let mut score = 1u8;
        let mut skill_diversity = task_types.len() as u8;
        let mut coordination_complexity = 1u8;
        let mut technical_difficulty = 1u8;
        let mut ambiguity_level = 1u8;

        // Check for high complexity indicators
        for keyword in &self.complexity_factors.high_complexity_keywords {
            if content_lower.contains(keyword) {
                score = (score + 1).min(10);
                technical_difficulty = (technical_difficulty + 1).min(10);
            }
        }

        // Check for multi-step processes
        for keyword in &self.complexity_factors.multi_step_keywords {
            if content_lower.contains(keyword) {
                score = (score + 1).min(10);
                coordination_complexity = (coordination_complexity + 1).min(10);
            }
        }

        // Check for research needs
        for keyword in &self.complexity_factors.research_keywords {
            if content_lower.contains(keyword) {
                skill_diversity = (skill_diversity + 1).min(10);
                ambiguity_level = (ambiguity_level + 1).min(10);
            }
        }

        // Check for code work
        for keyword in &self.complexity_factors.code_keywords {
            if content_lower.contains(keyword) {
                technical_difficulty = (technical_difficulty + 1).min(10);
            }
        }

        // Check for collaboration needs
        for keyword in &self.complexity_factors.collaboration_keywords {
            if content_lower.contains(keyword) {
                coordination_complexity = (coordination_complexity + 1).min(10);
            }
        }

        // Adjust score based on content length and structure
        let word_count = content.split_whitespace().count();
        if word_count > 100 {
            score = (score + 1).min(10);
        }
        if word_count > 200 {
            score = (score + 1).min(10);
        }

        // Check for multiple questions or requests
        let question_count = content.matches('?').count();
        if question_count > 1 {
            coordination_complexity = (coordination_complexity + question_count as u8).min(10);
        }

        TaskComplexity {
            score,
            skill_diversity,
            coordination_complexity,
            technical_difficulty,
            ambiguity_level,
        }
    }

    /// Extract required capabilities from the request
    fn extract_required_capabilities(&self, content: &str) -> Vec<String> {
        let content_lower = content.to_lowercase();
        let mut capabilities = HashSet::new();

        for (capability, keywords) in &self.capability_keywords {
            for keyword in keywords {
                if content_lower.contains(keyword) {
                    capabilities.insert(capability.clone());
                    break;
                }
            }
        }

        capabilities.into_iter().collect()
    }

    /// Generate subtasks based on analysis
    fn generate_subtasks(&self, request: &UserRequest, task_types: &[String],
                        required_capabilities: &[String], complexity: &TaskComplexity) -> Result<Vec<Subtask>> {
        let mut subtasks = Vec::new();

        // Determine if we need to break down into multiple subtasks
        let should_break_down = complexity.score > 3 || task_types.len() > 1 || request.content.len() > 200;

        if should_break_down {
            // Generate specific subtasks based on task types
            for task_type in task_types {
                match task_type.as_str() {
                    "research" => {
                        subtasks.push(self.create_research_subtask(request)?);
                    }
                    "code_generation" => {
                        subtasks.push(self.create_code_generation_subtask(request)?);
                    }
                    "analysis" => {
                        subtasks.push(self.create_analysis_subtask(request)?);
                    }
                    "writing" => {
                        subtasks.push(self.create_writing_subtask(request)?);
                    }
                    _ => {
                        // Generic subtask for unknown types
                        subtasks.push(self.create_generic_subtask(request, task_type)?);
                    }
                }
            }

            // Add coordination subtask if multiple agents needed
            if subtasks.len() > 1 {
                subtasks.insert(0, self.create_coordination_subtask(request)?);
            }
        } else {
            // Single task for simple requests
            let subtask_type = self.determine_primary_subtask_type(task_types);
            subtasks.push(self.create_single_subtask(request, &subtask_type, required_capabilities)?);
        }

        Ok(subtasks)
    }

    /// Create a research subtask
    fn create_research_subtask(&self, request: &UserRequest) -> Result<Subtask> {
        Ok(Subtask {
            id: Uuid::new_v4(),
            description: format!("Research and gather information for: {}",
                               request.content.chars().take(100).collect::<String>()),
            subtask_type: SubtaskType::Research,
            required_capabilities: vec!["research".to_string()],
            required_tools: vec!["search".to_string(), "web_fetch".to_string()],
            estimated_duration_minutes: 15,
            priority: request.priority.clone(),
            can_parallelize: true,
            input_requirements: vec!["search query".to_string()],
            expected_output: "Research findings and relevant information".to_string(),
        })
    }

    /// Create a code generation subtask
    fn create_code_generation_subtask(&self, request: &UserRequest) -> Result<Subtask> {
        Ok(Subtask {
            id: Uuid::new_v4(),
            description: format!("Generate code for: {}",
                               request.content.chars().take(100).collect::<String>()),
            subtask_type: SubtaskType::CodeGeneration,
            required_capabilities: vec!["coding".to_string()],
            required_tools: vec!["file_operations".to_string(), "code_editor".to_string()],
            estimated_duration_minutes: 20,
            priority: request.priority.clone(),
            can_parallelize: false,
            input_requirements: vec!["requirements".to_string(), "specifications".to_string()],
            expected_output: "Functional code implementation".to_string(),
        })
    }

    /// Create an analysis subtask
    fn create_analysis_subtask(&self, request: &UserRequest) -> Result<Subtask> {
        Ok(Subtask {
            id: Uuid::new_v4(),
            description: format!("Analyze: {}",
                               request.content.chars().take(100).collect::<String>()),
            subtask_type: SubtaskType::Analysis,
            required_capabilities: vec!["analysis".to_string(), "critical_thinking".to_string()],
            required_tools: vec!["data_analysis".to_string()],
            estimated_duration_minutes: 12,
            priority: request.priority.clone(),
            can_parallelize: true,
            input_requirements: vec!["data_to_analyze".to_string()],
            expected_output: "Analysis results and insights".to_string(),
        })
    }

    /// Create a writing subtask
    fn create_writing_subtask(&self, request: &UserRequest) -> Result<Subtask> {
        Ok(Subtask {
            id: Uuid::new_v4(),
            description: format!("Write: {}",
                               request.content.chars().take(100).collect::<String>()),
            subtask_type: SubtaskType::Writing,
            required_capabilities: vec!["writing".to_string()],
            required_tools: vec!["text_editor".to_string()],
            estimated_duration_minutes: 18,
            priority: request.priority.clone(),
            can_parallelize: false,
            input_requirements: vec!["topic".to_string(), "requirements".to_string()],
            expected_output: "Well-written content".to_string(),
        })
    }

    /// Create a generic subtask
    fn create_generic_subtask(&self, request: &UserRequest, task_type: &str) -> Result<Subtask> {
        Ok(Subtask {
            id: Uuid::new_v4(),
            description: format!("Handle {} task: {}", task_type,
                               request.content.chars().take(100).collect::<String>()),
            subtask_type: self.map_task_type_to_subtask_type(task_type),
            required_capabilities: vec![task_type.to_string()],
            required_tools: Vec::new(),
            estimated_duration_minutes: 10,
            priority: request.priority.clone(),
            can_parallelize: true,
            input_requirements: vec!["task_details".to_string()],
            expected_output: "Task completion".to_string(),
        })
    }

    /// Create a coordination subtask
    fn create_coordination_subtask(&self, _request: &UserRequest) -> Result<Subtask> {
        Ok(Subtask {
            id: Uuid::new_v4(),
            description: "Coordinate multi-agent task execution".to_string(),
            subtask_type: SubtaskType::Coordination,
            required_capabilities: vec!["coordination".to_string(), "planning".to_string()],
            required_tools: vec!["collaboration_tools".to_string()],
            estimated_duration_minutes: 5,
            priority: TaskPriority::High,
            can_parallelize: false,
            input_requirements: vec!["task_plan".to_string(), "agent_list".to_string()],
            expected_output: "Coordinated task execution plan".to_string(),
        })
    }

    /// Create a single subtask for simple requests
    fn create_single_subtask(&self, request: &UserRequest, subtask_type: &str,
                           required_capabilities: &[String]) -> Result<Subtask> {
        Ok(Subtask {
            id: Uuid::new_v4(),
            description: request.content.clone(),
            subtask_type: self.map_task_type_to_subtask_type(subtask_type),
            required_capabilities: required_capabilities.to_vec(),
            required_tools: self.determine_required_tools(subtask_type),
            estimated_duration_minutes: 10,
            priority: request.priority.clone(),
            can_parallelize: true,
            input_requirements: vec!["user_request".to_string()],
            expected_output: "Task completion result".to_string(),
        })
    }

    /// Map task type string to SubtaskType enum
    fn map_task_type_to_subtask_type(&self, task_type: &str) -> SubtaskType {
        match task_type {
            "research" => SubtaskType::Research,
            "code_generation" => SubtaskType::CodeGeneration,
            "analysis" => SubtaskType::Analysis,
            "writing" => SubtaskType::Writing,
            "collaboration" => SubtaskType::Coordination,
            _ => SubtaskType::Analysis, // Default
        }
    }

    /// Determine primary subtask type
    fn determine_primary_subtask_type(&self, task_types: &[String]) -> String {
        if task_types.is_empty() {
            return "analysis".to_string();
        }

        // Priority order for selecting primary type
        let priority_order = vec![
            "code_generation", "research", "analysis", "writing", "collaboration"
        ];

        for preferred_type in priority_order {
            if task_types.contains(&preferred_type.to_string()) {
                return preferred_type.to_string();
            }
        }

        task_types[0].clone()
    }

    /// Determine required tools for a subtask type
    fn determine_required_tools(&self, subtask_type: &str) -> Vec<String> {
        match subtask_type {
            "code_generation" => vec!["file_operations".to_string(), "code_editor".to_string()],
            "research" => vec!["search".to_string(), "web_fetch".to_string()],
            "analysis" => vec!["data_analysis".to_string()],
            "writing" => vec!["text_editor".to_string()],
            "collaboration" => vec!["collaboration_tools".to_string()],
            _ => Vec::new(),
        }
    }

    /// Identify dependencies between subtasks
    fn identify_dependencies(&self, subtasks: &[Subtask]) -> Vec<TaskDependency> {
        let mut dependencies = Vec::new();

        // If there's a coordination subtask, others depend on it
        if let Some(coordination_task) = subtasks.iter().find(|st| matches!(st.subtask_type, SubtaskType::Coordination)) {
            for subtask in subtasks {
                if subtask.id != coordination_task.id {
                    dependencies.push(TaskDependency {
                        dependent_id: subtask.id,
                        prerequisite_id: coordination_task.id,
                        dependency_type: DependencyType::FinishToStart,
                        description: "Coordination must complete before other tasks start".to_string(),
                    });
                }
            }
        }

        // Research tasks often need to complete before other tasks
        let research_tasks: Vec<_> = subtasks.iter()
            .filter(|st| matches!(st.subtask_type, SubtaskType::Research))
            .collect();

        let non_research_tasks: Vec<_> = subtasks.iter()
            .filter(|st| !matches!(st.subtask_type, SubtaskType::Research))
            .collect();

        for research_task in &research_tasks {
            for other_task in &non_research_tasks {
                dependencies.push(TaskDependency {
                    dependent_id: other_task.id,
                    prerequisite_id: research_task.id,
                    dependency_type: DependencyType::DataDependency,
                    description: "Research findings needed for other tasks".to_string(),
                });
            }
        }

        dependencies
    }

    /// Recommend execution strategy
    fn recommend_execution_strategy(&self, subtasks: &[Subtask], dependencies: &[TaskDependency],
                                 complexity: &TaskComplexity) -> ExecutionStrategy {
        if subtasks.len() == 1 {
            return ExecutionStrategy::SingleAgent;
        }

        let can_parallelize = subtasks.iter().all(|st| st.can_parallelize) && dependencies.is_empty();
        let needs_collaboration = subtasks.iter().any(|st| matches!(st.subtask_type, SubtaskType::Coordination));

        if can_parallelize && complexity.coordination_complexity < 5 {
            ExecutionStrategy::ParallelExecution
        } else if needs_collaboration || complexity.coordination_complexity >= 7 {
            ExecutionStrategy::Collaborative
        } else if dependencies.len() > subtasks.len() / 2 {
            ExecutionStrategy::SequentialMultiAgent
        } else {
            ExecutionStrategy::Hybrid
        }
    }

    /// Estimate total duration
    fn estimate_duration(&self, subtasks: &[Subtask]) -> u32 {
        subtasks.iter().map(|st| st.estimated_duration_minutes).sum()
    }

    /// Calculate analysis confidence
    fn calculate_analysis_confidence(&self, subtasks: &[Subtask], complexity: &TaskComplexity) -> f32 {
        let mut confidence = 0.8; // Base confidence

        // Higher complexity reduces confidence
        confidence -= (complexity.score as f32 / 10.0) * 0.2;

        // More subtasks increases confidence (better breakdown)
        if subtasks.len() > 1 {
            confidence += 0.1;
        }

        // High ambiguity reduces confidence
        confidence -= (complexity.ambiguity_level as f32 / 10.0) * 0.3;

        confidence.max(0.1).min(1.0)
    }
}

impl ComplexityFactors {
    /// Create new complexity factors
    fn new() -> Self {
        Self {
            high_complexity_keywords: vec![
                "complex".to_string(), "difficult".to_string(), "advanced".to_string(),
                "sophisticated".to_string(), "intricate".to_string(), "challenging".to_string(),
            ],
            multi_step_keywords: vec![
                "step by step".to_string(), "process".to_string(), "workflow".to_string(),
                "multiple".to_string(), "several".to_string(), "various".to_string(),
                "then".to_string(), "after that".to_string(), "next".to_string(),
            ],
            research_keywords: vec![
                "research".to_string(), "investigate".to_string(), "study".to_string(),
                "explore".to_string(), "find out".to_string(), "learn about".to_string(),
            ],
            code_keywords: vec![
                "code".to_string(), "programming".to_string(), "function".to_string(),
                "class".to_string(), "algorithm".to_string(), "debug".to_string(),
                "implement".to_string(), "develop".to_string(),
            ],
            collaboration_keywords: vec![
                "collaborate".to_string(), "team".to_string(), "together".to_string(),
                "multiple".to_string(), "coordinate".to_string(), "cooperate".to_string(),
            ],
        }
    }
}

impl Default for TaskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}