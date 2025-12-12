//! File Hash Lookup Module
//!
//! This module provides efficient database queries for looking up stored file hashes
//! to enable change detection during the file scanning process. It supports both
//! individual file lookups and batch operations for optimal performance.
//!
//! This module also implements the HashLookupStorage trait from crucible-core,
//! providing a complete abstraction layer for hash storage and retrieval operations.

use crate::types::Record;
use crate::utils::normalize_path_string;
use crate::SurrealClient;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, info, warn};

// Import the trait and related types from crucible-core
use crucible_core::traits::change_detection::{
    BatchLookupConfig as CoreBatchLookupConfig, HashLookupResult as CoreHashLookupResult,
    HashLookupStorage, StoredHash,
};
use crucible_core::{FileHash, FileHashInfo, HashAlgorithm, HashError};

/// Information about a stored file hash from the database
#[derive(Debug, Clone, PartialEq)]
pub struct StoredFileHash {
    /// The database record ID (e.g., "entities:note:Projects/file.md")
    pub record_id: String,
    /// The relative file path (e.g., "Projects/file.md")
    pub relative_path: String,
    /// The stored BLAKE3 content hash as a 64-character hex string
    pub file_hash: String,
    /// File size in bytes
    pub file_size: u64,
    /// Last modification timestamp
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

/// Result of a hash lookup operation
#[derive(Debug, Clone, PartialEq)]
pub struct HashLookupResult {
    /// Files that were found in the database with their stored hashes
    pub found_files: HashMap<String, StoredFileHash>, // Key: relative_path
    /// Files that were not found in the database
    pub missing_files: Vec<String>, // relative_path values
    /// Total number of files queried
    pub total_queried: usize,
    /// Number of database round trips
    pub database_round_trips: usize,
}

fn entity_id_body_from_path(relative_path: &str) -> String {
    let normalized = normalize_path_string(relative_path).replace(':', "_");
    if normalized.starts_with("note:") {
        normalized
    } else {
        format!("note:{}", normalized)
    }
}

/// Batch hash lookup configuration
#[derive(Debug, Clone)]
pub struct BatchLookupConfig {
    /// Maximum number of files to query in a single database round trip
    pub max_batch_size: usize,
    /// Whether to use parameterized queries (recommended for security)
    pub use_parameterized_queries: bool,
    /// Whether to cache results during the scanning session
    pub enable_session_cache: bool,
}

impl Default for BatchLookupConfig {
    fn default() -> Self {
        Self {
            // SurrealDB handles IN clauses well up to a few hundred items
            // 100 is a safe default that balances performance and memory
            max_batch_size: 100,
            use_parameterized_queries: true,
            enable_session_cache: true,
        }
    }
}

/// Lookup a single file hash by relative path
pub async fn lookup_file_hash(
    client: &SurrealClient,
    relative_path: &str,
) -> Result<Option<StoredFileHash>> {
    debug!("Looking up hash for file: {}", relative_path);

    let sql = "
        SELECT
            id,
            data.relative_path AS relative_path,
            content_hash,
            data.file_size AS file_size,
            data.source_modified_at AS source_modified_at,
            data.parsed_at AS parsed_at,
            updated_at
        FROM type::thing('entities', $id)
        LIMIT 1
    ";
    let params = vec![serde_json::json!({"id": entity_id_body_from_path(relative_path)})];

    let result = client
        .query(sql, &params)
        .await
        .map_err(|e| anyhow!("Failed to lookup file hash for '{}': {}", relative_path, e))?;

    if let Some(record) = result.records.first() {
        let stored_hash = convert_record_to_stored_hash(&record)?;
        debug!(
            "Found hash for {}: {} (record: {})",
            relative_path,
            &stored_hash.file_hash[..8], // Show first 8 chars for debugging
            stored_hash.record_id
        );
        Ok(Some(stored_hash))
    } else {
        debug!("No hash found for file: {}", relative_path);
        Ok(None)
    }
}

/// Lookup file hashes for multiple files in batches for optimal performance
pub async fn lookup_file_hashes_batch(
    client: &SurrealClient,
    relative_paths: &[String],
    config: Option<BatchLookupConfig>,
) -> Result<HashLookupResult> {
    let config = config.unwrap_or_default();

    if relative_paths.is_empty() {
        return Ok(HashLookupResult {
            found_files: HashMap::new(),
            missing_files: vec![],
            total_queried: 0,
            database_round_trips: 0,
        });
    }

    info!(
        "Looking up hashes for {} files in batches of max {}",
        relative_paths.len(),
        config.max_batch_size
    );

    let mut found_files = HashMap::new();
    let mut missing_files = Vec::new();
    let mut round_trips = 0;

    // Process in batches to avoid overwhelming the database
    for chunk in relative_paths.chunks(config.max_batch_size) {
        round_trips += 1;
        debug!(
            "Processing batch of {} files (round trip {})",
            chunk.len(),
            round_trips
        );

        let batch_result = lookup_file_hashes_batch_internal(client, chunk, &config).await?;

        // Merge results
        let found_count = batch_result.found_files.len();
        let missing_count = batch_result.missing_files.len();

        found_files.extend(batch_result.found_files);
        missing_files.extend(batch_result.missing_files);

        debug!(
            "Batch complete: {} found, {} missing",
            found_count, missing_count
        );
    }

    let total_result = HashLookupResult {
        found_files,
        missing_files,
        total_queried: relative_paths.len(),
        database_round_trips: round_trips,
    };

    info!(
        "Hash lookup complete: {}/{} files found in {} round trips",
        total_result.found_files.len(),
        total_result.total_queried,
        total_result.database_round_trips
    );

    Ok(total_result)
}

/// Internal batch lookup implementation
async fn lookup_file_hashes_batch_internal(
    client: &SurrealClient,
    relative_paths: &[String],
    _config: &BatchLookupConfig,
) -> Result<HashLookupResult> {
    if relative_paths.is_empty() {
        return Ok(HashLookupResult {
            found_files: HashMap::new(),
            missing_files: vec![],
            total_queried: 0,
            database_round_trips: 0,
        });
    }

    // Build query with array parameter for SurrealDB against the entity payload
    let sql = "
        SELECT
            id,
            data.relative_path AS relative_path,
            content_hash,
            data.file_size AS file_size,
            data.source_modified_at AS source_modified_at,
            data.parsed_at AS parsed_at,
            updated_at
        FROM entities
        WHERE data.relative_path IN $paths
    ";

    // Create parameter object with the paths array
    let params = vec![serde_json::json!({
        "paths": relative_paths
    })];

    debug!("Executing batch query for {} files", relative_paths.len());

    let result = client
        .query(sql, &params)
        .await
        .map_err(|e| anyhow!("Batch hash lookup failed: {}", e))?;

    let mut found_files = HashMap::new();
    let found_paths: std::collections::HashSet<_> = result
        .records
        .iter()
        .map(|record| {
            let stored_hash = convert_record_to_stored_hash(&record)?;
            let path = stored_hash.relative_path.clone();
            found_files.insert(path.clone(), stored_hash);
            Ok::<String, anyhow::Error>(path)
        })
        .collect::<Result<_, _>>()?;

    // Determine which files were not found
    let missing_files: Vec<String> = relative_paths
        .iter()
        .filter(|path| !found_paths.contains(*path))
        .cloned()
        .collect();

    debug!(
        "Batch query returned {} records, {} files missing",
        result.records.len(),
        missing_files.len()
    );

    Ok(HashLookupResult {
        found_files,
        missing_files,
        total_queried: relative_paths.len(),
        database_round_trips: 1,
    })
}

/// Lookup file hashes by file content hashes (for finding duplicate content)
pub async fn lookup_files_by_content_hashes(
    client: &SurrealClient,
    content_hashes: &[String],
) -> Result<HashMap<String, Vec<StoredFileHash>>> {
    if content_hashes.is_empty() {
        return Ok(HashMap::new());
    }

    debug!(
        "Looking up files by {} content hashes",
        content_hashes.len()
    );

    // Build query with array parameter for SurrealDB
    // SurrealDB expects named parameters as objects, not positional parameters
    let sql = "
        SELECT
            id,
            data.relative_path AS relative_path,
            content_hash,
            data.file_size AS file_size,
            data.source_modified_at AS source_modified_at,
            data.parsed_at AS parsed_at,
            updated_at
        FROM entities
        WHERE content_hash IN $hashes
    ";

    // Create parameter object with the hashes array
    let params = vec![serde_json::json!({
        "hashes": content_hashes
    })];

    let result = client
        .query(sql, &params)
        .await
        .map_err(|e| anyhow!("Content hash lookup failed: {}", e))?;

    let mut hash_to_files = HashMap::new();

    for record in result.records {
        let stored_hash = convert_record_to_stored_hash(&record)?;
        hash_to_files
            .entry(stored_hash.file_hash.clone())
            .or_insert_with(Vec::new)
            .push(stored_hash);
    }

    debug!(
        "Found {} files matching {} content hashes",
        hash_to_files.values().map(|v| v.len()).sum::<usize>(),
        content_hashes.len()
    );

    Ok(hash_to_files)
}

/// Get all files that have changed since a given timestamp
pub async fn lookup_changed_files_since(
    client: &SurrealClient,
    since: chrono::DateTime<chrono::Utc>,
    limit: Option<usize>,
) -> Result<Vec<StoredFileHash>> {
    debug!("Looking up files changed since {}", since);

    let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();

    let sql = format!(
        "
        SELECT
            id,
            data.relative_path AS relative_path,
            content_hash,
            data.file_size AS file_size,
            data.source_modified_at AS source_modified_at,
            data.parsed_at AS parsed_at,
            updated_at
        FROM entities
        WHERE entity_type = 'note'
          AND updated_at > time::('{}')
        ORDER BY updated_at DESC
        {}
        ",
        since.to_rfc3339(),
        limit_clause
    );

    let result = client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow!("Failed to lookup changed files: {}", e))?;

    let mut changed_files = Vec::new();
    for record in result.records {
        match convert_record_to_stored_hash(&record) {
            Ok(stored_hash) => changed_files.push(stored_hash),
            Err(e) => warn!("Failed to convert record to stored hash: {}", e),
        }
    }

    debug!(
        "Found {} files changed since {}",
        changed_files.len(),
        since
    );
    Ok(changed_files)
}

/// Check if a file needs to be reprocessed based on hash comparison
pub async fn check_file_needs_update(
    client: &SurrealClient,
    relative_path: &str,
    new_hash: &str,
) -> Result<bool> {
    match lookup_file_hash(client, relative_path).await? {
        Some(stored_hash) => {
            let needs_update = stored_hash.file_hash != new_hash;
            if needs_update {
                debug!(
                    "File {} needs update (stored: {}, new: {})",
                    relative_path,
                    &stored_hash.file_hash[..8],
                    &new_hash[..8]
                );
            } else {
                debug!(
                    "File {} unchanged (hash: {})",
                    relative_path,
                    &new_hash[..8]
                );
            }
            Ok(needs_update)
        }
        None => {
            debug!(
                "File {} not found in database, needs processing",
                relative_path
            );
            Ok(true) // File doesn't exist, needs processing
        }
    }
}

/// Convert StoredFileHash to the core StoredHash type
pub fn convert_to_core_stored_hash(stored: &StoredFileHash) -> Result<StoredHash, HashError> {
    // Parse the file_hash string into a FileHash
    let content_hash = FileHash::from_hex(&stored.file_hash).map_err(|e| HashError::IoError {
        error: format!("Failed to parse hash: {}", e),
    })?;

    Ok(StoredHash::new(
        stored.record_id.clone(),
        stored.relative_path.clone(),
        content_hash,
        stored.file_size,
        stored.modified_at,
    ))
}

/// Convert core StoredHash to local StoredFileHash type
pub fn convert_from_core_stored_hash(stored: &StoredHash) -> StoredFileHash {
    StoredFileHash {
        record_id: stored.record_id.clone(),
        relative_path: stored.relative_path.clone(),
        file_hash: stored.content_hash.to_hex(),
        file_size: stored.file_size,
        modified_at: stored.modified_at,
    }
}

/// Convert local HashLookupResult to core HashLookupResult
pub fn convert_to_core_hash_lookup_result(
    local: &crate::hash_lookup::HashLookupResult,
) -> Result<CoreHashLookupResult, HashError> {
    let mut found_files = HashMap::new();

    for (path, stored) in &local.found_files {
        let core_stored = convert_to_core_stored_hash(stored)?;
        found_files.insert(path.clone(), core_stored);
    }

    Ok(CoreHashLookupResult {
        found_files,
        missing_files: local.missing_files.clone(),
        total_queried: local.total_queried,
        database_round_trips: local.database_round_trips,
    })
}

/// Convert core BatchLookupConfig to local BatchLookupConfig
pub fn convert_from_core_batch_config(config: Option<CoreBatchLookupConfig>) -> BatchLookupConfig {
    config
        .map(|core_config| BatchLookupConfig {
            max_batch_size: core_config.max_batch_size,
            use_parameterized_queries: core_config.use_parameterized_queries,
            enable_session_cache: core_config.enable_session_cache,
        })
        .unwrap_or_default()
}

/// Convert a database record to StoredFileHash
fn convert_record_to_stored_hash(record: &Record) -> Result<StoredFileHash> {
    let record_id = record
        .id
        .as_ref()
        .ok_or_else(|| anyhow!("Missing id in record"))?
        .to_string();

    let relative_path = record
        .data
        .get("relative_path")
        .or_else(|| record.data.get("path"))
        .or_else(|| record.data.get("data").and_then(|v| v.get("relative_path")))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid relative_path in record"))?
        .to_string();

    let file_hash = record
        .data
        .get("file_hash")
        .or_else(|| record.data.get("content_hash"))
        .or_else(|| record.data.get("data").and_then(|v| v.get("content_hash")))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid content_hash in record"))?
        .to_string();

    // Validate hash format (should be 64 hex characters for BLAKE3)
    if file_hash.len() != 64 || !file_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!(
            "Invalid file_hash format: {} (expected 64 hex chars)",
            file_hash
        ));
    }

    let file_size = record
        .data
        .get("file_size")
        .or_else(|| record.data.get("data").and_then(|v| v.get("file_size")))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("Missing or invalid file_size in record"))?;

    let modified_at = record
        .data
        .get("source_modified_at")
        .or_else(|| record.data.get("modified_at"))
        .or_else(|| record.data.get("parsed_at"))
        .or_else(|| record.data.get("updated_at"))
        .or_else(|| {
            record
                .data
                .get("data")
                .and_then(|v| v.get("source_modified_at"))
        })
        .or_else(|| record.data.get("data").and_then(|v| v.get("parsed_at")))
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    Ok(StoredFileHash {
        record_id,
        relative_path,
        file_hash,
        file_size,
        modified_at,
    })
}

/// Session cache for hash lookups during scanning
#[derive(Debug, Clone, Default)]
pub struct HashLookupCache {
    cache: HashMap<String, Option<StoredFileHash>>,
    hits: u64,
    misses: u64,
}

impl HashLookupCache {
    /// Create a new cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a value from the cache
    pub fn get(&self, key: &str) -> Option<Option<StoredFileHash>> {
        self.cache.get(key).cloned()
    }

    /// Set a value in the cache
    pub fn set(&mut self, key: String, value: Option<StoredFileHash>) {
        self.cache.insert(key, value);
    }

    /// Get multiple values from cache, returning which ones are cached and which are not
    pub fn get_cached_keys(
        &self,
        keys: &[String],
    ) -> (HashMap<String, Option<StoredFileHash>>, Vec<String>) {
        let mut cached = HashMap::new();
        let mut uncached = Vec::new();

        for key in keys {
            match self.get(key) {
                Some(value) => {
                    cached.insert(key.clone(), value);
                }
                None => {
                    uncached.push(key.clone());
                }
            }
        }

        (cached, uncached)
    }

    /// Cache multiple values
    pub fn set_batch(&mut self, values: HashMap<String, Option<StoredFileHash>>) {
        for (key, value) in values {
            self.set(key, value);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            hits: self.hits,
            misses: self.misses,
            hit_rate: if self.hits + self.misses > 0 {
                self.hits as f64 / (self.hits + self.misses) as f64
            } else {
                0.0
            },
        }
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in cache
    pub entries: usize,
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Hit rate as a percentage (0.0 to 1.0)
    pub hit_rate: f64,
}

/// Batch hash lookup with caching support
pub async fn lookup_file_hashes_batch_cached(
    client: &SurrealClient,
    relative_paths: &[String],
    config: Option<BatchLookupConfig>,
    cache: &mut HashLookupCache,
) -> Result<HashLookupResult> {
    let config = config.unwrap_or_default();

    if relative_paths.is_empty() {
        return Ok(HashLookupResult {
            found_files: HashMap::new(),
            missing_files: vec![],
            total_queried: 0,
            database_round_trips: 0,
        });
    }

    // Check cache first if enabled
    let (cached_results, uncached_paths) = if config.enable_session_cache {
        cache.get_cached_keys(relative_paths)
    } else {
        (HashMap::new(), relative_paths.to_vec())
    };

    debug!(
        "Cache hit: {} cached, {} uncached",
        cached_results.len(),
        uncached_paths.len()
    );

    // Update cache statistics
    if config.enable_session_cache {
        cache.hits += cached_results.len() as u64;
        cache.misses += uncached_paths.len() as u64;
    }

    // Process uncached files
    let mut final_found_files = HashMap::new();
    let mut final_missing_files = Vec::new();

    // Add cached results to final results
    for (path, cached_value) in cached_results {
        match cached_value {
            Some(stored_hash) => {
                final_found_files.insert(path, stored_hash);
            }
            None => {
                final_missing_files.push(path);
            }
        }
    }

    // Query uncached files
    if !uncached_paths.is_empty() {
        let db_result =
            lookup_file_hashes_batch(client, &uncached_paths, Some(config.clone())).await?;

        // Cache the database results
        if config.enable_session_cache {
            let mut cache_updates = HashMap::new();

            for (path, stored_hash) in &db_result.found_files {
                cache_updates.insert(path.clone(), Some(stored_hash.clone()));
            }

            for missing_path in &db_result.missing_files {
                cache_updates.insert(missing_path.clone(), None);
            }

            cache.set_batch(cache_updates);
        }

        // Merge database results
        final_found_files.extend(db_result.found_files);
        final_missing_files.extend(db_result.missing_files);
    }

    Ok(HashLookupResult {
        found_files: final_found_files,
        missing_files: final_missing_files,
        total_queried: relative_paths.len(),
        database_round_trips: if uncached_paths.is_empty() {
            0
        } else {
            (uncached_paths.len() + config.max_batch_size - 1) / config.max_batch_size
        },
    })
}

/// Implementation of the HashLookupStorage trait for SurrealDB
///
/// This struct provides a complete implementation of the HashLookupStorage trait
/// using the existing hash lookup functions in this module.
pub struct SurrealHashLookupStorage<'a> {
    client: &'a SurrealClient,
}

impl<'a> SurrealHashLookupStorage<'a> {
    /// Create a new hash lookup storage instance
    pub fn new(client: &'a SurrealClient) -> Self {
        Self { client }
    }

    /// Get the underlying client
    pub fn client(&self) -> &'a SurrealClient {
        self.client
    }
}

#[async_trait]
impl<'a> HashLookupStorage for SurrealHashLookupStorage<'a> {
    /// Lookup a single file hash by relative path
    async fn lookup_file_hash(&self, relative_path: &str) -> Result<Option<StoredHash>, HashError> {
        match lookup_file_hash(self.client, relative_path).await {
            Ok(Some(stored)) => {
                let core_stored = convert_to_core_stored_hash(&stored)?;
                Ok(Some(core_stored))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(HashError::IoError {
                error: e.to_string(),
            }),
        }
    }

    /// Lookup file hashes for multiple files in batches for optimal performance
    async fn lookup_file_hashes_batch(
        &self,
        relative_paths: &[String],
        config: Option<CoreBatchLookupConfig>,
    ) -> Result<CoreHashLookupResult, HashError> {
        let local_config = convert_from_core_batch_config(config);

        match lookup_file_hashes_batch(self.client, relative_paths, Some(local_config)).await {
            Ok(result) => convert_to_core_hash_lookup_result(&result),
            Err(e) => Err(HashError::IoError {
                error: e.to_string(),
            }),
        }
    }

    /// Lookup files by their content hashes (for finding duplicate content)
    async fn lookup_files_by_content_hashes(
        &self,
        content_hashes: &[FileHash],
    ) -> Result<HashMap<String, Vec<StoredHash>>, HashError> {
        let hash_strings: Vec<String> = content_hashes.iter().map(|hash| hash.to_hex()).collect();

        match lookup_files_by_content_hashes(self.client, &hash_strings).await {
            Ok(local_results) => {
                let mut core_results = HashMap::new();

                for (hash_string, stored_files) in local_results {
                    let mut core_stored_files = Vec::new();

                    for stored in stored_files {
                        let core_stored = convert_to_core_stored_hash(&stored)?;
                        core_stored_files.push(core_stored);
                    }

                    core_results.insert(hash_string, core_stored_files);
                }

                Ok(core_results)
            }
            Err(e) => Err(HashError::IoError {
                error: e.to_string(),
            }),
        }
    }

    /// Get all files that have changed since a given timestamp
    async fn lookup_changed_files_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredHash>, HashError> {
        match lookup_changed_files_since(self.client, since, limit).await {
            Ok(local_files) => {
                let mut core_files = Vec::new();

                for stored in local_files {
                    let core_stored = convert_to_core_stored_hash(&stored)?;
                    core_files.push(core_stored);
                }

                Ok(core_files)
            }
            Err(e) => Err(HashError::IoError {
                error: e.to_string(),
            }),
        }
    }

    /// Check if a file needs to be reprocessed based on hash comparison
    async fn check_file_needs_update(
        &self,
        relative_path: &str,
        new_hash: &FileHash,
    ) -> Result<bool, HashError> {
        let new_hash_string = new_hash.to_hex();

        match check_file_needs_update(self.client, relative_path, &new_hash_string).await {
            Ok(needs_update) => Ok(needs_update),
            Err(e) => Err(HashError::IoError {
                error: e.to_string(),
            }),
        }
    }

    /// Store hash information for multiple files
    async fn store_hashes(&self, files: &[FileHashInfo]) -> Result<(), HashError> {
        if files.is_empty() {
            return Ok(());
        }

        debug!("Storing hashes for {} files", files.len());

        for file_info in files {
            let record_body = entity_id_body_from_path(&file_info.relative_path);
            let modified_at = chrono::DateTime::<chrono::Utc>::from(file_info.modified);
            let params = serde_json::json!({
                "id": record_body,
                "relative_path": file_info.relative_path,
                "content_hash": file_info.content_hash.to_hex(),
                "file_size": file_info.size,
                "source_modified_at": modified_at.to_rfc3339(),
                "algorithm": file_info.algorithm.to_string()
            });

            // Check if record exists first
            let check_sql = "SELECT id FROM type::thing('entities', $id)";
            let exists = match self.client.query(check_sql, &[params.clone()]).await {
                Ok(result) => !result.records.is_empty(),
                Err(_) => false,
            };

            let sql = if exists {
                // Update existing record
                "
                    UPDATE type::thing('entities', $id)
                    SET
                        content_hash = $content_hash,
                        data.relative_path = $relative_path,
                        data.path = $relative_path,
                        data.file_size = $file_size,
                        data.source_modified_at = $source_modified_at,
                        data.hash_algorithm = $algorithm,
                        data.content_hash = $content_hash,
                        updated_at = time::now()
                    RETURN NONE;
                "
            } else {
                // Create new record with all required fields
                "
                    CREATE type::thing('entities', $id)
                    SET
                        type = 'note',
                        entity_type = 'note',
                        content_hash = $content_hash,
                        data.relative_path = $relative_path,
                        data.path = $relative_path,
                        data.file_size = $file_size,
                        data.source_modified_at = $source_modified_at,
                        data.hash_algorithm = $algorithm,
                        data.content_hash = $content_hash,
                        updated_at = time::now()
                    RETURN NONE;
                "
            };

            if let Err(e) = self.client.query(sql, &[params]).await {
                warn!(
                    "Failed to {} hash metadata for {}: {}",
                    if exists { "update" } else { "create" },
                    file_info.relative_path,
                    e
                );
                return Err(HashError::IoError {
                    error: e.to_string(),
                });
            }
        }

        debug!("Successfully stored hashes for {} files", files.len());
        Ok(())
    }

    /// Remove hash information for specific files
    async fn remove_hashes(&self, paths: &[String]) -> Result<(), HashError> {
        if paths.is_empty() {
            return Ok(());
        }

        debug!("Removing hashes for {} files", paths.len());

        let sql = "
            DELETE entities
            WHERE entity_type = 'note'
              AND data.relative_path IN $paths
        ";
        let params = vec![serde_json::json!({ "paths": paths })];

        match self.client.query(sql, &params).await {
            Ok(_) => {
                debug!("Successfully removed hashes for {} files", paths.len());
                Ok(())
            }
            Err(e) => {
                warn!("Failed to remove hashes for {} files: {}", paths.len(), e);
                Err(HashError::IoError {
                    error: e.to_string(),
                })
            }
        }
    }

    /// Get all stored hash information
    async fn get_all_hashes(&self) -> Result<HashMap<String, FileHashInfo>, HashError> {
        debug!("Retrieving all stored hashes");

        let sql = "
            SELECT
                id,
                data.relative_path AS relative_path,
                content_hash,
                data.file_size AS file_size,
                data.source_modified_at AS source_modified_at,
                data.parsed_at AS parsed_at,
                updated_at
            FROM entities
            WHERE entity_type = 'note'
              AND content_hash != NONE
            ORDER BY data.relative_path
        ";

        match self.client.query(sql, &[]).await {
            Ok(result) => {
                let mut all_hashes = HashMap::new();

                for record in result.records {
                    match convert_record_to_stored_hash(&record) {
                        Ok(stored) => {
                            // Convert to FileHashInfo
                            let content_hash =
                                FileHash::from_hex(&stored.file_hash).map_err(|e| {
                                    HashError::IoError {
                                        error: format!("Failed to parse hash: {}", e),
                                    }
                                })?;

                            let file_info = FileHashInfo::new(
                                content_hash,
                                stored.file_size,
                                stored.modified_at.into(),
                                HashAlgorithm::Blake3, // Default to Blake3 for existing records
                                stored.relative_path.clone(),
                            );

                            all_hashes.insert(stored.relative_path, file_info);
                        }
                        Err(e) => {
                            warn!("Failed to convert record to stored hash: {}", e);
                            // Continue processing other records
                        }
                    }
                }

                debug!("Retrieved {} stored hashes", all_hashes.len());
                Ok(all_hashes)
            }
            Err(e) => {
                warn!("Failed to retrieve all hashes: {}", e);
                Err(HashError::IoError {
                    error: e.to_string(),
                })
            }
        }
    }

    /// Clear all stored hash information
    async fn clear_all_hashes(&self) -> Result<(), HashError> {
        debug!("Clearing all stored hashes");

        let sql = "DELETE entities WHERE entity_type = 'note'";

        match self.client.query(sql, &[]).await {
            Ok(_) => {
                debug!("Successfully cleared all stored hashes");
                Ok(())
            }
            Err(e) => {
                warn!("Failed to clear all hashes: {}", e);
                Err(HashError::IoError {
                    error: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiln_integration::initialize_kiln_schema;
    use crate::types::RecordId;
    use crate::SurrealClient;

    async fn setup_client() -> SurrealClient {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        initialize_kiln_schema(&client).await.unwrap();
        client
    }

    #[tokio::test]
    async fn test_convert_record_to_stored_hash() {
        let record = Record {
            id: Some(RecordId("entities:note:test_file".to_string())),
            data: serde_json::json!({
                "relative_path": "test/path.md",
                "content_hash": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "file_size": 1024,
                "source_modified_at": "2023-01-01T00:00:00Z"
            })
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        };

        let result = convert_record_to_stored_hash(&record).unwrap();

        assert_eq!(result.record_id, "entities:note:test_file");
        assert_eq!(result.relative_path, "test/path.md");
        assert_eq!(
            result.file_hash,
            "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        );
        assert_eq!(result.file_size, 1024);
    }

    #[tokio::test]
    async fn test_hash_lookup_cache() {
        let mut cache = HashLookupCache::new();

        // Test empty cache
        assert_eq!(cache.get("test.md"), None);

        // Test setting and getting
        let test_hash = StoredFileHash {
            record_id: "entities:note:test".to_string(),
            relative_path: "test.md".to_string(),
            file_hash: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
                .to_string(),
            file_size: 1024,
            modified_at: chrono::Utc::now(),
        };

        cache.set("test.md".to_string(), Some(test_hash.clone()));
        assert_eq!(cache.get("test.md"), Some(Some(test_hash)));

        // Test cache statistics
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[tokio::test]
    async fn test_batch_lookup_config() {
        let config = BatchLookupConfig::default();
        assert_eq!(config.max_batch_size, 100);
        assert!(config.use_parameterized_queries);
        assert!(config.enable_session_cache);
    }

    #[tokio::test]
    async fn test_hash_lookup_empty_paths() {
        let client = setup_client().await;

        let result = lookup_file_hashes_batch(&client, &[], None).await.unwrap();
        assert_eq!(result.total_queried, 0);
        assert_eq!(result.found_files.len(), 0);
        assert_eq!(result.missing_files.len(), 0);
        assert_eq!(result.database_round_trips, 0);
    }

    #[tokio::test]
    async fn test_hash_lookup_storage_trait_implementation() {
        let client = setup_client().await;
        let storage = SurrealHashLookupStorage::new(&client);

        // Test lookup_file_hash for non-existent file
        let result = storage.lookup_file_hash("nonexistent.md").await.unwrap();
        assert!(result.is_none());

        // Test batch lookup for empty list
        let batch_result = storage.lookup_file_hashes_batch(&[], None).await.unwrap();
        assert_eq!(batch_result.total_queried, 0);
        assert_eq!(batch_result.found_files.len(), 0);
        assert_eq!(batch_result.missing_files.len(), 0);

        // Test store_hashes with empty list
        storage.store_hashes(&[]).await.unwrap();

        // Test remove_hashes with empty list
        storage.remove_hashes(&[]).await.unwrap();

        // Test get_all_hashes on empty database
        let all_hashes = storage.get_all_hashes().await.unwrap();
        assert_eq!(all_hashes.len(), 0);

        // Test clear_all_hashes on empty database
        storage.clear_all_hashes().await.unwrap();
    }

    #[tokio::test]
    async fn test_hash_lookup_storage_store_and_retrieve() {
        let client = setup_client().await;
        let storage = SurrealHashLookupStorage::new(&client);

        // Create test file hash info
        let hash = FileHash::new([1u8; 32]);
        let file_info = FileHashInfo::new(
            hash,
            1024,
            std::time::SystemTime::now(),
            HashAlgorithm::Blake3,
            "test.md".to_string(),
        );

        // Store the hash
        storage.store_hashes(&[file_info.clone()]).await.unwrap();

        // Test single lookup
        let result = storage.lookup_file_hash("test.md").await.unwrap();
        assert!(result.is_some());
        let stored = result.unwrap();
        assert_eq!(stored.relative_path, "test.md");
        assert_eq!(stored.content_hash, hash);
        assert_eq!(stored.file_size, 1024);

        // Test batch lookup
        let paths = vec!["test.md".to_string(), "missing.md".to_string()];
        let batch_result = storage
            .lookup_file_hashes_batch(&paths, None)
            .await
            .unwrap();
        assert_eq!(batch_result.found_files.len(), 1);
        assert_eq!(batch_result.missing_files.len(), 1);
        assert!(batch_result.found_files.contains_key("test.md"));
        assert!(batch_result
            .missing_files
            .contains(&"missing.md".to_string()));

        // Test get_all_hashes
        let all_hashes = storage.get_all_hashes().await.unwrap();
        assert_eq!(all_hashes.len(), 1);
        assert!(all_hashes.contains_key("test.md"));

        // Test check_file_needs_update
        let needs_update_same = storage
            .check_file_needs_update("test.md", &hash)
            .await
            .unwrap();
        assert!(!needs_update_same);

        let different_hash = FileHash::new([2u8; 32]);
        let needs_update_diff = storage
            .check_file_needs_update("test.md", &different_hash)
            .await
            .unwrap();
        assert!(needs_update_diff);

        // Test remove_hashes
        storage
            .remove_hashes(&["test.md".to_string()])
            .await
            .unwrap();
        let result_after_removal = storage.lookup_file_hash("test.md").await.unwrap();
        assert!(result_after_removal.is_none());
    }

    #[tokio::test]
    async fn test_conversion_functions() {
        let stored = StoredFileHash {
            record_id: "entities:note:test".to_string(),
            relative_path: "test.md".to_string(),
            file_hash: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
                .to_string(),
            file_size: 1024,
            modified_at: chrono::Utc::now(),
        };

        // Test conversion to core type
        let core_stored = convert_to_core_stored_hash(&stored).unwrap();
        assert_eq!(core_stored.record_id, stored.record_id);
        assert_eq!(core_stored.relative_path, stored.relative_path);
        assert_eq!(core_stored.file_size, stored.file_size);
        assert_eq!(core_stored.content_hash.to_hex(), stored.file_hash);

        // Test conversion back from core type
        let converted_back = convert_from_core_stored_hash(&core_stored);
        assert_eq!(converted_back, stored);
    }

    #[tokio::test]
    async fn test_batch_lookup_config_conversion() {
        let core_config = CoreBatchLookupConfig {
            max_batch_size: 50,
            use_parameterized_queries: false,
            enable_session_cache: false,
        };

        let local_config = convert_from_core_batch_config(Some(core_config.clone()));
        assert_eq!(local_config.max_batch_size, core_config.max_batch_size);
        assert_eq!(
            local_config.use_parameterized_queries,
            core_config.use_parameterized_queries
        );
        assert_eq!(
            local_config.enable_session_cache,
            core_config.enable_session_cache
        );

        // Test default conversion
        let default_config = convert_from_core_batch_config(None);
        assert_eq!(default_config.max_batch_size, 100); // Default value
        assert!(default_config.use_parameterized_queries); // Default value
        assert!(default_config.enable_session_cache); // Default value
    }
}
