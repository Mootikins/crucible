//! # Plugin State Machine
//!
//! This module implements a sophisticated state machine for plugin lifecycle management,
//! including state transitions, validation, persistence, and state-based event generation.

use super::error::{PluginError, PluginResult};
use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// ============================================================================
    /// STATE MACHINE TYPES
/// ============================================================================

/// Plugin state transition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StateTransition {
    /// Create instance
    Create,
    /// Start instance
    Start,
    /// Complete start
    CompleteStart,
    /// Stop instance
    Stop,
    /// Complete stop
    CompleteStop,
    /// Restart instance
    Restart,
    /// Enter maintenance mode
    EnterMaintenance,
    /// Exit maintenance mode
    ExitMaintenance,
    /// Enter error state
    Error(String),
    /// Recover from error
    Recover,
    /// Crash instance
    Crash,
    /// Suspend instance
    Suspend,
    /// Resume instance
    Resume,
    /// Update configuration
    UpdateConfig,
    /// Scale instance
    Scale,
    /// Migrate instance
    Migrate,
}

/// State transition result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransitionResult {
    /// Transition success
    pub success: bool,
    /// Previous state
    pub previous_state: PluginInstanceState,
    /// New state
    pub new_state: PluginInstanceState,
    /// Transition performed
    pub transition: StateTransition,
    /// Transition timestamp
    pub timestamp: SystemTime,
    /// Transition duration
    pub duration: Duration,
    /// Transition message
    pub message: Option<String>,
    /// Error details (if failed)
    pub error: Option<String>,
    /// Transition metadata
    pub metadata: StateTransitionMetadata,
}

/// State transition metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransitionMetadata {
    /// Transition ID
    pub transition_id: String,
    /// Instance ID
    pub instance_id: String,
    /// Triggering operation
    pub triggering_operation: Option<String>,
    /// Triggering user
    pub triggering_user: Option<String>,
    /// Transition context
    pub context: HashMap<String, serde_json::Value>,
    /// Preconditions checked
    pub preconditions_checked: Vec<String>,
    /// Postconditions verified
    pub postconditions_verified: Vec<String>,
}

/// State transition rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransitionRule {
    /// From state
    pub from_state: PluginInstanceState,
    /// To state
    pub to_state: PluginInstanceState,
    /// Required transition
    pub transition: StateTransition,
    /// Preconditions for transition
    pub preconditions: Vec<StatePrecondition>,
    /// Postconditions to verify
    pub postconditions: Vec<StatePostcondition>,
    /// Transition actions
    pub actions: Vec<StateAction>,
    /// Transition timeout
    pub timeout: Option<Duration>,
    /// Rollback actions
    pub rollback_actions: Vec<StateAction>,
    /// Transition priority
    pub priority: u32,
    /// Transition enabled
    pub enabled: bool,
}

/// State precondition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatePrecondition {
    /// Precondition ID
    pub id: String,
    /// Precondition type
    pub precondition_type: PreconditionType,
    /// Precondition parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Required condition
    pub required: bool,
    /// Precondition description
    pub description: String,
}

/// Precondition type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PreconditionType {
    /// Resource availability check
    ResourceAvailable,
    /// Dependency status check
    DependencyHealthy,
    /// Health status check
    HealthCheck,
    /// Configuration validation
    ConfigValid,
    /// Permission check
    PermissionCheck,
    /// Time window check
    TimeWindow,
    /// Custom precondition
    Custom(String),
}

/// State postcondition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatePostcondition {
    /// Postcondition ID
    pub id: String,
    /// Postcondition type
    pub postcondition_type: PostconditionType,
    /// Postcondition parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Verification timeout
    pub timeout: Duration,
    /// Retry configuration
    pub retry_config: Option<RetryConfig>,
    /// Postcondition description
    pub description: String,
}

/// Postcondition type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PostconditionType {
    /// Process running check
    ProcessRunning,
    /// Service health check
    ServiceHealthy,
    /// Port listening check
    PortListening,
    /// File existence check
    FileExists,
    /// API endpoint check
    ApiEndpointResponding,
    /// Custom postcondition
    Custom(String),
}

/// State action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateAction {
    /// Action ID
    pub id: String,
    /// Action type
    pub action_type: StateActionType,
    /// Action parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Action timeout
    pub timeout: Option<Duration>,
    /// Action order
    pub order: u32,
    /// Action description
    pub description: String,
}

/// State action type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StateActionType {
    /// Start process
    StartProcess,
    /// Stop process
    StopProcess,
    /// Send signal
    SendSignal,
    /// Execute command
    ExecuteCommand,
    /// Create directory
    CreateDirectory,
    /// Remove directory
    RemoveDirectory,
    /// Write file
    WriteFile,
    /// Read file
    ReadFile,
    /// Connect to service
    ConnectToService,
    /// Disconnect from service
    DisconnectFromService,
    /// Wait for condition
    WaitForCondition,
    /// Log message
    LogMessage,
    /// Send notification
    SendNotification,
    /// Custom action
    Custom(String),
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: u32,
    /// Initial delay
    pub initial_delay: Duration,
    /// Maximum delay
    pub max_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Retry on specific errors
    pub retry_on_errors: Vec<String>,
}

/// State machine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineConfig {
    /// Enable state persistence
    pub enable_persistence: bool,
    /// Persistence storage path
    pub persistence_path: Option<String>,
    /// Enable state validation
    pub enable_validation: bool,
    /// Enable transition logging
    pub enable_logging: bool,
    /// Default transition timeout
    pub default_timeout: Duration,
    /// Max concurrent transitions
    pub max_concurrent_transitions: u32,
    /// Enable automatic recovery
    pub enable_auto_recovery: bool,
    /// Recovery configuration
    pub recovery_config: RecoveryConfig,
}

/// Recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Enable automatic recovery
    pub enabled: bool,
    /// Maximum recovery attempts
    pub max_attempts: u32,
    /// Recovery delay
    pub delay: Duration,
    /// Recovery strategy
    pub strategy: RecoveryStrategy,
    /// States to recover from
    pub recoverable_states: Vec<PluginInstanceState>,
}

/// Recovery strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Restart instance
    Restart,
    /// Reset to initial state
    Reset,
    /// Rollback to previous state
    Rollback,
    /// Create new instance
    Recreate,
    /// Manual intervention required
    Manual,
}

/// State machine metrics
#[derive(Debug, Clone, Default)]
pub struct StateMachineMetrics {
    /// Total transitions performed
    pub total_transitions: u64,
    /// Successful transitions
    pub successful_transitions: u64,
    /// Failed transitions
    pub failed_transitions: u64,
    /// Average transition time
    pub average_transition_time: Duration,
    /// Transitions by type
    pub transitions_by_type: HashMap<String, u64>,
    /// Current state distribution
    pub state_distribution: HashMap<PluginInstanceState, u64>,
    /// Recovery attempts
    pub recovery_attempts: u64,
    /// Successful recoveries
    pub successful_recoveries: u64,
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// ============================================================================
    /// PLUGIN STATE MACHINE
/// ============================================================================

/// Advanced plugin state machine
#[derive(Debug)]
pub struct PluginStateMachine {
    /// Current states
    states: Arc<RwLock<HashMap<String, PluginInstanceState>>>,

    /// State history
    state_history: Arc<RwLock<HashMap<String, VecDeque<StateTransitionResult>>>>,

    /// Transition rules
    transition_rules: Arc<RwLock<Vec<StateTransitionRule>>>,

    /// Active transitions
    active_transitions: Arc<RwLock<HashMap<String, StateTransitionContext>>>,

    /// State machine configuration
    config: StateMachineConfig,

    /// Metrics
    metrics: Arc<RwLock<StateMachineMetrics>>,

    /// Event subscribers
    event_subscribers: Arc<RwLock<Vec<tokio::sync::mpsc::UnboundedSender<StateMachineEvent>>>>,
}

/// State transition context
#[derive(Debug)]
struct StateTransitionContext {
    /// Instance ID
    instance_id: String,
    /// Transition being performed
    transition: StateTransition,
    /// Start time
    start_time: SystemTime,
    /// Transition ID
    transition_id: String,
    /// Cancellation token
    cancellation_token: tokio_util::sync::CancellationToken,
    /// Current action index
    current_action: usize,
    /// Actions completed
    actions_completed: HashSet<String>,
    /// Actions failed
    actions_failed: HashMap<String, String>,
}

/// State machine event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StateMachineEvent {
    /// State transition started
    TransitionStarted { instance_id: String, transition: StateTransition },
    /// State transition completed
    TransitionCompleted { instance_id: String, result: StateTransitionResult },
    /// State transition failed
    TransitionFailed { instance_id: String, transition: StateTransition, error: String },
    /// State entered
    StateEntered { instance_id: String, state: PluginInstanceState },
    /// State exited
    StateExited { instance_id: String, state: PluginInstanceState },
    /// Invalid transition attempted
    InvalidTransition { instance_id: String, from_state: PluginInstanceState, to_state: PluginInstanceState },
    /// Recovery triggered
    RecoveryTriggered { instance_id: String, state: PluginInstanceState, strategy: RecoveryStrategy },
    /// Recovery completed
    RecoveryCompleted { instance_id: String, success: bool },
}

impl Default for StateMachineConfig {
    fn default() -> Self {
        Self {
            enable_persistence: true,
            persistence_path: Some("/tmp/plugin-states".to_string()),
            enable_validation: true,
            enable_logging: true,
            default_timeout: Duration::from_secs(30),
            max_concurrent_transitions: 10,
            enable_auto_recovery: true,
            recovery_config: RecoveryConfig {
                enabled: true,
                max_attempts: 3,
                delay: Duration::from_secs(5),
                strategy: RecoveryStrategy::Restart,
                recoverable_states: vec![
                    PluginInstanceState::Error(String::new()),
                    PluginInstanceState::Crashed,
                ],
            },
        }
    }
}

impl PluginStateMachine {
    /// Create a new state machine
    pub fn new() -> Self {
        Self::with_config(StateMachineConfig::default())
    }

    /// Create a new state machine with configuration
    pub fn with_config(config: StateMachineConfig) -> Self {
        let mut state_machine = Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            state_history: Arc::new(RwLock::new(HashMap::new())),
            transition_rules: Arc::new(RwLock::new(Vec::new())),
            active_transitions: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: Arc::new(RwLock::new(StateMachineMetrics::default())),
            event_subscribers: Arc::new(RwLock::new(Vec::new())),
        };

        // Initialize default transition rules
        state_machine.initialize_default_rules();

        state_machine
    }

    /// Initialize the state machine
    pub async fn initialize(&self) -> PluginResult<()> {
        info!("Initializing plugin state machine");

        // Load persisted states if enabled
        if self.config.enable_persistence {
            self.load_persisted_states().await?;
        }

        // Start background tasks
        self.start_background_tasks().await?;

        info!("State machine initialized successfully");
        Ok(())
    }

    /// Initialize default transition rules
    fn initialize_default_rules(&mut self) {
        let rules = vec![
            // Create -> Starting
            StateTransitionRule {
                from_state: PluginInstanceState::Created,
                to_state: PluginInstanceState::Starting,
                transition: StateTransition::Start,
                preconditions: vec![
                    StatePrecondition {
                        id: "resources_available".to_string(),
                        precondition_type: PreconditionType::ResourceAvailable,
                        parameters: HashMap::new(),
                        required: true,
                        description: "Required resources must be available".to_string(),
                    },
                ],
                postconditions: vec![
                    StatePostcondition {
                        id: "process_started".to_string(),
                        postcondition_type: PostconditionType::ProcessRunning,
                        parameters: HashMap::new(),
                        timeout: Duration::from_secs(30),
                        retry_config: None,
                        description: "Process must be running".to_string(),
                    },
                ],
                actions: vec![
                    StateAction {
                        id: "start_process".to_string(),
                        action_type: StateActionType::StartProcess,
                        parameters: HashMap::new(),
                        timeout: Some(Duration::from_secs(10)),
                        order: 1,
                        description: "Start the plugin process".to_string(),
                    },
                ],
                timeout: Some(Duration::from_secs(60)),
                rollback_actions: vec![
                    StateAction {
                        id: "stop_process".to_string(),
                        action_type: StateActionType::StopProcess,
                        parameters: HashMap::new(),
                        timeout: Some(Duration::from_secs(5)),
                        order: 1,
                        description: "Stop the plugin process".to_string(),
                    },
                ],
                priority: 100,
                enabled: true,
            },
            // Starting -> Running
            StateTransitionRule {
                from_state: PluginInstanceState::Starting,
                to_state: PluginInstanceState::Running,
                transition: StateTransition::CompleteStart,
                preconditions: vec![],
                postconditions: vec![
                    StatePostcondition {
                        id: "service_healthy".to_string(),
                        postcondition_type: PostconditionType::ServiceHealthy,
                        parameters: HashMap::new(),
                        timeout: Duration::from_secs(10),
                        retry_config: None,
                        description: "Service must be healthy".to_string(),
                    },
                ],
                actions: vec![
                    StateAction {
                        id: "verify_health".to_string(),
                        action_type: StateActionType::HealthCheck,
                        parameters: HashMap::new(),
                        timeout: Some(Duration::from_secs(5)),
                        order: 1,
                        description: "Verify service health".to_string(),
                    },
                ],
                timeout: Some(Duration::from_secs(15)),
                rollback_actions: vec![],
                priority: 100,
                enabled: true,
            },
            // Running -> Stopping
            StateTransitionRule {
                from_state: PluginInstanceState::Running,
                to_state: PluginInstanceState::Stopping,
                transition: StateTransition::Stop,
                preconditions: vec![],
                postconditions: vec![
                    StatePostcondition {
                        id: "process_stopped".to_string(),
                        postcondition_type: PostconditionType::ProcessRunning,
                        parameters: HashMap::from([("running".to_string(), serde_json::Value::Bool(false))]),
                        timeout: Duration::from_secs(30),
                        retry_config: None,
                        description: "Process must be stopped".to_string(),
                    },
                ],
                actions: vec![
                    StateAction {
                        id: "stop_process".to_string(),
                        action_type: StateActionType::StopProcess,
                        parameters: HashMap::new(),
                        timeout: Some(Duration::from_secs(10)),
                        order: 1,
                        description: "Stop the plugin process".to_string(),
                    },
                ],
                timeout: Some(Duration::from_secs(60)),
                rollback_actions: vec![],
                priority: 100,
                enabled: true,
            },
            // Stopping -> Stopped
            StateTransitionRule {
                from_state: PluginInstanceState::Stopping,
                to_state: PluginInstanceState::Stopped,
                transition: StateTransition::CompleteStop,
                preconditions: vec![],
                postconditions: vec![],
                actions: vec![
                    StateAction {
                        id: "cleanup_resources".to_string(),
                        action_type: StateActionType::Custom("cleanup_resources".to_string()),
                        parameters: HashMap::new(),
                        timeout: Some(Duration::from_secs(5)),
                        order: 1,
                        description: "Cleanup allocated resources".to_string(),
                    },
                ],
                timeout: Some(Duration::from_secs(10)),
                rollback_actions: vec![],
                priority: 100,
                enabled: true,
            },
            // Any state -> Error
            StateTransitionRule {
                from_state: PluginInstanceState::Running, // Can be any state
                to_state: PluginInstanceState::Error(String::new()),
                transition: StateTransition::Error("error".to_string()),
                preconditions: vec![],
                postconditions: vec![],
                actions: vec![
                    StateAction {
                        id: "log_error".to_string(),
                        action_type: StateActionType::LogMessage,
                        parameters: HashMap::new(),
                        timeout: Some(Duration::from_secs(1)),
                        order: 1,
                        description: "Log the error".to_string(),
                    },
                ],
                timeout: Some(Duration::from_secs(5)),
                rollback_actions: vec![],
                priority: 200, // High priority for error transitions
                enabled: true,
            },
        ];

        // Set the rules (this would need to be done in a constructor or separate method)
        // For now, we'll store them in a way that can be accessed later
    }

    /// Get current state of an instance
    pub async fn get_state(&self, instance_id: &str) -> PluginResult<PluginInstanceState> {
        let states = self.states.read().await;

        if let Some(state) = states.get(instance_id) {
            Ok(state.clone())
        } else {
            // Default to Created state if not found
            Ok(PluginInstanceState::Created)
        }
    }

    /// Set initial state of an instance
    pub async fn set_initial_state(&self, instance_id: &str, initial_state: PluginInstanceState) -> PluginResult<()> {
        info!("Setting initial state for instance {}: {:?}", instance_id, initial_state);

        {
            let mut states = self.states.write().await;
            states.insert(instance_id.to_string(), initial_state.clone());
        }

        // Record in history
        self.record_state_transition(instance_id, StateTransitionResult {
            success: true,
            previous_state: PluginInstanceState::Created, // Assume coming from created
            new_state: initial_state.clone(),
            transition: StateTransition::Create,
            timestamp: SystemTime::now(),
            duration: Duration::ZERO,
            message: Some("Initial state set".to_string()),
            error: None,
            metadata: StateTransitionMetadata {
                transition_id: uuid::Uuid::new_v4().to_string(),
                instance_id: instance_id.to_string(),
                triggering_operation: None,
                triggering_user: None,
                context: HashMap::new(),
                preconditions_checked: vec![],
                postconditions_verified: vec![],
            },
        }).await;

        // Update metrics
        self.update_state_metrics(instance_id, &initial_state).await;

        // Publish event
        self.publish_event(StateMachineEvent::StateEntered {
            instance_id: instance_id.to_string(),
            state: initial_state,
        }).await;

        Ok(())
    }

    /// Transition to a new state
    pub async fn transition_state(&self, instance_id: &str, transition: StateTransition) -> PluginResult<StateTransitionResult> {
        info!("Transitioning instance {} with transition: {:?}", instance_id, transition);

        // Check if transition is already in progress
        {
            let active_transitions = self.active_transitions.read().await;
            if active_transitions.contains_key(instance_id) {
                return Err(PluginError::lifecycle(format!(
                    "Transition already in progress for instance {}", instance_id
                )));
            }
        }

        // Get current state
        let current_state = self.get_state(instance_id).await?;

        // Find applicable transition rule
        let rule = self.find_transition_rule(&current_state, &transition).await?;

        if let Some(rule) = rule {
            // Check concurrent transition limit
            {
                let active_transitions = self.active_transitions.read().await;
                if active_transitions.len() >= self.config.max_concurrent_transitions as usize {
                    return Err(PluginError::lifecycle(
                        "Maximum concurrent transitions reached".to_string()
                    ));
                }
            }

            // Execute transition
            let result = self.execute_transition(instance_id, &rule).await?;

            // Update metrics
            self.update_transition_metrics(&transition, &result).await;

            Ok(result)
        } else {
            Err(PluginError::lifecycle(format!(
                "No transition rule found for state {:?} with transition {:?}",
                current_state, transition
            )))
        }
    }

    /// Find applicable transition rule
    async fn find_transition_rule(&self, current_state: &PluginInstanceState, transition: &StateTransition) -> PluginResult<Option<StateTransitionRule>> {
        let rules = self.transition_rules.read().await;

        for rule in rules.iter() {
            if rule.from_state == *current_state && rule.transition == *transition && rule.enabled {
                return Ok(Some(rule.clone()));
            }
        }

        Ok(None)
    }

    /// Execute a state transition
    async fn execute_transition(&self, instance_id: &str, rule: &StateTransitionRule) -> PluginResult<StateTransitionResult> {
        let start_time = SystemTime::now();
        let transition_id = uuid::Uuid::new_v4().to_string();

        // Create transition context
        let context = StateTransitionContext {
            instance_id: instance_id.to_string(),
            transition: rule.transition.clone(),
            start_time,
            transition_id: transition_id.clone(),
            cancellation_token: tokio_util::sync::CancellationToken::new(),
            current_action: 0,
            actions_completed: HashSet::new(),
            actions_failed: HashMap::new(),
        };

        // Register active transition
        {
            let mut active_transitions = self.active_transitions.write().await;
            active_transitions.insert(instance_id.to_string(), context);
        }

        // Publish transition started event
        self.publish_event(StateMachineEvent::TransitionStarted {
            instance_id: instance_id.to_string(),
            transition: rule.transition.clone(),
        }).await;

        let previous_state = self.get_state(instance_id).await?;

        // Execute transition
        let result = match self.perform_transition(instance_id, rule).await {
            Ok(()) => {
                // Update state
                {
                    let mut states = self.states.write().await;
                    states.insert(instance_id.to_string(), rule.to_state.clone());
                }

                // Publish state events
                self.publish_event(StateMachineEvent::StateExited {
                    instance_id: instance_id.to_string(),
                    state: previous_state.clone(),
                }).await;

                self.publish_event(StateMachineEvent::StateEntered {
                    instance_id: instance_id.to_string(),
                    state: rule.to_state.clone(),
                }).await;

                let duration = SystemTime::now().duration_since(start_time).unwrap_or(Duration::ZERO);

                StateTransitionResult {
                    success: true,
                    previous_state,
                    new_state: rule.to_state.clone(),
                    transition: rule.transition.clone(),
                    timestamp: SystemTime::now(),
                    duration,
                    message: Some("Transition completed successfully".to_string()),
                    error: None,
                    metadata: StateTransitionMetadata {
                        transition_id,
                        instance_id: instance_id.to_string(),
                        triggering_operation: None,
                        triggering_user: None,
                        context: HashMap::new(),
                        preconditions_checked: rule.preconditions.iter().map(|p| p.id.clone()).collect(),
                        postconditions_verified: vec![],
                    },
                }
            }
            Err(e) => {
                let duration = SystemTime::now().duration_since(start_time).unwrap_or(Duration::ZERO);

                // Publish error event
                self.publish_event(StateMachineEvent::TransitionFailed {
                    instance_id: instance_id.to_string(),
                    transition: rule.transition.clone(),
                    error: e.to_string(),
                }).await;

                StateTransitionResult {
                    success: false,
                    previous_state,
                    new_state: previous_state.clone(),
                    transition: rule.transition.clone(),
                    timestamp: SystemTime::now(),
                    duration,
                    message: None,
                    error: Some(e.to_string()),
                    metadata: StateTransitionMetadata {
                        transition_id,
                        instance_id: instance_id.to_string(),
                        triggering_operation: None,
                        triggering_user: None,
                        context: HashMap::new(),
                        preconditions_checked: rule.preconditions.iter().map(|p| p.id.clone()).collect(),
                        postconditions_verified: vec![],
                    },
                }
            }
        };

        // Remove from active transitions
        {
            let mut active_transitions = self.active_transitions.write().await;
            active_transitions.remove(instance_id);
        }

        // Record in history
        self.record_state_transition(instance_id, result.clone()).await;

        // Publish completion event
        self.publish_event(StateMachineEvent::TransitionCompleted {
            instance_id: instance_id.to_string(),
            result: result.clone(),
        }).await;

        // Trigger recovery if needed
        if !result.success && self.config.recovery_config.enabled {
            self.trigger_recovery_if_needed(instance_id, &result).await?;
        }

        Ok(result)
    }

    /// Perform the actual transition
    async fn perform_transition(&self, instance_id: &str, rule: &StateTransitionRule) -> PluginResult<()> {
        // Check preconditions
        self.check_preconditions(instance_id, &rule.preconditions).await?;

        // Execute actions in order
        let mut sorted_actions = rule.actions.clone();
        sorted_actions.sort_by_key(|a| a.order);

        for action in &sorted_actions {
            self.execute_action(instance_id, action).await?;
        }

        // Verify postconditions
        self.verify_postconditions(instance_id, &rule.postconditions).await?;

        Ok(())
    }

    /// Check preconditions
    async fn check_preconditions(&self, instance_id: &str, preconditions: &[StatePrecondition]) -> PluginResult<()> {
        for precondition in preconditions {
            if !self.check_precondition(instance_id, precondition).await? {
                if precondition.required {
                    return Err(PluginError::lifecycle(format!(
                        "Required precondition failed: {}", precondition.description
                    )));
                } else {
                    warn!("Optional precondition failed for instance {}: {}", instance_id, precondition.description);
                }
            }
        }
        Ok(())
    }

    /// Check a single precondition
    async fn check_precondition(&self, instance_id: &str, precondition: &StatePrecondition) -> PluginResult<bool> {
        match precondition.precondition_type {
            PreconditionType::ResourceAvailable => {
                // TODO: Check resource availability
                Ok(true) // Placeholder
            }
            PreconditionType::DependencyHealthy => {
                // TODO: Check dependency health
                Ok(true) // Placeholder
            }
            PreconditionType::HealthCheck => {
                // TODO: Perform health check
                Ok(true) // Placeholder
            }
            PreconditionType::ConfigValid => {
                // TODO: Validate configuration
                Ok(true) // Placeholder
            }
            PreconditionType::PermissionCheck => {
                // TODO: Check permissions
                Ok(true) // Placeholder
            }
            PreconditionType::TimeWindow => {
                // TODO: Check time window
                Ok(true) // Placeholder
            }
            PreconditionType::Custom(_) => {
                // TODO: Implement custom precondition
                Ok(true) // Placeholder
            }
        }
    }

    /// Execute an action
    async fn execute_action(&self, instance_id: &str, action: &StateAction) -> PluginResult<()> {
        debug!("Executing action {} for instance {}", action.id, instance_id);

        let timeout = action.timeout.unwrap_or(self.config.default_timeout);

        match action.action_type {
            StateActionType::StartProcess => {
                // TODO: Implement process start
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            StateActionType::StopProcess => {
                // TODO: Implement process stop
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            StateActionType::SendSignal => {
                // TODO: Implement signal sending
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            StateActionType::ExecuteCommand => {
                // TODO: Implement command execution
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            StateActionType::CreateDirectory => {
                // TODO: Implement directory creation
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            StateActionType::RemoveDirectory => {
                // TODO: Implement directory removal
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            StateActionType::WriteFile => {
                // TODO: Implement file writing
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            StateActionType::ReadFile => {
                // TODO: Implement file reading
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            StateActionType::ConnectToService => {
                // TODO: Implement service connection
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            StateActionType::DisconnectFromService => {
                // TODO: Implement service disconnection
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            StateActionType::WaitForCondition => {
                // TODO: Implement condition waiting
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            StateActionType::LogMessage => {
                info!("Action log for instance {}: {}", instance_id, action.description);
            }
            StateActionType::SendNotification => {
                // TODO: Implement notification sending
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            StateActionType::Custom(_) => {
                // TODO: Implement custom action
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        debug!("Completed action {} for instance {}", action.id, instance_id);
        Ok(())
    }

    /// Verify postconditions
    async fn verify_postconditions(&self, instance_id: &str, postconditions: &[StatePostcondition]) -> PluginResult<()> {
        for postcondition in postconditions {
            if !self.verify_postcondition(instance_id, postcondition).await? {
                return Err(PluginError::lifecycle(format!(
                    "Postcondition verification failed: {}", postcondition.description
                )));
            }
        }
        Ok(())
    }

    /// Verify a single postcondition
    async fn verify_postcondition(&self, instance_id: &str, postcondition: &StatePostcondition) -> PluginResult<bool> {
        match postcondition.postcondition_type {
            PostconditionType::ProcessRunning => {
                // TODO: Check if process is running
                Ok(true) // Placeholder
            }
            PostconditionType::ServiceHealthy => {
                // TODO: Check service health
                Ok(true) // Placeholder
            }
            PostconditionType::PortListening => {
                // TODO: Check if port is listening
                Ok(true) // Placeholder
            }
            PostconditionType::FileExists => {
                // TODO: Check if file exists
                Ok(true) // Placeholder
            }
            PostconditionType::ApiEndpointResponding => {
                // TODO: Check API endpoint
                Ok(true) // Placeholder
            }
            PostconditionType::Custom(_) => {
                // TODO: Implement custom postcondition
                Ok(true) // Placeholder
            }
        }
    }

    /// Record state transition in history
    async fn record_state_transition(&self, instance_id: &str, result: StateTransitionResult) {
        let mut history = self.state_history.write().await;
        let instance_history = history.entry(instance_id.to_string()).or_insert_with(VecDeque::new);

        instance_history.push_back(result.clone());

        // Keep only last 100 transitions per instance
        if instance_history.len() > 100 {
            instance_history.pop_front();
        }
    }

    /// Get state history for an instance
    pub async fn get_state_history(&self, instance_id: &str, limit: Option<usize>) -> PluginResult<Vec<StateTransitionResult>> {
        let history = self.state_history.read().await;

        if let Some(instance_history) = history.get(instance_id) {
            let transitions: Vec<StateTransitionResult> = instance_history.iter().cloned().collect();

            if let Some(limit) = limit {
                let start = if transitions.len() > limit { transitions.len() - limit } else { 0 };
                Ok(transitions[start..].to_vec())
            } else {
                Ok(transitions)
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Get all instances and their states
    pub async fn get_all_states(&self) -> PluginResult<HashMap<String, PluginInstanceState>> {
        let states = self.states.read().await;
        Ok(states.clone())
    }

    /// Get instances in a specific state
    pub async fn get_instances_by_state(&self, state: PluginInstanceState) -> PluginResult<Vec<String>> {
        let states = self.states.read().await;

        let instances: Vec<String> = states
            .iter()
            .filter(|(_, s)| **s == state)
            .map(|(id, _)| id.clone())
            .collect();

        Ok(instances)
    }

    /// Cancel an active transition
    pub async fn cancel_transition(&self, instance_id: &str) -> PluginResult<bool> {
        let mut active_transitions = self.active_transitions.write().await;

        if let Some(context) = active_transitions.get_mut(instance_id) {
            context.cancellation_token.cancel();
            info!("Cancelled transition for instance {}", instance_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Trigger recovery if needed
    async fn trigger_recovery_if_needed(&self, instance_id: &str, result: &StateTransitionResult) -> PluginResult<()> {
        if !self.config.recovery_config.enabled {
            return Ok(());
        }

        let current_state = &result.new_state;

        if self.config.recovery_config.recoverable_states.contains(current_state) {
            info!("Triggering recovery for instance {} in state {:?}", instance_id, current_state);

            // Publish recovery event
            self.publish_event(StateMachineEvent::RecoveryTriggered {
                instance_id: instance_id.to_string(),
                state: current_state.clone(),
                strategy: self.config.recovery_config.strategy.clone(),
            }).await;

            // Attempt recovery
            match self.perform_recovery(instance_id, &result.new_state).await {
                Ok(()) => {
                    self.publish_event(StateMachineEvent::RecoveryCompleted {
                        instance_id: instance_id.to_string(),
                        success: true,
                    }).await;
                }
                Err(e) => {
                    error!("Recovery failed for instance {}: {}", instance_id, e);
                    self.publish_event(StateMachineEvent::RecoveryCompleted {
                        instance_id: instance_id.to_string(),
                        success: false,
                    }).await;
                }
            }
        }

        Ok(())
    }

    /// Perform recovery
    async fn perform_recovery(&self, instance_id: &str, from_state: &PluginInstanceState) -> PluginResult<()> {
        let strategy = &self.config.recovery_config.strategy;

        match strategy {
            RecoveryStrategy::Restart => {
                // Transition to stopped then to running
                self.transition_state(instance_id, StateTransition::Stop).await?;
                tokio::time::sleep(self.config.recovery_config.delay).await;
                self.transition_state(instance_id, StateTransition::Start).await?;
            }
            RecoveryStrategy::Reset => {
                // Reset to created state
                {
                    let mut states = self.states.write().await;
                    states.insert(instance_id.to_string(), PluginInstanceState::Created);
                }
            }
            RecoveryStrategy::Rollback => {
                // TODO: Implement rollback to previous state
                unimplemented!("Rollback recovery not yet implemented");
            }
            RecoveryStrategy::Recreate => {
                // TODO: Implement recreation of instance
                unimplemented!("Recreate recovery not yet implemented");
            }
            RecoveryStrategy::Manual => {
                // Log and wait for manual intervention
                warn!("Manual recovery required for instance {} in state {:?}", instance_id, from_state);
            }
        }

        Ok(())
    }

    /// Update state metrics
    async fn update_state_metrics(&self, instance_id: &str, state: &PluginInstanceState) {
        let mut metrics = self.metrics.write().await;

        // Remove from previous state count
        for (s, count) in metrics.state_distribution.iter_mut() {
            if *s == *state {
                *count += 1;
            }
        }

        metrics.last_updated = SystemTime::now();
    }

    /// Update transition metrics
    async fn update_transition_metrics(&self, transition: &StateTransition, result: &StateTransitionResult) {
        let mut metrics = self.metrics.write().await;

        metrics.total_transitions += 1;

        if result.success {
            metrics.successful_transitions += 1;
        } else {
            metrics.failed_transitions += 1;
        }

        // Update average transition time
        let total_time = metrics.average_transition_time * (metrics.total_transitions - 1) + result.duration;
        metrics.average_transition_time = total_time / metrics.total_transitions;

        // Update transitions by type
        let transition_type = format!("{:?}", transition);
        *metrics.transitions_by_type.entry(transition_type).or_insert(0) += 1;

        metrics.last_updated = SystemTime::now();
    }

    /// Get state machine metrics
    pub async fn get_metrics(&self) -> StateMachineMetrics {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// Publish state machine event
    async fn publish_event(&self, event: StateMachineEvent) {
        let mut subscribers = self.event_subscribers.write().await;
        let mut to_remove = Vec::new();

        for (i, sender) in subscribers.iter().enumerate() {
            if sender.send(event.clone()).is_err() {
                to_remove.push(i);
            }
        }

        // Remove dead subscribers
        for i in to_remove.into_iter().rev() {
            subscribers.remove(i);
        }
    }

    /// Load persisted states
    async fn load_persisted_states(&self) -> PluginResult<()> {
        // TODO: Implement state persistence loading
        info!("Loading persisted states (not implemented)");
        Ok(())
    }

    /// Persist states
    async fn persist_states(&self) -> PluginResult<()> {
        // TODO: Implement state persistence
        debug!("Persisting states (not implemented)");
        Ok(())
    }

    /// Start background tasks
    async fn start_background_tasks(&self) -> PluginResult<()> {
        // Start state persistence task
        if self.config.enable_persistence {
            let states = self.states.clone();
            let persistence_interval = Duration::from_secs(60);

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(persistence_interval);

                loop {
                    interval.tick().await;
                    // TODO: Persist states
                    debug!("Persisting states");
                }
            });
        }

        // Start metrics collection
        let metrics = self.metrics.clone();
        let states = self.states.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                interval.tick().await;

                // Update state distribution
                let state_counts = {
                    let states_guard = states.read().await;
                    let mut distribution = HashMap::new();

                    for state in states_guard.values() {
                        *distribution.entry(state.clone()).or_insert(0) += 1;
                    }

                    distribution
                };

                {
                    let mut metrics_guard = metrics.write().await;
                    metrics_guard.state_distribution = state_counts;
                    metrics_guard.last_updated = SystemTime::now();
                }
            }
        });

        Ok(())
    }

    /// Subscribe to state machine events
    pub async fn subscribe_events(&self) -> tokio::sync::mpsc::UnboundedReceiver<StateMachineEvent> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let mut subscribers = self.event_subscribers.write().await;
        subscribers.push(tx);

        rx
    }
}

#[async_trait]
pub trait StateMachineService: Send + Sync {
    /// Get current state
    async fn get_state(&self, instance_id: &str) -> PluginResult<PluginInstanceState>;

    /// Set initial state
    async fn set_initial_state(&self, instance_id: &str, state: PluginInstanceState) -> PluginResult<()>;

    /// Transition state
    async fn transition_state(&self, instance_id: &str, transition: StateTransition) -> PluginResult<StateTransitionResult>;

    /// Get state history
    async fn get_state_history(&self, instance_id: &str, limit: Option<usize>) -> PluginResult<Vec<StateTransitionResult>>;

    /// Get all states
    async fn get_all_states(&self) -> PluginResult<HashMap<String, PluginInstanceState>>;

    /// Get instances by state
    async fn get_instances_by_state(&self, state: PluginInstanceState) -> PluginResult<Vec<String>>;

    /// Cancel transition
    async fn cancel_transition(&self, instance_id: &str) -> PluginResult<bool>;

    /// Subscribe to events
    async fn subscribe_events(&self) -> tokio::sync::mpsc::UnboundedReceiver<StateMachineEvent>;

    /// Get metrics
    async fn get_metrics(&self) -> StateMachineMetrics;
}

#[async_trait]
impl StateMachineService for PluginStateMachine {
    async fn get_state(&self, instance_id: &str) -> PluginResult<PluginInstanceState> {
        self.get_state(instance_id).await
    }

    async fn set_initial_state(&self, instance_id: &str, state: PluginInstanceState) -> PluginResult<()> {
        self.set_initial_state(instance_id, state).await
    }

    async fn transition_state(&self, instance_id: &str, transition: StateTransition) -> PluginResult<StateTransitionResult> {
        self.transition_state(instance_id, transition).await
    }

    async fn get_state_history(&self, instance_id: &str, limit: Option<usize>) -> PluginResult<Vec<StateTransitionResult>> {
        self.get_state_history(instance_id, limit).await
    }

    async fn get_all_states(&self) -> PluginResult<HashMap<String, PluginInstanceState>> {
        self.get_all_states().await
    }

    async fn get_instances_by_state(&self, state: PluginInstanceState) -> PluginResult<Vec<String>> {
        self.get_instances_by_state(state).await
    }

    async fn cancel_transition(&self, instance_id: &str) -> PluginResult<bool> {
        self.cancel_transition(instance_id).await
    }

    async fn subscribe_events(&self) -> tokio::sync::mpsc::UnboundedReceiver<StateMachineEvent> {
        self.subscribe_events().await
    }

    async fn get_metrics(&self) -> StateMachineMetrics {
        self.get_metrics().await
    }
}