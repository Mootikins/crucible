use anyhow::Result;
use std::path::PathBuf;
use tabled::{Table, Tabled, settings::Style};
use tracing::info;

use crate::cli::AgentCommands;
use crate::config::CliConfig;
use super::enhanced_chat::{EnhancedAgentRegistry, EnhancedAgent};
use super::performance_tracker::{AgentPerformanceTracker, LearningInsights};
use super::collaboration_manager::{CollaborationManager, CollaborationStats};

/// Execute agent management commands
pub async fn execute(config: CliConfig, command: AgentCommands) -> Result<()> {
    match command {
        AgentCommands::List { format, detailed } => {
            execute_list_agents(config, format, detailed).await
        },
        AgentCommands::Rankings { limit, sort_by } => {
            execute_show_rankings(config, limit, sort_by).await
        },
        AgentCommands::Performance { agent_name, insights } => {
            execute_show_performance(config, agent_name, insights).await
        },
        AgentCommands::Suggest { task, capabilities, limit } => {
            execute_suggest_agents(config, task, capabilities, limit).await
        },
        AgentCommands::CollabStats { active, detailed } => {
            execute_collab_stats(config, active, detailed).await
        },
        AgentCommands::Workflows { detailed } => {
            execute_list_workflows(config, detailed).await
        },
    }
}

/// List all available agents
async fn execute_list_agents(config: CliConfig, format: String, detailed: bool) -> Result<()> {
    let mut agent_registry = EnhancedAgentRegistry::new();

    // Add kiln paths for agent discovery
    if let Ok(kiln_path) = config.kiln_path_str() {
        agent_registry.add_kiln_path(std::path::Path::new(&kiln_path));
    }

    // Load agents
    let loaded_count = agent_registry.load_agents().await?;
    if loaded_count == 0 {
        println!("No agents found. Please check your agent configuration.");
        return Ok(());
    }

    let agents = agent_registry.list_enhanced_agents();

    match format.as_str() {
        "json" => {
            let agent_data: Vec<AgentDisplay> = agents.iter().map(|agent| {
                let metrics = agent_registry.get_performance_tracker()
                    .get_metrics(&agent.id());

                AgentDisplay {
                    id: agent.id().to_string(),
                    name: agent.name().to_string(),
                    description: agent.definition.description.clone(),
                    capabilities: agent.definition.capabilities.iter()
                        .map(|cap| cap.name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    success_rate: metrics.map(|m| format!("{:.1}%", m.success_rate * 100.0))
                        .unwrap_or_else(|| "N/A".to_string()),
                    satisfaction: metrics.map(|m| format!("{:.1}", m.avg_user_satisfaction))
                        .unwrap_or_else(|| "N/A".to_string()),
                    tasks_completed: metrics.map(|m| m.total_tasks.to_string())
                        .unwrap_or_else(|| "0".to_string()),
                }
            }).collect();

            println!("{}", serde_json::to_string_pretty(&agent_data)?);
        },
        "table" | "plain" => {
            if detailed {
                println!("🤖 Available Agents (Detailed):\n");
                for agent in agents {
                    println!("Name: {}", agent.name());
                    println!("ID: {}", agent.id());
                    println!("Description: {}", agent.definition.description);
                    println!("Capabilities: {}", agent.definition.capabilities.iter()
                        .map(|cap| &cap.name)
                        .collect::<Vec<_>>()
                        .join(", "));
                    println!("Tags: {}", agent.definition.tags.join(", "));

                    if let Some(metrics) = agent_registry.get_performance_tracker()
                        .get_metrics(&agent.id()) {
                        println!("Performance Metrics:");
                        println!("  Success Rate: {:.1}%", metrics.success_rate * 100.0);
                        println!("  User Satisfaction: {:.1}/1.0", metrics.avg_user_satisfaction);
                        println!("  Tasks Completed: {}", metrics.total_tasks);
                        println!("  Performance Trend: {:?}", metrics.performance_trend);
                        println!("  Specialization Score: {:.1}", metrics.specialization_score);
                        println!("  Reliability Score: {:.1}", metrics.reliability_score);
                    }

                    println!("---\n");
                }
            } else {
                let agent_data: Vec<AgentTableDisplay> = agents.iter().map(|agent| {
                    let metrics = agent_registry.get_performance_tracker()
                        .get_metrics(&agent.id());

                    AgentTableDisplay {
                        name: agent.name().to_string(),
                        capabilities: agent.definition.capabilities.len(),
                        success_rate: metrics.map(|m| format!("{:.1}%", m.success_rate * 100.0))
                            .unwrap_or_else(|| "N/A".to_string()),
                        tasks: metrics.map(|m| m.total_tasks.to_string())
                            .unwrap_or_else(|| "0".to_string()),
                    }
                }).collect();

                let table = Table::new(&agent_data)
                    .with(Style::modern())
                    .to_string();

                println!("🤖 Available Agents:\n");
                println!("{}", table);
            }
        },
        _ => return Err(anyhow::anyhow!("Unsupported format: {}", format)),
    }

    Ok(())
}

/// Show agent rankings
async fn execute_show_rankings(config: CliConfig, limit: usize, sort_by: String) -> Result<()> {
    let mut agent_registry = EnhancedAgentRegistry::new();

    // Add kiln paths for agent discovery
    if let Ok(kiln_path) = config.kiln_path_str() {
        agent_registry.add_kiln_path(std::path::Path::new(&kiln_path));
    }

    // Load agents
    let loaded_count = agent_registry.load_agents().await?;
    if loaded_count == 0 {
        println!("No agents found. Please check your agent configuration.");
        return Ok(());
    }

    let rankings = agent_registry.get_agent_ranking();
    let limited_rankings = rankings.into_iter().take(limit);

    println!("🏆 Agent Rankings (sorted by {}):\n", sort_by);

    for (i, (id, name, score)) in limited_rankings.enumerate() {
        let metrics = agent_registry.get_performance_tracker().get_metrics(&id);

        println!("{}. {} - Overall Score: {:.1}", i + 1, name, score);

        if let Some(metrics) = metrics {
            println!("   Success Rate: {:.1}%", metrics.success_rate * 100.0);
            println!("   User Satisfaction: {:.1}/1.0", metrics.avg_user_satisfaction);
            println!("   Tasks Completed: {}", metrics.total_tasks);
            println!("   Specialization: {:.1}", metrics.specialization_score);
            println!("   Reliability: {:.1}", metrics.reliability_score);
        }

        println!();
    }

    Ok(())
}

/// Show performance insights for an agent
async fn execute_show_performance(config: CliConfig, agent_name: String, show_insights: bool) -> Result<()> {
    let mut agent_registry = EnhancedAgentRegistry::new();

    // Add kiln paths for agent discovery
    if let Ok(kiln_path) = config.kiln_path_str() {
        agent_registry.add_kiln_path(std::path::Path::new(&kiln_path));
    }

    // Load agents
    let loaded_count = agent_registry.load_agents().await?;
    if loaded_count == 0 {
        println!("No agents found. Please check your agent configuration.");
        return Ok(());
    }

    let agent = match agent_registry.get_enhanced_agent_by_name(&agent_name) {
        Some(agent) => agent,
        None => {
            println!("Agent '{}' not found.", agent_name);
            return Ok(());
        }
    };

    println!("📊 Performance Report for: {}\n", agent.name());

    let metrics = agent_registry.get_performance_tracker().get_metrics(&agent.id());

    if let Some(metrics) = metrics {
        println!("Overall Metrics:");
        println!("  Total Tasks Completed: {}", metrics.total_tasks);
        println!("  Success Rate: {:.1}%", metrics.success_rate * 100.0);
        println!("  Average User Satisfaction: {:.1}/1.0", metrics.avg_user_satisfaction);
        println!("  Performance Trend: {:?}", metrics.performance_trend);
        println!("  Specialization Score: {:.1}", metrics.specialization_score);
        println!("  Reliability Score: {:.1}", metrics.reliability_score);

        if !metrics.performance_by_task_type.is_empty() {
            println!("\nPerformance by Task Type:");
            for (task_type, task_metrics) in &metrics.performance_by_task_type {
                println!("  {}:", task_type);
                println!("    Tasks: {}", task_metrics.count);
                println!("    Success Rate: {:.1}%", task_metrics.success_rate * 100.0);
                println!("    Proficiency Score: {:.1}", task_metrics.proficiency_score);
                println!("    Avg Completion Time: {:.1}ms", task_metrics.avg_completion_time_ms);
            }
        }

        if show_insights {
            if let Some(insights) = agent_registry.get_performance_tracker()
                .get_learning_insights(&agent.id()) {

                println!("\n🧠 Learning Insights:");

                if !insights.strengths.is_empty() {
                    println!("  Strengths:");
                    for strength in &insights.strengths {
                        println!("    ✓ {}", strength);
                    }
                }

                if !insights.improvement_areas.is_empty() {
                    println!("  Areas for Improvement:");
                    for area in &insights.improvement_areas {
                        println!("    • {}", area);
                    }
                }

                if !insights.training_recommendations.is_empty() {
                    println!("  Training Recommendations:");
                    for rec in &insights.training_recommendations {
                        println!("    → {}", rec);
                    }
                }
            }
        }
    } else {
        println!("No performance data available for this agent.");
        println!("Performance tracking will begin after the agent completes tasks.");
    }

    Ok(())
}

/// Suggest agents for a task
async fn execute_suggest_agents(
    config: CliConfig,
    task: String,
    required_capabilities: Vec<String>,
    limit: usize
) -> Result<()> {
    let mut agent_registry = EnhancedAgentRegistry::new();

    // Add kiln paths for agent discovery
    if let Ok(kiln_path) = config.kiln_path_str() {
        agent_registry.add_kiln_path(std::path::Path::new(&kiln_path));
    }

    // Load agents
    let loaded_count = agent_registry.load_agents().await?;
    if loaded_count == 0 {
        println!("No agents found. Please check your agent configuration.");
        return Ok(());
    }

    println!("🎯 Finding agents for task: {}\n", task);

    let matches = agent_registry.find_best_agents_for_task(&task, &required_capabilities);
    let limited_matches = matches.into_iter().take(limit);

    if limited_matches.len() == 0 {
        println!("No suitable agents found for this task.");
        if !required_capabilities.is_empty() {
            println!("Required capabilities: {}", required_capabilities.join(", "));
        }
        return Ok(());
    }

    println!("Top Recommendations:\n");

    for (i, agent_match) in limited_matches.enumerate() {
        println!("{}. {} (Match Score: {})", i + 1, agent_match.agent.name, agent_match.score);
        println!("   Description: {}", agent_match.agent.description);

        if !agent_match.matched_criteria.is_empty() {
            println!("   Matched Criteria: {}", agent_match.matched_criteria.join(", "));
        }

        if !agent_match.missing_requirements.is_empty() {
            println!("   Missing Requirements: {}", agent_match.missing_requirements.join(", "));
        }

        // Show performance metrics if available
        if let Some(metrics) = agent_registry.get_performance_tracker()
            .get_metrics(&agent_match.agent.id) {
            println!("   Performance: {:.1}% success rate, {:.1} satisfaction",
                    metrics.success_rate * 100.0, metrics.avg_user_satisfaction);
        }

        println!();
    }

    Ok(())
}

/// Show collaboration statistics
async fn execute_collab_stats(config: CliConfig, show_active: bool, detailed: bool) -> Result<()> {
    let collaboration_manager = CollaborationManager::new();
    let stats = collaboration_manager.get_collaboration_stats();

    println!("🤝 Collaboration Statistics\n");
    println!("Total Sessions: {}", stats.total_sessions);
    println!("Successful Sessions: {}", stats.successful_sessions);
    println!("Success Rate: {:.1}%", stats.success_rate * 100.0);
    println!("Average Quality Score: {:.1}/1.0", stats.avg_quality_score);
    println!("Average Duration: {} minutes", stats.avg_duration_minutes);
    println!("Active Sessions: {}\n", stats.active_sessions);

    if show_active {
        let active_sessions: Vec<_> = collaboration_manager.get_active_sessions().collect();
        if !active_sessions.is_empty() {
            println!("🔄 Active Collaboration Sessions:\n");
            for session in active_sessions {
                println!("Session ID: {}", session.id);
                println!("Task: {}", session.task);
                println!("Participants: {}", session.participant_agents.len());
                println!("Status: {:?}", session.state);
                println!("Started: {}", session.start_time.format("%Y-%m-%d %H:%M:%S"));
                println!("---\n");
            }
        }
    }

    if detailed {
        let history = collaboration_manager.get_collaboration_history();
        if !history.is_empty() {
            println!("📚 Recent Collaboration History:\n");
            for record in history.iter().rev().take(5) {
                println!("Task: {}", record.task);
                println!("Success: {}", record.success);
                println!("Duration: {} minutes", record.duration_minutes);
                println!("Timestamp: {}", record.timestamp.format("%Y-%m-%d %H:%M:%S"));
                if !record.lessons_learned.is_empty() {
                    println!("Lessons: {}", record.lessons_learned.join("; "));
                }
                println!("---\n");
            }
        }
    }

    Ok(())
}

/// List available workflow templates
async fn execute_list_workflows(config: CliConfig, detailed: bool) -> Result<()> {
    let collaboration_manager = CollaborationManager::new();
    let workflows = collaboration_manager.get_workflow_templates();

    if workflows.is_empty() {
        println!("No workflow templates available.");
        return Ok(());
    }

    println!("🔧 Available Workflow Templates:\n");

    for (id, workflow) in workflows {
        if detailed {
            println!("ID: {}", id);
            println!("Name: {}", workflow.name);
            println!("Description: {}", workflow.description);
            println!("Estimated Duration: {} minutes", workflow.estimated_duration_minutes);
            println!("Required Roles: {:?}", workflow.required_roles);
            println!("Tags: {}", workflow.tags.join(", "));
            println!("Steps: {}", workflow.steps.len());

            for (i, step) in workflow.steps.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, step.name, step.step_type);
                println!("     Role: {:?}", step.assigned_role);
                println!("     Est. Duration: {} minutes", step.estimated_duration_minutes);
                if !step.dependencies.is_empty() {
                    println!("     Dependencies: {}", step.dependencies.join(", "));
                }
            }

            println!("Success Criteria:");
            for criteria in &workflow.success_criteria {
                println!("  ✓ {}", criteria);
            }

            println!("---\n");
        } else {
            println!("• {} ({}): {}", workflow.name, id, workflow.description);
            println!("  Duration: {} min, Steps: {}, Roles: {}",
                    workflow.estimated_duration_minutes,
                    workflow.steps.len(),
                    workflow.required_roles.len());
        }
    }

    Ok(())
}

/// Display structure for table formatting
#[derive(Tabled)]
struct AgentTableDisplay {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Capabilities")]
    capabilities: usize,
    #[tabled(rename = "Success Rate")]
    success_rate: String,
    #[tabled(rename = "Tasks")]
    tasks: String,
}

/// Display structure for JSON output
#[derive(serde::Serialize)]
struct AgentDisplay {
    id: String,
    name: String,
    description: String,
    capabilities: String,
    success_rate: String,
    satisfaction: String,
    tasks_completed: String,
}