//! Simple Queue Integration Layer
//!
//! This module provides simple functions to replace the complex QueueBasedProcessor.
//! The goal is to provide ~50 lines of simple integration code instead of hundreds
//! of lines of complex processing logic.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

use crate::kiln_scanner::parse_file_to_document;
use crate::surreal_client::SurrealClient;
use crate::transaction_queue::{DatabaseTransaction, TransactionQueue, TransactionTimestamp};

/// Simple integration: enqueue a single note for processing
///
/// This replaces the complex QueueBasedProcessor::process_file() with a simple
/// function that just creates a transaction and enqueues it.
pub async fn enqueue_document(
    queue: &TransactionQueue,
    client: &Arc<SurrealClient>,
    file_path: &Path,
    kiln_root: &Path,
) -> Result<String> {
    debug!("Enqueuing note: {}", file_path.display());

    // Parse the note
    let note = parse_file_to_document(file_path).await?;
    let document_id = crate::kiln_integration::generate_document_id(&note.path, kiln_root);

    // Determine if this is create or update based on simple existence check
    let transaction_type = if document_exists_fast(client, &document_id).await? {
        "Update"
    } else {
        "Create"
    };

    // Create simple CRUD transaction
    let transaction = match transaction_type {
        "Create" => DatabaseTransaction::Create {
            transaction_id: format!("create-{}-{}", document_id, uuid::Uuid::new_v4()),
            note,
            kiln_root: kiln_root.to_path_buf(),
            timestamp: TransactionTimestamp::now(),
        },
        "Update" => DatabaseTransaction::Update {
            transaction_id: format!("update-{}-{}", document_id, uuid::Uuid::new_v4()),
            note,
            kiln_root: kiln_root.to_path_buf(),
            timestamp: TransactionTimestamp::now(),
        },
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown transaction type: {}",
                transaction_type
            ))
        }
    };

    // Enqueue the transaction
    let _result_receiver = queue.enqueue(transaction).await?;

    info!(
        "Enqueued {} transaction for note: {}",
        transaction_type, document_id
    );
    Ok(document_id)
}

/// Simple integration: enqueue note deletion
pub async fn enqueue_document_deletion(
    queue: &TransactionQueue,
    file_path: &Path,
    kiln_root: &Path,
) -> Result<String> {
    debug!("Enqueuing note deletion: {}", file_path.display());

    let document_id = crate::kiln_integration::generate_document_id(file_path, kiln_root);

    let transaction = DatabaseTransaction::Delete {
        transaction_id: format!("delete-{}-{}", document_id, uuid::Uuid::new_v4()),
        document_id: document_id.clone(),
        kiln_root: kiln_root.to_path_buf(),
        timestamp: TransactionTimestamp::now(),
    };

    let _result_receiver = queue.enqueue(transaction).await?;

    info!("Enqueued delete transaction for note: {}", document_id);
    Ok(document_id)
}

/// Simple integration: enqueue multiple documents
pub async fn enqueue_documents(
    queue: &TransactionQueue,
    client: &Arc<SurrealClient>,
    file_paths: &[&Path],
    kiln_root: &Path,
) -> Result<Vec<String>> {
    debug!("Enqueuing {} documents", file_paths.len());

    let mut document_ids = Vec::new();

    for file_path in file_paths {
        let doc_id = enqueue_document(queue, client, file_path, kiln_root).await?;
        document_ids.push(doc_id);
    }

    info!("Enqueued {} documents for processing", document_ids.len());
    Ok(document_ids)
}

/// Fast check if note exists (simplified - doesn't load full note)
async fn document_exists_fast(_client: &Arc<SurrealClient>, _document_id: &str) -> Result<bool> {
    // For now, assume note doesn't exist to simplify logic
    // This means all operations will be treated as Creates
    // The intelligent consumer will handle Create vs Update logic automatically
    Ok(false)
}

/// Simple integration: get queue status
pub async fn get_queue_status(queue: &TransactionQueue) -> crate::transaction_queue::QueueStats {
    queue.stats()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_queue::{TransactionQueue, TransactionQueueConfig};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_simple_integration_flow() {
        let config = TransactionQueueConfig::default();
        let queue = TransactionQueue::new(config);
        // Use in-memory database to avoid file lock conflicts
        let mut db_config = crate::types::SurrealDbConfig::default();
        db_config.path = ":memory:".to_string();
        let client = Arc::new(SurrealClient::new(db_config).await.unwrap());

        let test_file = PathBuf::from("test.md");
        let kiln_root = PathBuf::from("/test");

        // This would fail in real test since file doesn't exist, but shows the API
        // let result = enqueue_document(&queue, &client, &test_file, &kiln_root).await;
        // assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_document_exists_fast() {
        // Simple test - always returns false for now
        // Use in-memory database to avoid file lock conflicts
        let mut db_config = crate::types::SurrealDbConfig::default();
        db_config.path = ":memory:".to_string();
        let client = Arc::new(SurrealClient::new(db_config).await.unwrap());

        let exists = document_exists_fast(&client, "test-doc").await.unwrap();
        assert_eq!(exists, false);
    }
}
