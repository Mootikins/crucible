//! Event filter engine for content-based event filtering

use crate::events::{DaemonEvent, EventFilter, EventType, EventPriority};
use crate::plugin_events::{
    error::{SubscriptionError, SubscriptionResult},
    types::SubscriptionId,
};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Advanced event filter engine with content-based filtering capabilities
#[derive(Clone)]
pub struct FilterEngine {
    /// Inner engine state
    inner: Arc<RwLock<FilterEngineInner>>,
}

/// Internal filter engine state
struct FilterEngineInner {
    /// Compiled filter cache
    compiled_filters: HashMap<String, CompiledFilter>,

    /// Filter execution statistics
    filter_stats: HashMap<String, FilterStats>,

    /// Engine configuration
    config: FilterEngineConfig,

    /// Custom filter functions registry
    custom_filters: HashMap<String, Arc<dyn FilterFunction + Send + Sync>>,

    /// Performance metrics
    metrics: EngineMetrics,
}

/// Compiled filter representation
#[derive(Clone)]
struct CompiledFilter {
    /// Filter expression
    expression: FilterExpression,

    /// Filter complexity score
    complexity: u8,

    /// Optimized execution path
    execution_path: ExecutionPath,

    /// Filter validation result
    validation: FilterValidation,
}

/// Filter expression tree
#[derive(Debug, Clone)]
enum FilterExpression {
    /// Logical AND
    And(Vec<FilterExpression>),

    /// Logical OR
    Or(Vec<FilterExpression>),

    /// Logical NOT
    Not(Box<FilterExpression>),

    /// Equality comparison
    Equals { field: String, value: Value },

    /// Inequality comparison
    NotEquals { field: String, value: Value },

    /// Greater than comparison
    GreaterThan { field: String, value: Value },

    /// Greater than or equal comparison
    GreaterThanOrEqual { field: String, value: Value },

    /// Less than comparison
    LessThan { field: String, value: Value },

    /// Less than or equal comparison
    LessThanOrEqual { field: String, value: Value },

    /// Contains operation
    Contains { field: String, value: Value },

    /// StartsWith operation
    StartsWith { field: String, value: Value },

    /// EndsWith operation
    EndsWith { field: String, value: Value },

    /// Regex match
    RegexMatch { field: String, pattern: String },

    /// In list operation
    In { field: String, values: Vec<Value> },

    /// Not in list operation
    NotIn { field: String, values: Vec<Value> },

    /// Range check
    Between { field: String, min: Value, max: Value },

    /// Null check
    IsNull { field: String },

    /// Not null check
    IsNotNull { field: String },

    /// Custom function call
    Function { name: String, args: Vec<Value> },

    /// Constant value
    Constant(Value),
}

/// Execution path optimization
#[derive(Debug, Clone)]
enum ExecutionPath {
    /// Sequential execution
    Sequential(Vec<FilterExpression>),

    /// Parallel execution
    Parallel(Vec<FilterExpression>),

    /// Short-circuit execution
    ShortCircuit(Vec<FilterExpression>),

    /// Indexed execution
    Indexed { index_field: String, expressions: HashMap<Value, Vec<FilterExpression>> },
}

/// Filter validation result
#[derive(Debug, Clone)]
struct FilterValidation {
    /// Is filter valid
    valid: bool,

    /// Validation errors
    errors: Vec<String>,

    /// Validation warnings
    warnings: Vec<String>,

    /// Estimated performance impact
    performance_impact: PerformanceImpact,
}

/// Performance impact assessment
#[derive(Debug, Clone)]
enum PerformanceImpact {
    Low,
    Medium,
    High,
    VeryHigh,
}

/// Filter execution statistics
#[derive(Debug, Clone, Default)]
struct FilterStats {
    /// Total executions
    executions: u64,

    /// Total matches
    matches: u64,

    /// Total execution time in nanoseconds
    total_time_ns: u64,

    /// Average execution time in nanoseconds
    avg_time_ns: u64,

    /// Last execution timestamp
    last_execution: Option<DateTime<Utc>>,

    /// Cache hit rate
    cache_hit_rate: f64,
}

/// Filter engine configuration
#[derive(Debug, Clone)]
pub struct FilterEngineConfig {
    /// Maximum filter complexity
    pub max_complexity: u8,

    /// Enable filter compilation cache
    pub enable_cache: bool,

    /// Maximum cache size
    pub max_cache_size: usize,

    /// Enable parallel execution
    pub enable_parallel: bool,

    /// Filter execution timeout in milliseconds
    pub execution_timeout_ms: u64,

    /// Enable statistics collection
    pub enable_stats: bool,

    /// Maximum regex complexity
    pub max_regex_complexity: u8,
}

impl Default for FilterEngineConfig {
    fn default() -> Self {
        Self {
            max_complexity: 10,
            enable_cache: true,
            max_cache_size: 1000,
            enable_parallel: true,
            execution_timeout_ms: 1000,
            enable_stats: true,
            max_regex_complexity: 5,
        }
    }
}

/// Engine performance metrics
#[derive(Debug, Clone, Default)]
struct EngineMetrics {
    /// Total filters compiled
    total_compiled: u64,

    /// Total filter executions
    total_executions: u64,

    /// Cache hits
    cache_hits: u64,

    /// Cache misses
    cache_misses: u64,

    /// Average compilation time in microseconds
    avg_compilation_time_us: u64,

    /// Average execution time in microseconds
    avg_execution_time_us: u64,

    /// Memory usage in bytes
    memory_usage_bytes: u64,
}

/// Custom filter function trait
pub trait FilterFunction: Send + Sync {
    /// Execute the custom filter
    fn execute(&self, event: &DaemonEvent, args: &[Value]) -> SubscriptionResult<bool>;

    /// Get function metadata
    fn metadata(&self) -> FilterFunctionMetadata;
}

/// Custom filter function metadata
#[derive(Debug, Clone)]
pub struct FilterFunctionMetadata {
    /// Function name
    pub name: String,

    /// Description
    pub description: String,

    /// Parameter definitions
    pub parameters: Vec<ParameterDefinition>,

    /// Return type description
    pub return_type: String,

    /// Example usage
    pub example: Option<String>,
}

/// Parameter definition for custom filters
#[derive(Debug, Clone)]
pub struct ParameterDefinition {
    /// Parameter name
    pub name: String,

    /// Parameter type
    pub param_type: String,

    /// Required flag
    pub required: bool,

    /// Default value
    pub default_value: Option<Value>,

    /// Description
    pub description: Option<String>,
}

impl FilterEngine {
    /// Create a new filter engine with default configuration
    pub fn new() -> Self {
        Self::with_config(FilterEngineConfig::default())
    }

    /// Create a new filter engine with custom configuration
    pub fn with_config(config: FilterEngineConfig) -> Self {
        let inner = FilterEngineInner {
            compiled_filters: HashMap::new(),
            filter_stats: HashMap::new(),
            config,
            custom_filters: HashMap::new(),
            metrics: EngineMetrics::default(),
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Register a custom filter function
    pub async fn register_custom_function<F>(&self, func: F) -> SubscriptionResult<()>
    where
        F: FilterFunction + 'static,
    {
        let mut inner = self.inner.write().await;
        let metadata = func.metadata();
        inner.custom_filters.insert(metadata.name.clone(), Arc::new(func));
        Ok(())
    }

    /// Compile a filter expression
    pub async fn compile_filter(&self, filter: &EventFilter) -> SubscriptionResult<String> {
        let mut inner = self.inner.write().await;

        // Generate filter key
        let filter_key = self.generate_filter_key(filter);

        // Check cache
        if inner.config.enable_cache && inner.compiled_filters.contains_key(&filter_key) {
            if inner.config.enable_stats {
                inner.metrics.cache_hits += 1;
            }
            return Ok(filter_key);
        }

        if inner.config.enable_stats {
            inner.metrics.cache_misses += 1;
        }

        // Compile filter
        let start_time = std::time::Instant::now();
        let compiled = self.compile_filter_internal(filter)?;
        let compilation_time = start_time.elapsed();

        // Validate compiled filter
        let validation = self.validate_compiled_filter(&compiled);
        if !validation.valid {
            return Err(SubscriptionError::FilteringError(format!(
                "Filter validation failed: {}",
                validation.errors.join(", ")
            )));
        }

        // Check complexity
        if compiled.complexity > inner.config.max_complexity {
            return Err(SubscriptionError::FilteringError(format!(
                "Filter complexity {} exceeds maximum {}",
                compiled.complexity, inner.config.max_complexity
            )));
        }

        // Create compiled filter
        let compiled_filter = CompiledFilter {
            expression: compiled,
            complexity: 0, // Will be calculated
            execution_path: ExecutionPath::Sequential(vec![]), // Will be optimized
            validation,
        };

        // Store in cache
        if inner.config.enable_cache {
            // Implement cache size limit
            if inner.compiled_filters.len() >= inner.config.max_cache_size {
                // Remove oldest entry (simple LRU)
                if let Some(oldest_key) = inner.compiled_filters.keys().next().cloned() {
                    inner.compiled_filters.remove(&oldest_key);
                    inner.filter_stats.remove(&oldest_key);
                }
            }

            inner.compiled_filters.insert(filter_key.clone(), compiled_filter);

            // Initialize statistics
            if inner.config.enable_stats {
                inner.filter_stats.insert(filter_key.clone(), FilterStats::default());
            }
        }

        // Update metrics
        if inner.config.enable_stats {
            inner.metrics.total_compiled += 1;
            inner.metrics.avg_compilation_time_us =
                (inner.metrics.avg_compilation_time_us * (inner.metrics.total_compiled - 1) +
                 compilation_time.as_micros() as u64) / inner.metrics.total_compiled;
        }

        debug!("Compiled filter: {} ({} μs)", filter_key, compilation_time.as_micros());

        Ok(filter_key)
    }

    /// Check if an event matches a compiled filter
    pub async fn matches_filter(
        &self,
        event: &DaemonEvent,
        filter_key: &str,
    ) -> SubscriptionResult<bool> {
        let inner = self.inner.read().await;

        let compiled_filter = inner.compiled_filters
            .get(filter_key)
            .ok_or_else(|| SubscriptionError::FilteringError(
                format!("Filter {} not found", filter_key)
            ))?;

        // Execute filter with timeout
        let start_time = std::time::Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(inner.config.execution_timeout_ms),
            self.execute_filter_expression(&compiled_filter.expression, event)
        ).await;

        let execution_time = start_time.elapsed();

        // Update statistics
        if inner.config.enable_stats {
            if let Ok(stats) = inner.filter_stats.get_mut(filter_key) {
                stats.executions += 1;
                stats.total_time_ns += execution_time.as_nanos() as u64;
                stats.avg_time_ns = stats.total_time_ns / stats.executions;
                stats.last_execution = Some(Utc::now());

                if let Ok(matches) = result {
                    if matches {
                        stats.matches += 1;
                    }
                }
            }

            inner.metrics.total_executions += 1;
            inner.metrics.avg_execution_time_us =
                (inner.metrics.avg_execution_time_us * (inner.metrics.total_executions - 1) +
                 execution_time.as_micros() as u64) / inner.metrics.total_executions;
        }

        match result {
            Ok(matches) => Ok(matches),
            Err(_) => Err(SubscriptionError::FilteringError(
                "Filter execution timeout".to_string()
            )),
        }
    }

    /// Batch filter matching for multiple filters
    pub async fn batch_match_filters(
        &self,
        event: &DaemonEvent,
        filter_keys: &[String],
    ) -> SubscriptionResult<Vec<(String, bool)>> {
        if inner.config.enable_parallel && filter_keys.len() > 1 {
            // Execute filters in parallel
            let futures: Vec<_> = filter_keys
                .iter()
                .map(|key| {
                    let engine = self.clone();
                    let key = key.clone();
                    let event = event.clone();
                    async move {
                        let result = engine.matches_filter(&event, &key).await;
                        (key, result)
                    }
                })
                .collect();

            let results = futures_util::future::join_all(futures).await;

            Ok(results
                .into_iter()
                .map(|(key, result)| {
                    let matches = result.unwrap_or(false);
                    (key, matches)
                })
                .collect())
        } else {
            // Execute filters sequentially
            let mut results = Vec::new();
            for key in filter_keys {
                let matches = self.matches_filter(event, key).await.unwrap_or(false);
                results.push((key.clone(), matches));
            }
            Ok(results)
        }
    }

    /// Get filter statistics
    pub async fn get_filter_stats(&self, filter_key: &str) -> Option<FilterStats> {
        let inner = self.inner.read().await;
        inner.filter_stats.get(filter_key).cloned()
    }

    /// Get engine metrics
    pub async fn get_metrics(&self) -> EngineMetrics {
        let inner = self.inner.read().await;
        inner.metrics.clone()
    }

    /// Clear filter cache
    pub async fn clear_cache(&self) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;
        inner.compiled_filters.clear();
        inner.filter_stats.clear();
        inner.metrics.cache_hits = 0;
        inner.metrics.cache_misses = 0;
        Ok(())
    }

    /// Generate filter key for caching
    fn generate_filter_key(&self, filter: &EventFilter) -> String {
        // Create a deterministic key from filter content
        format!("{}|{}|{}|{}|{}|{}|{}",
            filter.event_types.join(","),
            filter.categories.iter().map(|c| format!("{:?}", c)).collect::<Vec<_>>().join(","),
            filter.priorities.iter().map(|p| format!("{:?}", p)).collect::<Vec<_>>().join(","),
            filter.sources.join(","),
            filter.expression.as_deref().unwrap_or(""),
            filter.max_payload_size.unwrap_or(0),
            serde_json::to_string(filter).unwrap_or_default()
        )
    }

    /// Internal filter compilation
    fn compile_filter_internal(&self, filter: &EventFilter) -> SubscriptionResult<FilterExpression> {
        let mut expressions = Vec::new();

        // Compile event type filters
        if !filter.event_types.is_empty() {
            expressions.push(FilterExpression::In {
                field: "event_type".to_string(),
                values: filter.event_types.iter().map(|t| json!(t)).collect(),
            });
        }

        // Compile category filters
        if !filter.categories.is_empty() {
            expressions.push(FilterExpression::In {
                field: "event_category".to_string(),
                values: filter.categories.iter().map(|c| json!(format!("{:?}", c))).collect(),
            });
        }

        // Compile priority filters
        if !filter.priorities.is_empty() {
            expressions.push(FilterExpression::In {
                field: "event_priority".to_string(),
                values: filter.priorities.iter().map(|p| json!(p.value())).collect(),
            });
        }

        // Compile source filters
        if !filter.sources.is_empty() {
            expressions.push(FilterExpression::In {
                field: "source_id".to_string(),
                values: filter.sources.iter().map(|s| json!(s)).collect(),
            });
        }

        // Compile custom expression
        if let Some(expression) = &filter.expression {
            let compiled_expr = self.compile_custom_expression(expression)?;
            expressions.push(compiled_expr);
        }

        // Combine expressions with AND logic
        match expressions.len() {
            0 => Ok(FilterExpression::Constant(json!(true))),
            1 => Ok(expressions.into_iter().next().unwrap()),
            _ => Ok(FilterExpression::And(expressions)),
        }
    }

    /// Compile custom expression string
    fn compile_custom_expression(&self, expression: &str) -> SubscriptionResult<FilterExpression> {
        // This is a simplified implementation
        // In a production system, you'd use a proper expression parser
        // like pest, nom, or a SQL parser for complex expressions

        // Handle simple field=value expressions
        if let Some((field, value)) = expression.split_once('=') {
            return Ok(FilterExpression::Equals {
                field: field.trim().to_string(),
                value: json!(value.trim().trim_matches('"')),
            });
        }

        // Handle field!=value expressions
        if let Some((field, value)) = expression.split_once("!=") {
            return Ok(FilterExpression::NotEquals {
                field: field.trim().to_string(),
                value: json!(value.trim().trim_matches('"')),
            });
        }

        // Handle field>value expressions
        if let Some((field, value)) = expression.split_once('>') {
            return Ok(FilterExpression::GreaterThan {
                field: field.trim().to_string(),
                value: json!(value.trim()),
            });
        }

        // Handle field<value expressions
        if let Some((field, value)) = expression.split_once('<') {
            return Ok(FilterExpression::LessThan {
                field: field.trim().to_string(),
                value: json!(value.trim()),
            });
        }

        // Handle contains expressions
        if expression.contains("contains") {
            let parts: Vec<&str> = expression.splitn(3, ' ').collect();
            if parts.len() == 3 {
                return Ok(FilterExpression::Contains {
                    field: parts[0].to_string(),
                    value: json!(parts[2].trim_matches('"')),
                });
            }
        }

        // Handle regex expressions
        if expression.contains("matches") {
            let parts: Vec<&str> = expression.splitn(3, ' ').collect();
            if parts.len() == 3 {
                return Ok(FilterExpression::RegexMatch {
                    field: parts[0].to_string(),
                    pattern: parts[2].trim_matches('"').to_string(),
                });
            }
        }

        Err(SubscriptionError::FilteringError(
            format!("Unable to parse expression: {}", expression)
        ))
    }

    /// Validate compiled filter
    fn validate_compiled_filter(&self, expression: &FilterExpression) -> FilterValidation {
        let mut validation = FilterValidation {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            performance_impact: PerformanceImpact::Low,
        };

        // Recursively validate expression
        self.validate_expression_recursive(expression, &mut validation);

        validation
    }

    /// Recursive expression validation
    fn validate_expression_recursive(&self, expression: &FilterExpression, validation: &mut FilterValidation) {
        match expression {
            FilterExpression::RegexMatch { pattern, .. } => {
                // Validate regex pattern
                if let Err(e) = Regex::new(pattern) {
                    validation.valid = false;
                    validation.errors.push(format!("Invalid regex pattern '{}': {}", pattern, e));
                } else {
                    // Check regex complexity
                    let complexity = self.calculate_regex_complexity(pattern);
                    if complexity > 10 {
                        validation.warnings.push("High regex complexity may impact performance".to_string());
                        validation.performance_impact = PerformanceImpact::High;
                    }
                }
            }

            FilterExpression::Function { name, .. } => {
                // Validate custom function exists
                // This would need access to the custom_functions map
                validation.warnings.push(format!("Custom function '{}' not validated at compile time", name));
            }

            FilterExpression::And(exprs) | FilterExpression::Or(exprs) => {
                for expr in exprs {
                    self.validate_expression_recursive(expr, validation);
                }
            }

            FilterExpression::Not(expr) => {
                self.validate_expression_recursive(expr, validation);
            }

            _ => {} // Other expression types are generally safe
        }
    }

    /// Calculate regex complexity score
    fn calculate_regex_complexity(&self, pattern: &str) -> u8 {
        let mut complexity = 0u8;

        // Add complexity for various regex features
        if pattern.contains('*') || pattern.contains('+') { complexity += 1; }
        if pattern.contains('?') { complexity += 1; }
        if pattern.contains('{') { complexity += 2; }
        if pattern.contains('(') { complexity += 2; }
        if pattern.contains('[') { complexity += 1; }
        if pattern.contains('^') || pattern.contains('$') { complexity += 1; }

        complexity
    }

    /// Execute filter expression
    async fn execute_filter_expression(&self, expression: &FilterExpression, event: &DaemonEvent) -> SubscriptionResult<bool> {
        match expression {
            FilterExpression::Constant(value) => {
                Ok(value.as_bool().unwrap_or(false))
            }

            FilterExpression::Equals { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                Ok(event_value == *value)
            }

            FilterExpression::NotEquals { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                Ok(event_value != *value)
            }

            FilterExpression::GreaterThan { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                self.compare_values(&event_value, value, |a, b| a > b)
            }

            FilterExpression::GreaterThanOrEqual { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                self.compare_values(&event_value, value, |a, b| a >= b)
            }

            FilterExpression::LessThan { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                self.compare_values(&event_value, value, |a, b| a < b)
            }

            FilterExpression::LessThanOrEqual { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                self.compare_values(&event_value, value, |a, b| a <= b)
            }

            FilterExpression::Contains { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                let event_str = self.value_to_string(&event_value);
                let value_str = self.value_to_string(value);
                Ok(event_str.contains(&value_str))
            }

            FilterExpression::StartsWith { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                let event_str = self.value_to_string(&event_value);
                let value_str = self.value_to_string(value);
                Ok(event_str.starts_with(&value_str))
            }

            FilterExpression::EndsWith { field, value } => {
                let event_value = self.extract_field_value(event, field)?;
                let event_str = self.value_to_string(&event_value);
                let value_str = self.value_to_string(value);
                Ok(event_str.ends_with(&value_str))
            }

            FilterExpression::RegexMatch { field, pattern } => {
                let event_value = self.extract_field_value(event, field)?;
                let event_str = self.value_to_string(&event_value);

                match Regex::new(pattern) {
                    Ok(regex) => Ok(regex.is_match(&event_str)),
                    Err(e) => Err(SubscriptionError::FilteringError(
                        format!("Invalid regex pattern '{}': {}", pattern, e)
                    )),
                }
            }

            FilterExpression::In { field, values } => {
                let event_value = self.extract_field_value(event, field)?;
                Ok(values.contains(&event_value))
            }

            FilterExpression::NotIn { field, values } => {
                let event_value = self.extract_field_value(event, field)?;
                Ok(!values.contains(&event_value))
            }

            FilterExpression::Between { field, min, max } => {
                let event_value = self.extract_field_value(event, field)?;
                let ge_min = self.compare_values(&event_value, min, |a, b| a >= b)?;
                let le_max = self.compare_values(&event_value, max, |a, b| a <= b)?;
                Ok(ge_min && le_max)
            }

            FilterExpression::IsNull { field } => {
                let event_value = self.extract_field_value(event, field)?;
                Ok(event_value.is_null())
            }

            FilterExpression::IsNotNull { field } => {
                let event_value = self.extract_field_value(event, field)?;
                Ok(!event_value.is_null())
            }

            FilterExpression::Function { name, args } => {
                // This would need access to custom_functions map
                // For now, return false
                warn!("Custom function '{}' not implemented", name);
                Ok(false)
            }

            FilterExpression::And(expressions) => {
                for expr in expressions {
                    if !self.execute_filter_expression(expr, event).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }

            FilterExpression::Or(expressions) => {
                for expr in expressions {
                    if self.execute_filter_expression(expr, event).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            FilterExpression::Not(expr) => {
                Ok(!self.execute_filter_expression(expr, event).await?)
            }
        }
    }

    /// Extract field value from event
    fn extract_field_value(&self, event: &DaemonEvent, field: &str) -> SubscriptionResult<Value> {
        match field {
            "event_type" => Ok(json!(match &event.event_type {
                EventType::Filesystem(_) => "filesystem",
                EventType::Database(_) => "database",
                EventType::External(_) => "external",
                EventType::Mcp(_) => "mcp",
                EventType::Service(_) => "service",
                EventType::System(_) => "system",
                EventType::Custom(name) => name,
            })),

            "event_category" => Ok(json!(format!("{:?}", event.event_type.category()))),

            "event_priority" => Ok(json!(event.priority.value())),

            "source_id" => Ok(json!(event.source.id)),

            "source_type" => Ok(json!(match &event.source.source_type {
                crate::events::SourceType::Service => "service",
                crate::events::SourceType::Filesystem => "filesystem",
                crate::events::SourceType::Database => "database",
                crate::events::SourceType::External => "external",
                crate::events::SourceType::Mcp => "mcp",
                crate::events::SourceType::System => "system",
                crate::events::SourceType::Manual => "manual",
                crate::events::SourceType::Custom(name) => name,
            })),

            "payload_size" => Ok(json!(event.payload.size_bytes)),

            "created_at" => Ok(json!(event.created_at.to_rfc3339())),

            // Handle nested field access
            field if field.contains('.') => {
                let parts: Vec<&str> = field.split('.').collect();
                self.extract_nested_field_value(event, &parts)
            }

            // Unknown field
            _ => Ok(json!(null)),
        }
    }

    /// Extract nested field value
    fn extract_nested_field_value(&self, event: &DaemonEvent, parts: &[&str]) -> SubscriptionResult<Value> {
        if parts.is_empty() {
            return Ok(json!(null));
        }

        match parts[0] {
            "payload" => {
                if parts.len() == 1 {
                    Ok(event.payload.data.clone())
                } else {
                    // Navigate through payload data
                    self.extract_json_path(&event.payload.data, &parts[1..])
                }
            }

            "metadata" => {
                if parts.len() == 1 {
                    Ok(json!(event.metadata.fields.clone()))
                } else {
                    let field_name = parts[1];
                    if let Some(value) = event.metadata.fields.get(field_name) {
                        if parts.len() == 2 {
                            Ok(json!(value.clone()))
                        } else {
                            // Try to parse as JSON for deeper navigation
                            match serde_json::from_str::<Value>(value) {
                                Ok(json_value) => self.extract_json_path(&json_value, &parts[2..]),
                                Err(_) => Ok(json!(null)),
                            }
                        }
                    } else {
                        Ok(json!(null))
                    }
                }
            }

            _ => Ok(json!(null)),
        }
    }

    /// Extract value from JSON using path
    fn extract_json_path(&self, value: &Value, path: &[&str]) -> SubscriptionResult<Value> {
        if path.is_empty() {
            return Ok(value.clone());
        }

        match value {
            Value::Object(map) => {
                if let Some(next_value) = map.get(path[0]) {
                    if path.len() == 1 {
                        Ok(next_value.clone())
                    } else {
                        self.extract_json_path(next_value, &path[1..])
                    }
                } else {
                    Ok(json!(null))
                }
            }

            Value::Array(arr) => {
                if let Ok(index) = path[0].parse::<usize>() {
                    if index < arr.len() {
                        if path.len() == 1 {
                            Ok(arr[index].clone())
                        } else {
                            self.extract_json_path(&arr[index], &path[1..])
                        }
                    } else {
                        Ok(json!(null))
                    }
                } else {
                    Ok(json!(null))
                }
            }

            _ => Ok(json!(null)),
        }
    }

    /// Compare two values
    fn compare_values<F>(&self, a: &Value, b: &Value, compare: F) -> SubscriptionResult<bool>
    where
        F: Fn(f64, f64) -> bool,
    {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => {
                let a_val = a.as_f64().unwrap_or(0.0);
                let b_val = b.as_f64().unwrap_or(0.0);
                Ok(compare(a_val, b_val))
            }

            (Value::String(a), Value::String(b)) => {
                // For string comparison, convert to numbers if possible
                match (a.parse::<f64>(), b.parse::<f64>()) {
                    (Ok(a_val), Ok(b_val)) => Ok(compare(a_val, b_val)),
                    _ => {
                        // Fall back to lexical comparison
                        Ok(compare(a.len() as f64, b.len() as f64))
                    }
                }
            }

            _ => Ok(false),
        }
    }

    /// Convert value to string
    fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            _ => serde_json::to_string(value).unwrap_or_default(),
        }
    }
}

impl Default for FilterEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventPayload, EventSource, SourceType};

    #[tokio::test]
    async fn test_basic_filtering() {
        let engine = FilterEngine::new();

        let filter = EventFilter {
            event_types: vec!["system".to_string()],
            ..Default::default()
        };

        let filter_key = engine.compile_filter(&filter).await.unwrap();

        let event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("test".to_string()),
            EventPayload::json(json!({})),
        );

        let matches = engine.matches_filter(&event, &filter_key).await.unwrap();
        assert!(matches);
    }

    #[tokio::test]
    async fn test_custom_expression() {
        let engine = FilterEngine::new();

        let filter = EventFilter {
            expression: Some("source_id=test".to_string()),
            ..Default::default()
        };

        let filter_key = engine.compile_filter(&filter).await.unwrap();

        let event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("test".to_string()),
            EventPayload::json(json!({})),
        );

        let matches = engine.matches_filter(&event, &filter_key).await.unwrap();
        assert!(matches);
    }

    #[tokio::test]
    async fn test_regex_filter() {
        let engine = FilterEngine::new();

        let filter = EventFilter {
            expression: Some("source_id matches test.*".to_string()),
            ..Default::default()
        };

        let filter_key = engine.compile_filter(&filter).await.unwrap();

        let event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("test-service".to_string()),
            EventPayload::json(json!({})),
        );

        let matches = engine.matches_filter(&event, &filter_key).await.unwrap();
        assert!(matches);
    }
}