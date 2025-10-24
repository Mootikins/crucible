//! Comprehensive unit tests for configuration management
//!
//! Tests for configuration loading, validation, environment variable overrides,
//! and integration with logging and debugging systems.

use crucible_services::config::*;
use crucible_services::logging::LoggingConfig;
use std::env;
use tempfile::TempDir;

/// Test crucible configuration creation and defaults
#[cfg(test)]
mod crucible_config_tests {
    use super::*;

    #[test]
    fn test_crucible_config_default() {
        let config = CrucibleConfig::default();

        // Check logging configuration
        assert_eq!(config.logging.default_level, Level::INFO);
        assert_eq!(config.logging.component_levels.len(), 2);
        assert!(config.logging.include_timestamps);
        assert!(config.logging.include_target);
        assert!(config.logging.use_ansi);
        assert!(config.logging.component_filter.is_none());

        // Check event routing configuration
        assert_eq!(config.event_routing.max_event_age_seconds, 300);
        assert_eq!(config.event_routing.max_concurrent_events, 1000);
        assert_eq!(config.event_routing.default_routing_strategy, "type_based");
        assert!(!config.event_routing.enable_detailed_tracing);
        assert_eq!(config.event_routing.routing_history_limit, 1000);

        // Check handler configuration
        assert!(config.event_routing.handlers.script_execution);
        assert!(config.event_routing.handlers.tool_execution);
        assert!(config.event_routing.handlers.system_events);
        assert!(config.event_routing.handlers.user_interaction);
        assert_eq!(config.event_routing.handlers.timeout_seconds, 30);

        // Check debugging configuration
        assert!(!config.debugging.enable_event_flow_debug);
        assert!(!config.debugging.enable_performance_profiling);
        assert!(!config.debugging.enable_memory_tracking);
        assert_eq!(config.debugging.component_debug_levels.len(), 2);
        assert!(config.debugging.debug_output_file.is_none());
        assert_eq!(config.debugging.max_debug_file_size_mb, 100);
    }

    #[test]
    fn test_crucible_config_clone() {
        let config = CrucibleConfig::default();
        let cloned = config.clone();

        // Verify deep clone
        assert_eq!(config.logging.default_level, cloned.logging.default_level);
        assert_eq!(config.logging.component_levels, cloned.logging.component_levels);
        assert_eq!(config.event_routing.max_event_age_seconds, cloned.event_routing.max_event_age_seconds);
        assert_eq!(config.debugging.enable_event_flow_debug, cloned.debugging.enable_event_flow_debug);
    }

    #[test]
    fn test_crucible_config_debug_format() {
        let config = CrucibleConfig::default();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("CrucibleConfig"));
        assert!(debug_str.contains("logging"));
        assert!(debug_str.contains("event_routing"));
        assert!(debug_str.contains("debugging"));
    }
}

/// Test event routing configuration
#[cfg(test)]
mod event_routing_config_tests {
    use super::*;

    #[test]
    fn test_event_routing_config_default() {
        let config = EventRoutingConfig::default();

        assert_eq!(config.max_event_age_seconds, 300);
        assert_eq!(config.max_concurrent_events, 1000);
        assert_eq!(config.default_routing_strategy, "type_based");
        assert!(!config.enable_detailed_tracing);
        assert_eq!(config.routing_history_limit, 1000);

        // Check handler configuration defaults
        assert!(config.handlers.script_execution);
        assert!(config.handlers.tool_execution);
        assert!(config.handlers.system_events);
        assert!(config.handlers.user_interaction);
        assert_eq!(config.handlers.timeout_seconds, 30);
    }

    #[test]
    fn test_event_routing_config_custom() {
        let config = EventRoutingConfig {
            max_event_age_seconds: 600,
            max_concurrent_events: 2000,
            default_routing_strategy: "broadcast".to_string(),
            enable_detailed_tracing: true,
            routing_history_limit: 500,
            handlers: HandlerConfig {
                script_execution: false,
                tool_execution: true,
                system_events: false,
                user_interaction: true,
                timeout_seconds: 60,
            },
        };

        assert_eq!(config.max_event_age_seconds, 600);
        assert_eq!(config.max_concurrent_events, 2000);
        assert_eq!(config.default_routing_strategy, "broadcast");
        assert!(config.enable_detailed_tracing);
        assert_eq!(config.routing_history_limit, 500);
        assert!(!config.handlers.script_execution);
        assert!(config.handlers.tool_execution);
        assert!(!config.handlers.system_events);
        assert!(config.handlers.user_interaction);
        assert_eq!(config.handlers.timeout_seconds, 60);
    }

    #[test]
    fn test_event_routing_config_serialization() {
        let config = EventRoutingConfig::default();

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: EventRoutingConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config.max_event_age_seconds, deserialized.max_event_age_seconds);
        assert_eq!(config.max_concurrent_events, deserialized.max_concurrent_events);
        assert_eq!(config.default_routing_strategy, deserialized.default_routing_strategy);
        assert_eq!(config.enable_detailed_tracing, deserialized.enable_detailed_tracing);
        assert_eq!(config.routing_history_limit, deserialized.routing_history_limit);
    }

    #[test]
    fn test_handler_config_serialization() {
        let config = HandlerConfig {
            script_execution: false,
            tool_execution: true,
            system_events: false,
            user_interaction: true,
            timeout_seconds: 120,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: HandlerConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config.script_execution, deserialized.script_execution);
        assert_eq!(config.tool_execution, deserialized.tool_execution);
        assert_eq!(config.system_events, deserialized.system_events);
        assert_eq!(config.user_interaction, deserialized.user_interaction);
        assert_eq!(config.timeout_seconds, deserialized.timeout_seconds);
    }

    #[test]
    fn test_handler_config_clone() {
        let config = HandlerConfig::default();
        let cloned = config.clone();

        assert_eq!(config.script_execution, cloned.script_execution);
        assert_eq!(config.tool_execution, cloned.tool_execution);
        assert_eq!(config.system_events, cloned.system_events);
        assert_eq!(config.user_interaction, cloned.user_interaction);
        assert_eq!(config.timeout_seconds, cloned.timeout_seconds);
    }
}

/// Test debugging configuration
#[cfg(test)]
mod debugging_config_tests {
    use super::*;

    #[test]
    fn test_debugging_config_default() {
        // Clear environment variables first
        env::remove_var("CRUCIBLE_DEBUG_EVENTS");
        env::remove_var("CRUCIBLE_DEBUG_PERFORMANCE");
        env::remove_var("CRUCIBLE_DEBUG_MEMORY");
        env::remove_var("CRUCIBLE_DEBUG_FILE");

        let config = DebuggingConfig::default();

        assert!(!config.enable_event_flow_debug);
        assert!(!config.enable_performance_profiling);
        assert!(!config.enable_memory_tracking);
        assert_eq!(config.component_debug_levels.len(), 2);
        assert!(config.debug_output_file.is_none());
        assert_eq!(config.max_debug_file_size_mb, 100);

        // Check default component debug levels
        let script_engine_debug = config.component_debug_levels
            .iter()
            .find(|(comp, _)| comp == "crucible_services::script_engine")
            .map(|(_, level)| level.as_str());
        assert_eq!(script_engine_debug, Some("debug"));

        let event_routing_debug = config.component_debug_levels
            .iter()
            .find(|(comp, _)| comp == "crucible_services::event_routing")
            .map(|(_, level)| level.as_str());
        assert_eq!(event_routing_debug, Some("trace"));
    }

    #[test]
    fn test_debugging_config_from_environment() {
        // Set environment variables
        env::set_var("CRUCIBLE_DEBUG_EVENTS", "true");
        env::set_var("CRUCIBLE_DEBUG_PERFORMANCE", "true");
        env::set_var("CRUCIBLE_DEBUG_MEMORY", "true");
        env::set_var("CRUCIBLE_DEBUG_FILE", "/tmp/debug.log");

        let config = DebuggingConfig::default();

        assert!(config.enable_event_flow_debug);
        assert!(config.enable_performance_profiling);
        assert!(config.enable_memory_tracking);
        assert_eq!(config.debug_output_file, Some("/tmp/debug.log".to_string()));

        // Clean up
        env::remove_var("CRUCIBLE_DEBUG_EVENTS");
        env::remove_var("CRUCIBLE_DEBUG_PERFORMANCE");
        env::remove_var("CRUCIBLE_DEBUG_MEMORY");
        env::remove_var("CRUCIBLE_DEBUG_FILE");
    }

    #[test]
    fn test_debugging_config_invalid_environment() {
        // Test with invalid boolean values
        env::set_var("CRUCIBLE_DEBUG_EVENTS", "invalid");

        let config = DebuggingConfig::default();
        assert!(!config.enable_event_flow_debug); // Should default to false

        env::remove_var("CRUCIBLE_DEBUG_EVENTS");
    }

    #[test]
    fn test_debugging_config_serialization() {
        let config = DebuggingConfig {
            enable_event_flow_debug: true,
            enable_performance_profiling: true,
            enable_memory_tracking: false,
            component_debug_levels: vec![
                ("test_component".to_string(), "debug".to_string()),
            ],
            debug_output_file: Some("/tmp/test.log".to_string()),
            max_debug_file_size_mb: 50,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: DebuggingConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config.enable_event_flow_debug, deserialized.enable_event_flow_debug);
        assert_eq!(config.enable_performance_profiling, deserialized.enable_performance_profiling);
        assert_eq!(config.enable_memory_tracking, deserialized.enable_memory_tracking);
        assert_eq!(config.component_debug_levels, deserialized.component_debug_levels);
        assert_eq!(config.debug_output_file, deserialized.debug_output_file);
        assert_eq!(config.max_debug_file_size_mb, deserialized.max_debug_file_size_mb);
    }

    #[test]
    fn test_debugging_config_clone() {
        let config = DebuggingConfig::default();
        let cloned = config.clone();

        assert_eq!(config.enable_event_flow_debug, cloned.enable_event_flow_debug);
        assert_eq!(config.enable_performance_profiling, cloned.enable_performance_profiling);
        assert_eq!(config.enable_memory_tracking, cloned.enable_memory_tracking);
        assert_eq!(config.component_debug_levels, cloned.component_debug_levels);
        assert_eq!(config.debug_output_file, cloned.debug_output_file);
        assert_eq!(config.max_debug_file_size_mb, cloned.max_debug_file_size_mb);
    }
}

/// Test environment variable override functionality
#[cfg(test)]
mod environment_override_tests {
    use super::*;

    #[test]
    fn test_logging_level_override() {
        env::set_var("CRUCIBLE_LOG_LEVEL", "debug");

        let config = CrucibleConfig::default().override_from_env();
        assert_eq!(config.logging.default_level, Level::DEBUG);

        env::remove_var("CRUCIBLE_LOG_LEVEL");
    }

    #[test]
    fn test_logging_level_override_invalid() {
        env::set_var("CRUCIBLE_LOG_LEVEL", "invalid_level");

        let config = CrucibleConfig::default().override_from_env();
        assert_eq!(config.logging.default_level, Level::INFO); // Should default to INFO

        env::remove_var("CRUCIBLE_LOG_LEVEL");
    }

    #[test]
    fn test_logging_level_override_case_insensitive() {
        env::set_var("CRUCIBLE_LOG_LEVEL", "TRACE");
        let config1 = CrucibleConfig::default().override_from_env();
        assert_eq!(config1.logging.default_level, Level::TRACE);
        env::remove_var("CRUCIBLE_LOG_LEVEL");

        env::set_var("CRUCIBLE_LOG_LEVEL", "warn");
        let config2 = CrucibleConfig::default().override_from_env();
        assert_eq!(config2.logging.default_level, Level::WARN);
        env::remove_var("CRUCIBLE_LOG_LEVEL");
    }

    #[test]
    fn test_logging_components_override() {
        env::set_var("CRUCIBLE_LOG_COMPONENTS", "component1=trace,component2=debug,component3=error");

        let config = CrucibleConfig::default().override_from_env();

        assert_eq!(config.logging.component_levels.len(), 3);

        let component1_level = config.logging.component_levels
            .iter()
            .find(|(comp, _)| comp == "component1")
            .map(|(_, level)| *level);
        assert_eq!(component1_level, Some(Level::TRACE));

        let component2_level = config.logging.component_levels
            .iter()
            .find(|(comp, _)| comp == "component2")
            .map(|(_, level)| *level);
        assert_eq!(component2_level, Some(Level::DEBUG));

        let component3_level = config.logging.component_levels
            .iter()
            .find(|(comp, _)| comp == "component3")
            .map(|(_, level)| *level);
        assert_eq!(component3_level, Some(Level::ERROR));

        env::remove_var("CRUCIBLE_LOG_COMPONENTS");
    }

    #[test]
    fn test_logging_components_override_invalid_format() {
        env::set_var("CRUCIBLE_LOG_COMPONENTS", "invalid_format,component2=debug");

        let config = CrucibleConfig::default().override_from_env();

        // Should only parse valid entries
        assert_eq!(config.logging.component_levels.len(), 1);
        assert_eq!(config.logging.component_levels[0].0, "component2");

        env::remove_var("CRUCIBLE_LOG_COMPONENTS");
    }

    #[test]
    fn test_event_routing_max_age_override() {
        env::set_var("CRUCIBLE_MAX_EVENT_AGE", "600");

        let config = CrucibleConfig::default().override_from_env();
        assert_eq!(config.event_routing.max_event_age_seconds, 600);

        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
    }

    #[test]
    fn test_event_routing_max_age_override_invalid() {
        env::set_var("CRUCIBLE_MAX_EVENT_AGE", "invalid");

        let config = CrucibleConfig::default().override_from_env();
        assert_eq!(config.event_routing.max_event_age_seconds, 300); // Should remain default

        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
    }

    #[test]
    fn test_max_concurrent_events_override() {
        env::set_var("CRUCIBLE_MAX_CONCURRENT_EVENTS", "2000");

        let config = CrucibleConfig::default().override_from_env();
        assert_eq!(config.event_routing.max_concurrent_events, 2000);

        env::remove_var("CRUCIBLE_MAX_CONCURRENT_EVENTS");
    }

    #[test]
    fn test_default_routing_strategy_override() {
        env::set_var("CRUCIBLE_DEFAULT_ROUTING_STRATEGY", "broadcast");

        let config = CrucibleConfig::default().override_from_env();
        assert_eq!(config.event_routing.default_routing_strategy, "broadcast");

        env::remove_var("CRUCIBLE_DEFAULT_ROUTING_STRATEGY");
    }

    #[test]
    fn test_debug_components_override() {
        env::set_var("CRUCIBLE_DEBUG_COMPONENTS", "component1=trace,component2=debug");

        let config = CrucibleConfig::default().override_from_env();

        assert_eq!(config.debugging.component_debug_levels.len(), 2);

        let component1_level = config.debugging.component_debug_levels
            .iter()
            .find(|(comp, _)| comp == "component1")
            .map(|(_, level)| level.as_str());
        assert_eq!(component1_level, Some("trace"));

        let component2_level = config.debugging.component_debug_levels
            .iter()
            .find(|(comp, _)| comp == "component2")
            .map(|(_, level)| level.as_str());
        assert_eq!(component2_level, Some("debug"));

        env::remove_var("CRUCIBLE_DEBUG_COMPONENTS");
    }

    #[test]
    fn test_multiple_environment_overrides() {
        env::set_var("CRUCIBLE_LOG_LEVEL", "debug");
        env::set_var("CRUCIBLE_MAX_EVENT_AGE", "600");
        env::set_var("CRUCIBLE_DEBUG_EVENTS", "true");

        let config = CrucibleConfig::default().override_from_env();

        assert_eq!(config.logging.default_level, Level::DEBUG);
        assert_eq!(config.event_routing.max_event_age_seconds, 600);
        assert!(config.debugging.enable_event_flow_debug);

        // Clean up
        env::remove_var("CRUCIBLE_LOG_LEVEL");
        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
        env::remove_var("CRUCIBLE_DEBUG_EVENTS");
    }
}

/// Test configuration validation
#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_valid_configuration() {
        let config = CrucibleConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_max_event_age() {
        let mut config = CrucibleConfig::default();
        config.event_routing.max_event_age_seconds = 0;

        let result = config.validate();
        assert!(result.is_err());

        match result.unwrap_err() {
            crucible_services::errors::ServiceError::ConfigurationError(msg) => {
                assert!(msg.contains("max_event_age_seconds"));
            }
            _ => panic!("Expected ConfigurationError"),
        }
    }

    #[test]
    fn test_invalid_max_concurrent_events() {
        let mut config = CrucibleConfig::default();
        config.event_routing.max_concurrent_events = 0;

        let result = config.validate();
        assert!(result.is_err());

        match result.unwrap_err() {
            crucible_services::errors::ServiceError::ConfigurationError(msg) => {
                assert!(msg.contains("max_concurrent_events"));
            }
            _ => panic!("Expected ConfigurationError"),
        }
    }

    #[test]
    fn test_invalid_handler_timeout() {
        let mut config = CrucibleConfig::default();
        config.event_routing.handlers.timeout_seconds = 0;

        let result = config.validate();
        assert!(result.is_err());

        match result.unwrap_err() {
            crucible_services::errors::ServiceError::ConfigurationError(msg) => {
                assert!(msg.contains("timeout_seconds"));
            }
            _ => panic!("Expected ConfigurationError"),
        }
    }

    #[test]
    fn test_invalid_debug_file_size() {
        let mut config = CrucibleConfig::default();
        config.debugging.max_debug_file_size_mb = 0;

        let result = config.validate();
        assert!(result.is_err());

        match result.unwrap_err() {
            crucible_services::errors::ServiceError::ConfigurationError(msg) => {
                assert!(msg.contains("max_debug_file_size_mb"));
            }
            _ => panic!("Expected ConfigurationError"),
        }
    }

    #[test]
    fn test_valid_edge_cases() {
        let mut config = CrucibleConfig::default();

        // Test minimum valid values
        config.event_routing.max_event_age_seconds = 1;
        config.event_routing.max_concurrent_events = 1;
        config.event_routing.handlers.timeout_seconds = 1;
        config.debugging.max_debug_file_size_mb = 1;

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_valid_large_values() {
        let mut config = CrucibleConfig::default();

        // Test large valid values
        config.event_routing.max_event_age_seconds = u64::MAX / 1000; // Very large but valid
        config.event_routing.max_concurrent_events = usize::MAX;
        config.event_routing.handlers.timeout_seconds = u64::MAX / 1000;
        config.debugging.max_debug_file_size_mb = u64::MAX / 1000;

        assert!(config.validate().is_ok());
    }
}

/// Test configuration loading and file operations
#[cfg(test)]
mod config_loading_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_config_load_from_defaults() {
        // Clean environment first
        env::remove_var("CRUCIBLE_LOG_LEVEL");
        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
        env::remove_var("CRUCIBLE_DEBUG_EVENTS");

        let config = CrucibleConfig::load().unwrap();

        // Should load defaults successfully
        assert_eq!(config.logging.default_level, Level::INFO);
        assert_eq!(config.event_routing.max_event_age_seconds, 300);
        assert!(!config.debugging.enable_event_flow_debug);
    }

    #[test]
    fn test_config_load_with_environment_overrides() {
        env::set_var("CRUCIBLE_LOG_LEVEL", "warn");
        env::set_var("CRUCIBLE_MAX_EVENT_AGE", "600");

        let config = CrucibleConfig::load().unwrap();

        assert_eq!(config.logging.default_level, Level::WARN);
        assert_eq!(config.event_routing.max_event_age_seconds, 600);

        // Clean up
        env::remove_var("CRUCIBLE_LOG_LEVEL");
        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
    }

    #[test]
    fn test_config_load_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.txt");

        let config = CrucibleConfig::default();
        config.save_to_file(config_path.to_str().unwrap()).unwrap();

        // Verify file was created
        assert!(config_path.exists());

        // Load from file
        let loaded_config = CrucibleConfig::load_from_file(config_path.to_str().unwrap()).unwrap();

        // Should have same values as default (with any environment overrides)
        assert_eq!(loaded_config.logging.default_level, config.logging.default_level);

        // Clean up
        env::remove_var("CRUCIBLE_LOG_LEVEL");
        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
        env::remove_var("CRUCIBLE_DEBUG_EVENTS");
    }

    #[test]
    fn test_config_save_to_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.txt");

        let config = CrucibleConfig::default();
        let result = config.save_to_file(config_path.to_str().unwrap());

        assert!(result.is_ok());
        assert!(config_path.exists());

        // Verify file content
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Crucible Configuration"));
        assert!(content.contains("Config:"));
        assert!(content.contains("logging="));
        assert!(content.contains("routing="));
        assert!(content.contains("debug="));
    }

    #[test]
    fn test_config_save_to_invalid_path() {
        let invalid_path = "/invalid/path/that/does/not/exist/config.txt";

        let config = CrucibleConfig::default();
        let result = config.save_to_file(invalid_path);

        assert!(result.is_err());
    }
}

/// Test configuration utility functions
#[cfg(test)]
mod config_utility_tests {
    use super::*;

    #[test]
    fn test_is_detailed_tracing_enabled() {
        let mut config = CrucibleConfig::default();

        // Default should be false
        assert!(!config.is_detailed_tracing_enabled());

        // Enable event tracing
        config.event_routing.enable_detailed_tracing = true;
        assert!(config.is_detailed_tracing_enabled());

        // Disable event tracing, enable debugging
        config.event_routing.enable_detailed_tracing = false;
        config.debugging.enable_event_flow_debug = true;
        assert!(config.is_detailed_tracing_enabled());
    }

    #[test]
    fn test_debug_output_file() {
        let mut config = CrucibleConfig::default();

        // Default should be None
        assert!(config.debug_output_file().is_none());

        // Set debug output file
        config.debugging.debug_output_file = Some("/tmp/debug.log".to_string());
        assert_eq!(config.debug_output_file(), Some("/tmp/debug.log"));
    }

    #[test]
    fn test_get_config_summary() {
        let config = CrucibleConfig::default();
        let summary = config.get_summary();

        assert!(summary.contains("Config:"));
        assert!(summary.contains("logging="));
        assert!(summary.contains("routing="));
        assert!(summary.contains("debug="));
        assert!(summary.contains(&format!("{:?}", config.logging.default_level)));
        assert!(summary.contains(&config.event_routing.max_concurrent_events.to_string()));
        assert!(summary.contains(&config.debugging.enable_event_flow_debug.to_string()));
    }
}

/// Test environment variable helper functions
#[cfg(test)]
mod env_helper_tests {
    use super::*;
    use crucible_services::config::env_vars;

    #[test]
    fn test_get_bool_with_existing_var() {
        env::set_var("TEST_BOOL_TRUE", "true");
        env::set_var("TEST_BOOL_FALSE", "false");
        env::set_var("TEST_BOOL_1", "1");
        env::set_var("TEST_BOOL_0", "0");

        assert_eq!(env_vars::get_bool("TEST_BOOL_TRUE", false), true);
        assert_eq!(env_vars::get_bool("TEST_BOOL_FALSE", true), false);
        assert_eq!(env_vars::get_bool("TEST_BOOL_1", false), true);
        assert_eq!(env_vars::get_bool("TEST_BOOL_0", true), false);

        env::remove_var("TEST_BOOL_TRUE");
        env::remove_var("TEST_BOOL_FALSE");
        env::remove_var("TEST_BOOL_1");
        env::remove_var("TEST_BOOL_0");
    }

    #[test]
    fn test_get_bool_with_missing_var() {
        assert_eq!(env_vars::get_bool("MISSING_BOOL", true), true);
        assert_eq!(env_vars::get_bool("MISSING_BOOL", false), false);
    }

    #[test]
    fn test_get_bool_with_invalid_value() {
        env::set_var("TEST_BOOL_INVALID", "invalid");

        assert_eq!(env_vars::get_bool("TEST_BOOL_INVALID", true), true);
        assert_eq!(env_vars::get_bool("TEST_BOOL_INVALID", false), false);

        env::remove_var("TEST_BOOL_INVALID");
    }

    #[test]
    fn test_get_int_with_existing_var() {
        env::set_var("TEST_INT", "42");
        env::set_var("TEST_INT_ZERO", "0");
        env::set_var("TEST_INT_LARGE", "9223372036854775807"); // i64::MAX

        assert_eq!(env_vars::get_int("TEST_INT", 0), 42);
        assert_eq!(env_vars::get_int("TEST_INT_ZERO", 1), 0);
        assert_eq!(env_vars::get_int("TEST_INT_LARGE", 0), 9223372036854775807);

        env::remove_var("TEST_INT");
        env::remove_var("TEST_INT_ZERO");
        env::remove_var("TEST_INT_LARGE");
    }

    #[test]
    fn test_get_int_with_missing_var() {
        assert_eq!(env_vars::get_int("MISSING_INT", 42), 42);
        assert_eq!(env_vars::get_int("MISSING_INT", 0), 0);
    }

    #[test]
    fn test_get_int_with_invalid_value() {
        env::set_var("TEST_INT_INVALID", "invalid");

        assert_eq!(env_vars::get_int("TEST_INT_INVALID", 42), 42);

        env::remove_var("TEST_INT_INVALID");
    }

    #[test]
    fn test_get_string_with_existing_var() {
        env::set_var("TEST_STRING", "test_value");
        env::set_var("TEST_STRING_EMPTY", "");

        assert_eq!(env_vars::get_string("TEST_STRING", "default"), "test_value");
        assert_eq!(env_vars::get_string("TEST_STRING_EMPTY", "default"), "");

        env::remove_var("TEST_STRING");
        env::remove_var("TEST_STRING_EMPTY");
    }

    #[test]
    fn test_get_string_with_missing_var() {
        assert_eq!(env_vars::get_string("MISSING_STRING", "default"), "default");
        assert_eq!(env_vars::get_string("MISSING_STRING", ""), "");
    }

    #[test]
    fn test_get_list_with_existing_var() {
        env::set_var("TEST_LIST", "item1,item2,item3");
        env::set_var("TEST_LIST_SINGLE", "single_item");
        env::set_var("TEST_LIST_EMPTY", "");
        env::set_var("TEST_LIST_WITH_SPACES", " item1 , item2 , item3 ");

        let list1 = env_vars::get_list("TEST_LIST");
        assert_eq!(list1, vec!["item1", "item2", "item3"]);

        let list2 = env_vars::get_list("TEST_LIST_SINGLE");
        assert_eq!(list2, vec!["single_item"]);

        let list3 = env_vars::get_list("TEST_LIST_EMPTY");
        assert!(list3.is_empty());

        let list4 = env_vars::get_list("TEST_LIST_WITH_SPACES");
        assert_eq!(list4, vec!["item1", "item2", "item3"]);

        env::remove_var("TEST_LIST");
        env::remove_var("TEST_LIST_SINGLE");
        env::remove_var("TEST_LIST_EMPTY");
        env::remove_var("TEST_LIST_WITH_SPACES");
    }

    #[test]
    fn test_get_list_with_missing_var() {
        let list = env_vars::get_list("MISSING_LIST");
        assert!(list.is_empty());
    }
}

/// Test configuration initialization with logging
#[cfg(test)]
mod logging_integration_tests {
    use super::*;

    #[test]
    fn test_init_logging_integration() {
        let config = CrucibleConfig::default();

        // Should not panic and initialize successfully
        let result = config.init_logging();
        assert!(result.is_ok());
    }

    #[test]
    fn test_init_logging_with_debug_config() {
        let mut config = CrucibleConfig::default();
        config.debugging.enable_event_flow_debug = true;
        config.debugging.enable_performance_profiling = true;

        // Should add debug component levels
        let result = config.init_logging();
        assert!(result.is_ok());
    }

    #[test]
    fn test_init_logging_with_debug_components() {
        let mut config = CrucibleConfig::default();
        config.debugging.component_debug_levels = vec![
            ("test_component".to_string(), "trace".to_string()),
            ("another_component".to_string(), "debug".to_string()),
        ];

        // Should set component filter
        let result = config.init_logging();
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_init_logging_calls() {
        let config = CrucibleConfig::default();

        // Multiple calls should not panic due to Once guard
        let result1 = config.init_logging();
        let result2 = config.init_logging();
        let result3 = config.init_logging();

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert!(result3.is_ok());
    }
}

/// Test thread safety and concurrent access
#[cfg(test)]
mod thread_safety_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_config_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CrucibleConfig>();
        assert_send_sync::<EventRoutingConfig>();
        assert_send_sync::<HandlerConfig>();
        assert_send_sync::<DebuggingConfig>();
        assert_send_sync::<LoggingConfig>();
    }

    #[test]
    fn test_concurrent_config_creation() {
        let config = Arc::new(CrucibleConfig::default());
        let mut handles = vec![];

        for _ in 0..10 {
            let config_clone = config.clone();
            let handle = thread::spawn(move || {
                // Test concurrent read access
                let _summary = config_clone.get_summary();
                let _tracing_enabled = config_clone.is_detailed_tracing_enabled();
                let _debug_file = config_clone.debug_output_file();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_environment_override() {
        let mut handles = vec![];

        for i in 0..10 {
            let handle = thread::spawn(move || {
                env::set_var("TEST_CONCURRENT_VAR", &i.to_string());

                let config = CrucibleConfig::default().override_from_env();

                // Clean up
                env::remove_var("TEST_CONCURRENT_VAR");

                config
            });
            handles.push(handle);
        }

        // All threads should complete successfully
        for handle in handles {
            handle.join().unwrap();
        }
    }
}