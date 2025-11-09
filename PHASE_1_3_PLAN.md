# Phase 1.3: PropertyStorage Implementation - Detailed Plan

## Current Status
- ✅ Phase 0: EAV+Graph foundation (5 storage traits, EAVDocument)
- ✅ Phase 1.1: Enhanced frontmatter parsing (9 tests)
- ✅ Phase 1.2: FrontmatterPropertyMapper (8 tests)
- 🔄 Phase 1.3: **PropertyStorage trait in SurrealDB** (NEXT)

## Phase 1.3 Overview

Implement the `PropertyStorage` trait in the SurrealDB backend to enable storing frontmatter properties in the database.

## Breaking Down into Small Steps

### Step 1.3.1: Find or Create EAVGraphStore
**Goal**: Locate the SurrealDB store implementation or create it if missing

**Tasks**:
1. Search for existing `EAVGraphStore` in `crates/crucible-surrealdb/`
2. If exists: Read and understand current structure
3. If not: Create minimal `EAVGraphStore` struct with SurrealDB client
4. Verify compilation

**Estimated Time**: 10-15 minutes
**Risk**: Low
**Output**: Working EAVGraphStore struct

---

### Step 1.3.2: Design SurrealDB Schema for Properties
**Goal**: Define the database schema for storing properties

**Tasks**:
1. Read existing schema in `crates/crucible-surrealdb/src/schema.surql` or `src/eav/schema.surql`
2. Design `property` table schema:
   ```surql
   DEFINE TABLE property SCHEMAFULL;
   DEFINE FIELD entity_id ON property TYPE string;
   DEFINE FIELD namespace ON property TYPE string;
   DEFINE FIELD key ON property TYPE string;
   DEFINE FIELD value_text ON property TYPE option<string>;
   DEFINE FIELD value_number ON property TYPE option<float>;
   DEFINE FIELD value_bool ON property TYPE option<bool>;
   DEFINE FIELD value_date ON property TYPE option<datetime>;
   DEFINE FIELD value_json ON property TYPE option<object>;
   DEFINE FIELD created_at ON property TYPE datetime;
   DEFINE FIELD updated_at ON property TYPE datetime;

   DEFINE INDEX idx_property_entity ON property FIELDS entity_id;
   DEFINE INDEX idx_property_namespace ON property FIELDS entity_id, namespace;
   ```
3. Write migration if needed
4. Document schema design decisions

**Estimated Time**: 15-20 minutes
**Risk**: Low (schema design only, no implementation)
**Output**: Schema definition file

---

### Step 1.3.3: Write Failing Test for batch_upsert_properties
**Goal**: RED phase - write test for batch property insertion

**Tasks**:
1. Create test file `crates/crucible-surrealdb/tests/property_storage_tests.rs`
2. Write minimal test:
   ```rust
   #[tokio::test]
   async fn test_batch_upsert_properties_basic() {
       // Setup test DB
       let store = create_test_store().await;
       let entity_id = "note:test123";

       // Create properties
       let properties = vec![
           Property {
               entity_id: entity_id.to_string(),
               namespace: PropertyNamespace::frontmatter(),
               key: "title".to_string(),
               value: PropertyValue::Text("Test Note".to_string()),
               created_at: Utc::now(),
               updated_at: Utc::now(),
           },
       ];

       // Insert should succeed
       let count = store.batch_upsert_properties(properties).await.unwrap();
       assert_eq!(count, 1);
   }
   ```
3. Run test - should fail (method not implemented)

**Estimated Time**: 15 minutes
**Risk**: Low
**Output**: Failing test (RED phase complete)

---

### Step 1.3.4: Implement Minimal batch_upsert_properties
**Goal**: GREEN phase - make test pass with simplest implementation

**Tasks**:
1. Implement `PropertyStorage` trait for `EAVGraphStore`:
   ```rust
   #[async_trait]
   impl PropertyStorage for EAVGraphStore {
       async fn batch_upsert_properties(
           &self,
           properties: Vec<Property>,
       ) -> StorageResult<usize> {
           // Simple loop implementation first (can optimize later)
           let mut count = 0;
           for prop in properties {
               // Single INSERT query per property
               let _: Vec<Record> = self.db.create("property")
                   .content(/* map Property to SurrealDB record */)
                   .await?;
               count += 1;
           }
           Ok(count)
       }

       // Stub other methods
       async fn get_properties(&self, entity_id: &str) -> StorageResult<Vec<Property>> {
           todo!("Implement in next step")
       }
       // ... other stubs
   }
   ```
2. Run test - should pass
3. Don't optimize yet (YAGNI principle)

**Estimated Time**: 20-25 minutes
**Risk**: Medium (SurrealDB API quirks)
**Output**: Passing test (GREEN phase complete)

---

### Step 1.3.5: Write Test for get_properties
**Goal**: RED phase - test property retrieval

**Tasks**:
1. Write test:
   ```rust
   #[tokio::test]
   async fn test_get_properties_retrieves_all() {
       let store = create_test_store().await;
       let entity_id = "note:test123";

       // Insert test data
       let properties = vec![
           Property { /* title */ },
           Property { /* author */ },
           Property { /* count */ },
       ];
       store.batch_upsert_properties(properties).await.unwrap();

       // Retrieve all properties for entity
       let retrieved = store.get_properties(entity_id).await.unwrap();

       assert_eq!(retrieved.len(), 3);
       assert!(retrieved.iter().any(|p| p.key == "title"));
       assert!(retrieved.iter().any(|p| p.key == "author"));
       assert!(retrieved.iter().any(|p| p.key == "count"));
   }
   ```
2. Run test - should fail

**Estimated Time**: 10 minutes
**Risk**: Low
**Output**: Failing test

---

### Step 1.3.6: Implement get_properties
**Goal**: GREEN phase - retrieve properties

**Tasks**:
1. Implement method:
   ```rust
   async fn get_properties(&self, entity_id: &str) -> StorageResult<Vec<Property>> {
       let results: Vec<PropertyRecord> = self.db
           .query("SELECT * FROM property WHERE entity_id = $entity_id")
           .bind(("entity_id", entity_id))
           .await?;

       // Map SurrealDB records to Property objects
       let properties = results.into_iter()
           .map(|record| self.record_to_property(record))
           .collect();

       Ok(properties)
   }
   ```
2. Run test - should pass

**Estimated Time**: 15-20 minutes
**Risk**: Medium (type conversions)
**Output**: Passing test

---

### Step 1.3.7: Write Test for get_properties_by_namespace
**Goal**: RED phase - test namespace filtering

**Tasks**:
1. Write test with mixed namespaces:
   ```rust
   #[tokio::test]
   async fn test_get_properties_by_namespace_filters() {
       let store = create_test_store().await;
       let entity_id = "note:test123";

       // Insert properties in different namespaces
       let properties = vec![
           Property { namespace: PropertyNamespace::frontmatter(), key: "title", /* ... */ },
           Property { namespace: PropertyNamespace::frontmatter(), key: "author", /* ... */ },
           Property { namespace: PropertyNamespace::core(), key: "hash", /* ... */ },
       ];
       store.batch_upsert_properties(properties).await.unwrap();

       // Get only frontmatter properties
       let frontmatter = store
           .get_properties_by_namespace(entity_id, &PropertyNamespace::frontmatter())
           .await
           .unwrap();

       assert_eq!(frontmatter.len(), 2);
       assert!(frontmatter.iter().all(|p| p.namespace == PropertyNamespace::frontmatter()));
   }
   ```
2. Run test - should fail

**Estimated Time**: 10 minutes
**Risk**: Low
**Output**: Failing test

---

### Step 1.3.8: Implement get_properties_by_namespace
**Goal**: GREEN phase - namespace filtering

**Tasks**:
1. Implement method with WHERE clause:
   ```rust
   async fn get_properties_by_namespace(
       &self,
       entity_id: &str,
       namespace: &PropertyNamespace,
   ) -> StorageResult<Vec<Property>> {
       let results: Vec<PropertyRecord> = self.db
           .query("SELECT * FROM property WHERE entity_id = $entity_id AND namespace = $namespace")
           .bind(("entity_id", entity_id))
           .bind(("namespace", &namespace.0))
           .await?;

       Ok(results.into_iter().map(|r| self.record_to_property(r)).collect())
   }
   ```
2. Run test - should pass

**Estimated Time**: 10 minutes
**Risk**: Low
**Output**: Passing test

---

### Step 1.3.9: Implement Remaining CRUD Methods
**Goal**: Complete PropertyStorage trait implementation

**Tasks**:
1. Implement `get_property` (single property by namespace + key)
2. Implement `delete_properties` (delete all for entity)
3. Implement `delete_properties_by_namespace`
4. Write tests for each (RED-GREEN cycle)

**Estimated Time**: 30-40 minutes
**Risk**: Low (following same pattern)
**Output**: Full trait implementation

---

### Step 1.3.10: Optimize batch_upsert_properties
**Goal**: REFACTOR phase - use true batch operations

**Tasks**:
1. Write performance test:
   ```rust
   #[tokio::test]
   async fn test_batch_upsert_performance() {
       let store = create_test_store().await;

       // Create 100 properties
       let properties: Vec<Property> = (0..100)
           .map(|i| Property { key: format!("key{}", i), /* ... */ })
           .collect();

       let start = Instant::now();
       store.batch_upsert_properties(properties).await.unwrap();
       let duration = start.elapsed();

       // Should complete in <100ms per original target
       assert!(duration.as_millis() < 100);
   }
   ```
2. Refactor to use SurrealDB batch operations:
   ```rust
   async fn batch_upsert_properties(/* ... */) -> StorageResult<usize> {
       // Use single query with multiple INSERTs
       let mut query = String::from("BEGIN TRANSACTION; ");
       for prop in &properties {
           query.push_str(&format!("INSERT INTO property {...}; "));
       }
       query.push_str("COMMIT TRANSACTION;");

       self.db.query(query).await?;
       Ok(properties.len())
   }
   ```
3. Run performance test - should pass <100ms target

**Estimated Time**: 20-30 minutes
**Risk**: Medium (batch operation complexity)
**Output**: Optimized batch operations

---

### Step 1.3.11: Integration Test - End-to-End
**Goal**: QA Checkpoint 1 - Verify full pipeline

**Tasks**:
1. Write end-to-end test:
   ```rust
   #[tokio::test]
   async fn test_frontmatter_to_storage_pipeline() {
       // 1. Parse frontmatter
       let yaml = r#"
   title: My Note
   author: John Doe
   count: 42
   published: true
   created: 2024-11-08
   tags: ["rust", "testing"]
   "#;
       let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

       // 2. Map to properties
       let mapper = FrontmatterPropertyMapper::new("note:test123");
       let properties = mapper.map_to_properties(fm.properties().clone());

       // 3. Store in database
       let store = create_test_store().await;
       store.batch_upsert_properties(properties).await.unwrap();

       // 4. Retrieve and verify
       let retrieved = store
           .get_properties_by_namespace("note:test123", &PropertyNamespace::frontmatter())
           .await
           .unwrap();

       assert_eq!(retrieved.len(), 6);
       assert!(retrieved.iter().any(|p| p.key == "title" && p.value == PropertyValue::Text("My Note".to_string())));
       assert!(retrieved.iter().any(|p| p.key == "count" && p.value == PropertyValue::Number(42.0)));
   }
   ```
2. Run test - should pass
3. Celebrate! 🎉

**Estimated Time**: 15-20 minutes
**Risk**: Low (all components tested individually)
**Output**: Working end-to-end pipeline

---

## Summary

**Total Steps**: 11
**Estimated Total Time**: 3-4 hours
**Total Tests**: ~15-20 tests

**Deliverables**:
- ✅ PropertyStorage trait implementation in SurrealDB
- ✅ SurrealDB schema for properties
- ✅ Comprehensive test suite
- ✅ Performance optimization (<100ms for 100 properties)
- ✅ End-to-end frontmatter → database pipeline

**Risk Assessment**:
- Low Risk: 7 steps (schema, tests, simple implementations)
- Medium Risk: 4 steps (SurrealDB API, batch operations, type conversions)
- High Risk: 0 steps

**Dependencies**:
- SurrealDB client already integrated
- Property types defined in Phase 0
- FrontmatterPropertyMapper from Phase 1.2

## Recommended Approach

1. Start with **Steps 1.3.1-1.3.2** (setup, schema design)
2. Do **Steps 1.3.3-1.3.4** together (first RED-GREEN cycle)
3. Continue with **Steps 1.3.5-1.3.8** (retrieval methods)
4. Complete **Step 1.3.9** (remaining CRUD)
5. Optimize with **Step 1.3.10**
6. Validate with **Step 1.3.11** (QA checkpoint)

Each step is small enough to commit separately, making rollback easy if needed.
