//! In-memory database backend implementation
//!
//! This module provides an in-memory database backend for testing and development
//! purposes. It implements the DatabaseBackendTrait using in-memory data structures.

use super::{DatabaseBackendTrait, DataStoreConfig};
use crate::service_types::*;
use anyhow::Result;
use async_trait::async_trait;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// In-memory database backend
pub struct MemoryBackend {
    /// In-memory storage for documents by database
    storage: Arc<RwLock<HashMap<String, HashMap<String, DocumentData>>>>,
    /// In-memory storage for indexes
    indexes: Arc<RwLock<HashMap<String, HashMap<String, IndexInfo>>>>,
    /// In-memory storage for schemas
    schemas: Arc<RwLock<HashMap<String, DatabaseSchema>>>,
    /// Database configuration
    config: MemoryConfig,
    /// Transaction storage
    transactions: Arc<RwLock<HashMap<String, TransactionState>>>,
}

/// Transaction state for in-memory backend
#[derive(Debug, Clone)]
struct TransactionState {
    /// Transaction ID
    id: String,
    /// Original state snapshot
    snapshot: HashMap<String, HashMap<String, DocumentData>>,
    /// Operations performed
    operations: Vec<TransactionOperation>,
}

/// Transaction operation
#[derive(Debug, Clone)]
enum TransactionOperation {
    Create { database: String, id: String, data: DocumentData },
    Update { database: String, id: String, old_data: DocumentData, new_data: DocumentData },
    Delete { database: String, id: String, old_data: DocumentData },
}

impl MemoryBackend {
    /// Create a new in-memory backend
    pub async fn new(config: MemoryConfig) -> Result<Self> {
        Ok(Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
            indexes: Arc::new(RwLock::new(HashMap::new())),
            schemas: Arc::new(RwLock::new(HashMap::new())),
            config,
            transactions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get or create database storage
    async fn get_database_storage(&self, database: &str) -> HashMap<String, DocumentData> {
        let storage = self.storage.read().await;
        storage.get(database).cloned().unwrap_or_default()
    }

    /// Update database storage
    async fn update_database_storage(&self, database: &str, documents: HashMap<String, DocumentData>) {
        let mut storage = self.storage.write().await;
        storage.insert(database.to_string(), documents);
    }

    /// Check document limit
    async fn check_document_limit(&self, database: &str) -> Result<()> {
        if let Some(max_docs) = self.config.max_documents {
            let storage = self.storage.read().await;
            if let Some(db_storage) = storage.get(database) {
                if db_storage.len() >= max_docs as usize {
                    return Err(anyhow::anyhow!("Document limit exceeded for database: {}", database));
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl DatabaseBackendTrait for MemoryBackend {
    async fn initialize(&self) -> Result<()> {
        // Initialize default database
        let mut storage = self.storage.write().await;
        storage.insert("default".to_string(), HashMap::new());
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        // Clear all data
        self.storage.write().await.clear();
        self.indexes.write().await.clear();
        self.schemas.write().await.clear();
        self.transactions.write().await.clear();

        // Persist to disk if configured
        if self.config.persist_to_disk.unwrap_or(false) {
            if let Some(path) = &self.config.persistence_path {
                // In a real implementation, we would serialize and save the data
                log::info!("Persisting in-memory database to: {}", path);
            }
        }

        Ok(())
    }

    async fn health_check(&self) -> Result<ServiceHealth> {
        let storage = self.storage.read().await;
        let total_documents: usize = storage.values().map(|db| db.len()).sum();

        Ok(ServiceHealth {
            status: crate::service_types::ServiceStatus::Healthy,
            message: Some("In-memory database is running".to_string()),
            last_check: chrono::Utc::now(),
            response_time: std::time::Duration::from_millis(1),
            resource_usage: Some(ResourceUsage {
                memory_bytes: (total_documents * 1024) as u64, // Rough estimate
                cpu_percentage: 0.0,
                disk_bytes: 0,
                network_bytes: 0,
                open_files: 0,
                active_threads: 1,
                measured_at: chrono::Utc::now(),
            }),
        })
    }

    async fn create(&self, database: &str, data: DocumentData) -> Result<DocumentId> {
        self.check_document_limit(database).await?;

        let mut db_storage = self.get_database_storage(database).await;
        let id = data.id.0.clone();

        if db_storage.contains_key(&id) {
            return Err(anyhow::anyhow!("Document already exists: {}", id));
        }

        db_storage.insert(id.clone(), data.clone());
        self.update_database_storage(database, db_storage).await;

        Ok(DocumentId(id))
    }

    async fn read(&self, database: &str, id: &str) -> Result<Option<DocumentData>> {
        let db_storage = self.get_database_storage(database).await;
        Ok(db_storage.get(id).cloned())
    }

    async fn update(&self, database: &str, id: &str, data: DocumentData) -> Result<DocumentData> {
        let mut db_storage = self.get_database_storage(database).await;

        if !db_storage.contains_key(id) {
            return Err(anyhow::anyhow!("Document not found: {}", id));
        }

        db_storage.insert(id.to_string(), data.clone());
        self.update_database_storage(database, db_storage).await;

        Ok(data)
    }

    async fn delete(&self, database: &str, id: &str) -> Result<bool> {
        let mut db_storage = self.get_database_storage(database).await;
        let existed = db_storage.remove(id).is_some();
        self.update_database_storage(database, db_storage).await;
        Ok(existed)
    }

    async fn upsert(&self, database: &str, id: &str, data: DocumentData) -> Result<DocumentData> {
        let mut db_storage = self.get_database_storage(database).await;
        db_storage.insert(id.to_string(), data.clone());
        self.update_database_storage(database, db_storage).await;
        Ok(data)
    }

    async fn query(&self, database: &str, query: Query) -> Result<QueryResult> {
        let db_storage = self.get_database_storage(database).await;
        let mut documents = Vec::new();

        // Apply filters
        for (id, document) in db_storage.iter() {
            let mut matches = true;

            // Apply query filter if present
            if let Some(filter) = &query.filter {
                matches = self.matches_filter(document, filter);
            }

            if matches {
                documents.push(document.clone());
            }
        }

        // Apply sorting
        if let Some(sort_orders) = &query.sort {
            self.apply_sorting(&mut documents, sort_orders);
        }

        // Apply limit and offset
        let total_count = documents.len() as u64;
        if let Some(offset) = query.offset {
            documents = documents.into_iter().skip(offset as usize).collect();
        }
        if let Some(limit) = query.limit {
            documents.truncate(limit as usize);
        }

        Ok(QueryResult {
            documents,
            total_count: Some(total_count),
            execution_time: std::time::Duration::from_millis(1),
            metadata: QueryMetadata {
                query_plan: None,
                index_used: None,
                documents_scanned: total_count,
                results_returned: documents.len() as u64,
            },
        })
    }

    async fn aggregate(&self, database: &str, pipeline: AggregationPipeline) -> Result<AggregationResult> {
        let db_storage = self.get_database_storage(database).await;
        let mut results = Vec::new();

        // Simple aggregation implementation
        for stage in pipeline.stages {
            match stage.stage_type.as_str() {
                "count" => {
                    results.push(serde_json::json!({"count": db_storage.len()}));
                }
                "sum" => {
                    if let Some(field) = stage.specification.get("field") {
                        if let Some(field_str) = field.as_str() {
                            let mut sum = 0.0;
                            for document in db_storage.values() {
                                if let Some(value) = self.get_nested_value(&document.content, field_str) {
                                    if let Some(num) = value.as_f64() {
                                        sum += num;
                                    }
                                }
                            }
                            results.push(serde_json::json!({"sum": sum, "field": field_str}));
                        }
                    }
                }
                "avg" => {
                    if let Some(field) = stage.specification.get("field") {
                        if let Some(field_str) = field.as_str() {
                            let mut sum = 0.0;
                            let mut count = 0;
                            for document in db_storage.values() {
                                if let Some(value) = self.get_nested_value(&document.content, field_str) {
                                    if let Some(num) = value.as_f64() {
                                        sum += num;
                                        count += 1;
                                    }
                                }
                            }
                            let avg = if count > 0 { sum / count as f64 } else { 0.0 };
                            results.push(serde_json::json!({"avg": avg, "field": field_str, "count": count}));
                        }
                    }
                }
                _ => {
                    // Unknown stage type, skip
                }
            }
        }

        Ok(AggregationResult {
            results,
            execution_time: std::time::Duration::from_millis(1),
            metadata: AggregationMetadata {
                stages_processed: pipeline.stages.len() as u32,
                documents_per_stage: vec![db_storage.len() as u64; pipeline.stages.len()],
                memory_usage: 1024, // Rough estimate
            },
        })
    }

    async fn search(&self, database: &str, search_query: SearchQuery) -> Result<SearchResult> {
        let db_storage = self.get_database_storage(database).await;
        let mut matching_documents = Vec::new();

        let query_lower = search_query.query.to_lowercase();

        for document in db_storage.values() {
            let content_str = serde_json::to_string(&document.content).unwrap_or_default();
            let content_lower = content_str.to_lowercase();

            // Simple text matching
            let matches = if search_query.search_type == SearchType::FullText {
                content_lower.contains(&query_lower)
            } else if search_query.search_type == SearchType::Phrase {
                content_lower.contains(&query_lower)
            } else {
                // Default to simple substring search
                content_lower.contains(&query_lower)
            };

            if matches {
                let score = self.calculate_search_score(&content_lower, &query_lower);
                matching_documents.push(SearchResultDocument {
                    document: document.clone(),
                    score,
                    highlights: self.generate_highlights(&content_str, &search_query.query),
                });
            }
        }

        // Sort by score (highest first)
        matching_documents.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Apply limit if specified
        if let Some(limit) = search_query.fields.as_ref().and_then(|_| Some(10)) {
            matching_documents.truncate(limit as usize);
        }

        Ok(SearchResult {
            documents: matching_documents,
            total_matches: matching_documents.len() as u64,
            execution_time: std::time::Duration::from_millis(1),
            metadata: SearchMetadata {
                query: search_query.query,
                search_type: search_query.search_type,
                fields_searched: search_query.fields.unwrap_or_default(),
                documents_scanned: db_storage.len() as u64,
            },
        })
    }

    async fn vector_search(&self, database: &str, vector: Vec<f32>, options: VectorSearchOptions) -> Result<VectorSearchResult> {
        let db_storage = self.get_database_storage(database).await;
        let mut results = Vec::new();

        for document in db_storage.values() {
            // Try to extract vector from document
            if let Some(doc_vector) = self.extract_vector_from_document(&document) {
                if doc_vector.len() == vector.len() {
                    let distance = self.calculate_distance(&vector, &doc_vector, &options.distance_metric);
                    results.push(VectorSearchDocument {
                        document: document.clone(),
                        distance,
                        vector: if options.include_vectors { Some(doc_vector) } else { None },
                    });
                }
            }
        }

        // Sort by distance (lowest first for distance metrics, highest for similarity)
        results.sort_by(|a, b| {
            match options.distance_metric {
                DistanceMetric::Cosine => a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal),
                DistanceMetric::Euclidean | DistanceMetric::Manhattan | DistanceMetric::DotProduct => {
                    a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal)
                }
            }
        });

        // Apply top_k limit
        results.truncate(options.top_k as usize);

        Ok(VectorSearchResult {
            results,
            execution_time: std::time::Duration::from_millis(1),
            metadata: VectorSearchMetadata {
                vector_dimension: vector.len() as u32,
                distance_metric: options.distance_metric,
                documents_scanned: db_storage.len() as u64,
                index_used: options.index_name,
            },
        })
    }

    async fn begin_transaction(&self) -> Result<TransactionId> {
        let transaction_id = Uuid::new_v4().to_string();
        let storage = self.storage.read().await;
        let snapshot = storage.clone();

        let transaction_state = TransactionState {
            id: transaction_id.clone(),
            snapshot,
            operations: Vec::new(),
        };

        let mut transactions = self.transactions.write().await;
        transactions.insert(transaction_id.clone(), transaction_state);

        Ok(TransactionId(transaction_id))
    }

    async fn commit_transaction(&self, transaction_id: &str) -> Result<()> {
        let mut transactions = self.transactions.write().await;
        transactions.remove(transaction_id);
        Ok(())
    }

    async fn rollback_transaction(&self, transaction_id: &str) -> Result<()> {
        let mut transactions = self.transactions.write().await;
        if let Some(transaction_state) = transactions.remove(transaction_id) {
            // Restore snapshot
            *self.storage.write().await = transaction_state.snapshot;
        }
        Ok(())
    }

    async fn bulk_insert(&self, database: &str, documents: Vec<DocumentData>) -> Result<BulkInsertResult> {
        let mut inserted_count = 0;
        let mut inserted_ids = Vec::new();
        let mut errors = Vec::new();

        for (index, document) in documents.into_iter().enumerate() {
            match self.create(database, document).await {
                Ok(id) => {
                    inserted_count += 1;
                    inserted_ids.push(id);
                }
                Err(e) => {
                    errors.push(BulkOperationError {
                        index: index as u32,
                        document_id: DocumentId("unknown".to_string()),
                        error: e.to_string(),
                        error_code: "CREATE_ERROR".to_string(),
                    });
                }
            }
        }

        Ok(BulkInsertResult {
            inserted_count,
            failed_count: errors.len() as u32,
            inserted_ids,
            errors,
            execution_time: std::time::Duration::from_millis(1),
        })
    }

    async fn bulk_update(&self, database: &str, updates: Vec<UpdateOperation>) -> Result<BulkUpdateResult> {
        let mut updated_count = 0;
        let mut updated_ids = Vec::new();
        let mut errors = Vec::new();

        for (index, update) in updates.into_iter().enumerate() {
            // Read current document
            if let Ok(Some(mut current_doc)) = self.read(database, &update.id.0).await {
                // Apply updates to the document
                for doc_update in update.updates {
                    self.apply_document_update(&mut current_doc, &doc_update);
                }

                match self.update(database, &update.id.0, current_doc).await {
                    Ok(_) => {
                        updated_count += 1;
                        updated_ids.push(update.id);
                    }
                    Err(e) => {
                        errors.push(BulkOperationError {
                            index: index as u32,
                            document_id: update.id.clone(),
                            error: e.to_string(),
                            error_code: "UPDATE_ERROR".to_string(),
                        });
                    }
                }
            } else {
                errors.push(BulkOperationError {
                    index: index as u32,
                    document_id: update.id.clone(),
                    error: "Document not found".to_string(),
                    error_code: "NOT_FOUND".to_string(),
                });
            }
        }

        Ok(BulkUpdateResult {
            updated_count,
            failed_count: errors.len() as u32,
            updated_ids,
            errors,
            execution_time: std::time::Duration::from_millis(1),
        })
    }

    async fn bulk_delete(&self, database: &str, ids: Vec<DocumentId>) -> Result<BulkDeleteResult> {
        let mut deleted_count = 0;
        let mut deleted_ids = Vec::new();
        let mut errors = Vec::new();

        for id in ids {
            match self.delete(database, &id.0).await {
                Ok(true) => {
                    deleted_count += 1;
                    deleted_ids.push(id);
                }
                Ok(false) => {
                    // Document didn't exist, not an error
                }
                Err(e) => {
                    errors.push(BulkOperationError {
                        index: deleted_ids.len() as u32,
                        document_id: id.clone(),
                        error: e.to_string(),
                        error_code: "DELETE_ERROR".to_string(),
                    });
                }
            }
        }

        Ok(BulkDeleteResult {
            deleted_count,
            failed_count: errors.len() as u32,
            deleted_ids,
            errors,
            execution_time: std::time::Duration::from_millis(1),
        })
    }

    // Index management
    async fn create_index(&self, database: &str, index: IndexDefinition) -> Result<IndexInfo> {
        let mut indexes = self.indexes.write().await;
        let db_indexes = indexes.entry(database.to_string()).or_insert_with(HashMap::new);

        let index_info = IndexInfo {
            name: index.name.clone(),
            fields: index.fields.clone(),
            index_type: index.index_type,
            size_bytes: 1024, // Rough estimate
            document_count: 0,
            unique: index.unique,
            sparse: index.sparse,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        db_indexes.insert(index.name.clone(), index_info.clone());
        Ok(index_info)
    }

    async fn drop_index(&self, database: &str, index_name: &str) -> Result<()> {
        let mut indexes = self.indexes.write().await;
        if let Some(db_indexes) = indexes.get_mut(database) {
            db_indexes.remove(index_name);
        }
        Ok(())
    }

    async fn list_indexes(&self, database: &str) -> Result<Vec<IndexInfo>> {
        let indexes = self.indexes.read().await;
        if let Some(db_indexes) = indexes.get(database) {
            Ok(db_indexes.values().cloned().collect())
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_index_stats(&self, database: &str, index_name: &str) -> Result<IndexStats> {
        let indexes = self.indexes.read().await;
        if let Some(db_indexes) = indexes.get(database) {
            if let Some(index_info) = db_indexes.get(index_name) {
                Ok(IndexStats {
                    name: index_name.to_string(),
                    size_bytes: index_info.size_bytes,
                    entry_count: index_info.document_count,
                    usage_stats: IndexUsageStats {
                        usage_count: 0,
                        average_query_time: std::time::Duration::from_millis(1),
                        selectivity: 0.5,
                    },
                    last_accessed: Some(chrono::Utc::now()),
                })
            } else {
                Err(anyhow::anyhow!("Index not found: {}", index_name))
            }
        } else {
            Err(anyhow::anyhow!("Database not found: {}", database))
        }
    }

    // Database management
    async fn create_database(&self, name: &str, _schema: Option<DatabaseSchema>) -> Result<DatabaseInfo> {
        let mut storage = self.storage.write().await;
        storage.insert(name.to_string(), HashMap::new());

        Ok(DatabaseInfo {
            name: name.to_string(),
            document_count: 0,
            size_bytes: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            status: crate::service_types::DatabaseStatus::Active,
        })
    }

    async fn drop_database(&self, name: &str) -> Result<()> {
        let mut storage = self.storage.write().await;
        storage.remove(name);
        Ok(())
    }

    async fn list_databases(&self) -> Result<Vec<DatabaseInfo>> {
        let storage = self.storage.read().await;
        let mut databases = Vec::new();

        for (name, documents) in storage.iter() {
            databases.push(DatabaseInfo {
                name: name.clone(),
                document_count: documents.len() as u64,
                size_bytes: (documents.len() * 1024) as u64, // Rough estimate
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                status: crate::service_types::DatabaseStatus::Active,
            });
        }

        Ok(databases)
    }

    async fn get_database(&self, name: &str) -> Result<Option<DatabaseInfo>> {
        let storage = self.storage.read().await;
        if let Some(documents) = storage.get(name) {
            Ok(Some(DatabaseInfo {
                name: name.to_string(),
                document_count: documents.len() as u64,
                size_bytes: (documents.len() * 1024) as u64,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                status: crate::service_types::DatabaseStatus::Active,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_connection_status(&self) -> Result<ConnectionStatus> {
        Ok(ConnectionStatus {
            status: crate::service_types::ConnectionStatusType::Connected,
            last_connected: chrono::Utc::now(),
            connection_count: 1,
            active_connections: 1,
        })
    }

    // Schema management
    async fn create_schema(&self, database: &str, schema: DatabaseSchema) -> Result<SchemaInfo> {
        let mut schemas = self.schemas.write().await;
        schemas.insert(database.to_string(), schema.clone());

        Ok(SchemaInfo {
            name: schema.name,
            version: schema.version,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            status: SchemaStatus::Active,
            document_count: 0,
        })
    }

    async fn update_schema(&self, database: &str, schema: DatabaseSchema) -> Result<SchemaInfo> {
        let mut schemas = self.schemas.write().await;
        schemas.insert(database.to_string(), schema.clone());

        Ok(SchemaInfo {
            name: schema.name,
            version: schema.version,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            status: SchemaStatus::Active,
            document_count: 0,
        })
    }

    async fn get_schema(&self, database: &str) -> Result<Option<DatabaseSchema>> {
        let schemas = self.schemas.read().await;
        Ok(schemas.get(database).cloned())
    }

    async fn validate_document(&self, _database: &str, _document: &DocumentData) -> Result<ValidationResult> {
        // Basic validation - in a real implementation, this would check against the schema
        Ok(ValidationResult {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            metadata: HashMap::new(),
        })
    }

    // Backup and restore (placeholder implementations)
    async fn create_backup(&self, database: &str, _backup_config: BackupConfig) -> Result<BackupInfo> {
        let storage = self.storage.read().await;
        let document_count = storage.get(database).map(|docs| docs.len()).unwrap_or(0);

        Ok(BackupInfo {
            backup_id: Uuid::new_v4().to_string(),
            name: format!("backup_{}", chrono::Utc::now().timestamp()),
            size_bytes: (document_count * 1024) as u64,
            document_count: document_count as u64,
            created_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            status: BackupStatus::Completed,
            collections: vec![database.to_string()],
            metadata: HashMap::new(),
        })
    }

    async fn restore_backup(&self, _backup_id: &str, _restore_config: RestoreConfig) -> Result<RestoreResult> {
        Ok(RestoreResult {
            backup_id: _backup_id.to_string(),
            restored_documents: 0,
            restored_collections: 0,
            duration: std::time::Duration::from_millis(100),
            status: RestoreStatus::Completed,
            errors: Vec::new(),
        })
    }

    async fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        Ok(Vec::new()) // No persistent backups in memory
    }

    async fn delete_backup(&self, _backup_id: &str) -> Result<()> {
        Ok(()) // Nothing to delete in memory
    }

    // Replication and sync (placeholder implementations)
    async fn configure_replication(&self, _config: ReplicationConfig) -> Result<()> {
        Ok(()) // Replication not supported in memory backend
    }

    async fn get_replication_status(&self) -> Result<ReplicationStatus> {
        Ok(ReplicationStatus {
            mode: ReplicationMode::PrimarySecondary,
            nodes: vec![],
            current_primary: Some("memory".to_string()),
            replication_lag: std::time::Duration::from_secs(0),
            last_sync: chrono::Utc::now(),
            status: ReplicationOverallStatus::Healthy,
        })
    }

    async fn sync_database(&self, _database: &str, _sync_config: SyncConfig) -> Result<SyncResult> {
        Ok(SyncResult {
            sync_id: Uuid::new_v4().to_string(),
            uploaded_count: 0,
            downloaded_count: 0,
            conflict_count: 0,
            error_count: 0,
            duration: std::time::Duration::from_millis(100),
            status: SyncStatus::Completed,
            last_sync: chrono::Utc::now(),
        })
    }
}

impl MemoryBackend {
    /// Check if document matches filter
    fn matches_filter(&self, document: &DocumentData, filter: &QueryFilter) -> bool {
        match filter {
            QueryFilter::Field { field, operator, value } => {
                if let Some(doc_value) = self.get_nested_value(&document.content, field) {
                    self.matches_operator(doc_value, operator, value)
                } else {
                    false
                }
            }
            QueryFilter::Compound { operator, filters } => {
                match operator {
                    LogicalOperator::And => filters.iter().all(|f| self.matches_filter(document, f)),
                    LogicalOperator::Or => filters.iter().any(|f| self.matches_filter(document, f)),
                    LogicalOperator::Not => !filters.iter().any(|f| self.matches_filter(document, f)),
                }
            }
            QueryFilter::Raw(_) => true, // Raw JSON filter not implemented
        }
    }

    /// Get nested value from JSON
    fn get_nested_value(&self, json: &serde_json::Value, path: &str) -> Option<&serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for part in parts {
            match current {
                serde_json::Value::Object(map) => {
                    current = map.get(part)?;
                }
                serde_json::Value::Array(arr) => {
                    if let Ok(index) = part.parse::<usize>() {
                        current = arr.get(index)?;
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Check if value matches operator
    fn matches_operator(&self, doc_value: &serde_json::Value, operator: &FilterOperator, filter_value: &serde_json::Value) -> bool {
        match operator {
            FilterOperator::Eq => doc_value == filter_value,
            FilterOperator::Ne => doc_value != filter_value,
            FilterOperator::Gt => {
                if let (Some(doc_num), Some(filter_num)) = (doc_value.as_f64(), filter_value.as_f64()) {
                    doc_num > filter_num
                } else {
                    false
                }
            }
            FilterOperator::Gte => {
                if let (Some(doc_num), Some(filter_num)) = (doc_value.as_f64(), filter_value.as_f64()) {
                    doc_num >= filter_num
                } else {
                    false
                }
            }
            FilterOperator::Lt => {
                if let (Some(doc_num), Some(filter_num)) = (doc_value.as_f64(), filter_value.as_f64()) {
                    doc_num < filter_num
                } else {
                    false
                }
            }
            FilterOperator::Lte => {
                if let (Some(doc_num), Some(filter_num)) = (doc_value.as_f64(), filter_value.as_f64()) {
                    doc_num <= filter_num
                } else {
                    false
                }
            }
            FilterOperator::In => {
                if let Some(filter_array) = filter_value.as_array() {
                    filter_array.contains(&doc_value)
                } else {
                    false
                }
            }
            FilterOperator::Nin => {
                if let Some(filter_array) = filter_value.as_array() {
                    !filter_array.contains(&doc_value)
                } else {
                    false
                }
            }
            FilterOperator::Regex => {
                if let (Some(doc_str), Some(filter_str)) = (doc_value.as_str(), filter_value.as_str()) {
                    // Simple regex matching (in a real implementation, use regex crate)
                    doc_str.contains(filter_str)
                } else {
                    false
                }
            }
            FilterOperator::Exists => {
                let exists = doc_value.is_null();
                if let Some(filter_bool) = filter_value.as_bool() {
                    exists == filter_bool
                } else {
                    false
                }
            }
            FilterOperator::Contains => {
                if let (Some(doc_array), _) = (doc_value.as_array(), filter_value) {
                    doc_array.contains(&filter_value)
                } else {
                    false
                }
            }
            FilterOperator::Size => {
                if let Some(doc_array) = doc_value.as_array() {
                    if let Some(filter_size) = filter_value.as_u64() {
                        doc_array.len() as u64 == filter_size
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }

    /// Apply sorting to documents
    fn apply_sorting(&self, documents: &mut Vec<DocumentData>, sort_orders: &[SortOrder]) {
        // Simple sorting implementation - sort by first field only
        if let Some(first_sort) = sort_orders.first() {
            documents.sort_by(|a, b| {
                let a_value = self.get_nested_value(&a.content, &first_sort.field);
                let b_value = self.get_nested_value(&b.content, &first_sort.field);

                let cmp = match (a_value, b_value) {
                    (Some(serde_json::Value::Number(a)), Some(serde_json::Value::Number(b))) => {
                        a.as_f64().partial_cmp(&b.as_f64()).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Some(serde_json::Value::String(a)), Some(serde_json::Value::String(b))) => {
                        a.cmp(b)
                    }
                    (Some(serde_json::Value::Bool(a)), Some(serde_json::Value::Bool(b))) => {
                        a.cmp(b)
                    }
                    _ => std::cmp::Ordering::Equal,
                };

                if first_sort.direction == SortDirection::Descending {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
    }

    /// Calculate search score
    fn calculate_search_score(&self, content: &str, query: &str) -> f32 {
        let content_words: Vec<&str> = content.split_whitespace().collect();
        let query_words: Vec<&str> = query.split_whitespace().collect();

        if query_words.is_empty() {
            return 0.0;
        }

        let mut matches = 0;
        for query_word in &query_words {
            for content_word in &content_words {
                if content_word.contains(query_word) {
                    matches += 1;
                    break;
                }
            }
        }

        matches as f32 / query_words.len() as f32
    }

    /// Generate highlight snippets
    fn generate_highlights(&self, content: &str, query: &str) -> Vec<String> {
        let mut highlights = Vec::new();
        let query_lower = query.to_lowercase();
        let content_lower = content.to_lowercase();

        if let Some(pos) = content_lower.find(&query_lower) {
            let start = if pos >= 50 { pos - 50 } else { 0 };
            let end = std::cmp::min(pos + query.len() + 50, content.len());
            let snippet = &content[start..end];
            highlights.push(format!("...{}...", snippet));
        }

        highlights
    }

    /// Extract vector from document
    fn extract_vector_from_document(&self, document: &DocumentData) -> Option<Vec<f32>> {
        document.content.get("embedding")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).map(|v| v as f32).collect())
    }

    /// Calculate distance between vectors
    fn calculate_distance(&self, vec1: &[f32], vec2: &[f32], metric: &DistanceMetric) -> f32 {
        if vec1.len() != vec2.len() {
            return f32::MAX;
        }

        match metric {
            DistanceMetric::Cosine => {
                let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
                let norm1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
                let norm2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

                if norm1 == 0.0 || norm2 == 0.0 {
                    f32::MAX
                } else {
                    1.0 - (dot_product / (norm1 * norm2))
                }
            }
            DistanceMetric::Euclidean => {
                let sum_squares: f32 = vec1.iter().zip(vec2.iter())
                    .map(|(a, b)| (a - b).powi(2))
                    .sum();
                sum_squares.sqrt()
            }
            DistanceMetric::Manhattan => {
                let sum_abs: f32 = vec1.iter().zip(vec2.iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum();
                sum_abs
            }
            DistanceMetric::DotProduct => {
                let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
                -dot_product // Negative for distance sorting
            }
        }
    }

    /// Apply document update
    fn apply_document_update(&self, document: &mut DocumentData, update: &DocumentUpdate) {
        match update.operation {
            UpdateOperationType::Set => {
                if let Some(value) = &update.value {
                    self.set_nested_value(&mut document.content, &update.field, value.clone());
                }
            }
            UpdateOperationType::Unset => {
                self.unset_nested_value(&mut document.content, &update.field);
            }
            UpdateOperationType::Increment => {
                if let Some(current) = self.get_nested_value(&document.content, &update.field) {
                    if let Some(current_num) = current.as_f64() {
                        let increment = update.value.as_ref().and_then(|v| v.as_f64()).unwrap_or(1.0);
                        let new_value = serde_json::Value::Number(serde_json::Number::from_f64(current_num + increment).unwrap());
                        self.set_nested_value(&mut document.content, &update.field, new_value);
                    }
                }
            }
            UpdateOperationType::Push => {
                if let Some(value) = &update.value {
                    if let Some(array) = self.get_or_create_nested_array(&mut document.content, &update.field) {
                        array.push(value.clone());
                    }
                }
            }
            UpdateOperationType::Pull => {
                if let Some(value) = &update.value {
                    if let Some(array) = self.get_nested_value_mut(&mut document.content, &update.field) {
                        if let Some(arr) = array.as_array_mut() {
                            arr.retain(|v| v != value);
                        }
                    }
                }
            }
            UpdateOperationType::AddToSet => {
                if let Some(value) = &update.value {
                    if let Some(array) = self.get_or_create_nested_array(&mut document.content, &update.field) {
                        if !array.contains(value) {
                            array.push(value.clone());
                        }
                    }
                }
            }
        }
    }

    /// Set nested value in JSON
    fn set_nested_value(&self, json: &mut serde_json::Value, path: &str, value: serde_json::Value) {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part - set the value
                match current {
                    serde_json::Value::Object(ref mut map) => {
                        map.insert(part.to_string(), value);
                    }
                    _ => {
                        // Can't set nested value on non-object
                    }
                }
            } else {
                // Navigate to the nested object
                match current {
                    serde_json::Value::Object(ref mut map) => {
                        if !map.contains_key(*part) {
                            map.insert(part.to_string(), serde_json::Value::Object(serde_json::Map::new()));
                        }
                        current = map.get_mut(*part).unwrap();
                    }
                    _ => {
                        // Can't navigate further on non-object
                        break;
                    }
                }
            }
        }
    }

    /// Unset nested value in JSON
    fn unset_nested_value(&self, json: &mut serde_json::Value, path: &str) {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part - remove the value
                match current {
                    serde_json::Value::Object(ref mut map) => {
                        map.remove(*part);
                    }
                    _ => {}
                }
            } else {
                // Navigate to the nested object
                match current {
                    serde_json::Value::Object(ref map) => {
                        if let Some(next) = map.get(*part) {
                            current = next;
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }
    }

    /// Get or create nested array
    fn get_or_create_nested_array(&self, json: &mut serde_json::Value, path: &str) -> Option<&mut Vec<serde_json::Value>> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part - get or create array
                match current {
                    serde_json::Value::Object(ref mut map) => {
                        if !map.contains_key(*part) {
                            map.insert(part.to_string(), serde_json::Value::Array(Vec::new()));
                        }
                        return map.get_mut(*part).and_then(|v| v.as_array_mut());
                    }
                    _ => return None,
                }
            } else {
                // Navigate to the nested object
                match current {
                    serde_json::Value::Object(ref mut map) => {
                        if !map.contains_key(*part) {
                            map.insert(part.to_string(), serde_json::Value::Object(serde_json::Map::new()));
                        }
                        current = map.get_mut(*part).unwrap();
                    }
                    _ => return None,
                }
            }
        }

        None
    }

    /// Get nested value as mutable
    fn get_nested_value_mut(&self, json: &mut serde_json::Value, path: &str) -> Option<&mut serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for part in parts {
            match current {
                serde_json::Value::Object(ref mut map) => {
                    current = map.get_mut(part)?;
                }
                _ => return None,
            }
        }

        // This is a limitation - we can't return a mutable reference to a nested value
        // without more complex borrowing. In a real implementation, this would be handled
        // differently.
        None
    }
}