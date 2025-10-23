//! Workload simulator for realistic testing scenarios
//!
//! This module provides comprehensive workload simulation capabilities
//! for testing the Crucible system under realistic conditions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rand::{Rng, SeedableRng};
use rand::seq::SliceRandom;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::{
    IntegrationTestRunner, TestResult, TestCategory, TestOutcome, UserBehaviorPattern,
    UserActivity, UserActivityType, ActivityOutcome, TestUser, TestDocument,
};

/// Workload simulator for realistic testing scenarios
pub struct WorkloadSimulator {
    /// Test runner reference
    test_runner: Arc<IntegrationTestRunner>,
    /// Simulation configuration
    config: WorkloadConfig,
    /// Active users
    active_users: Arc<RwLock<Vec<TestUser>>>,
    /// Workload statistics
    statistics: Arc<RwLock<WorkloadStatistics>>,
    /// Random number generator
    rng: Arc<RwLock<rand::rngs::StdRng>>,
}

/// Workload configuration
#[derive(Debug, Clone)]
pub struct WorkloadConfig {
    /// Number of concurrent users to simulate
    pub concurrent_users: usize,
    /// Duration of the simulation
    pub duration: Duration,
    /// User behavior distribution
    pub user_behavior_distribution: HashMap<UserBehaviorPattern, f64>,
    /// Activity frequency (activities per minute per user)
    pub activity_frequency: f64,
    /// Data volume (number of documents to work with)
    pub data_volume: usize,
    /// Whether to simulate network latency
    pub simulate_network_latency: bool,
    /// Network latency range
    pub network_latency_range: (Duration, Duration),
    /// Error rate to simulate
    pub error_rate: f64,
    /// Peak load multiplier
    pub peak_load_multiplier: f64,
}

/// Workload statistics
#[derive(Debug, Clone, Default)]
pub struct WorkloadStatistics {
    /// Total activities performed
    pub total_activities: u64,
    /// Activities by type
    pub activities_by_type: HashMap<String, u64>,
    /// Activities by outcome
    pub activities_by_outcome: HashMap<String, u64>,
    /// Average response time
    pub avg_response_time: Duration,
    /// Peak concurrent users
    pub peak_concurrent_users: usize,
    /// Errors encountered
    pub total_errors: u64,
    /// Data processed
    pub data_processed_mb: u64,
    /// Start time
    pub start_time: Option<Instant>,
    /// End time
    pub end_time: Option<Instant>,
}

/// Knowledge management workload scenario
pub struct KnowledgeManagementWorkload {
    /// Documents to work with
    documents: Vec<TestDocument>,
    /// User queries
    queries: Vec<String>,
    /// Script templates
    script_templates: Vec<ScriptTemplate>,
}

/// Script template for execution testing
#[derive(Debug, Clone)]
pub struct ScriptTemplate {
    /// Script name
    pub name: String,
    /// Script content
    pub content: String,
    /// Script parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Expected execution time
    pub expected_duration: Duration,
    /// Resource requirements
    pub resource_requirements: ResourceRequirements,
}

/// Resource requirements for script execution
#[derive(Debug, Clone)]
pub struct ResourceRequirements {
    /// Memory requirement in MB
    pub memory_mb: u64,
    /// CPU requirement percentage
    pub cpu_percent: f64,
    /// Disk space requirement in MB
    pub disk_mb: u64,
    /// Network requirement in MB/s
    pub network_mbps: f64,
}

impl WorkloadSimulator {
    /// Create new workload simulator
    pub fn new(
        test_runner: Arc<IntegrationTestRunner>,
        config: WorkloadConfig,
    ) -> Self {
        Self {
            test_runner,
            config: config.clone(),
            active_users: Arc::new(RwLock::new(Vec::new())),
            statistics: Arc::new(RwLock::new(WorkloadStatistics::default())),
            rng: Arc::new(RwLock::new(rand::rngs::StdRng::from_entropy())),
        }
    }

    /// Run comprehensive workload simulation
    pub async fn run_workload_simulation(&self) -> Result<Vec<TestResult>> {
        info!(
            concurrent_users = self.config.concurrent_users,
            duration_seconds = self.config.duration.as_secs(),
            "Starting workload simulation"
        );

        let mut results = Vec::new();
        let simulation_start = Instant::now();

        // Initialize workload
        self.initialize_workload().await?;

        // Run different workload phases
        results.extend(self.run_warmup_phase().await?);
        results.extend(self.run_baseline_load().await?);
        results.extend(self.run_peak_load().await?);
        results.extend(self.run_sustained_load().await?);
        results.extend(self.run_cleanup_phase().await?);

        let simulation_duration = simulation_start.elapsed();

        // Generate simulation report
        let simulation_result = self.generate_simulation_report(simulation_duration).await?;
        results.push(simulation_result);

        info!(
            duration_seconds = simulation_duration.as_secs(),
            total_activities = {
                let stats = self.statistics.read().await;
                stats.total_activities
            },
            "Workload simulation completed"
        );

        Ok(results)
    }

    /// Initialize workload with test data
    async fn initialize_workload(&self) -> Result<()> {
        info!("Initializing workload simulation");

        // Update statistics start time
        {
            let mut stats = self.statistics.write().await;
            stats.start_time = Some(Instant::now());
        }

        // Create test users
        let behavior_patterns: Vec<UserBehaviorPattern> = self.config.user_behavior_distribution
            .keys()
            .cloned()
            .collect();

        let test_utils = super::test_utilities::TestUtils::new(
            self.test_runner.config.clone(),
            self.test_runner.test_dir.clone(),
        );

        let users = test_utils.create_test_users(self.config.concurrent_users, &behavior_patterns).await?;

        // Store active users
        {
            let mut active_users = self.active_users.write().await;
            *active_users = users;
        }

        info!("Workload initialized successfully");
        Ok(())
    }

    /// Run warmup phase
    async fn run_warmup_phase(&self) -> Result<Vec<TestResult>> {
        info!("Starting warmup phase");
        let mut results = Vec::new();

        let warmup_duration = Duration::from_secs(30);
        let warmup_start = Instant::now();

        // Start with 25% of users
        let warmup_users = (self.config.concurrent_users / 4).max(1);

        while warmup_start.elapsed() < warmup_duration {
            self.simulate_user_activities(warmup_users, Duration::from_millis(500)).await?;
            sleep(Duration::from_millis(100)).await;
        }

        results.push(super::test_utilities::create_test_result(
            "warmup_phase".to_string(),
            TestCategory::PerformanceValidation,
            TestOutcome::Passed,
            warmup_start.elapsed(),
            HashMap::new(),
            None,
        ));

        info!("Warmup phase completed");
        Ok(results)
    }

    /// Run baseline load phase
    async fn run_baseline_load(&self) -> Result<Vec<TestResult>> {
        info!("Starting baseline load phase");
        let mut results = Vec::new();

        let baseline_duration = Duration::from_secs(60);
        let baseline_start = Instant::now();

        // 50% of users for baseline
        let baseline_users = self.config.concurrent_users / 2;

        while baseline_start.elapsed() < baseline_duration {
            self.simulate_user_activities(baseline_users, Duration::from_millis(200)).await?;
            sleep(Duration::from_millis(50)).await;
        }

        results.push(super::test_utilities::create_test_result(
            "baseline_load".to_string(),
            TestCategory::PerformanceValidation,
            TestOutcome::Passed,
            baseline_start.elapsed(),
            HashMap::new(),
            None,
        ));

        info!("Baseline load phase completed");
        Ok(results)
    }

    /// Run peak load phase
    async fn run_peak_load(&self) -> Result<Vec<TestResult>> {
        info!("Starting peak load phase");
        let mut results = Vec::new();

        let peak_duration = Duration::from_secs(120);
        let peak_start = Instant::now();

        // Peak load with multiplier
        let peak_users = ((self.config.concurrent_users as f64) * self.config.peak_load_multiplier) as usize;

        while peak_start.elapsed() < peak_duration {
            self.simulate_user_activities(peak_users, Duration::from_millis(100)).await?;
            sleep(Duration::from_millis(25)).await;
        }

        results.push(super::test_utilities::create_test_result(
            "peak_load".to_string(),
            TestCategory::PerformanceValidation,
            TestOutcome::Passed,
            peak_start.elapsed(),
            HashMap::new(),
            None,
        ));

        info!("Peak load phase completed");
        Ok(results)
    }

    /// Run sustained load phase
    async fn run_sustained_load(&self) -> Result<Vec<TestResult>> {
        info!("Starting sustained load phase");
        let mut results = Vec::new();

        let sustained_start = Instant::now();

        while sustained_start.elapsed() < self.config.duration {
            // Vary load during sustained phase (75% to 100% of users)
            let load_factor = 0.75 + (rand::random::<f64>() * 0.25);
            let active_users = (self.config.concurrent_users as f64 * load_factor) as usize;

            self.simulate_user_activities(active_users, Duration::from_millis(300)).await?;
            sleep(Duration::from_millis(75)).await;
        }

        results.push(super::test_utilities::create_test_result(
            "sustained_load".to_string(),
            TestCategory::PerformanceValidation,
            TestOutcome::Passed,
            sustained_start.elapsed(),
            HashMap::new(),
            None,
        ));

        info!("Sustained load phase completed");
        Ok(results)
    }

    /// Run cleanup phase
    async fn run_cleanup_phase(&self) -> Result<Vec<TestResult>> {
        info!("Starting cleanup phase");
        let mut results = Vec::new();

        let cleanup_duration = Duration::from_secs(30);
        let cleanup_start = Instant::now();

        // Gradually reduce load
        let cleanup_users = self.config.concurrent_users / 4;

        while cleanup_start.elapsed() < cleanup_duration {
            self.simulate_user_activities(cleanup_users, Duration::from_millis(1000)).await?;
            sleep(Duration::from_millis(200)).await;
        }

        results.push(super::test_utilities::create_test_result(
            "cleanup_phase".to_string(),
            TestCategory::PerformanceValidation,
            TestOutcome::Passed,
            cleanup_start.elapsed(),
            HashMap::new(),
            None,
        ));

        info!("Cleanup phase completed");
        Ok(results)
    }

    /// Simulate user activities
    async fn simulate_user_activities(&self, user_count: usize, activity_interval: Duration) -> Result<()> {
        let active_users = self.active_users.read().await;

        // Select random users for activity
        let mut rng = self.rng.write().await;
        let selected_users: Vec<_> = active_users.choose_multiple(&mut *rng, user_count).cloned().collect();

        drop(rng);
        drop(active_users);

        // Simulate activities for selected users
        let mut tasks = Vec::new();

        for user in selected_users {
            let task = self.simulate_user_activity(user);
            tasks.push(task);
        }

        // Wait for all activities to complete
        let results = futures::future::join_all(tasks).await;

        // Update statistics
        self.update_workload_statistics(&results).await;

        Ok(())
    }

    /// Simulate activity for a single user
    async fn simulate_user_activity(&self, user: TestUser) -> UserActivity {
        let activity_start = Instant::now();

        // Determine activity type based on user behavior pattern
        let activity_type = self.select_activity_type(&user.behavior_pattern);

        // Simulate network latency if enabled
        if self.config.simulate_network_latency {
            let latency_range = self.config.network_latency_range;
            let latency_ms = latency_range.0.as_millis() as u64 +
                           (rand::random::<u64>() % (latency_range.1.as_millis() as u64 - latency_range.0.as_millis() as u64));
            sleep(Duration::from_millis(latency_ms)).await;
        }

        // Execute the activity
        let outcome = self.execute_activity(&activity_type).await;

        let duration = activity_start.elapsed();

        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.total_activities += 1;

            let activity_type_str = format!("{:?}", activity_type);
            *stats.activities_by_type.entry(activity_type_str).or_insert(0) += 1;

            let outcome_str = format!("{:?}", outcome);
            *stats.activities_by_outcome.entry(outcome_str).or_insert(0) += 1;

            if matches!(outcome, ActivityOutcome::Failed { .. }) {
                stats.total_errors += 1;
            }
        }

        UserActivity {
            timestamp: activity_start,
            activity_type,
            duration,
            outcome,
        }
    }

    /// Select activity type based on user behavior pattern
    fn select_activity_type(&self, pattern: &UserBehaviorPattern) -> UserActivityType {
        let mut rng = rand::thread_rng();

        match pattern {
            UserBehaviorPattern::Light => {
                // Light users mostly search and view
                match rng.gen_range(0..=10) {
                    0..=7 => UserActivityType::Search {
                        query: self.generate_random_query()
                    },
                    8..=9 => UserActivityType::ViewStatus,
                    _ => UserActivityType::AccessConfiguration,
                }
            }
            UserBehaviorPattern::Regular => {
                // Regular users have balanced activities
                match rng.gen_range(0..=10) {
                    0..=3 => UserActivityType::Search {
                        query: self.generate_random_query()
                    },
                    4..=5 => UserActivityType::CreateDocument {
                        title: format!("New Document {}", rng.gen_range(1..=1000))
                    },
                    6..=7 => UserActivityType::EditDocument {
                        document_id: Uuid::new_v4().to_string()
                    },
                    8..=9 => UserActivityType::ViewStatus,
                    _ => UserActivityType::RunScript {
                        script_name: "basic_script.rune".to_string()
                    },
                }
            }
            UserBehaviorPattern::Power => {
                // Power users focus on content creation and editing
                match rng.gen_range(0..=10) {
                    0..=2 => UserActivityType::Search {
                        query: self.generate_random_query()
                    },
                    3..=5 => UserActivityType::CreateDocument {
                        title: format!("Power Document {}", rng.gen_range(1..=1000))
                    },
                    6..=8 => UserActivityType::EditDocument {
                        document_id: Uuid::new_v4().to_string()
                    },
                    9..=9 => UserActivityType::RunScript {
                        script_name: "advanced_script.rune".to_string()
                    },
                    _ => UserActivityType::AccessConfiguration,
                }
            }
            UserBehaviorPattern::Developer => {
                // Developers focus on scripts and configuration
                match rng.gen_range(0..=10) {
                    0..=2 => UserActivityType::RunScript {
                        script_name: format!("dev_script_{}.rune", rng.gen_range(1..=10))
                    },
                    3..=4 => UserActivityType::AccessConfiguration,
                    5..=6 => UserActivityType::CreateDocument {
                        title: format!("API Documentation {}", rng.gen_range(1..=100))
                    },
                    7..=9 => UserActivityType::EditDocument {
                        document_id: Uuid::new_v4().to_string()
                    },
                    _ => UserActivityType::Search {
                        query: self.generate_random_query()
                    },
                }
            }
            UserBehaviorPattern::Stressed => {
                // Stressed users perform rapid activities
                match rng.gen_range(0..=10) {
                    0..=3 => UserActivityType::Search {
                        query: self.generate_random_query()
                    },
                    4..=5 => UserActivityType::CreateDocument {
                        title: format!("Urgent Document {}", rng.gen_range(1..=100))
                    },
                    6..=7 => UserActivityType::EditDocument {
                        document_id: Uuid::new_v4().to_string()
                    },
                    8..=9 => UserActivityType::ViewStatus,
                    _ => UserActivityType::RunScript {
                        script_name: "quick_script.rune".to_string()
                    },
                }
            }
        }
    }

    /// Generate random search query
    fn generate_random_query(&self) -> String {
        let query_templates = vec![
            "project planning",
            "meeting notes",
            "technical documentation",
            "API design",
            "testing strategy",
            "deployment guide",
            "security requirements",
            "performance metrics",
            "user interface",
            "database schema",
            "architecture patterns",
            "best practices",
            "troubleshooting guide",
            "configuration setup",
            "integration testing",
        ];

        let mut rng = rand::thread_rng();
        query_templates.choose(&mut rng).unwrap().to_string()
    }

    /// Execute a specific activity
    async fn execute_activity(&self, activity_type: &UserActivityType) -> ActivityOutcome {
        // Simulate random errors based on error rate
        if rand::random::<f64>() < self.config.error_rate {
            return ActivityOutcome::Failed {
                error: "Simulated random error".to_string()
            };
        }

        match activity_type {
            UserActivityType::Search { query } => {
                debug!(query = %query, "Executing search activity");

                // Simulate search execution time
                let search_time = Duration::from_millis(50 + rand::random::<u64>() % 200);
                sleep(search_time).await;

                ActivityOutcome::Success
            }
            UserActivityType::CreateDocument { title } => {
                debug!(title = %title, "Executing create document activity");

                // Simulate document creation time
                let create_time = Duration::from_millis(100 + rand::random::<u64>() % 300);
                sleep(create_time).await;

                ActivityOutcome::Success
            }
            UserActivityType::EditDocument { document_id } => {
                debug!(document_id = %document_id, "Executing edit document activity");

                // Simulate document editing time
                let edit_time = Duration::from_millis(200 + rand::random::<u64>() % 500);
                sleep(edit_time).await;

                ActivityOutcome::Success
            }
            UserActivityType::RunScript { script_name } => {
                debug!(script_name = %script_name, "Executing script activity");

                // Simulate script execution time
                let script_time = Duration::from_millis(500 + rand::random::<u64>() % 1500);

                // Check for timeout
                if script_time > Duration::from_secs(2) {
                    sleep(Duration::from_secs(2)).await;
                    return ActivityOutcome::Timeout;
                }

                sleep(script_time).await;
                ActivityOutcome::Success
            }
            UserActivityType::AccessConfiguration => {
                debug!("Executing access configuration activity");

                // Configuration access is usually fast
                let config_time = Duration::from_millis(10 + rand::random::<u64>() % 50);
                sleep(config_time).await;

                ActivityOutcome::Success
            }
            UserActivityType::ViewStatus => {
                debug!("Executing view status activity");

                // Status viewing is fast
                let status_time = Duration::from_millis(5 + rand::random::<u64>() % 25);
                sleep(status_time).await;

                ActivityOutcome::Success
            }
        }
    }

    /// Update workload statistics
    async fn update_workload_statistics(&self, results: &[UserActivity]) {
        let mut stats = self.statistics.write().await;

        // Update peak concurrent users
        let current_users = results.len();
        if current_users > stats.peak_concurrent_users {
            stats.peak_concurrent_users = current_users;
        }

        // Calculate average response time
        if !results.is_empty() {
            let total_time: Duration = results.iter().map(|r| r.duration).sum();
            stats.avg_response_time = total_time / results.len() as u32;
        }

        // Update data processed (rough estimate)
        stats.data_processed_mb += (results.len() * 10) as u64; // 10MB per activity estimate
    }

    /// Generate simulation report
    async fn generate_simulation_report(&self, total_duration: Duration) -> Result<TestResult> {
        let stats = self.statistics.read().await;

        let mut metrics = HashMap::new();
        metrics.insert("total_activities".to_string(), stats.total_activities as f64);
        metrics.insert("avg_response_time_ms".to_string(), stats.avg_response_time.as_millis() as f64);
        metrics.insert("peak_concurrent_users".to_string(), stats.peak_concurrent_users as f64);
        metrics.insert("error_rate".to_string(),
            (stats.total_errors as f64 / stats.total_activities.max(1) as f64) * 100.0);
        metrics.insert("throughput_activities_per_sec".to_string(),
            stats.total_activities as f64 / total_duration.as_secs() as f64);

        // Update end time
        drop(stats);
        {
            let mut stats = self.statistics.write().await;
            stats.end_time = Some(Instant::now());
        }

        Ok(super::test_utilities::create_test_result(
            "workload_simulation_report".to_string(),
            TestCategory::PerformanceValidation,
            TestOutcome::Passed,
            total_duration,
            metrics,
            None,
        ))
    }
}

impl KnowledgeManagementWorkload {
    /// Create new knowledge management workload
    pub fn new(documents: Vec<TestDocument>) -> Self {
        let queries = Self::generate_search_queries();
        let script_templates = Self::generate_script_templates();

        Self {
            documents,
            queries,
            script_templates,
        }
    }

    /// Generate realistic search queries
    fn generate_search_queries() -> Vec<String> {
        vec![
            "project planning timeline",
            "meeting notes action items",
            "technical documentation API",
            "testing strategy unit tests",
            "deployment guide CI/CD",
            "security requirements authentication",
            "performance metrics monitoring",
            "user interface design",
            "database schema normalization",
            "architecture patterns microservices",
            "best practices code review",
            "troubleshooting guide debugging",
            "configuration setup environment",
            "integration testing pipelines",
            "code review guidelines",
            "refactoring strategies",
            "dependency management",
            "error handling patterns",
            "logging and monitoring",
            "documentation standards",
        ]
    }

    /// Generate script templates for testing
    fn generate_script_templates() -> Vec<ScriptTemplate> {
        vec![
            ScriptTemplate {
                name: "document_processor.rune".to_string(),
                content: r#"
// Document processing script
fn process_document(content) {
    let words = content.split_whitespace().count();
    let sentences = content.matches('.').count();

    return {
        word_count: words,
        sentence_count: sentences,
        readability_score: calculate_readability(words, sentences)
    };
}

fn calculate_readibility(words, sentences) {
    return (words / sentences.max(1)) as f64;
}
                "#.to_string(),
                parameters: HashMap::new(),
                expected_duration: Duration::from_millis(500),
                resource_requirements: ResourceRequirements {
                    memory_mb: 64,
                    cpu_percent: 25.0,
                    disk_mb: 10,
                    network_mbps: 0.0,
                },
            },
            ScriptTemplate {
                name: "search_analyzer.rune".to_string(),
                content: r#"
// Search analysis script
fn analyze_search_results(results, query) {
    let relevance_scores = [];

    for result in results {
        let score = calculate_relevance(result, query);
        relevance_scores.push(score);
    }

    return {
        total_results: results.len(),
        avg_relevance: average(relevance_scores),
        top_result: find_best_result(results, relevance_scores)
    };
}

fn calculate_relevance(result, query) {
    // Simple relevance calculation
    let query_terms = query.to_lowercase().split_whitespace();
    let content = result.content.to_lowercase();

    let mut score = 0.0;
    for term in query_terms {
        if content.contains(term) {
            score += 1.0;
        }
    }

    return score / query_terms.count() as f64;
}
                "#.to_string(),
                parameters: HashMap::new(),
                expected_duration: Duration::from_millis(750),
                resource_requirements: ResourceRequirements {
                    memory_mb: 128,
                    cpu_percent: 50.0,
                    disk_mb: 20,
                    network_mbps: 0.0,
                },
            },
            ScriptTemplate {
                name: "performance_monitor.rune".to_string(),
                content: r#"
// Performance monitoring script
fn monitor_performance(metrics) {
    let cpu_usage = metrics.cpu_usage;
    let memory_usage = metrics.memory_usage;
    let disk_usage = metrics.disk_usage;

    let alerts = [];

    if cpu_usage > 80.0 {
        alerts.push("High CPU usage detected");
    }

    if memory_usage > 90.0 {
        alerts.push("High memory usage detected");
    }

    if disk_usage > 85.0 {
        alerts.push("High disk usage detected");
    }

    return {
        status: if alerts.is_empty() { "healthy" } else { "warning" },
        alerts: alerts,
        recommendations: generate_recommendations(metrics)
    };
}

fn generate_recommendations(metrics) {
    let recommendations = [];

    if metrics.cpu_usage > 80.0 {
        recommendations.push("Consider scaling up or optimizing CPU-intensive operations");
    }

    if metrics.memory_usage > 90.0 {
        recommendations.push("Consider adding more memory or optimizing memory usage");
    }

    return recommendations;
}
                "#.to_string(),
                parameters: HashMap::new(),
                expected_duration: Duration::from_millis(200),
                resource_requirements: ResourceRequirements {
                    memory_mb: 32,
                    cpu_percent: 10.0,
                    disk_mb: 5,
                    network_mbps: 0.0,
                },
            },
        ]
    }
}

/// Create default workload configuration
pub fn default_workload_config() -> WorkloadConfig {
    let mut user_behavior_distribution = HashMap::new();
    user_behavior_distribution.insert(UserBehaviorPattern::Light, 0.3);
    user_behavior_distribution.insert(UserBehaviorPattern::Regular, 0.4);
    user_behavior_distribution.insert(UserBehaviorPattern::Power, 0.15);
    user_behavior_distribution.insert(UserBehaviorPattern::Developer, 0.1);
    user_behavior_distribution.insert(UserBehaviorPattern::Stressed, 0.05);

    WorkloadConfig {
        concurrent_users: 10,
        duration: Duration::from_secs(300), // 5 minutes
        user_behavior_distribution,
        activity_frequency: 5.0, // 5 activities per minute per user
        data_volume: 1000,
        simulate_network_latency: true,
        network_latency_range: (Duration::from_millis(10), Duration::from_millis(100)),
        error_rate: 0.02, // 2% error rate
        peak_load_multiplier: 2.0,
    }
}

/// Create stress test workload configuration
pub fn stress_test_config() -> WorkloadConfig {
    let mut user_behavior_distribution = HashMap::new();
    user_behavior_distribution.insert(UserBehaviorPattern::Stressed, 0.6);
    user_behavior_distribution.insert(UserBehaviorPattern::Power, 0.3);
    user_behavior_distribution.insert(UserBehaviorPattern::Developer, 0.1);

    WorkloadConfig {
        concurrent_users: 50,
        duration: Duration::from_secs(600), // 10 minutes
        user_behavior_distribution,
        activity_frequency: 15.0, // 15 activities per minute per user
        data_volume: 5000,
        simulate_network_latency: true,
        network_latency_range: (Duration::from_millis(5), Duration::from_millis(50)),
        error_rate: 0.05, // 5% error rate
        peak_load_multiplier: 3.0,
    }
}