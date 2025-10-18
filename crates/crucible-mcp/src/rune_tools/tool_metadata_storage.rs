/// Thread-safe global storage for tool macro metadata
///
/// This module provides a centralized registry for storing metadata extracted from
/// `#[tool]` attribute macros. The storage is thread-safe and uses interior mutability
/// to allow concurrent access from multiple compilation contexts.
///
/// Architecture:
/// - Global singleton using `OnceLock` for lazy initialization
/// - `RwLock` for concurrent read access with exclusive writes
/// - Metadata includes function name, description, and parameter specifications

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// Global singleton instance of the metadata storage
static GLOBAL_STORAGE: OnceLock<ToolMetadataStorage> = OnceLock::new();

/// Metadata extracted from a `#[tool]` attribute macro
///
/// This structure captures all information needed to generate MCP tool schemas:
/// - Tool name (derived from function name)
/// - Human-readable description
/// - Parameter list with types and optional flags
#[derive(Debug, Clone)]
pub struct ToolMacroMetadata {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ParameterMetadata>,
}

/// Metadata for a single function parameter
///
/// Extracted from Rune function signatures, this includes:
/// - Parameter name
/// - Type specification (primitive, array, object)
/// - Optional flag (whether the parameter can be omitted)
#[derive(Debug, Clone)]
pub struct ParameterMetadata {
    pub name: String,
    pub type_spec: TypeSpec,
    pub is_optional: bool,
}

/// Type specification for Rune parameters
///
/// Maps Rune type annotations to a simplified type system suitable for
/// JSON Schema generation. Supports primitives, arrays, and objects.
///
/// Type Detection Rules:
/// - `string` → TypeSpec::String
/// - `number` → TypeSpec::Number
/// - `boolean` → TypeSpec::Boolean
/// - `[T]` or `array<T>` → TypeSpec::Array(Box::new(T))
/// - Default → TypeSpec::Object
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeSpec {
    /// String type - maps to JSON Schema "string"
    String,

    /// Numeric type - maps to JSON Schema "number"
    Number,

    /// Boolean type - maps to JSON Schema "boolean"
    Boolean,

    /// Array type with element type - maps to JSON Schema "array"
    Array(Box<TypeSpec>),

    /// Object type - maps to JSON Schema "object"
    Object,
}

impl TypeSpec {
    /// Convert a Rune type string to a TypeSpec
    ///
    /// Parses common Rune type annotations:
    /// - "string" → String
    /// - "number" → Number
    /// - "boolean" → Boolean
    /// - "[T]" or "array<T>" → Array
    /// - Default → Object
    pub fn from_rune_type(type_str: &str) -> Self {
        let normalized = type_str.trim().to_lowercase();

        match normalized.as_str() {
            "string" | "str" => TypeSpec::String,
            "number" | "int" | "i64" | "f64" | "float" => TypeSpec::Number,
            "boolean" | "bool" => TypeSpec::Boolean,
            _ => {
                // Check for array syntax: [T] or array<T>
                if normalized.starts_with('[') && normalized.ends_with(']') {
                    // Extract inner type from [T]
                    let inner = &type_str[1..type_str.len() - 1];
                    TypeSpec::Array(Box::new(Self::from_rune_type(inner)))
                } else if normalized.starts_with("array<") && normalized.ends_with('>') {
                    // Extract inner type from array<T>
                    let inner = &type_str[6..type_str.len() - 1];
                    TypeSpec::Array(Box::new(Self::from_rune_type(inner)))
                } else {
                    // Default to object for unknown types
                    TypeSpec::Object
                }
            }
        }
    }

    /// Convert TypeSpec to JSON Schema type string
    pub fn to_json_schema_type(&self) -> &'static str {
        match self {
            TypeSpec::String => "string",
            TypeSpec::Number => "number",
            TypeSpec::Boolean => "boolean",
            TypeSpec::Array(_) => "array",
            TypeSpec::Object => "object",
        }
    }
}

/// Thread-safe global storage for tool metadata
///
/// Provides concurrent access to tool metadata extracted during macro expansion.
/// Uses RwLock to allow multiple readers or a single writer.
///
/// Memory Model:
/// - Storage is never deallocated (static lifetime)
/// - Metadata is cloned on retrieval to avoid lock contention
/// - Write operations are rare (only during macro expansion)
pub struct ToolMetadataStorage {
    storage: RwLock<HashMap<String, ToolMacroMetadata>>,
}

impl ToolMetadataStorage {
    /// Get the global metadata storage instance
    ///
    /// Lazily initializes the storage on first access.
    /// Thread-safe and lock-free after initialization.
    pub fn global() -> &'static ToolMetadataStorage {
        GLOBAL_STORAGE.get_or_init(|| ToolMetadataStorage {
            storage: RwLock::new(HashMap::new()),
        })
    }

    /// Insert or update tool metadata
    ///
    /// Stores metadata for a tool by name. If a tool with the same name
    /// already exists, it will be overwritten.
    ///
    /// # Panics
    /// Panics if the storage lock is poisoned (indicates a panic during write)
    pub fn insert(&self, name: String, metadata: ToolMacroMetadata) {
        let mut storage = self.storage.write().expect("Storage lock poisoned");
        storage.insert(name, metadata);
    }

    /// Retrieve tool metadata by name
    ///
    /// Returns a clone of the metadata if found, or None if the tool is not registered.
    /// Cloning allows the caller to work with the data without holding the read lock.
    ///
    /// # Panics
    /// Panics if the storage lock is poisoned
    pub fn get(&self, name: &str) -> Option<ToolMacroMetadata> {
        let storage = self.storage.read().expect("Storage lock poisoned");
        storage.get(name).cloned()
    }

    /// Clear all stored metadata
    ///
    /// Primarily used for testing to reset state between test cases.
    ///
    /// # Panics
    /// Panics if the storage lock is poisoned
    pub fn clear(&self) {
        let mut storage = self.storage.write().expect("Storage lock poisoned");
        storage.clear();
    }

    /// Get all stored tool names
    ///
    /// Returns a vector of all registered tool names.
    /// Useful for discovery and debugging.
    pub fn list_tools(&self) -> Vec<String> {
        let storage = self.storage.read().expect("Storage lock poisoned");
        storage.keys().cloned().collect()
    }

    /// Get the number of registered tools
    pub fn count(&self) -> usize {
        let storage = self.storage.read().expect("Storage lock poisoned");
        storage.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_spec_from_rune_type() {
        assert_eq!(TypeSpec::from_rune_type("string"), TypeSpec::String);
        assert_eq!(TypeSpec::from_rune_type("number"), TypeSpec::Number);
        assert_eq!(TypeSpec::from_rune_type("boolean"), TypeSpec::Boolean);

        // Array syntax
        assert_eq!(
            TypeSpec::from_rune_type("[string]"),
            TypeSpec::Array(Box::new(TypeSpec::String))
        );
        assert_eq!(
            TypeSpec::from_rune_type("array<number>"),
            TypeSpec::Array(Box::new(TypeSpec::Number))
        );

        // Unknown types default to object
        assert_eq!(TypeSpec::from_rune_type("CustomType"), TypeSpec::Object);
    }

    #[test]
    fn test_type_spec_to_json_schema() {
        assert_eq!(TypeSpec::String.to_json_schema_type(), "string");
        assert_eq!(TypeSpec::Number.to_json_schema_type(), "number");
        assert_eq!(TypeSpec::Boolean.to_json_schema_type(), "boolean");
        assert_eq!(
            TypeSpec::Array(Box::new(TypeSpec::String)).to_json_schema_type(),
            "array"
        );
        assert_eq!(TypeSpec::Object.to_json_schema_type(), "object");
    }

    #[test]
    fn test_storage_insert_and_get() {
        let storage = ToolMetadataStorage::global();
        storage.clear(); // Clear any previous test data

        let metadata = ToolMacroMetadata {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: vec![
                ParameterMetadata {
                    name: "input".to_string(),
                    type_spec: TypeSpec::String,
                    is_optional: false,
                },
            ],
        };

        storage.insert("test_tool".to_string(), metadata.clone());

        let retrieved = storage.get("test_tool").expect("Metadata should exist");
        assert_eq!(retrieved.name, "test_tool");
        assert_eq!(retrieved.description, "A test tool");
        assert_eq!(retrieved.parameters.len(), 1);
        assert_eq!(retrieved.parameters[0].name, "input");
    }

    #[test]
    fn test_storage_get_nonexistent() {
        let storage = ToolMetadataStorage::global();
        storage.clear();

        let result = storage.get("nonexistent_tool");
        assert!(result.is_none());
    }

    #[test]
    fn test_storage_clear() {
        let storage = ToolMetadataStorage::global();
        storage.clear();

        let metadata = ToolMacroMetadata {
            name: "temp_tool".to_string(),
            description: "Temporary".to_string(),
            parameters: vec![],
        };

        storage.insert("temp_tool".to_string(), metadata);
        assert_eq!(storage.count(), 1);

        storage.clear();
        assert_eq!(storage.count(), 0);
        assert!(storage.get("temp_tool").is_none());
    }

    #[test]
    fn test_storage_list_tools() {
        let storage = ToolMetadataStorage::global();
        storage.clear();

        storage.insert(
            "tool1".to_string(),
            ToolMacroMetadata {
                name: "tool1".to_string(),
                description: "First".to_string(),
                parameters: vec![],
            },
        );

        storage.insert(
            "tool2".to_string(),
            ToolMacroMetadata {
                name: "tool2".to_string(),
                description: "Second".to_string(),
                parameters: vec![],
            },
        );

        let mut tools = storage.list_tools();
        tools.sort();
        assert_eq!(tools, vec!["tool1", "tool2"]);
    }
}
