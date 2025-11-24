use anyhow::Result;
use tracing::trace;

use crate::SurrealClient;

/// Apply the Entity-Attribute-Value + Graph schema to the provided database.
///
/// This simply executes every statement contained in `schema_eav_graph.surql`. The
/// helper is intentionally forgiving: it skips empty/comment lines and ignores
/// errors from statements that already exist so it can be re-run safely in
/// tests.
pub async fn apply_eav_graph_schema(client: &SurrealClient) -> Result<()> {
    let schema = include_str!("../schema_eav_graph.surql");

    for statement in schema.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }

        // Run each statement individually so failures are easier to diagnose.
        // Ignore "already exists" errors to make schema initialization idempotent
        let result = client.query(trimmed, &[]).await;
        if let Err(e) = result {
            let err_msg = format!("{}", e);
            if !err_msg.contains("already exists") {
                return Err(anyhow::anyhow!(
                    "Failed to execute EAV+Graph schema statement '{}': {}",
                    trimmed,
                    e
                ));
            }
            // Ignore "already exists" errors
            trace!("Schema element already exists (ignoring): {}", trimmed);
        }
    }

    Ok(())
}
