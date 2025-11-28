//! Merkle store factory - creates SurrealDB-backed merkle persistence
//! Phase 5: Uses public adapters API instead of importing concrete types.

use crucible_merkle::MerkleStore;
use crucible_surrealdb::adapters;
use std::sync::Arc;

/// Create SurrealDB-backed merkle store
///
/// Takes an opaque handle to a SurrealDB client and returns a trait object.
/// Phase 5: Uses public factory function from adapters module.
pub fn create_surrealdb_merkle_store(
    client: adapters::SurrealClientHandle,
) -> Arc<dyn MerkleStore> {
    adapters::create_merkle_store(client)
}
