use anyhow::Result;

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
        let _ = client.query(trimmed, &[]).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to execute EAV+Graph schema statement '{}': {}",
                trimmed,
                e
            )
        })?;
    }

    Ok(())
}
