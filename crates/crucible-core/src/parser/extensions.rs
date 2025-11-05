//! Syntax extension system for pluggable markdown parsing
//!
//! This module provides the trait-based extension system that allows
//! modular addition of new syntax features to the markdown parser.

use super::error::ParseError;
use super::types::DocumentContent;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Trait for syntax extensions that can parse specific markdown patterns
///
/// Extensions are the primary way to add new syntax features to the parser
/// without modifying the core parsing logic.
#[async_trait]
pub trait SyntaxExtension: Send + Sync {
    /// Get the unique name of this extension
    fn name(&self) -> &'static str;

    /// Get the version of this extension
    fn version(&self) -> &'static str;

    /// Get a description of what this extension does
    fn description(&self) -> &'static str;

    /// Check if this extension can handle the given content
    ///
    /// This is a quick check to determine if the extension should be applied
    /// to the given content. It should be very fast (O(1) preferred).
    fn can_handle(&self, content: &str) -> bool;

    /// Parse the content and extract structured data
    ///
    /// This method should parse the content and extract any structured data
    /// that the extension recognizes. The parsed data should be added to the
    /// DocumentContent.
    ///
    /// # Arguments
    /// * `content` - The markdown content to parse
    /// * `doc_content` - The document content to modify with parsed results
    ///
    /// # Returns
    /// A list of parse errors encountered (non-fatal)
    async fn parse(&self, content: &str, doc_content: &mut DocumentContent) -> Vec<ParseError>;

    /// Get the priority of this extension (higher = applied first)
    fn priority(&self) -> u8 {
        50 // Default priority
    }

    /// Check if this extension is enabled
    fn is_enabled(&self) -> bool {
        true // Default to enabled
    }
}

/// Registry for managing syntax extensions
///
/// The registry handles extension discovery, registration, and execution order.
pub struct ExtensionRegistry {
    /// Registered extensions
    extensions: Vec<Arc<dyn SyntaxExtension>>,

    /// Extension index for fast lookup by name
    extension_index: HashMap<&'static str, Arc<dyn SyntaxExtension>>,

    /// Cached sorted extensions (by priority)
    sorted_extensions: Vec<Arc<dyn SyntaxExtension>>,
}

impl std::fmt::Debug for ExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionRegistry")
            .field("extension_count", &self.extensions.len())
            .field(
                "extension_names",
                &self.extensions.iter().map(|e| e.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ExtensionRegistry {
    /// Create a new extension registry
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
            extension_index: HashMap::new(),
            sorted_extensions: Vec::new(),
        }
    }

    /// Register a syntax extension
    ///
    /// # Arguments
    /// * `extension` - The extension to register
    ///
    /// # Returns
    /// * `Ok(())` - Extension registered successfully
    /// * `Err(String)` - Extension with same name already registered
    pub fn register(&mut self, extension: Arc<dyn SyntaxExtension>) -> Result<(), String> {
        let name = extension.name();

        if self.extension_index.contains_key(name) {
            return Err(format!("Extension '{}' already registered", name));
        }

        self.extension_index.insert(name, extension.clone());
        self.extensions.push(extension);
        self.resort_extensions();

        Ok(())
    }

    /// Unregister a syntax extension by name
    ///
    /// # Arguments
    /// * `name` - The name of the extension to unregister
    ///
    /// # Returns
    /// * `Ok(())` - Extension unregistered successfully
    /// * `Err(String)` - Extension not found
    pub fn unregister(&mut self, name: &'static str) -> Result<(), String> {
        if !self.extension_index.contains_key(name) {
            return Err(format!("Extension '{}' not found", name));
        }

        self.extension_index.remove(name);
        self.extensions.retain(|ext| ext.name() != name);
        self.resort_extensions();

        Ok(())
    }

    /// Get an extension by name
    pub fn get(&self, name: &str) -> Option<&Arc<dyn SyntaxExtension>> {
        self.extension_index.get(name)
    }

    /// Get all enabled extensions sorted by priority
    pub fn enabled_extensions(&self) -> Vec<&Arc<dyn SyntaxExtension>> {
        self.sorted_extensions
            .iter()
            .filter(|ext| ext.is_enabled())
            .collect()
    }

    /// Get all registered extensions
    pub fn all_extensions(&self) -> Vec<&Arc<dyn SyntaxExtension>> {
        self.extensions.iter().collect()
    }

    /// Apply all enabled extensions to content
    ///
    /// # Arguments
    /// * `content` - The markdown content to parse
    /// * `doc_content` - The document content to modify
    ///
    /// # Returns
    /// A list of all parse errors from all extensions
    pub async fn apply_extensions(
        &self,
        content: &str,
        doc_content: &mut DocumentContent,
    ) -> Vec<ParseError> {
        let mut all_errors = Vec::new();

        for extension in self.enabled_extensions() {
            if extension.can_handle(content) {
                let errors = extension.parse(content, doc_content).await;
                all_errors.extend(errors);
            }
        }

        all_errors
    }

    /// Get registry statistics
    pub fn stats(&self) -> ExtensionRegistryStats {
        let enabled_count = self.enabled_extensions().len();

        ExtensionRegistryStats {
            total_extensions: self.extensions.len(),
            enabled_extensions: enabled_count,
            disabled_extensions: self.extensions.len() - enabled_count,
        }
    }

    /// Resort extensions by priority (internal)
    fn resort_extensions(&mut self) {
        let mut extensions = self.extensions.clone();
        extensions.sort_by(|a, b| b.priority().cmp(&a.priority()));
        self.sorted_extensions = extensions;
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionRegistryStats {
    /// Total number of registered extensions
    pub total_extensions: usize,

    /// Number of enabled extensions
    pub enabled_extensions: usize,

    /// Number of disabled extensions
    pub disabled_extensions: usize,
}

/// Builder for creating and configuring extension registries
pub struct ExtensionRegistryBuilder {
    registry: ExtensionRegistry,
}

impl ExtensionRegistryBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            registry: ExtensionRegistry::new(),
        }
    }

    /// Add an extension to the registry
    pub fn with_extension(mut self, extension: Arc<dyn SyntaxExtension>) -> Self {
        // Ignore registration errors for builder - let user handle them
        let _ = self.registry.register(extension);
        self
    }

    /// Add multiple extensions
    pub fn with_extensions<I>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn SyntaxExtension>>,
    {
        for ext in extensions {
            let _ = self.registry.register(ext);
        }
        self
    }

    /// Build the registry
    pub fn build(self) -> ExtensionRegistry {
        self.registry
    }
}

impl Default for ExtensionRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ParseErrorType;

    // Mock extension for testing
    #[derive(Debug)]
    struct TestExtension {
        name: &'static str,
        version: &'static str,
        priority: u8,
    }

    #[async_trait]
    impl SyntaxExtension for TestExtension {
        fn name(&self) -> &'static str {
            self.name
        }

        fn version(&self) -> &'static str {
            self.version
        }

        fn description(&self) -> &'static str {
            "Test extension for unit testing"
        }

        fn can_handle(&self, content: &str) -> bool {
            content.contains("test") || content.contains("error")
        }

        async fn parse(
            &self,
            content: &str,
            _doc_content: &mut DocumentContent,
        ) -> Vec<ParseError> {
            if content.contains("error") {
                vec![ParseError::error(
                    "Test error".to_string(),
                    ParseErrorType::SyntaxError,
                    0,
                    0,
                    0,
                )]
            } else {
                Vec::new()
            }
        }

        fn priority(&self) -> u8 {
            self.priority
        }
    }

    #[tokio::test]
    async fn test_extension_registration() {
        let mut registry = ExtensionRegistry::new();
        let ext = Arc::new(TestExtension {
            name: "test",
            version: "1.0.0",
            priority: 50,
        });

        assert!(registry.register(ext.clone()).is_ok());
        assert!(registry.get("test").is_some());
        assert!(registry.register(ext).is_err()); // Duplicate
    }

    #[tokio::test]
    async fn test_extension_application() {
        let mut registry = ExtensionRegistry::new();
        let ext = Arc::new(TestExtension {
            name: "test",
            version: "1.0.0",
            priority: 50,
        });

        registry.register(ext).unwrap();

        let mut doc_content = DocumentContent::new();
        let errors = registry
            .apply_extensions("test content", &mut doc_content)
            .await;
        assert_eq!(errors.len(), 0);

        let mut doc_content2 = DocumentContent::new();
        let errors = registry
            .apply_extensions("error content", &mut doc_content2)
            .await;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ParseErrorType::SyntaxError);
    }

    #[test]
    fn test_builder_pattern() {
        let ext1 = Arc::new(TestExtension {
            name: "ext1",
            version: "1.0.0",
            priority: 100,
        });

        let ext2 = Arc::new(TestExtension {
            name: "ext2",
            version: "1.0.0",
            priority: 50,
        });

        let registry = ExtensionRegistryBuilder::new()
            .with_extension(ext1)
            .with_extension(ext2)
            .build();

        assert_eq!(registry.all_extensions().len(), 2);
        assert_eq!(registry.stats().total_extensions, 2);
    }
}
