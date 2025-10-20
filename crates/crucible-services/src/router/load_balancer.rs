use async_trait::async_trait;
use crate::types::*;
use crate::ServiceLoad;
use super::ServiceInstance;
use std::collections::HashMap;

/// Load balancer strategy for selecting service instances
#[async_trait]
pub trait LoadBalancer: Send + Sync {
    /// Select the best service instance for a request
    async fn select_instance(
        &self,
        instances: &[ServiceInstance],
        loads: &HashMap<uuid::Uuid, ServiceLoad>,
        request: &ServiceRequest,
    ) -> Option<ServiceInstance>;

    /// Get load balancer name
    fn name(&self) -> &'static str;

    /// Get load balancer statistics
    async fn get_stats(&self) -> LoadBalancerStats;
}

/// Load balancer statistics
#[derive(Debug, Clone, Default)]
pub struct LoadBalancerStats {
    /// Total selections made
    pub total_selections: u64,
    /// Selections by instance
    pub selections_by_instance: std::collections::HashMap<uuid::Uuid, u64>,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Load balancer name
    pub name: String,
}

/// Round-robin load balancer
pub struct RoundRobinLoadBalancer {
    /// Current index for round-robin selection
    current_index: std::sync::atomic::AtomicUsize,
    /// Statistics
    stats: std::sync::Mutex<LoadBalancerStats>,
}

impl RoundRobinLoadBalancer {
    /// Create a new round-robin load balancer
    pub fn new() -> Self {
        Self {
            current_index: std::sync::atomic::AtomicUsize::new(0),
            stats: std::sync::Mutex::new(LoadBalancerStats {
                name: "RoundRobin".to_string(),
                ..Default::default()
            }),
        }
    }
}

impl Default for RoundRobinLoadBalancer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoadBalancer for RoundRobinLoadBalancer {
    async fn select_instance(
        &self,
        instances: &[ServiceInstance],
        _loads: &HashMap<uuid::Uuid, ServiceLoad>,
        _request: &ServiceRequest,
    ) -> Option<ServiceInstance> {
        if instances.is_empty() {
            return None;
        }

        let index = self.current_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % instances.len();
        let instance = instances[index].clone();

        // Update statistics
        if let Ok(mut stats) = self.stats.lock() {
            stats.total_selections += 1;
            *stats.selections_by_instance.entry(instance.service_id).or_insert(0) += 1;
        }

        Some(instance)
    }

    fn name(&self) -> &'static str {
        "RoundRobin"
    }

    async fn get_stats(&self) -> LoadBalancerStats {
        self.stats.lock().unwrap().clone()
    }
}

/// Weighted round-robin load balancer
pub struct WeightedRoundRobinLoadBalancer {
    /// Instance weights
    weights: std::sync::RwLock<std::collections::HashMap<uuid::Uuid, f32>>,
    /// Current weights for weighted round-robin
    current_weights: std::sync::RwLock<std::collections::HashMap<uuid::Uuid, f32>>,
    /// Statistics
    stats: std::sync::Mutex<LoadBalancerStats>,
}

impl WeightedRoundRobinLoadBalancer {
    /// Create a new weighted round-robin load balancer
    pub fn new() -> Self {
        Self {
            weights: std::sync::RwLock::new(std::collections::HashMap::new()),
            current_weights: std::sync::RwLock::new(std::collections::HashMap::new()),
            stats: std::sync::Mutex::new(LoadBalancerStats {
                name: "WeightedRoundRobin".to_string(),
                ..Default::default()
            }),
        }
    }

    /// Set weight for a service instance
    pub async fn set_weight(&self, service_id: uuid::Uuid, weight: f32) {
        let mut weights = self.weights.write().await;
        weights.insert(service_id, weight.max(0.1)); // Minimum weight of 0.1

        let mut current_weights = self.current_weights.write().await;
        current_weights.insert(service_id, 0.0);
    }

    /// Set weights based on service load
    pub async fn set_weights_by_load(&self, loads: &HashMap<uuid::Uuid, ServiceLoad>) {
        let mut weights = self.weights.write().await;
        for (service_id, load) in loads {
            // Higher load score results in lower weight
            let weight = (1.0 - load.load_score).max(0.1);
            weights.insert(*service_id, weight);
        }

        // Reset current weights
        let mut current_weights = self.current_weights.write().await;
        for service_id in weights.keys() {
            current_weights.insert(*service_id, 0.0);
        }
    }
}

impl Default for WeightedRoundRobinLoadBalancer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoadBalancer for WeightedRoundRobinLoadBalancer {
    async fn select_instance(
        &self,
        instances: &[ServiceInstance],
        loads: &HashMap<uuid::Uuid, ServiceLoad>,
        _request: &ServiceRequest,
    ) -> Option<ServiceInstance> {
        if instances.is_empty() {
            return None;
        }

        // Update weights based on current loads
        self.set_weights_by_load(loads).await;

        let weights = self.weights.read().await;
        let mut current_weights = self.current_weights.write().await;

        // Find instance with highest current weight
        let mut best_instance = None;
        let mut best_weight = -1.0;

        for instance in instances {
            let service_id = instance.service_id;
            let weight = weights.get(&service_id).copied().unwrap_or(1.0);
            let current_weight = current_weights.get_mut(&service_id).unwrap_or(&mut 0.0);

            *current_weight += weight;

            if *current_weight > best_weight {
                best_weight = *current_weight;
                best_instance = Some(instance.clone());
            }
        }

        // Reduce the selected instance's current weight
        if let Some(ref instance) = best_instance {
            if let Some(current_weight) = current_weights.get_mut(&instance.service_id) {
                *current_weight -= best_weight;
            }

            // Update statistics
            if let Ok(mut stats) = self.stats.lock() {
                stats.total_selections += 1;
                *stats.selections_by_instance.entry(instance.service_id).or_insert(0) += 1;
            }
        }

        best_instance
    }

    fn name(&self) -> &'static str {
        "WeightedRoundRobin"
    }

    async fn get_stats(&self) -> LoadBalancerStats {
        self.stats.lock().unwrap().clone()
    }
}

/// Least connections load balancer
pub struct LeastConnectionsLoadBalancer {
    /// Statistics
    stats: std::sync::Mutex<LoadBalancerStats>,
}

impl LeastConnectionsLoadBalancer {
    /// Create a new least connections load balancer
    pub fn new() -> Self {
        Self {
            stats: std::sync::Mutex::new(LoadBalancerStats {
                name: "LeastConnections".to_string(),
                ..Default::default()
            }),
        }
    }
}

impl Default for LeastConnectionsLoadBalancer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoadBalancer for LeastConnectionsLoadBalancer {
    async fn select_instance(
        &self,
        instances: &[ServiceInstance],
        loads: &HashMap<uuid::Uuid, ServiceLoad>,
        _request: &ServiceRequest,
    ) -> Option<ServiceInstance> {
        if instances.is_empty() {
            return None;
        }

        // Find instance with least current requests
        let mut best_instance = None;
        let mut min_requests = u32::MAX;

        for instance in instances {
            if let Some(load) = loads.get(&instance.service_id) {
                if load.current_requests < min_requests {
                    min_requests = load.current_requests;
                    best_instance = Some(instance.clone());
                }
            } else {
                // If no load information available, assume 0 connections
                if 0 < min_requests {
                    min_requests = 0;
                    best_instance = Some(instance.clone());
                }
            }
        }

        // Update statistics
        if let Some(ref instance) = best_instance {
            if let Ok(mut stats) = self.stats.lock() {
                stats.total_selections += 1;
                *stats.selections_by_instance.entry(instance.service_id).or_insert(0) += 1;
            }
        }

        best_instance
    }

    fn name(&self) -> &'static str {
        "LeastConnections"
    }

    async fn get_stats(&self) -> LoadBalancerStats {
        self.stats.lock().unwrap().clone()
    }
}

/// Random load balancer
pub struct RandomLoadBalancer {
    /// Random number generator
    rng: std::sync::Mutex<rand::rngs::StdRng>,
    /// Statistics
    stats: std::sync::Mutex<LoadBalancerStats>,
}

impl RandomLoadBalancer {
    /// Create a new random load balancer
    pub fn new() -> Self {
        Self {
            rng: std::sync::Mutex::new(rand::rngs::StdRng::from_entropy()),
            stats: std::sync::Mutex::new(LoadBalancerStats {
                name: "Random".to_string(),
                ..Default::default()
            }),
        }
    }
}

impl Default for RandomLoadBalancer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoadBalancer for RandomLoadBalancer {
    async fn select_instance(
        &self,
        instances: &[ServiceInstance],
        _loads: &HashMap<uuid::Uuid, ServiceLoad>,
        _request: &ServiceRequest,
    ) -> Option<ServiceInstance> {
        if instances.is_empty() {
            return None;
        }

        let index = {
            let mut rng = self.rng.lock().unwrap();
            rng.gen_range(0..instances.len())
        };

        let instance = instances[index].clone();

        // Update statistics
        if let Ok(mut stats) = self.stats.lock() {
            stats.total_selections += 1;
            *stats.selections_by_instance.entry(instance.service_id).or_insert(0) += 1;
        }

        Some(instance)
    }

    fn name(&self) -> &'static str {
        "Random"
    }

    async fn get_stats(&self) -> LoadBalancerStats {
        self.stats.lock().unwrap().clone()
    }
}

/// Load balancer factory
pub struct LoadBalancerFactory;

impl LoadBalancerFactory {
    /// Create load balancer by strategy name
    pub fn create(strategy: LoadBalancingStrategy) -> std::sync::Arc<dyn LoadBalancer> {
        match strategy {
            LoadBalancingStrategy::RoundRobin => {
                std::sync::Arc::new(RoundRobinLoadBalancer::new())
            }
            LoadBalancingStrategy::WeightedRoundRobin => {
                std::sync::Arc::new(WeightedRoundRobinLoadBalancer::new())
            }
            LoadBalancingStrategy::LeastConnections => {
                std::sync::Arc::new(LeastConnectionsLoadBalancer::new())
            }
            LoadBalancingStrategy::Random => {
                std::sync::Arc::new(RandomLoadBalancer::new())
            }
        }
    }
}

/// Load balancing strategies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadBalancingStrategy {
    /// Round-robin selection
    RoundRobin,
    /// Weighted round-robin based on load
    WeightedRoundRobin,
    /// Least connections
    LeastConnections,
    /// Random selection
    Random,
}

impl LoadBalancingStrategy {
    /// Get strategy name
    pub fn name(&self) -> &'static str {
        match self {
            LoadBalancingStrategy::RoundRobin => "round_robin",
            LoadBalancingStrategy::WeightedRoundRobin => "weighted_round_robin",
            LoadBalancingStrategy::LeastConnections => "least_connections",
            LoadBalancingStrategy::Random => "random",
        }
    }

    /// Create strategy from name
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "round_robin" | "roundrobin" => Some(LoadBalancingStrategy::RoundRobin),
            "weighted_round_robin" | "weightedroundrobin" => Some(LoadBalancingStrategy::WeightedRoundRobin),
            "least_connections" | "leastconnections" => Some(LoadBalancingStrategy::LeastConnections),
            "random" => Some(LoadBalancingStrategy::Random),
            _ => None,
        }
    }
}