//! WebTools initialization from configuration

use crucible_config::WebToolsConfig;

/// Web tools container
///
/// Holds configuration and provides fetch/search operations.
#[derive(Clone)]
pub struct WebTools {
    config: WebToolsConfig,
}

impl WebTools {
    /// Create new WebTools from configuration
    pub fn new(config: &WebToolsConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Check if web tools are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_tools_disabled_by_default() {
        let config = WebToolsConfig::default();
        let tools = WebTools::new(&config);
        assert!(!tools.is_enabled());
    }

    #[test]
    fn test_web_tools_enabled() {
        let config = WebToolsConfig {
            enabled: true,
            ..Default::default()
        };
        let tools = WebTools::new(&config);
        assert!(tools.is_enabled());
    }
}
