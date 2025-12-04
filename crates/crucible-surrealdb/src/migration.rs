//! Database migration utilities for the Crucible SurrealDB schema
//!
//! This module provides utilities to migrate existing databases to newer schema versions.
//! It handles adding new fields, indexes, and updating data structures.

#![allow(dead_code)] // Migration code kept for future schema updates

use crate::SurrealClient;
use anyhow::Result;
use tracing::{debug, info, warn};

/// Run all pending migrations to bring the database to the latest schema version
///
/// This function should be called during application startup to ensure the database
/// schema is up to date with the current code expectations.
///
/// # Arguments
/// * `client` - SurrealDB client connection
///
/// # Returns
/// Result<()> indicating success or failure of the migration process
pub async fn run_migrations(client: &SurrealClient) -> Result<()> {
    info!("Starting database migration process");

    // Get current schema version
    let current_version = get_schema_version(client).await.unwrap_or(0);
    info!("Current database schema version: {}", current_version);

    // Define migration steps in order
    let migrations = vec![Migration {
        version: 1,
        description: "Add file_hash column to notes table",
        sql: include_str!("migrations/v1_add_file_hash.surql"),
    }];

    // Run migrations that haven't been applied yet
    for migration in migrations {
        if migration.version > current_version {
            info!(
                "Running migration v{}: {}",
                migration.version, migration.description
            );
            run_migration(client, &migration).await?;
            info!("Successfully completed migration v{}", migration.version);
        }
    }

    info!("Database migration process completed");
    Ok(())
}

/// Get the current schema version from the metadata table
async fn get_schema_version(client: &SurrealClient) -> Result<i32> {
    let result = client
        .query("SELECT schema_version FROM metadata:system", &[])
        .await?;

    if let Some(record) = result.records.first() {
        if let Some(version) = record.data.get("schema_version") {
            if let Some(version_num) = version.as_i64() {
                return Ok(version_num as i32);
            }
        }
    }

    // If no version is found, assume version 0 (pre-schema metadata)
    Ok(0)
}

/// Run a single migration
async fn run_migration(client: &SurrealClient, migration: &Migration) -> Result<()> {
    let statements: Vec<&str> = migration.sql.split(';').collect();

    for statement in statements {
        let statement = statement.trim();
        if statement.is_empty() || statement.starts_with("--") {
            continue;
        }

        match client.query(statement, &[]).await {
            Ok(_) => {
                debug!(
                    "Executed migration statement: {}",
                    &statement[..statement.len().min(80)]
                );
            }
            Err(e) => {
                warn!(
                    "Migration statement failed (this may be expected): {} - {}",
                    statement, e
                );
                // Continue with other statements - some may fail if they already exist
            }
        }
    }

    // Update the schema version
    let update_sql = format!(
        "UPDATE metadata:system SET schema_version = {}, updated_at = time::now()",
        migration.version
    );

    client.query(&update_sql, &[]).await?;

    Ok(())
}

/// Migration definition
struct Migration {
    version: i32,
    description: &'static str,
    sql: &'static str,
}

/// Ensure the file_hash column exists on existing notes
///
/// This function can be called to add the file_hash column to notes that don't have it yet.
/// It populates the file_hash field by calculating the BLAKE3 hash of the note content.
///
/// # Arguments
/// * `client` - SurrealDB client connection
///
/// # Returns
/// Result<()> indicating success or failure
pub async fn populate_missing_file_hashes(client: &SurrealClient) -> Result<()> {
    info!("Populating missing file_hash fields for existing notes");

    // Find all notes that don't have a file_hash
    let result = client
        .query(
            "SELECT id, content FROM notes WHERE file_hash IS NONE LIMIT 1000",
            &[],
        )
        .await?;

    let mut updated_count = 0;
    for record in result.records {
        if let (Some(id), Some(content)) = (record.data.get("id"), record.data.get("content")) {
            if let (Some(id_str), Some(content_str)) = (id.as_str(), content.as_str()) {
                // Calculate BLAKE3 hash of the content
                let hash = calculate_blake3_hash(content_str);

                // Update the note with the hash
                let update_sql = format!(
                    "UPDATE {} SET file_hash = '{}' WHERE id = {}",
                    id_str, hash, id_str
                );

                match client.query(&update_sql, &[]).await {
                    Ok(_) => updated_count += 1,
                    Err(e) => warn!("Failed to update file_hash for {}: {}", id_str, e),
                }
            }
        }
    }

    info!("Updated file_hash for {} notes", updated_count);
    Ok(())
}

/// Calculate BLAKE3 hash of a string and return as hex
fn calculate_blake3_hash(content: &str) -> String {
    use blake3::Hasher;

    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();

    // Convert to hex string (64 characters for BLAKE3)
    hash.to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_blake3_hash() {
        let content = "Hello, World!";
        let hash = calculate_blake3_hash(content);

        // BLAKE3 hash should be 64 characters long
        assert_eq!(hash.len(), 64);

        // Same content should produce same hash
        let hash2 = calculate_blake3_hash(content);
        assert_eq!(hash, hash2);

        // Different content should produce different hash
        let hash3 = calculate_blake3_hash("Different content");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_hash_consistency() {
        let content = include_str!("schema.surql");
        let hash1 = calculate_blake3_hash(content);
        let hash2 = calculate_blake3_hash(content);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }
}
