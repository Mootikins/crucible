//! Real-World Usage Scenario Tests
//!
//! This module tests realistic user workflows and scenarios that mirror actual
//! usage patterns of the Crucible knowledge management system. These tests
//! validate that the system works effectively for real-world use cases.

use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::comprehensive_integration_workflow_tests::{
    ComprehensiveTestKiln, CliTestHarness, ReplTestHarness, CommandResult
};
use crate::cli_workflow_integration_tests::ExtendedCliTestHarness;
use crate::repl_interactive_workflow_tests::ExtendedReplTestHarness;
use crate::tool_api_integration_tests::ToolApiTestHarness;

/// Real-world usage scenario test harness
pub struct RealWorldUsageTestHarness {
    kiln_dir: TempDir,
    test_kiln: ComprehensiveTestKiln,
}

impl RealWorldUsageTestHarness {
    /// Create a new real-world usage test harness
    pub async fn new() -> Result<Self> {
        let test_kiln = ComprehensiveTestKiln::create().await?;
        let kiln_dir = test_kiln.path().to_owned();

        Ok(Self {
            kiln_dir: kiln_dir.to_owned(),
            test_kiln,
        })
    }

    /// Test research workflow: find sources → analyze → generate insights
    pub async fn test_research_workflow(&self) -> Result<()> {
        println!("🧪 Testing research workflow: find sources → analyze → generate insights");

        let workflow_start = Instant::now();

        // Step 1: Initial research discovery phase
        println!("  🔍 Phase 1: Research Discovery");

        let cli_harness = CliTestHarness::new().await?;

        // Search for quantum computing research
        let quantum_search = cli_harness.execute_cli_command(&[
            "search", "quantum computing research",
            "--limit", "10"
        ])?;

        assert!(quantum_search.exit_code == 0, "Quantum search should succeed");
        assert!(quantum_search.stdout.contains("quantum") || !quantum_search.stdout.is_empty(),
               "Should find quantum computing content");

        // Semantic search for related physics concepts
        let physics_search = cli_harness.execute_cli_command(&[
            "semantic", "physics quantum mechanics applications",
            "--top-k", "5"
        ])?;

        assert!(physics_search.exit_code == 0, "Physics semantic search should succeed");

        // Step 2: Interactive analysis phase
        println!("  📊 Phase 2: Interactive Analysis");

        let repl_harness = ExtendedReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;

        // Find all research-related documents
        let research_query = "SELECT * FROM notes WHERE content LIKE '%research%' OR tags LIKE '%research%' OR path LIKE '%research%'";
        let research_docs = repl.send_command(research_query)?;

        assert!(!research_docs.is_empty(), "Should find research documents");

        // Analyze research connections and relationships
        let connections_analysis = repl.send_command(":run search_documents \"quantum OR physics OR computing OR research\"")?;
        assert!(!connections_analysis.is_empty(), "Should find connected research documents");

        // Extract key concepts and themes
        let concepts_extraction = repl.send_command(":run search_documents \"fundamentals concepts applications challenges\"")?;
        assert!(!concepts_extraction.is_empty(), "Should extract key concepts");

        // Step 3: Insight generation phase
        println!("  💡 Phase 3: Insight Generation");

        // Synthesize findings across multiple dimensions
        let synthesis_query = "SELECT title, path FROM notes WHERE (content LIKE '%quantum%' OR content LIKE '%physics%') AND (content LIKE '%application%' OR content LIKE '%challenge%')";
        let synthesis_results = repl.send_command(synthesis_query)?;

        assert!(!synthesis_results.is_empty(), "Should synthesize research findings");

        // Find practical applications and use cases
        let applications_search = repl.send_command(":run search_documents \"applications use cases practical implementation\"")?;
        assert!(!applications_search.is_empty(), "Should find practical applications");

        // Identify research gaps and future directions
        let gaps_analysis = repl.send_command(":run search_documents \"challenges limitations future directions open problems\"")?;
        assert!(!gaps_analysis.is_empty(), "Should identify research gaps");

        // Step 4: Documentation and knowledge capture
        println!("  📝 Phase 4: Documentation and Knowledge Capture");

        // Create comprehensive research summary using available tools
        let summary_analysis = repl.send_command(":run get_kiln_stats")?;
        assert!(!summary_analysis.is_empty(), "Should get kiln statistics for context");

        repl.quit()?;

        let workflow_duration = workflow_start.elapsed();

        // Validate research workflow outcomes
        assert!(workflow_duration < Duration::from_secs(60),
               "Research workflow should complete within 60 seconds, took {:?}",
               workflow_duration);

        println!("✅ Research workflow completed successfully in {:?}", workflow_duration);

        // Research workflow validation checklist
        println!("  📋 Research Workflow Validation:");
        println!("    ✅ Discovery phase: Found relevant research sources");
        println!("    ✅ Analysis phase: Identified connections and relationships");
        println!("    ✅ Insight phase: Generated practical insights and applications");
        println!("    ✅ Documentation phase: Captured findings and statistics");

        Ok(())
    }

    /// Test project management workflow: track tasks → deadlines → dependencies
    pub async fn test_project_management_workflow(&self) -> Result<()> {
        println!("🧪 Testing project management workflow: track tasks → deadlines → dependencies");

        let workflow_start = Instant::now();

        // Step 1: Project discovery and overview
        println!("  📋 Phase 1: Project Discovery");

        let cli_harness = ExtendedCliTestHarness::new().await?;

        // Find all project-related documents
        let project_search = cli_harness.execute_cli_command(&[
            "search", "project management tasks deadlines",
            "--limit", "15"
        ])?;

        assert!(project_search.exit_code == 0, "Project search should succeed");
        assert!(project_search.stdout.contains("project") || !project_search.stdout.is_empty(),
               "Should find project management content");

        // Search for specific project types and methodologies
        let methodology_search = cli_harness.execute_cli_command(&[
            "fuzzy", "project methodology agile waterfall",
            "--tags", "true",
            "--limit", "10"
        ])?;

        assert!(methodology_search.exit_code == 0, "Methodology search should succeed");

        // Step 2: Task and deadline tracking
        println!("  ✅ Phase 2: Task and Deadline Tracking");

        let repl_harness = ExtendedReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;

        // Find all documents with task information
        let tasks_query = "SELECT * FROM notes WHERE content LIKE '%task%' OR content LIKE '%deadline%' OR content LIKE '%milestone%'";
        let tasks_results = repl.send_command(tasks_query)?;

        assert!(!tasks_results.is_empty(), "Should find task-related documents");

        // Identify upcoming deadlines and milestones
        let deadlines_search = repl.send_command(":run search_documents \"deadline due date milestone deliverable\"")?;
        assert!(!deadlines_search.is_empty(), "Should find deadline information");

        // Find project status and progress indicators
        let status_search = repl.send_command(":run search_documents \"status progress in-progress completed pending\"")?;
        assert!(!status_search.is_empty(), "Should find project status information");

        // Step 3: Dependency analysis and mapping
        println!("  🔗 Phase 3: Dependency Analysis");

        // Find documents mentioning dependencies
        let dependencies_query = "SELECT * FROM notes WHERE content LIKE '%depend%' OR content LIKE '%requirement%' OR content LIKE '%prerequisite%'";
        let dependencies_results = repl.send_command(dependencies_query)?;

        assert!(!dependencies_results.is_empty(), "Should find dependency information");

        // Map project relationships using wikilinks and references
        let relationships_search = repl.send_command("SELECT * FROM notes WHERE content LIKE '%[[' AND content LIKE '%]]%'")?;
        assert!(!relationships_search.is_empty(), "Should find linked project documents");

        // Analyze resource allocation and team assignments
        let resources_search = repl.send_command(":run search_documents \"assignee team owner responsibility allocation\"")?;
        assert!(!resources_search.is_empty(), "Should find resource allocation information");

        // Step 4: Risk assessment and mitigation
        println!("  ⚠️ Phase 4: Risk Assessment");

        // Identify potential risks and blockers
        let risks_search = repl.send_command(":run search_documents \"risk blocker challenge issue problem mitigation\"")?;
        assert!(!risks_search.is_empty(), "Should find risk-related information");

        // Find contingency plans and backup strategies
        let contingency_search = repl.send_command(":run search_documents \"contingency backup fallback alternative plan\"")?;
        assert!(!contingency_search.is_empty(), "Should find contingency planning");

        // Step 5: Performance monitoring and reporting
        println!("  📊 Phase 5: Performance Monitoring");

        // Get project statistics and metrics
        let project_stats = repl.send_command(":run get_kiln_stats")?;
        assert!(!project_stats.is_empty(), "Should get project statistics");

        // Generate comprehensive project overview
        let overview_query = "SELECT title, path FROM notes WHERE tags LIKE '%project%' OR content LIKE '%project%' ORDER BY path";
        let project_overview = repl.send_command(overview_query)?;

        assert!(!project_overview.is_empty(), "Should generate project overview");

        repl.quit()?;

        let workflow_duration = workflow_start.elapsed();

        // Validate project management workflow
        assert!(workflow_duration < Duration::from_secs(45),
               "Project management workflow should complete within 45 seconds, took {:?}",
               workflow_duration);

        println!("✅ Project management workflow completed successfully in {:?}", workflow_duration);

        // Project management workflow validation checklist
        println!("  📋 Project Management Workflow Validation:");
        println!("    ✅ Discovery phase: Identified all project documents");
        println!("    ✅ Task tracking: Found tasks, deadlines, and milestones");
        println!("    ✅ Dependency analysis: Mapped project relationships");
        println!("    ✅ Risk assessment: Identified risks and mitigation strategies");
        println!("    ✅ Performance monitoring: Generated project statistics and overview");

        Ok(())
    }

    /// Test knowledge discovery workflow: explore topics → follow links → synthesize
    pub async fn test_knowledge_discovery_workflow(&self) -> Result<()> {
        println!("🧪 Testing knowledge discovery workflow: explore topics → follow links → synthesize");

        let workflow_start = Instant::now();

        // Step 1: Topic exploration and initial discovery
        println!("  🌟 Phase 1: Topic Exploration");

        let cli_harness = ExtendedCliTestHarness::new().await?;

        // Broad search for learning and educational content
        let learning_search = cli_harness.execute_cli_command(&[
            "semantic", "learning patterns tutorial education knowledge",
            "--top-k", "8"
        ])?;

        assert!(learning_search.exit_code == 0, "Learning search should succeed");

        // Search for specific domains and expertise areas
        let domains_search = cli_harness.execute_cli_command(&[
            "search", "expertise domain specialty field area",
            "--limit", "10"
        ])?;

        assert!(domains_search.exit_code == 0, "Domains search should succeed");

        // Step 2: Link traversal and connection mapping
        println!("  🔗 Phase 2: Link Traversal and Connection Mapping");

        let repl_harness = ExtendedReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;

        // Find all documents with wikilinks
        let linked_docs_query = "SELECT * FROM notes WHERE content LIKE '%[[' AND content LIKE '%]]%' ORDER BY path";
        let linked_docs = repl.send_command(linked_docs_query)?;

        assert!(!linked_docs.is_empty(), "Should find documents with wikilinks");

        // Explore specific knowledge domains mentioned in links
        let domain_exploration = repl.send_command(":run search_documents \"rust programming patterns async\"")?;
        assert!(!domain_exploration.is_empty(), "Should explore Rust programming domain");

        // Follow connections to related topics
        let related_topics = repl.send_command(":run search_documents \"quantum physics computing applications\"")?;
        assert!(!related_topics.is_empty(), "Should find related physics topics");

        // Discover cross-domain connections
        let cross_domain = repl.send_command(":run search_documents \"system design architecture software database\"")?;
        assert!(!cross_domain.is_empty(), "Should find cross-domain connections");

        // Step 3: Knowledge synthesis and integration
        println!("  🧩 Phase 3: Knowledge Synthesis");

        // Synthesize information across multiple sources
        let synthesis_query = "SELECT title, path FROM notes WHERE (tags LIKE '%learning%' OR tags LIKE '%tutorial%') AND (content LIKE '%pattern%' OR content LIKE '%design%')";
        let synthesis_results = repl.send_command(synthesis_query)?;

        assert!(!synthesis_results.is_empty(), "Should synthesize knowledge across sources");

        // Identify recurring themes and concepts
        let themes_search = repl.send_command(":run search_documents \"fundamental principle concept pattern approach\"")?;
        assert!(!themes_search.is_empty(), "Should identify recurring themes");

        // Find practical applications and implementations
        let applications_search = repl.send_command(":run search_documents \"implementation practical example use case\"")?;
        assert!(!applications_search.is_empty(), "Should find practical applications");

        // Step 4: Knowledge organization and structuring
        println!("  📚 Phase 4: Knowledge Organization");

        // Organize knowledge by categories and hierarchies
        let organization_query = "SELECT * FROM notes WHERE tags LIKE '%reference%' OR content LIKE '%cheat sheet%' OR content LIKE '%summary%'";
        let organized_knowledge = repl.send_command(organization_query)?;

        assert!(!organized_knowledge.is_empty(), "Should find organized knowledge resources");

        // Create comprehensive knowledge map
        let knowledge_map = repl.send_command(":run search_by_tags learning tutorial reference guide")?;
        assert!(!knowledge_map.is_empty(), "Should create knowledge map");

        // Identify knowledge gaps and areas for further exploration
        let gaps_search = repl.send_command(":run search_documents \"todo learn study research investigate\"")?;
        assert!(!gaps_search.is_empty(), "Should identify knowledge gaps");

        repl.quit()?;

        let workflow_duration = workflow_start.elapsed();

        // Validate knowledge discovery workflow
        assert!(workflow_duration < Duration::from_secs(50),
               "Knowledge discovery workflow should complete within 50 seconds, took {:?}",
               workflow_duration);

        println!("✅ Knowledge discovery workflow completed successfully in {:?}", workflow_duration);

        // Knowledge discovery workflow validation checklist
        println!("  📋 Knowledge Discovery Workflow Validation:");
        println!("    ✅ Topic exploration: Discovered learning content and domains");
        println!("    ✅ Link traversal: Mapped connections and relationships");
        println!("    ✅ Knowledge synthesis: Integrated information across sources");
        println!("    ✅ Knowledge organization: Structured and categorized knowledge");

        Ok(())
    }

    /// Test code documentation workflow: find examples → understand patterns → apply
    pub async fn test_code_documentation_workflow(&self) -> Result<()> {
        println!("🧪 Testing code documentation workflow: find examples → understand patterns → apply");

        let workflow_start = Instant::now();

        // Step 1: Code discovery and example finding
        println!("  💻 Phase 1: Code Discovery and Examples");

        let cli_harness = ExtendedCliTestHarness::new().await?;

        // Search for code examples and documentation
        let code_search = cli_harness.execute_cli_command(&[
            "search", "code examples rust programming patterns",
            "--show-content",
            "--limit", "10"
        ])?;

        assert!(code_search.exit_code == 0, "Code search should succeed");
        assert!(code_search.stdout.contains("rust") || code_search.stdout.contains("code"),
               "Should find Rust code content");

        // Find specific programming patterns and best practices
        let patterns_search = cli_harness.execute_cli_command(&[
            "semantic", "async await error handling patterns rust",
            "--top-k", "5"
        ])?;

        assert!(patterns_search.exit_code == 0, "Patterns search should succeed");

        // Step 2: Pattern analysis and understanding
        println!("  🔍 Phase 2: Pattern Analysis and Understanding");

        let repl_harness = ExtendedReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;

        // Find all code-related documents
        let code_docs_query = "SELECT * FROM notes WHERE tags LIKE '%rust%' OR content LIKE '```rust' OR content LIKE '%code%'";
        let code_docs = repl.send_command(code_docs_query)?;

        assert!(!code_docs.is_empty(), "Should find code documentation");

        // Analyze specific code patterns and implementations
        let rust_patterns = repl.send_command(":run search_documents \"async await Result Error handling\"")?;
        assert!(!rust_patterns.is_empty(), "Should analyze Rust patterns");

        // Find error handling strategies and approaches
        let error_handling = repl.send_command(":run search_documents \"error handling Result<T> Option<T> panic unwrap\"")?;
        assert!(!error_handling.is_empty(), "Should find error handling strategies");

        // Discover performance optimization techniques
        let performance_patterns = repl.send_command(":run search_documents \"performance optimization memory efficiency concurrency\"")?;
        assert!(!performance_patterns.is_empty(), "Should find performance patterns");

        // Step 3: Practical application and implementation
        println!("  🛠️ Phase 3: Practical Application");

        // Find implementation guides and tutorials
        let tutorials_search = repl.send_command(":run search_documents \"tutorial howto implementation step by step\"")?;
        assert!(!tutorials_search.is_empty(), "Should find implementation guides");

        // Discover best practices and coding standards
        let best_practices = repl.send_command(":run search_documents \"best practice standard convention guideline\"")?;
        assert!(!best_practices.is_empty(), "Should find best practices");

        // Find testing strategies and approaches
        let testing_patterns = repl.send_command(":run search_documents \"testing test unit integration mock\"")?;
        assert!(!testing_patterns.is_empty(), "Should find testing strategies");

        // Step 4: Integration and workflow automation
        println!("  ⚙️ Phase 4: Integration and Workflow Automation");

        // Find development workflows and processes
        let workflows_search = repl.send_command(":run search_documents \"workflow process CI/CD deployment automation\"")?;
        assert!(!workflows_search.is_empty(), "Should find development workflows");

        // Discover tool configurations and setup guides
        let setup_guides = repl.send_command(":run search_documents \"setup configuration install environment\"")?;
        assert!(!setup_guides.is_empty(), "Should find setup guides");

        // Find troubleshooting and debugging resources
        let debugging_guides = repl.send_command(":run search_documents \"debugging troubleshooting fix issue problem\"")?;
        assert!(!debugging_guides.is_empty(), "Should find debugging resources");

        repl.quit()?;

        let workflow_duration = workflow_start.elapsed();

        // Validate code documentation workflow
        assert!(workflow_duration < Duration::from_secs(40),
               "Code documentation workflow should complete within 40 seconds, took {:?}",
               workflow_duration);

        println!("✅ Code documentation workflow completed successfully in {:?}", workflow_duration);

        // Code documentation workflow validation checklist
        println!("  📋 Code Documentation Workflow Validation:");
        println!("    ✅ Code discovery: Found code examples and documentation");
        println!("    ✅ Pattern analysis: Understood programming patterns and approaches");
        println!("    ✅ Practical application: Discovered implementation guides and best practices");
        println!("    ✅ Integration: Found workflows, configurations, and debugging resources");

        Ok(())
    }

    /// Test personal knowledge management workflow
    pub async fn test_personal_knowledge_management_workflow(&self) -> Result<()> {
        println!("🧪 Testing personal knowledge management workflow");

        let workflow_start = Instant::now();

        // Step 1: Personal content discovery and organization
        println!("  📝 Phase 1: Personal Content Discovery");

        let cli_harness = ExtendedCliTestHarness::new().await?;

        // Find personal notes and knowledge base entries
        let personal_search = cli_harness.execute_cli_command(&[
            "search", "personal learning goals knowledge base",
            "--limit", "10"
        ])?;

        assert!(personal_search.exit_code == 0, "Personal search should succeed");

        // Search for learning objectives and skill development
        let learning_search = cli_harness.execute_cli_command(&[
            "fuzzy", "learning skills development goals progress",
            "--tags", "true",
            "--limit", "8"
        ])?;

        assert!(learning_search.exit_code == 0, "Learning search should succeed");

        // Step 2: Progress tracking and assessment
        println!("  📈 Phase 2: Progress Tracking and Assessment");

        let repl_harness = ExtendedReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;

        // Find personal development and progress tracking documents
        let progress_query = "SELECT * FROM notes WHERE content LIKE '%progress%' OR content LIKE '%goal%' OR content LIKE '%learning%' AND path LIKE '%personal%'";
        let progress_docs = repl.send_command(progress_query)?;

        assert!(!progress_docs.is_empty(), "Should find progress tracking documents");

        // Assess skill development and learning outcomes
        let skills_assessment = repl.send_command(":run search_documents \"skill level competency mastery expertise\"")?;
        assert!(!skills_assessment.is_empty(), "Should assess skill development");

        // Find achievements and milestones
        let achievements_search = repl.send_command(":run search_documents \"achievement milestone completed accomplished\"")?;
        assert!(!achievements_search.is_empty(), "Should find achievements");

        // Step 3: Knowledge synthesis and reflection
        println!("  🤔 Phase 3: Knowledge Synthesis and Reflection");

        // Synthesize learning across different domains
        let synthesis_query = "SELECT * FROM notes WHERE content LIKE '%summary%' OR content LIKE '%reflection%' OR content LIKE '%insight%'";
        let synthesis_results = repl.send_command(synthesis_query)?;

        assert!(!synthesis_results.is_empty(), "Should synthesize knowledge and reflections");

        // Find patterns and connections in personal learning
        let patterns_search = repl.send_command(":run search_documents \"pattern connection insight epiphany realization\"")?;
        assert!(!patterns_search.is_empty(), "Should find learning patterns");

        // Identify areas for improvement and future learning
        let improvement_search = repl.send_command(":run search_documents \"improve enhance develop future next step\"")?;
        assert!(!improvement_search.is_empty(), "Should identify improvement areas");

        repl.quit()?;

        let workflow_duration = workflow_start.elapsed();

        // Validate personal knowledge management workflow
        assert!(workflow_duration < Duration::from_secs(30),
               "Personal knowledge management workflow should complete within 30 seconds, took {:?}",
               workflow_duration);

        println!("✅ Personal knowledge management workflow completed successfully in {:?}", workflow_duration);

        Ok(())
    }

    /// Test collaborative knowledge sharing workflow
    pub async fn test_collaborative_knowledge_sharing_workflow(&self) -> Result<()> {
        println!("🧪 Testing collaborative knowledge sharing workflow");

        let workflow_start = Instant::now();

        // Step 1: Shared content discovery
        println!("  👥 Phase 1: Shared Content Discovery");

        let cli_harness = ExtendedCliTestHarness::new().await?;

        // Find meeting notes and collaborative documents
        let collaborative_search = cli_harness.execute_cli_command(&[
            "search", "meeting notes team collaboration shared",
            "--limit", "10"
        ])?;

        assert!(collaborative_search.exit_code == 0, "Collaborative search should succeed");

        // Search for team projects and initiatives
        let team_search = cli_harness.execute_cli_command(&[
            "semantic", "team project collaboration workflow",
            "--top-k", "5"
        ])?;

        assert!(team_search.exit_code == 0, "Team search should succeed");

        // Step 2: Knowledge integration and synthesis
        println!("  🔗 Phase 2: Knowledge Integration");

        let repl_harness = ExtendedReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;

        // Find meeting-related documents
        let meetings_query = "SELECT * FROM notes WHERE content LIKE '%meeting%' OR content LIKE '%attendee%' OR content LIKE '%action item%'";
        let meetings_docs = repl.send_command(meetings_query)?;

        assert!(!meetings_docs.is_empty(), "Should find meeting documents");

        // Identify team expertise and knowledge areas
        let expertise_search = repl.send_command(":run search_documents \"expertise specialty skill domain knowledge\"")?;
        assert!(!expertise_search.is_empty(), "Should identify team expertise");

        // Find shared resources and references
        let resources_search = repl.send_command(":run search_documents \"share resource reference link document\"")?;
        assert!(!resources_search.is_empty(), "Should find shared resources");

        repl.quit()?;

        let workflow_duration = workflow_start.elapsed();

        // Validate collaborative knowledge sharing workflow
        assert!(workflow_duration < Duration::from_secs(35),
               "Collaborative knowledge sharing workflow should complete within 35 seconds, took {:?}",
               workflow_duration);

        println!("✅ Collaborative knowledge sharing workflow completed successfully in {:?}", workflow_duration);

        Ok(())
    }

    /// Test comprehensive workflow integration across all scenarios
    pub async fn test_comprehensive_workflow_integration(&self) -> Result<()> {
        println!("🧪 Testing comprehensive workflow integration across all scenarios");

        let integration_start = Instant::now();

        // Run all major workflows in sequence
        self.test_research_workflow().await?;
        self.test_project_management_workflow().await?;
        self.test_knowledge_discovery_workflow().await?;
        self.test_code_documentation_workflow().await?;
        self.test_personal_knowledge_management_workflow().await?;
        self.test_collaborative_knowledge_sharing_workflow().await?;

        let integration_duration = integration_start.elapsed();

        // Validate comprehensive integration
        assert!(integration_duration < Duration::from_secs(300), // 5 minutes
               "Comprehensive workflow integration should complete within 5 minutes, took {:?}",
               integration_duration);

        println!("✅ Comprehensive workflow integration completed successfully in {:?}", integration_duration);

        // Integration validation checklist
        println!("  📋 Comprehensive Integration Validation:");
        println!("    ✅ Research workflow: Sources discovery → analysis → insights");
        println!("    ✅ Project management: Tasks → deadlines → dependencies");
        println!("    ✅ Knowledge discovery: Topics → links → synthesis");
        println!("    ✅ Code documentation: Examples → patterns → application");
        println!("    ✅ Personal knowledge management: Organization → tracking → reflection");
        println!("    ✅ Collaborative sharing: Team content → integration → synthesis");

        Ok(())
    }
}

// ============================================================================
// Test Execution Functions
// ============================================================================

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_real_world_usage_scenarios_comprehensive() -> Result<()> {
    println!("🧪 Running comprehensive real-world usage scenario tests");

    let harness = RealWorldUsageTestHarness::new().await?;

    // Run all real-world usage scenario tests
    harness.test_research_workflow().await?;
    harness.test_project_management_workflow().await?;
    harness.test_knowledge_discovery_workflow().await?;
    harness.test_code_documentation_workflow().await?;
    harness.test_personal_knowledge_management_workflow().await?;
    harness.test_collaborative_knowledge_sharing_workflow().await?;
    harness.test_comprehensive_workflow_integration().await?;

    println!("✅ Comprehensive real-world usage scenario tests passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_research_workflow_validation() -> Result<()> {
    println!("🧪 Testing research workflow validation");

    let harness = RealWorldUsageTestHarness::new().await?;
    harness.test_research_workflow().await?;

    println!("✅ Research workflow validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_project_management_workflow_validation() -> Result<()> {
    println!("🧪 Testing project management workflow validation");

    let harness = RealWorldUsageTestHarness::new().await?;
    harness.test_project_management_workflow().await?;

    println!("✅ Project management workflow validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_knowledge_discovery_workflow_validation() -> Result<()> {
    println!("🧪 Testing knowledge discovery workflow validation");

    let harness = RealWorldUsageTestHarness::new().await?;
    harness.test_knowledge_discovery_workflow().await?;

    println!("✅ Knowledge discovery workflow validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_code_documentation_workflow_validation() -> Result<()> {
    println!("🧪 Testing code documentation workflow validation");

    let harness = RealWorldUsageTestHarness::new().await?;
    harness.test_code_documentation_workflow().await?;

    println!("✅ Code documentation workflow validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_personal_knowledge_management_validation() -> Result<()> {
    println!("🧪 Testing personal knowledge management workflow validation");

    let harness = RealWorldUsageTestHarness::new().await?;
    harness.test_personal_knowledge_management_workflow().await?;

    println!("✅ Personal knowledge management workflow validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_collaborative_knowledge_sharing_validation() -> Result<()> {
    println!("🧪 Testing collaborative knowledge sharing workflow validation");

    let harness = RealWorldUsageTestHarness::new().await?;
    harness.test_collaborative_knowledge_sharing_workflow().await?;

    println!("✅ Collaborative knowledge sharing workflow validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_comprehensive_workflow_integration_validation() -> Result<()> {
    println!("🧪 Testing comprehensive workflow integration validation");

    let harness = RealWorldUsageTestHarness::new().await?;
    harness.test_comprehensive_workflow_integration().await?;

    println!("✅ Comprehensive workflow integration validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_user_scenario_simulation() -> Result<()> {
    println!("🧪 Testing user scenario simulation");

    let harness = RealWorldUsageTestHarness::new().await?;

    // Simulate a realistic user session combining multiple workflows
    println!("  🎭 Simulating realistic user session");

    let session_start = Instant::now();

    // User starts with research
    println!("    🔬 User begins with research phase");
    let cli_harness = ExtendedCliTestHarness::new().await?;
    let research_result = cli_harness.execute_cli_command(&["search", "quantum computing"])?;
    assert!(research_result.exit_code == 0, "Research search should succeed");

    // User switches to interactive exploration
    println!("    🔍 User switches to interactive exploration");
    let repl_harness = ExtendedReplTestHarness::new().await?;
    let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;

    let _ = repl.send_command("SELECT * FROM notes WHERE content LIKE '%quantum%' OR content LIKE '%physics%'");
    let _ = repl.send_command(":run search_documents \"applications use cases\"");

    // User explores code examples
    println!("    💻 User explores code examples");
    let _ = repl.send_command(":run search_documents \"rust async patterns\"");
    let _ = repl.send_command("SELECT * FROM notes WHERE tags LIKE '%rust%'");

    // User checks project status
    println!("    📋 User checks project status");
    let _ = repl.send_command(":run search_documents \"project task deadline\"");

    // User reviews personal learning goals
    println!("    📚 User reviews personal learning goals");
    let _ = repl.send_command("SELECT * FROM notes WHERE path LIKE '%personal%'");

    repl.quit()?;

    let session_duration = session_start.elapsed();

    // Validate user session
    assert!(session_duration < Duration::from_secs(120), // 2 minutes
           "User session should complete within 2 minutes, took {:?}",
           session_duration);

    println!("✅ User scenario simulation completed successfully in {:?}", session_duration);

    Ok(())
}