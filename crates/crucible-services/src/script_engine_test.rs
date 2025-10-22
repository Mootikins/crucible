//! Simple test to verify ScriptEngine implementation compiles

#[cfg(test)]
mod tests {
    use super::script_engine::*;
    use crate::service_traits::ScriptEngine;

    #[tokio::test]
    async fn test_basic_compilation() {
        // Just test that the types compile correctly
        let config = ScriptEngineConfig::default();
        let engine = CrucibleScriptEngine::new(config).await;
        assert!(engine.is_ok());

        let mut engine = engine.unwrap();
        assert!(!engine.is_running());

        // Test starting the service
        engine.start().await.unwrap();
        assert!(engine.is_running());

        // Test health check
        let health = engine.health_check().await;
        assert!(health.is_ok());

        // Test getting config
        let config = engine.get_config().await;
        assert!(config.is_ok());

        engine.stop().await.unwrap();
        assert!(!engine.is_running());
    }
}