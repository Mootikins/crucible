use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, trace};

use crate::SurrealClient;

/// Schema version - increment when schema changes
const SCHEMA_VERSION: &str = "v1.0.0";

/// Static flag to track if schema has been applied in this process
/// This avoids repeated schema checks for the same client connection
static SCHEMA_APPLIED: AtomicBool = AtomicBool::new(false);

/// Apply the Entity-Attribute-Value + Graph schema to the provided database.
///
/// This function is optimized for fast startup:
/// 1. Checks if schema was already applied in this process (instant return)
/// 2. Checks if schema version exists in database (fast query)
/// 3. If needed, batches all statements into a single transaction
///
/// The helper is intentionally forgiving: it ignores "already exists" errors
/// to make schema initialization idempotent.
pub async fn apply_eav_graph_schema(client: &SurrealClient) -> Result<()> {
    // Fast path: if we've already applied schema in this process, skip
    if SCHEMA_APPLIED.load(Ordering::Relaxed) {
        trace!("Schema already applied in this process, skipping");
        return Ok(());
    }

    // Check if schema version exists in database
    if check_schema_version(client).await? {
        debug!(
            "Schema version {} already present, skipping initialization",
            SCHEMA_VERSION
        );
        SCHEMA_APPLIED.store(true, Ordering::Relaxed);
        return Ok(());
    }

    // Apply schema using batched approach
    apply_schema_batched(client).await?;

    // Mark schema version in database
    mark_schema_version(client).await?;

    // Update process-level cache
    SCHEMA_APPLIED.store(true, Ordering::Relaxed);
    debug!("Schema {} applied successfully", SCHEMA_VERSION);

    Ok(())
}

/// Check if the current schema version is already applied
async fn check_schema_version(client: &SurrealClient) -> Result<bool> {
    let query = "SELECT * FROM _schema_version WHERE version = $version LIMIT 1";
    let params = vec![serde_json::json!({ "version": SCHEMA_VERSION })];

    let result = client.query(query, &params).await;

    // If query fails (e.g., table doesn't exist), schema isn't applied
    match result {
        Ok(r) => Ok(!r.records.is_empty()),
        Err(_) => Ok(false),
    }
}

/// Mark the current schema version as applied
async fn mark_schema_version(client: &SurrealClient) -> Result<()> {
    // Create version tracking table if needed and insert version
    let queries = format!(
        r#"
        DEFINE TABLE IF NOT EXISTS _schema_version SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS version ON TABLE _schema_version TYPE string;
        DEFINE FIELD IF NOT EXISTS applied_at ON TABLE _schema_version TYPE datetime DEFAULT time::now();
        DELETE _schema_version;
        CREATE _schema_version SET version = '{}', applied_at = time::now();
        "#,
        SCHEMA_VERSION
    );

    // Execute version tracking queries
    for query in queries.split(';') {
        let trimmed = query.trim();
        if !trimmed.is_empty() {
            let _ = client.query(trimmed, &[]).await;
        }
    }

    Ok(())
}

/// Apply schema using batched statements for better performance
async fn apply_schema_batched(client: &SurrealClient) -> Result<()> {
    let schema = include_str!("../schema_eav_graph.surql");
    let start = std::time::Instant::now();

    // Collect valid statements
    let statements: Vec<&str> = schema
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with("--"))
        .collect();

    debug!("Applying {} schema statements", statements.len());

    // Try batched execution first (faster but may fail on some statements)
    let batch_query = statements.join(";\n");
    let batch_result = client.query(&batch_query, &[]).await;

    if batch_result.is_ok() {
        debug!("Schema applied via batch in {:?}", start.elapsed());
        return Ok(());
    }

    // Fallback: execute statements individually for better error handling
    debug!("Batch failed, falling back to individual statement execution");
    for statement in statements {
        let result = client.query(statement, &[]).await;
        if let Err(e) = result {
            let err_msg = format!("{}", e);
            // Ignore "already exists" errors for idempotency
            if !err_msg.contains("already exists")
                && !err_msg.contains("already defined")
                && !err_msg.contains("IF NOT EXISTS")
            {
                return Err(anyhow::anyhow!(
                    "Failed to execute EAV+Graph schema statement '{}...': {}",
                    &statement[..statement.len().min(50)],
                    e
                ));
            }
            trace!(
                "Schema element already exists (ignoring): {}...",
                &statement[..statement.len().min(30)]
            );
        }
    }

    debug!(
        "Schema applied via individual statements in {:?}",
        start.elapsed()
    );
    Ok(())
}

/// Clear the schema cache (useful for testing)
#[allow(dead_code)]
pub fn clear_schema_cache() {
    SCHEMA_APPLIED.store(false, Ordering::Relaxed);
}
