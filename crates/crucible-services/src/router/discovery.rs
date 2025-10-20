use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::traits::*;
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Service discovery implementation
pub struct DefaultServiceDiscovery {
    /// Service registry
    registry: Arc<dyn ServiceRegistry>,
    /// Service cache
    service_cache: Arc<RwLock<HashMap<ServiceType, Vec<ServiceInfo>>>>,
    /// Cache TTL in seconds
    cache_ttl_seconds: u64,
    /// Last cache update timestamp
    last_cache_update: Arc<RwLock<HashMap<ServiceType, chrono::DateTime<chrono::Utc>>>>,
    /// Service health cache
    health_cache: Arc<RwLock<HashMap<Uuid, (ServiceHealth, chrono::DateTime<chrono::Utc>)>>>,
    /// Health cache TTL in seconds
    health_cache_ttl_seconds: u64,
}

impl DefaultServiceDiscovery {
    /// Create a new service discovery
    pub fn new(registry: Arc<dyn ServiceRegistry>) -> Self {
        Self::with_cache_ttl(registry, 30) // 30 seconds default cache TTL
    }

    /// Create service discovery with custom cache TTL
    pub fn with_cache_ttl(registry: Arc<dyn ServiceRegistry>, cache_ttl_seconds: u64) -> Self {
        Self {
            registry,
            service_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl_seconds,
            last_cache_update: Arc::new(RwLock::new(HashMap::new())),
            health_cache: Arc::new(RwLock::new(HashMap::new())),
            health_cache_ttl_seconds: 10, // 10 seconds health cache TTL
        }
    }

    /// Check if cache is expired for a service type
    async fn is_cache_expired(&self, service_type: &ServiceType) -> bool {
        let last_updates = self.last_cache_update.read().await;
        if let Some(last_update) = last_updates.get(service_type) {
            let elapsed = chrono::Utc::now().signed_duration_since(*last_update);
            elapsed.num_seconds() > self.cache_ttl_seconds as i64
        } else {
            true // No cache entry
        }
    }

    /// Update cache for a service type
    async fn update_cache(&self, service_type: &ServiceType) -> ServiceResult<()> {
        let services = self.registry.list_services_by_type(service_type.clone()).await?;

        let mut cache = self.service_cache.write().await;
        cache.insert(service_type.clone(), services);

        let mut last_updates = self.last_cache_update.write().await;
        last_updates.insert(service_type.clone(), chrono::Utc::now());

        Ok(())
    }

    /// Check if health cache is expired
    async fn is_health_cache_expired(&self, service_id: Uuid) -> bool {
        let health_cache = self.health_cache.read().await;
        if let Some((_, last_update)) = health_cache.get(&service_id) {
            let elapsed = chrono::Utc::now().signed_duration_since(*last_update);
            elapsed.num_seconds() > self.health_cache_ttl_seconds as i64
        } else {
            true
        }
    }

    /// Update health cache
    async fn update_health_cache(&self, service_id: Uuid) -> ServiceResult<()> {
        if let Ok(Some(health)) = self.registry.get_service_health(service_id).await {
            let mut health_cache = self.health_cache.write().await;
            health_cache.insert(service_id, (health, chrono::Utc::now()));
        }
        Ok(())
    }
}

#[async_trait]
impl ServiceDiscovery for DefaultServiceDiscovery {
    async fn discover_services(&self, service_type: ServiceType) -> ServiceResult<Vec<ServiceInfo>> {
        // Check cache first
        if !self.is_cache_expired(&service_type).await {
            let cache = self.service_cache.read().await;
            if let Some(services) = cache.get(&service_type) {
                return Ok(services.clone());
            }
        }

        // Update cache and return services
        self.update_cache(&service_type).await?;
        let cache = self.service_cache.read().await;
        Ok(cache.get(&service_type).cloned().unwrap_or_default())
    }

    async fn watch_services(&self, service_type: ServiceType) -> ServiceResult<Box<dyn ServiceWatcher>> {
        Ok(Box::new(DefaultServiceWatcher::new(
            self.registry.clone(),
            service_type,
        )))
    }

    async fn resolve_endpoint(&self, service_id: Uuid) -> ServiceResult<String> {
        let service_info = self.registry.get_service(service_id).await?
            .ok_or_else(|| ServiceError::service_unavailable(service_id.to_string()))?;

        // Extract endpoint from service metadata
        service_info.metadata
            .get("endpoint")
            .cloned()
            .ok_or_else(|| ServiceError::routing_error("No endpoint found in service metadata"))
    }

    async fn get_service_load(&self, service_id: Uuid) -> ServiceResult<ServiceLoad> {
        // This would typically connect to the service to get load information
        // For now, return default load information
        Ok(ServiceLoad {
            service_id,
            current_requests: 0,
            avg_response_time_ms: 0.0,
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            load_score: 0.0,
        })
    }
}

/// Default service watcher implementation
pub struct DefaultServiceWatcher {
    /// Service registry
    registry: Arc<dyn ServiceRegistry>,
    /// Service type to watch
    service_type: ServiceType,
    /// Current known services
    known_services: Arc<RwLock<HashMap<Uuid, ServiceInfo>>>,
    /// Watch state
    active: Arc<RwLock<bool>>,
}

impl DefaultServiceWatcher {
    /// Create a new service watcher
    pub fn new(registry: Arc<dyn ServiceRegistry>, service_type: ServiceType) -> Self {
        Self {
            registry,
            service_type,
            known_services: Arc::new(RwLock::new(HashMap::new())),
            active: Arc::new(RwLock::new(true)),
        }
    }
}

#[async_trait]
impl ServiceWatcher for DefaultServiceWatcher {
    async fn next_change(&mut self) -> ServiceResult<ServiceChangeEvent> {
        let active = *self.active.read().await;
        if !active {
            return Err(ServiceError::routing_error("Watcher is not active"));
        }

        // Get current services
        let current_services = self.registry.list_services_by_type(self.service_type.clone()).await?;
        let current_map: HashMap<Uuid, ServiceInfo> = current_services
            .into_iter()
            .map(|s| (s.id, s))
            .collect();

        let mut known_services = self.known_services.write().await;

        // Check for new services
        for (id, service) in &current_map {
            if !known_services.contains_key(id) {
                known_services.insert(*id, service.clone());
                return Ok(ServiceChangeEvent::Added(service.clone()));
            }
        }

        // Check for removed services
        let mut removed_services = Vec::new();
        for id in known_services.keys() {
            if !current_map.contains_key(id) {
                removed_services.push(*id);
            }
        }

        if let Some(id) = removed_services.first() {
            known_services.remove(id);
            return Ok(ServiceChangeEvent::Removed(*id));
        }

        // Check for updated services
        for (id, current_service) in &current_map {
            if let Some(known_service) = known_services.get(id) {
                // Simple comparison - in practice, you'd want more sophisticated comparison
                if known_service.status != current_service.status {
                    known_services.insert(*id, current_service.clone());
                    return Ok(ServiceChangeEvent::Updated(current_service.clone()));
                }
            }
        }

        // No changes detected, wait a bit and try again
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Return a dummy event or continue waiting
        Err(ServiceError::routing_error("No changes detected"))
    }

    async fn stop(&self) -> ServiceResult<()> {
        let mut active = self.active.write().await;
        *active = false;
        Ok(())
    }
}

/// Service registry implementation
pub struct DefaultServiceRegistry {
    /// Registered services by ID
    services: Arc<RwLock<HashMap<Uuid, ServiceInfo>>>,
    /// Services by type
    services_by_type: Arc<RwLock<HashMap<ServiceType, Vec<Uuid>>>>,
    /// Service health information
    health_info: Arc<RwLock<HashMap<Uuid, ServiceHealth>>>,
}

impl DefaultServiceRegistry {
    /// Create a new service registry
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            services_by_type: Arc::new(RwLock::new(HashMap::new())),
            health_info: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceRegistry for DefaultServiceRegistry {
    async fn register_service(&self, service_info: ServiceInfo) -> ServiceResult<()> {
        let service_id = service_info.id;
        let service_type = service_info.service_type.clone();

        // Add to services map
        let mut services = self.services.write().await;
        services.insert(service_id, service_info.clone());

        // Add to type map
        let mut services_by_type = self.services_by_type.write().await;
        services_by_type
            .entry(service_type)
            .or_default()
            .push(service_id);

        // Initialize health info
        let mut health_info = self.health_info.write().await;
        health_info.insert(service_id, ServiceHealth {
            service_id,
            status: ServiceStatus::Starting,
            last_check: chrono::Utc::now(),
            metrics: HashMap::new(),
            message: Some("Service registered".to_string()),
            uptime_seconds: None,
        });

        Ok(())
    }

    async fn unregister_service(&self, service_id: Uuid) -> ServiceResult<()> {
        // Get service info before removing
        let service_info = {
            let services = self.services.read().await;
            services.get(&service_id).cloned()
        };

        if let Some(info) = service_info {
            // Remove from services map
            let mut services = self.services.write().await;
            services.remove(&service_id);

            // Remove from type map
            let mut services_by_type = self.services_by_type.write().await;
            if let Some(service_list) = services_by_type.get_mut(&info.service_type) {
                service_list.retain(|&id| id != service_id);
                if service_list.is_empty() {
                    services_by_type.remove(&info.service_type);
                }
            }

            // Remove health info
            let mut health_info = self.health_info.write().await;
            health_info.remove(&service_id);
        }

        Ok(())
    }

    async fn get_service(&self, service_id: Uuid) -> ServiceResult<Option<ServiceInfo>> {
        let services = self.services.read().await;
        Ok(services.get(&service_id).cloned())
    }

    async fn list_services(&self) -> ServiceResult<Vec<ServiceInfo>> {
        let services = self.services.read().await;
        Ok(services.values().cloned().collect())
    }

    async fn list_services_by_type(&self, service_type: ServiceType) -> ServiceResult<Vec<ServiceInfo>> {
        let services = self.services.read().await;
        let services_by_type = self.services_by_type.read().await;

        if let Some(service_ids) = services_by_type.get(&service_type) {
            let result: Vec<ServiceInfo> = service_ids
                .iter()
                .filter_map(|&id| services.get(&id).cloned())
                .collect();
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    async fn find_services_by_capability(&self, capability: &str) -> ServiceResult<Vec<ServiceInfo>> {
        let services = self.services.read().await;
        let result: Vec<ServiceInfo> = services
            .values()
            .filter(|s| s.capabilities.contains(&capability.to_string()))
            .cloned()
            .collect();
        Ok(result)
    }

    async fn update_service_status(&self, service_id: Uuid, status: ServiceStatus) -> ServiceResult<()> {
        let mut services = self.services.write().await;
        if let Some(service) = services.get_mut(&service_id) {
            service.status = status.clone();

            // Update health info
            let mut health_info = self.health_info.write().await;
            if let Some(health) = health_info.get_mut(&service_id) {
                health.status = status;
                health.last_check = chrono::Utc::now();
            }
        }
        Ok(())
    }

    async fn get_service_health(&self, service_id: Uuid) -> ServiceResult<Option<ServiceHealth>> {
        let health_info = self.health_info.read().await;
        Ok(health_info.get(&service_id).cloned())
    }

    async fn list_healthy_services(&self) -> ServiceResult<Vec<ServiceInfo>> {
        let services = self.services.read().await;
        let result: Vec<ServiceInfo> = services
            .values()
            .filter(|s| s.status.is_available())
            .cloned()
            .collect();
        Ok(result)
    }

    async fn list_unhealthy_services(&self) -> ServiceResult<Vec<ServiceInfo>> {
        let services = self.services.read().await;
        let result: Vec<ServiceInfo> = services
            .values()
            .filter(|s| !s.status.is_available())
            .cloned()
            .collect();
        Ok(result)
    }
}

/// Service discovery factory
pub struct ServiceDiscoveryFactory;

impl ServiceDiscoveryFactory {
    /// Create service discovery with default configuration
    pub fn create_default(registry: Arc<dyn ServiceRegistry>) -> Arc<dyn ServiceDiscovery> {
        Arc::new(DefaultServiceDiscovery::new(registry))
    }

    /// Create service discovery with custom cache TTL
    pub fn create_with_cache_ttl(
        registry: Arc<dyn ServiceRegistry>,
        cache_ttl_seconds: u64,
    ) -> Arc<dyn ServiceDiscovery> {
        Arc::new(DefaultServiceDiscovery::with_cache_ttl(registry, cache_ttl_seconds))
    }

    /// Create service registry
    pub fn create_registry() -> Arc<dyn ServiceRegistry> {
        Arc::new(DefaultServiceRegistry::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_registry_basic_operations() {
        let registry = DefaultServiceRegistry::new();

        // Create test service info
        let service_info = ServiceInfo {
            id: Uuid::new_v4(),
            name: "test-service".to_string(),
            service_type: ServiceType::Tool,
            version: "1.0.0".to_string(),
            description: Some("Test service".to_string()),
            status: ServiceStatus::Healthy,
            capabilities: vec!["test-capability".to_string()],
            config_schema: None,
            metadata: HashMap::new(),
        };

        // Register service
        registry.register_service(service_info.clone()).await.unwrap();

        // Get service
        let retrieved = registry.get_service(service_info.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-service");

        // List services by type
        let tools = registry.list_services_by_type(ServiceType::Tool).await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test-service");

        // Find services by capability
        let capable = registry.find_services_by_capability("test-capability").await.unwrap();
        assert_eq!(capable.len(), 1);

        // Unregister service
        registry.unregister_service(service_info.id).await.unwrap();

        // Verify service is gone
        let retrieved = registry.get_service(service_info.id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_service_discovery_caching() {
        let registry = Arc::new(DefaultServiceRegistry::new());
        let discovery = DefaultServiceDiscovery::with_cache_ttl(registry.clone(), 1); // 1 second TTL

        // Initially no services
        let services = discovery.discover_services(ServiceType::Tool).await.unwrap();
        assert!(services.is_empty());

        // Register a service
        let service_info = ServiceInfo {
            id: Uuid::new_v4(),
            name: "test-service".to_string(),
            service_type: ServiceType::Tool,
            version: "1.0.0".to_string(),
            description: Some("Test service".to_string()),
            status: ServiceStatus::Healthy,
            capabilities: vec!["test-capability".to_string()],
            config_schema: None,
            metadata: HashMap::new(),
        };

        registry.register_service(service_info).await.unwrap();

        // Discovery should still return empty (cache)
        let services = discovery.discover_services(ServiceType::Tool).await.unwrap();
        assert!(services.is_empty());

        // Wait for cache to expire
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Now discovery should return the service
        let services = discovery.discover_services(ServiceType::Tool).await.unwrap();
        assert_eq!(services.len(), 1);
    }
}