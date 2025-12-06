//! Generic registry trait for immutable key-value lookups
//!
//! Registries are built once and then read-only. Use a builder to construct,
//! then call `.build()` to get an immutable registry. Rebuild on changes.
//!
//! ## Design
//!
//! - **Trait defines contract**: `get`, `contains`, `list`, `len`
//! - **Implementations choose storage**: HashMap, BTreeMap, Vec, etc.
//! - **Builder pattern**: Mutation during init, immutable at runtime
//! - **Rune-ready**: Simple interface for scripting exposure

use std::borrow::Borrow;

/// A read-only registry for key-value lookups
///
/// Implementations are expected to be immutable after construction.
/// Use a builder pattern to construct registries.
pub trait Registry {
    /// The key type used for lookups
    type Key;

    /// The value type stored in the registry
    type Value;

    /// Get a value by key
    fn get<Q>(&self, key: &Q) -> Option<&Self::Value>
    where
        Self::Key: Borrow<Q>,
        Q: ?Sized + Eq + std::hash::Hash;

    /// Check if the registry contains a key
    fn contains<Q>(&self, key: &Q) -> bool
    where
        Self::Key: Borrow<Q>,
        Q: ?Sized + Eq + std::hash::Hash,
    {
        self.get(key).is_some()
    }

    /// List all key-value pairs
    fn iter(&self) -> impl Iterator<Item = (&Self::Key, &Self::Value)>;

    /// Number of entries in the registry
    fn len(&self) -> usize;

    /// Check if the registry is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A mutable registry builder
///
/// Accumulates registrations, then call `.build()` to create an immutable registry.
pub trait RegistryBuilder: Default {
    /// The immutable registry type this builder produces
    type Registry: Registry;

    /// The key type
    type Key;

    /// The value type
    type Value;

    /// Register a key-value pair
    fn register(self, key: Self::Key, value: Self::Value) -> Self;

    /// Build the immutable registry
    fn build(self) -> Self::Registry;
}
