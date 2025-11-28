//! Storage factory - creates SurrealDB storage implementation
//!
//! This is part of the composition root where concrete types are wired together.
//! Phase 5: Uses public adapters API instead of importing concrete types.

use crate::config::CliConfig;
use anyhow::Result;
use crucible_core::enrichment::EnrichedNoteStore;
use crucible_surrealdb::{adapters, SurrealDbConfig};
use once_cell::sync::Lazy;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::{Arc, Mutex};

/// Create SurrealDB storage from CLI configuration
///
/// Returns an opaque handle that can be passed to other factory functions.
/// Phase 5: Now returns SurrealClientHandle instead of concrete SurrealClient.
static SURREAL_CLIENT_CACHE: Lazy<Mutex<HashMap<String, adapters::SurrealClientHandle>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn storage_cache_key(config: &SurrealDbConfig) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        config.path,
        config.namespace,
        config.database,
        config.max_connections.unwrap_or(0),
        config.timeout_seconds.unwrap_or(0)
    )
}

pub async fn create_surrealdb_storage(config: &CliConfig) -> Result<adapters::SurrealClientHandle> {
    let db_config = SurrealDbConfig {
        path: config.database_path_str()?,
        namespace: "crucible".to_string(),
        database: "kiln".to_string(),
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let cache_key = storage_cache_key(&db_config);

    if let Some(cached) = {
        let cache = SURREAL_CLIENT_CACHE.lock().unwrap();
        cache.get(&cache_key).cloned()
    } {
        return Ok(cached);
    }

    let client = adapters::create_surreal_client(db_config.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create SurrealDB storage: {}", e))?;

    let mut cache = SURREAL_CLIENT_CACHE.lock().unwrap();
    let entry = cache.entry(cache_key);

    let client_handle = match entry {
        Entry::Occupied(entry) => entry.get().clone(),
        Entry::Vacant(entry) => {
            entry.insert(client.clone());
            client
        }
    };

    Ok(client_handle)
}

/// Initialize SurrealDB schema
///
/// This requires access to the internal client, so we expose it via
/// a special function in the adapters module.
pub async fn initialize_surrealdb_schema(client: &adapters::SurrealClientHandle) -> Result<()> {
    // Call kiln_integration via the handle
    // This is a temporary measure - ideally schema initialization should be part of factory
    crucible_surrealdb::kiln_integration::initialize_kiln_schema(client.inner()).await
}

/// Create SurrealDB-backed enriched note store
///
/// This factory creates an adapter that implements the `EnrichedNoteStore` trait
/// using SurrealDB as the backend.
///
/// # Architecture
///
/// Uses the public factory function from the adapters module, which handles
/// all the internal wiring (EAVGraphStore, NoteIngestor lifetimes, etc.).
pub fn create_surrealdb_enriched_note_store(
    client: adapters::SurrealClientHandle,
) -> Arc<dyn EnrichedNoteStore> {
    adapters::create_enriched_note_store(client)
}
