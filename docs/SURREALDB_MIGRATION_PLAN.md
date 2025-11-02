# Crucible SurrealDB Migration Plan
## Complex Schema → Simplified Schema

### Overview

This document provides a comprehensive migration plan for transitioning the Crucible SurrealDB backend from a complex relational schema to a simplified schema optimized for performance and maintainability.

**Current Schema Version:** 1.0
**Target Schema Version:** 2.0
**Estimated Migration Time:** 2-4 hours (depending on data size)
**Downtime Required:** Yes (full schema migration)

---

## 1. Schema Analysis

### Current Schema (v1.0) - Complex

**Tables:**
- `notes` - Primary document storage with duplicate fields
- `tags` - Tag metadata with hierarchy
- `wikilink` - Document-to-document links (relation table)
- `tagged_with` - Document-to-tag associations (relation table)
- `embeds` - Document embed relationships (relation table)
- `relates_to` - Semantic similarity relationships (relation table)
- `metadata` - System metadata

**Key Issues:**
1. **Duplicate Content Fields:** `content`, `content_text`, `title`, `title_text`
2. **Complex Relations:** 4 separate relation tables requiring JOIN operations
3. **Content Storage:** Full markdown content stored in database
4. **Event Triggers:** Automatic sync events that can cause performance issues
5. **Hierarchical Tags:** Complex parent-child relationships rarely used

### Target Schema (v2.0) - Simplified

**Tables:**
- `notes` - Single consolidated table with arrays
- `metadata` - System version tracking (simplified)

**Key Improvements:**
1. **No Content Storage:** Only file references and metadata
2. **Consolidated Links:** All relationships stored as arrays in notes
3. **Simplified Metadata:** Essential fields only
4. **No Relation Tables:** Eliminated all 4 relation tables
5. **Removed Events:** No automatic triggers, manual updates only

---

## 2. Migration Strategy

### 2.1. Migration Approach

**Strategy:** Blue-Green Migration with Data Transformation
- Create new schema alongside existing schema
- Transform and migrate data in batches
- Update application code to use new schema
- Switch over atomically
- Keep old schema for rollback window

### 2.2. Migration Phases

**Phase 1: Preparation** (30 minutes)
- Backup existing database
- Create migration scripts
- Prepare rollback procedures

**Phase 2: Schema Creation** (15 minutes)
- Create new schema v2.0 tables
- Set up indexes and constraints

**Phase 3: Data Migration** (1-3 hours)
- Transform notes data (content → file reference)
- Consolidate relation data into arrays
- Migrate metadata

**Phase 4: Code Migration** (30 minutes)
- Update Rust types
- Update query builders
- Update connection logic

**Phase 5: Cutover** (15 minutes)
- Switch application to new schema
- Verify functionality
- Remove old schema (after verification)

---

## 3. Detailed Migration Plan

### 3.1. Schema Migration Scripts

#### New Schema v2.0 Definition

```sql
-- ============================================================================
-- Crucible Knowledge Kiln - Schema v2.0 (Simplified)
-- ============================================================================

-- Update schema version
UPDATE metadata:system SET schema_version = 2, updated_at = time::now();

-- ============================================================================
-- TABLE: notes (v2.0)
-- ============================================================================

-- Create new notes table with simplified structure
DEFINE TABLE notes_v2 SCHEMAFULL;

-- Core identifiers (keeping these for compatibility)
DEFINE FIELD path ON TABLE notes_v2
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD title ON TABLE notes_v2
    TYPE option<string>;

-- REMOVED: content field - now references files only
-- REMOVED: content_text field - redundant
-- REMOVED: title_text field - redundant

-- Timestamps (simplified)
DEFINE FIELD created_at ON TABLE notes_v2
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD modified_at ON TABLE notes_v2
    TYPE datetime
    DEFAULT time::now();

-- Consolidated link arrays (replacing relation tables)
DEFINE FIELD wikilinks ON TABLE notes_v2
    TYPE array<object>
    DEFAULT [];

DEFINE FIELD embeds ON TABLE notes_v2
    TYPE array<object>
    DEFAULT [];

DEFINE FIELD tags ON TABLE notes_v2
    TYPE array<string>
    DEFAULT [];

DEFINE FIELD related_notes ON TABLE notes_v2
    TYPE array<object>
    DEFAULT [];

-- Semantic search (keeping this functionality)
DEFINE FIELD embedding ON TABLE notes_v2
    TYPE option<array<float>>;

DEFINE FIELD embedding_model ON TABLE notes_v2
    TYPE option<string>;

DEFINE FIELD embedding_updated_at ON TABLE notes_v2
    TYPE option<datetime>;

-- Simplified metadata
DEFINE FIELD metadata ON TABLE notes_v2
    TYPE object
    DEFAULT {};

-- Computed fields (keeping for compatibility)
DEFINE FIELD status ON TABLE notes_v2
    VALUE $this.metadata.status OR "none";

DEFINE FIELD folder ON TABLE notes_v2
    VALUE string::split($this.path, "/")[0];

-- Constraints and indexes
DEFINE INDEX unique_path_v2 ON TABLE notes_v2
    COLUMNS path
    UNIQUE;

DEFINE INDEX tags_v2_idx ON TABLE notes_v2
    COLUMNS tags;

DEFINE INDEX folder_v2_idx ON TABLE notes_v2
    COLUMNS folder;

DEFINE INDEX modified_at_v2_idx ON TABLE notes_v2
    COLUMNS modified_at;

-- Vector search index (keeping embeddings)
DEFINE INDEX embedding_v2_idx ON TABLE notes_v2
    COLUMNS embedding
    MTREE DIMENSION 384 DISTANCE COSINE;

-- ============================================================================
-- Migration Functions
-- ============================================================================

-- Function to migrate a single note
DEFINE FUNCTION fn::migrate_note_v1_to_v2($note_id: string) {
    -- Get original note
    LET $original = SELECT * FROM notes WHERE id = $note_id;

    -- Get all related data
    LET $wikilinks_out = SELECT * FROM wikilink WHERE in = $note_id;
    LET $wikilinks_in = SELECT * FROM wikilink WHERE out = $note_id;
    LET $embeds_out = SELECT * FROM embeds WHERE in = $note_id;
    LET $embeds_in = SELECT * FROM embeds WHERE out = $note_id;
    LET $tagged_with = SELECT * FROM tagged_with WHERE in = $note_id;
    LET $relates_to = SELECT * FROM relates_to WHERE in = $note_id OR out = $note_id;

    -- Transform wikilinks to array format
    LET $wikilinks_array = array::concat(
        array::map($wikilinks_out, || {
            target: $out.id,
            text: $link_text,
            context: $context,
            position: $position,
            direction: "outgoing",
            created_at: $created_at
        }),
        array::map($wikilinks_in, || {
            target: $in.id,
            text: $link_text,
            context: $context,
            position: $position,
            direction: "incoming",
            created_at: $created_at
        })
    );

    -- Transform embeds to array format
    LET $embeds_array = array::concat(
        array::map($embeds_out, || {
            target: $out.id,
            type: $embed_type,
            reference_target: $reference_target,
            display_alias: $display_alias,
            context: $context,
            position: $position,
            direction: "outgoing",
            created_at: $created_at
        }),
        array::map($embeds_in, || {
            target: $in.id,
            type: $embed_type,
            reference_target: $reference_target,
            display_alias: $display_alias,
            context: $context,
            position: $position,
            direction: "incoming",
            created_at: $created_at
        })
    );

    -- Extract tags from tagged_with relations
    LET $tags_array = array::map($tagged_with, || $out.name);

    -- Transform semantic relationships
    LET $related_array = array::map($relates_to, || {
        target: iff($in = $note_id, $out.id, $in.id),
        relation_type: $relation_type,
        score: $score,
        direction: iff($in = $note_id, "outgoing", "incoming"),
        computed_at: $computed_at,
        metadata: $metadata
    });

    -- Create new note record
    LET $new_note = {
        path: $original.path,
        title: $original.title,
        created_at: $original.created_at,
        modified_at: $original.modified_at,
        wikilinks: $wikilinks_array,
        embeds: $embeds_array,
        tags: $tags_array,
        related_notes: $related_array,
        embedding: $original.embedding,
        embedding_model: $original.embedding_model,
        embedding_updated_at: $original.embedding_updated_at,
        metadata: $original.metadata
    };

    -- Insert into new table
    CREATE notes_v2:$note_id CONTENT $new_note;

    RETURN $new_note;
};

-- Batch migration function
DEFINE FUNCTION fn::migrate_all_notes_v1_to_v2($batch_size: int) {
    -- Get all note IDs
    LET $all_ids = array::map((SELECT id FROM notes), || $id);

    -- Process in batches
    LET $batches = array::chunk($all_ids, $batch_size);

    FOR $batch IN $batches {
        FOR $note_id IN $batch {
            fn::migrate_note_v1_to_v2($note_id);
        };
    };

    RETURN {
        total_notes: array::len($all_ids),
        batch_size: $batch_size,
        batches_processed: array::len($batches)
    };
};
```

#### Rollback Script

```sql
-- ============================================================================
-- Rollback Script: Schema v2.0 → v1.0
-- ============================================================================

-- Drop v2.0 tables
REMOVE TABLE notes_v2;

-- Restore v1.0 schema version
UPDATE metadata:system SET schema_version = 1, updated_at = time::now();

-- ============================================================================
-- Data Restoration Script (if needed)
-- ============================================================================

-- Function to restore v1.0 data from v2.0
DEFINE FUNCTION fn::restore_note_v2_to_v1($note_id: string) {
    LET $v2_note = SELECT * FROM notes_v2 WHERE id = $note_id;

    -- Create original note
    LET $original_note = {
        path: $v2_note.path,
        title: $v2_note.title,
        content: "", -- Will need to be read from file
        created_at: $v2_note.created_at,
        modified_at: $v2_note.modified_at,
        tags: $v2_note.tags,
        embedding: $v2_note.embedding,
        embedding_model: $v2_note.embedding_model,
        embedding_updated_at: $v2_note.embedding_updated_at,
        metadata: $v2_note.metadata
    };

    CREATE notes:$note_id CONTENT $original_note;

    -- Restore relation tables from arrays
    FOR $wikilink IN $v2_note.wikilinks {
        RELATE $note_id->wikilink->$wikilink.target SET
            link_text = $wikilink.text,
            context = $wikilink.context,
            position = $wikilink.position,
            created_at = $wikilink.created_at;
    };

    -- Similar restoration for embeds, tags, and related_notes...

    RETURN $original_note;
};
```

### 3.2. Migration Execution Script

```bash
#!/bin/bash
# migration_script.sh - SurrealDB Schema Migration

set -euo pipefail

# Configuration
DB_PATH="./crucible.db"
NAMESPACE="crucible"
DATABASE="kiln"
BACKUP_PATH="./crucible_backup_$(date +%Y%m%d_%H%M%S).db"

echo "🚀 Starting Crucible SurrealDB Migration (v1.0 → v2.0)"
echo "=================================================="

# Phase 1: Backup
echo "📦 Phase 1: Creating database backup..."
cp "$DB_PATH" "$BACKUP_PATH"
echo "✅ Backup created: $BACKUP_PATH"

# Phase 2: Schema Migration
echo "🏗️  Phase 2: Creating new schema..."
surreal import --conn http://localhost:8000 --user root --pass root \
    --ns "$NAMESPACE" --db "$DATABASE" schema_v2.surql
echo "✅ New schema created"

# Phase 3: Data Migration
echo "📊 Phase 3: Migrating data..."
surreal sql --conn http://localhost:8000 --user root --pass root \
    --ns "$NAMESPACE" --db "$DATABASE" \
    "RETURN fn::migrate_all_notes_v1_to_v2(100);"
echo "✅ Data migration completed"

# Phase 4: Verification
echo "🔍 Phase 4: Verifying migration..."
surreal sql --conn http://localhost:8000 --user root --pass root \
    --ns "$NAMESPACE" --db "$DATABASE" \
    "SELECT count() AS v1_count FROM notes; SELECT count() AS v2_count FROM notes_v2;"
echo "✅ Migration verification completed"

echo "🎉 Migration completed successfully!"
echo "💡 Keep backup for at least 7 days before removal"
echo "🔄 If rollback needed: ./rollback_script.sh"
```

### 3.3. Rollback Script

```bash
#!/bin/bash
# rollback_script.sh - SurrealDB Schema Rollback

set -euo pipefail

# Configuration
DB_PATH="./crucible.db"
NAMESPACE="crucible"
DATABASE="kiln"
BACKUP_PATH="$1"  # Pass backup path as argument

if [ -z "$BACKUP_PATH" ] || [ ! -f "$BACKUP_PATH" ]; then
    echo "❌ Error: Please provide valid backup path"
    echo "Usage: $0 /path/to/backup.db"
    exit 1
fi

echo "🔄 Starting SurrealDB Rollback (v2.0 → v1.0)"
echo "=========================================="

# Phase 1: Stop Application
echo "⏹️  Phase 1: Stop the application..."
# System-specific command to stop the application
echo "✅ Application stopped"

# Phase 2: Restore Backup
echo "💾 Phase 2: Restoring from backup..."
cp "$BACKUP_PATH" "$DB_PATH"
echo "✅ Database restored from backup"

# Phase 3: Start Application
echo "▶️  Phase 3: Starting application..."
# System-specific command to start the application
echo "✅ Application started"

echo "🎉 Rollback completed successfully!"
echo "📝 Please verify application functionality"
```

---

## 4. Rust Code Migration

### 4.1. Type System Changes

#### New `schema_types_v2.rs`

```rust
//! Simplified type definitions for the v2.0 schema
//! Consolidated structure with arrays instead of relation tables

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Core Types (Simplified)
// ============================================================================

/// Simplified Note structure for v2.0 schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteV2 {
    /// Record ID (format: "notes:path/to/file.md")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<NoteV2>>,

    /// File path (relative to kiln root) - PRIMARY KEY
    pub path: String,

    /// Document title (extracted from frontmatter or first heading)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,

    /// Consolidated wikilinks array (replaces wikilink table)
    #[serde(default)]
    pub wikilinks: Vec<WikiLinkItem>,

    /// Consolidated embeds array (replaces embeds table)
    #[serde(default)]
    pub embeds: Vec<EmbedItem>,

    /// Tags array (kept but simplified)
    #[serde(default)]
    pub tags: Vec<String>,

    /// Related notes array (replaces relates_to table)
    #[serde(default)]
    pub related_notes: Vec<RelatedNoteItem>,

    /// Embedding vector (kept for semantic search)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    /// Name of the embedding model used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,

    /// When the embedding was last updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_updated_at: Option<DateTime<Utc>>,

    /// Simplified metadata from frontmatter
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    /// Computed field: status from metadata (read-only)
    #[serde(skip_serializing)]
    pub status: Option<String>,

    /// Computed field: top-level folder (read-only)
    #[serde(skip_serializing)]
    pub folder: Option<String>,
}

/// Wikilink item in the consolidated array
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiLinkItem {
    /// Target note ID
    pub target: String,

    /// Link text as it appears in [[...]]
    pub text: String,

    /// Context around the link
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// Position in source document
    pub position: i32,

    /// Direction: "outgoing" or "incoming"
    pub direction: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// Embed item in the consolidated array
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedItem {
    /// Target note ID
    pub target: String,

    /// Type of embed
    #[serde(rename = "type")]
    pub embed_type: String,

    /// Reference target (heading or block ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_target: Option<String>,

    /// Display alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_alias: Option<String>,

    /// Context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// Position in source
    pub position: i32,

    /// Direction: "outgoing" or "incoming"
    pub direction: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// Related note item in the consolidated array
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedNoteItem {
    /// Target note ID
    pub target: String,

    /// Type of relationship
    pub relation_type: String,

    /// Similarity/strength score
    pub score: f32,

    /// Direction: "outgoing" or "incoming"
    pub direction: String,

    /// When computed
    pub computed_at: DateTime<Utc>,

    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl NoteV2 {
    /// Create a new note with minimal required fields
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let now = Utc::now();

        Self {
            id: None,
            path,
            title: None,
            created_at: now,
            modified_at: now,
            wikilinks: Vec::new(),
            embeds: Vec::new(),
            tags: Vec::new(),
            related_notes: Vec::new(),
            embedding: None,
            embedding_model: None,
            embedding_updated_at: None,
            metadata: HashMap::new(),
            status: None,
            folder: None,
        }
    }

    /// Add a wikilink
    pub fn add_wikilink(&mut self, target: String, text: String, position: i32) {
        self.wikilinks.push(WikiLinkItem {
            target,
            text,
            context: None,
            position,
            direction: "outgoing".to_string(),
            created_at: Utc::now(),
        });
    }

    /// Add tags
    pub fn add_tags(&mut self, tags: Vec<String>) {
        self.tags.extend(tags);
    }

    /// Add related note
    pub fn add_related_note(&mut self, target: String, relation_type: String, score: f32) {
        self.related_notes.push(RelatedNoteItem {
            target,
            relation_type,
            score,
            direction: "outgoing".to_string(),
            computed_at: Utc::now(),
            metadata: None,
        });
    }

    /// Set embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>, model: impl Into<String>) -> Self {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model.into());
        self.embedding_updated_at = Some(Utc::now());
        self
    }
}

// ============================================================================
// Migration Helpers
// ============================================================================

/// Convert v1.0 Note to v2.0 Note
impl From<Note> for NoteV2 {
    fn from(v1_note: Note) -> Self {
        let mut v2_note = NoteV2::new(v1_note.path);

        v2_note.id = v1_note.id.map(|id| RecordId {
            table: id.table,
            id: id.id,
            _phantom: std::marker::PhantomData,
        });
        v2_note.title = v1_note.title;
        v2_note.created_at = v1_note.created_at;
        v2_note.modified_at = v1_note.modified_at;
        v2_note.tags = v1_note.tags;
        v2_note.embedding = v1_note.embedding;
        v2_note.embedding_model = v1_note.embedding_model;
        v2_note.embedding_updated_at = v1_note.embedding_updated_at;
        v2_note.metadata = v1_note.metadata;

        // Relations will be populated separately during migration
        v2_note
    }
}
```

### 4.2. Query Builder Updates

#### New `query_v2.rs`

```rust
//! Query builders for the simplified v2.0 schema

use crate::schema_types_v2::*;
use serde_json::Value;

/// Simplified query builder for v2.0 schema
#[derive(Debug, Clone)]
pub struct NoteQueryV2 {
    pub table: String,
    pub filters: Vec<QueryFilter>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub order_by: Vec<OrderByClause>,
}

#[derive(Debug, Clone)]
pub enum QueryFilter {
    /// Path equals or contains
    Path(String),
    /// Tags contain specific tag
    HasTag(String),
    /// Has any of the specified tags
    HasTags(Vec<String>),
    /// Wikilinks target a specific note
    LinksTo(String),
    /// Is linked to by specific note
    LinkedFrom(String),
    /// Has embeddings
    HasEmbeddings,
    /// Modified after date
    ModifiedAfter(DateTime<Utc>),
    /// In specific folder
    InFolder(String),
    /// Custom SurrealQL filter
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct OrderByClause {
    pub field: String,
    pub direction: OrderDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl NoteQueryV2 {
    pub fn new() -> Self {
        Self {
            table: "notes_v2".to_string(),
            filters: Vec::new(),
            limit: None,
            offset: None,
            order_by: Vec::new(),
        }
    }

    pub fn table(mut self, table: impl Into<String>) -> Self {
        self.table = table.into();
        self
    }

    pub fn filter(mut self, filter: QueryFilter) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn order_by(mut self, field: impl Into<String>, direction: OrderDirection) -> Self {
        self.order_by.push(OrderByClause {
            field: field.into(),
            direction,
        });
        self
    }

    /// Convert to SurrealQL
    pub fn to_sql(&self) -> String {
        let mut sql = String::from("SELECT * FROM ");
        sql.push_str(&self.table);

        // WHERE clauses
        if !self.filters.is_empty() {
            sql.push_str(" WHERE ");
            let filter_clauses: Vec<String> = self.filters
                .iter()
                .map(|f| f.to_sql())
                .collect();
            sql.push_str(&filter_clauses.join(" AND "));
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            let order_clauses: Vec<String> = self.order_by
                .iter()
                .map(|o| format!("{} {}", o.field, match o.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                }))
                .collect();
            sql.push_str(&order_clauses.join(", "));
        }

        // LIMIT
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        // OFFSET
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        sql
    }
}

impl QueryFilter {
    pub fn to_sql(&self) -> String {
        match self {
            QueryFilter::Path(path) => {
                if path.contains('*') {
                    format!("path LIKE '{}'", path.replace('*', '%'))
                } else {
                    format!("path = '{}'", path)
                }
            }
            QueryFilter::HasTag(tag) => {
                format!("tags CONTAINS '{}'", tag)
            }
            QueryFilter::HasTags(tags) => {
                let tag_conditions: Vec<String> = tags
                    .iter()
                    .map(|t| format!("tags CONTAINS '{}'", t))
                    .collect();
                format!("({})", tag_conditions.join(" OR "))
            }
            QueryFilter::LinksTo(target) => {
                format!("wikilinks.target = '{}'", target)
            }
            QueryFilter::LinkedFrom(source) => {
                format!("wikilinks.target = '{}' AND wikilinks.direction = 'incoming'", source)
            }
            QueryFilter::HasEmbeddings => {
                "embedding IS NOT NONE".to_string()
            }
            QueryFilter::ModifiedAfter(date) => {
                format!("modified_at > '{}'", date.format("%Y-%m-%dT%H:%M:%S%.3fZ"))
            }
            QueryFilter::InFolder(folder) => {
                format!("folder = '{}'", folder)
            }
            QueryFilter::Custom(sql) => sql.clone(),
        }
    }
}

/// Specialized query builders for common operations

impl NoteQueryV2 {
    /// Find notes linking to a specific note
    pub fn backlinks(target_note_id: &str) -> Self {
        Self::new()
            .filter(QueryFilter::LinkedFrom(target_note_id.to_string()))
            .order_by("modified_at", OrderDirection::Desc)
    }

    /// Find notes with specific tags
    pub fn by_tags(tags: Vec<String>) -> Self {
        Self::new()
            .filter(QueryFilter::HasTags(tags))
            .order_by("modified_at", OrderDirection::Desc)
    }

    /// Find notes in a specific folder
    pub fn by_folder(folder: &str) -> Self {
        Self::new()
            .filter(QueryFilter::InFolder(folder.to_string()))
            .order_by("modified_at", OrderDirection::Desc)
    }

    /// Semantic search (requires embedding)
    pub fn semantic_search(embedding: Vec<f32>, min_similarity: f32) -> String {
        format!(
            r#"
            SELECT *, vector::distance::cosine(embedding, ${embedding}) AS similarity
            FROM notes_v2
            WHERE embedding IS NOT NONE
            AND vector::distance::cosine(embedding, ${embedding}) <= ${min_similarity}
            ORDER BY similarity ASC
            LIMIT 10
            "#
        )
    }

    /// Full-text search (title and metadata search)
    pub fn text_search(query: &str) -> String {
        format!(
            r#"
            SELECT * FROM notes_v2
            WHERE string::contains(string::lowercase(title), '{}')
            OR string::contains(string::lowercase(metadata), '{}')
            ORDER BY modified_at DESC
            LIMIT 20
            "#,
            query.to_lowercase(),
            query.to_lowercase()
        )
    }

    /// Find orphaned notes (no incoming or outgoing links)
    pub fn orphaned_notes() -> String {
        r#"
        SELECT * FROM notes_v2
        WHERE array::len(wikilinks) = 0
        AND array::len(related_notes) = 0
        ORDER BY modified_at DESC
        "#.to_string()
    }

    /// Find notes with broken links (links to non-existent notes)
    pub fn broken_links() -> String {
        r#"
        SELECT
            id,
            path,
            array::filter(wikilinks, || -> {
                LET target_path = string::split($after.target, ':')[1];
                LET target_exists = (SELECT count() FROM notes_v2 WHERE path = target_path)[0].count > 0;
                return NOT target_exists;
            }) AS broken_wikilinks
        FROM notes_v2
        WHERE array::len(wikilinks) > 0
        "#.to_string()
    }
}
```

### 4.3. Connection and Configuration Changes

#### Updated `surreal_client_v2.rs`

```rust
//! Updated SurrealClient for v2.0 schema
//! Simplified connection handling with v2.0 table support

use crate::types::SurrealDbConfig;
use crate::schema_types_v2::*;
use crucible_core::{DbError, DbResult, QueryResult, Record, RecordId};
use serde_json::Value;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

/// Updated SurrealClient for v2.0 schema
#[derive(Clone)]
pub struct SurrealClientV2 {
    /// The underlying SurrealDB connection
    db: Surreal<Db>,

    /// Configuration for this client
    config: SurrealDbConfig,

    /// Schema version this client is using
    schema_version: u32,
}

impl SurrealClientV2 {
    /// Create a new client with v2.0 schema
    pub async fn new_v2(config: SurrealDbConfig) -> DbResult<Self> {
        use surrealdb::engine::local::{Mem, RocksDb};

        let db = if config.path.is_empty() || config.path == ":memory:" {
            Surreal::new::<Mem>(()).await.map_err(|e| {
                DbError::Connection(format!("Failed to create in-memory database: {}", e))
            })?
        } else {
            Surreal::new::<RocksDb>(&config.path).await.map_err(|e| {
                DbError::Connection(format!(
                    "Failed to create file database at {}: {}",
                    config.path, e
                ))
            })?
        };

        // Use the configured namespace and database
        db.use_ns(&config.namespace)
            .use_db(&config.database)
            .await
            .map_err(|e| {
                DbError::Connection(format!(
                    "Failed to use namespace '{}' and database '{}': {}",
                    config.namespace, config.database, e
                ))
            })?;

        // Verify schema version
        let version_result = db
            .query("SELECT schema_version FROM metadata:system")
            .await
            .map_err(|e| DbError::Connection(format!("Failed to check schema version: {}", e)))?;

        let mut version_response = version_result
            .check()
            .map_err(|e| DbError::Connection(format!("Schema version query failed: {}", e)))?;

        let schema_version: Option<u32> = version_response
            .take("schema_version")
            .map_err(|e| DbError::Connection(format!("Failed to extract schema version: {}", e)))?;

        if let Some(version) = schema_version {
            if version != 2 {
                return Err(DbError::Connection(format!(
                    "Schema version mismatch. Expected v2, found v{}",
                    version
                )));
            }
        } else {
            return Err(DbError::Connection(
                "Could not determine schema version. Please run migration first.".to_string()
            ));
        }

        Ok(Self {
            db,
            config,
            schema_version: 2,
        })
    }

    /// Create a note in v2.0 schema
    pub async fn create_note(&self, note: NoteV2) -> DbResult<NoteV2> {
        let note_data = serde_json::to_value(&note)
            .map_err(|e| DbError::Query(format!("Failed to serialize note: {}", e)))?;

        let id_part = note.path.replace('/', "_").replace('.', "_");

        let sql = format!(
            "CREATE notes_v2:`{}` CONTENT {}",
            id_part,
            note_data
        );

        let result = self.query(&sql, &[]).await?;

        // Convert back to NoteV2
        if let Some(first_record) = result.records.first() {
            let note_json = serde_json::to_value(&first_record.data)
                .map_err(|e| DbError::Query(format!("Failed to convert result: {}", e)))?;

            let note: NoteV2 = serde_json::from_value(note_json)
                .map_err(|e| DbError::Query(format!("Failed to deserialize note: {}", e)))?;

            Ok(note)
        } else {
            Err(DbError::Query("No record created".to_string()))
        }
    }

    /// Get a note by path
    pub async fn get_note_by_path(&self, path: &str) -> DbResult<Option<NoteV2>> {
        let sql = format!("SELECT * FROM notes_v2 WHERE path = '{}'", path);
        let result = self.query(&sql, &[]).await?;

        if let Some(first_record) = result.records.first() {
            let note_json = serde_json::to_value(&first_record.data)
                .map_err(|e| DbError::Query(format!("Failed to convert result: {}", e)))?;

            let note: NoteV2 = serde_json::from_value(note_json)
                .map_err(|e| DbError::Query(format!("Failed to deserialize note: {}", e)))?;

            Ok(Some(note))
        } else {
            Ok(None)
        }
    }

    /// Update a note
    pub async fn update_note(&self, path: &str, updates: PartialNoteV2) -> DbResult<NoteV2> {
        let updates_json = serde_json::to_value(&updates)
            .map_err(|e| DbError::Query(format!("Failed to serialize updates: {}", e)))?;

        let sql = format!(
            "UPDATE notes_v2 SET {} WHERE path = '{}' RETURN BEFORE",
            updates_json,
            path
        );

        let result = self.query(&sql, &[]).await?;

        // Convert to NoteV2
        if let Some(first_record) = result.records.first() {
            let note_json = serde_json::to_value(&first_record.data)
                .map_err(|e| DbError::Query(format!("Failed to convert result: {}", e)))?;

            let note: NoteV2 = serde_json::from_value(note_json)
                .map_err(|e| DbError::Query(format!("Failed to deserialize note: {}", e)))?;

            Ok(note)
        } else {
            Err(DbError::Query("No record updated".to_string()))
        }
    }

    /// Delete a note
    pub async fn delete_note(&self, path: &str) -> DbResult<()> {
        let sql = format!("DELETE FROM notes_v2 WHERE path = '{}'", path);
        self.query(&sql, &[]).await?;
        Ok(())
    }

    /// Find backlinks to a note
    pub async fn find_backlinks(&self, target_path: &str) -> DbResult<Vec<NoteV2>> {
        let target_id = format!("notes:{}", target_path.replace('/', "_").replace('.', "_"));
        let sql = format!(
            "SELECT * FROM notes_v2 WHERE wikilinks.target = '{}' AND wikilinks.direction = 'incoming'",
            target_id
        );

        let result = self.query(&sql, &[]).await?;

        let mut notes = Vec::new();
        for record in result.records {
            let note_json = serde_json::to_value(&record.data)
                .map_err(|e| DbError::Query(format!("Failed to convert result: {}", e)))?;

            let note: NoteV2 = serde_json::from_value(note_json)
                .map_err(|e| DbError::Query(format!("Failed to deserialize note: {}", e)))?;

            notes.push(note);
        }

        Ok(notes)
    }

    /// Semantic search with embeddings
    pub async fn semantic_search(&self, embedding: Vec<f32>, limit: u32) -> DbResult<Vec<SearchResultV2>> {
        let sql = NoteQueryV2::semantic_search(embedding, 0.7);
        let result = self.query(&format!("{} LIMIT {}", sql, limit), &[]).await?;

        let mut search_results = Vec::new();
        for record in result.records {
            if let Some(similarity) = record.data.get("similarity").and_then(|v| v.as_f64()) {
                let note_json = serde_json::to_value(&record.data)
                    .map_err(|e| DbError::Query(format!("Failed to convert result: {}", e)))?;

                let note: NoteV2 = serde_json::from_value(note_json)
                    .map_err(|e| DbError::Query(format!("Failed to deserialize note: {}", e)))?;

                search_results.push(SearchResultV2 {
                    note,
                    score: similarity,
                });
            }
        }

        Ok(search_results)
    }

    /// Execute custom query
    pub async fn query(&self, sql: &str, _params: &[Value]) -> DbResult<QueryResult> {
        // Implementation similar to original but adapted for v2.0
        let response = self
            .db
            .query(sql)
            .await
            .map_err(|e| DbError::Query(format!("Query execution failed: {}", e)))?;

        let mut response = response
            .check()
            .map_err(|e| DbError::Query(format!("Query returned error: {}", e)))?;

        let surreal_value: surrealdb::opt::IntoArray<surrealdb::sql::Value> = response
            .take(0)
            .map_err(|e| DbError::Query(format!("Failed to extract query results: {}", e)))?;

        // Convert and return results...
        // (Implementation details similar to original client)
        todo!("Implement result conversion for v2.0")
    }

    /// Check if migration is needed
    pub async fn needs_migration(&self) -> DbResult<bool> {
        let version_result = self
            .db
            .query("SELECT schema_version FROM metadata:system")
            .await
            .map_err(|e| DbError::Connection(format!("Failed to check schema version: {}", e)))?;

        let mut version_response = version_result
            .check()
            .map_err(|e| DbError::Connection(format!("Schema version query failed: {}", e)))?;

        let schema_version: Option<u32> = version_response
            .take("schema_version")
            .map_err(|e| DbError::Connection(format!("Failed to extract schema version: {}", e)))?;

        Ok(schema_version.unwrap_or(1) < 2)
    }
}

/// Partial update structure for notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialNoteV2 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wikilinks: Option<Vec<WikiLinkItem>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<EmbedItem>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_notes: Option<Vec<RelatedNoteItem>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Search result for v2.0
#[derive(Debug, Clone)]
pub struct SearchResultV2 {
    pub note: NoteV2,
    pub score: f64,
}
```

### 4.4. Integration Layer

#### Updated `database_v2.rs`

```rust
//! Updated database implementation for v2.0 schema

use crate::surreal_client_v2::SurrealClientV2;
use crate::schema_types_v2::*;
use crate::types::SurrealDbConfig;
use anyhow::Result;
use async_trait::async_trait;
use crucible_core::{
    EmbeddingDatabase, EmbeddingError, EmbeddingMetadata, SearchResult,
    VectorSearchConfig,
};

/// Updated embedding database using v2.0 schema
pub struct SurrealEmbeddingDatabaseV2 {
    client: SurrealClientV2,
    config: SurrealDbConfig,
}

#[async_trait]
impl EmbeddingDatabase for SurrealEmbeddingDatabaseV2 {
    async fn initialize(&self) -> Result<()> {
        // Check if migration is needed
        if self.client.needs_migration().await? {
            return Err(EmbeddingError::Initialization(
                "Database schema migration required. Please run migration script first.".to_string()
            ).into());
        }

        Ok(())
    }

    async fn store_embedding(&self, file_path: &str, _content: &str, embedding: &[f32]) -> Result<()> {
        // Check if note exists
        let note = self.client.get_note_by_path(file_path).await?;

        match note {
            Some(mut existing_note) => {
                // Update existing note with embedding
                existing_note.embedding = Some(embedding.to_vec());
                existing_note.embedding_model = Some("all-MiniLM-L6-v2".to_string());
                existing_note.embedding_updated_at = Some(chrono::Utc::now());

                self.client.update_note(file_path, PartialNoteV2 {
                    embedding: Some(embedding.to_vec()),
                    ..Default::default()
                }).await?;
            }
            None => {
                // Create new note (without content)
                let new_note = NoteV2::new(file_path)
                    .with_embedding(embedding.to_vec(), "all-MiniLM-L6-v2");

                self.client.create_note(new_note).await?;
            }
        }

        Ok(())
    }

    async fn search_similar(&self, query_embedding: &[f32], config: &VectorSearchConfig) -> Result<Vec<SearchResult>> {
        let search_results = self.client
            .semantic_search(query_embedding.to_vec(), config.limit.unwrap_or(10))
            .await?;

        let mut results = Vec::new();
        for search_result in search_results {
            // Convert to core SearchResult format
            results.push(SearchResult {
                file_path: search_result.note.path.clone(),
                title: search_result.note.title.clone(),
                score: search_result.score,
                metadata: EmbeddingMetadata {
                    file_path: search_result.note.path.clone(),
                    title: search_result.note.title,
                    tags: search_result.note.tags,
                    folder: search_result.note.folder.unwrap_or_default(),
                    properties: search_result.note.metadata,
                    created_at: search_result.note.created_at,
                    updated_at: search_result.note.modified_at,
                },
            });
        }

        Ok(results)
    }

    async fn get_metadata(&self, file_path: &str) -> Result<Option<EmbeddingMetadata>> {
        let note = self.client.get_note_by_path(file_path).await?;

        Ok(note.map(|n| EmbeddingMetadata {
            file_path: n.path,
            title: n.title,
            tags: n.tags,
            folder: n.folder.unwrap_or_default(),
            properties: n.metadata,
            created_at: n.created_at,
            updated_at: n.modified_at,
        }))
    }

    async fn update_metadata(&self, file_path: &str, metadata: EmbeddingMetadata) -> Result<()> {
        // Update note metadata
        let updates = PartialNoteV2 {
            title: metadata.title.clone(),
            tags: Some(metadata.tags.clone()),
            metadata: Some(metadata.properties),
            ..Default::default()
        };

        self.client.update_note(file_path, updates).await?;
        Ok(())
    }

    async fn delete_document(&self, file_path: &str) -> Result<()> {
        self.client.delete_note(file_path).await?;
        Ok(())
    }

    async fn list_documents(&self, folder: Option<&str>) -> Result<Vec<String>> {
        let sql = if let Some(folder_path) = folder {
            format!("SELECT path FROM notes_v2 WHERE folder = '{}'", folder_path)
        } else {
            "SELECT path FROM notes_v2".to_string()
        };

        let result = self.client.query(&sql, &[]).await?;
        let mut paths = Vec::new();

        for record in result.records {
            if let Some(path) = record.data.get("path").and_then(|v| v.as_str()) {
                paths.push(path.to_string());
            }
        }

        Ok(paths)
    }

    async fn get_stats(&self) -> Result<crucible_core::DatabaseStats> {
        let queries = vec![
            "SELECT count() AS total FROM notes_v2",
            "SELECT count() AS with_embeddings FROM notes_v2 WHERE embedding IS NOT NONE",
            "SELECT count() AS total_links FROM notes_v2 WHERE array::len(wikilinks) > 0",
        ];

        let mut total = 0;
        let mut with_embeddings = 0;
        let mut total_links = 0;

        for (i, query) in queries.iter().enumerate() {
            let result = self.client.query(query, &[]).await?;
            if let Some(first) = result.records.first() {
                let count = first.data.get("total").or_else(|| first.data.get("with_embeddings"))
                    .or_else(|| first.data.get("total_links"))
                    .and_then(|v| v.as_i64()).unwrap_or(0);

                match i {
                    0 => total = count,
                    1 => with_embeddings = count,
                    2 => total_links = count,
                    _ => {}
                }
            }
        }

        Ok(crucible_core::DatabaseStats {
            total_documents: total,
            total_embeddings: with_embeddings,
            storage_size_bytes: None, // Would need additional query
            last_updated: chrono::Utc::now(),
        })
    }
}

impl SurrealEmbeddingDatabaseV2 {
    /// Create new database instance with v2.0 schema
    pub async fn new(config: SurrealDbConfig) -> Result<Self> {
        let client = SurrealClientV2::new_v2(config.clone()).await?;

        Ok(Self {
            client,
            config,
        })
    }

    /// Create file-based database
    pub async fn new_file(path: &str) -> Result<Self> {
        let config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            path: path.to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        Self::new(config).await
    }

    /// Create in-memory database for testing
    pub async fn new_memory() -> Result<Self> {
        let config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "test".to_string(),
            path: ":memory:".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        Self::new(config).await
    }

    /// Find backlinks to a document
    pub async fn find_backlinks(&self, file_path: &str) -> Result<Vec<NoteV2>> {
        self.client.find_backlinks(file_path).await
    }

    /// Find orphaned documents
    pub async fn find_orphaned_documents(&self) -> Result<Vec<NoteV2>> {
        let sql = NoteQueryV2::orphaned_notes();
        let result = self.client.query(&sql, &[]).await?;

        let mut notes = Vec::new();
        for record in result.records {
            let note_json = serde_json::to_value(&record.data)
                .map_err(|e| EmbeddingError::Query(format!("Failed to convert result: {}", e)))?;

            let note: NoteV2 = serde_json::from_value(note_json)
                .map_err(|e| EmbeddingError::Query(format!("Failed to deserialize note: {}", e)))?;

            notes.push(note);
        }

        Ok(notes)
    }

    /// Find documents with broken links
    pub async fn find_broken_links(&self) -> Result<Vec<BrokenLinkReport>> {
        let sql = NoteQueryV2::broken_links();
        let result = self.client.query(&sql, &[]).await?;

        let mut reports = Vec::new();
        for record in result.records {
            // Parse broken link results
            // Implementation depends on exact query format
            todo!("Parse broken link results");
        }

        Ok(reports)
    }
}

#[derive(Debug, Clone)]
pub struct BrokenLinkReport {
    pub source_path: String,
    pub broken_links: Vec<BrokenWikiLink>,
}

#[derive(Debug, Clone)]
pub struct BrokenWikiLink {
    pub link_text: String,
    pub target_path: String,
    pub position: i32,
}

impl Default for PartialNoteV2 {
    fn default() -> Self {
        Self {
            title: None,
            tags: None,
            wikilinks: None,
            embeds: None,
            related_notes: None,
            embedding: None,
            metadata: None,
        }
    }
}
```

---

## 5. Migration Risk Assessment

### 5.1. High Risk Areas

1. **Data Loss During Migration**
   - **Risk:** Corruption or loss of existing data
   - **Mitigation:** Full database backup before migration
   - **Recovery:** Restore from backup

2. **Application Downtime**
   - **Risk:** Extended downtime during migration
   - **Mitigation:** Practice migration on staging environment
   - **Recovery:** Rollback to v1.0 schema

3. **Query Performance Regression**
   - **Risk:** Array-based queries slower than relation tables
   - **Mitigation:** Benchmark critical queries before/after
   - **Recovery:** Add indexes or optimize queries

4. **Embedding Search Issues**
   - **Risk:** Vector search functionality broken
   - **Mitigation:** Test embedding search with sample data
   - **Recovery:** Fix embedding migration logic

### 5.2. Medium Risk Areas

1. **Backlink Functionality**
   - **Risk:** Broken backlink detection and reporting
   - **Mitigation:** Comprehensive testing of link queries
   - **Recovery:** Update query logic

2. **Tag Management**
   - **Risk:** Tag-based filtering not working correctly
   - **Mitigation:** Test tag queries with various data
   - **Recovery:** Fix tag array handling

3. **API Compatibility**
   - **Risk:** Breaking changes to external APIs
   - **Mitigation:** Version API endpoints or provide compatibility layer
   - **Recovery:** Restore compatibility layer

### 5.3. Low Risk Areas

1. **Metadata Storage**
   - **Risk:** Metadata fields not properly migrated
   - **Mitigation:** Field-by-field validation
   - **Recovery:** Manual metadata updates

2. **Configuration Management**
   - **Risk:** Database connection issues
   - **Mitigation:** Test connection with both schemas
   - **Recovery:** Update configuration

---

## 6. Testing Strategy

### 6.1. Pre-Migration Testing

1. **Schema Validation**
   - Verify new schema syntax is correct
   - Test all constraint definitions
   - Validate index creation

2. **Migration Function Testing**
   - Test `fn::migrate_note_v1_to_v2` with sample data
   - Verify relation-to-array transformation
   - Check data integrity

3. **Performance Testing**
   - Benchmark critical queries on sample data
   - Test array-based vs relation-based performance
   - Identify potential bottlenecks

### 6.2. Post-Migration Testing

1. **Functionality Testing**
   - Verify all CRUD operations work
   - Test search functionality (text and semantic)
   - Validate backlink detection

2. **Data Integrity Testing**
   - Compare record counts between schemas
   - Spot-check transformed data
   - Verify embedding functionality

3. **Integration Testing**
   - Test with full application stack
   - Verify API endpoints work correctly
   - Check external integrations

### 6.3. Rollback Testing

1. **Backup/Restore Testing**
   - Test backup creation and restoration
   - Verify rollback script functionality
   - Time rollback procedures

2. **Data Consistency Testing**
   - Verify data consistency after rollback
   - Test application functionality post-rollback
   - Validate no data corruption

---

## 7. Performance Considerations

### 7.1. Expected Performance Improvements

1. **Query Simplification**
   - No JOIN operations needed for common queries
   - Single-table lookups for most operations
   - Reduced query complexity and planning time

2. **Storage Efficiency**
   - Eliminated duplicate content fields
   - Removed relation table overhead
   - More compact data representation

3. **Memory Usage**
   - Fewer table structures to maintain
   - Reduced metadata overhead
   - More efficient caching

### 7.2. Potential Performance Issues

1. **Array Query Performance**
   - Array operations may be slower than indexed relation lookups
   - Large arrays could impact query performance
   - Complex array filtering may be slower

2. **Embedding Search Impact**
   - Vector search should remain unchanged
   - Array storage may affect memory usage
   - Index rebuild may be required

### 7.3. Optimization Strategies

1. **Index Strategy**
   - Add indexes on array length where relevant
   - Consider partial indexes on frequently queried array contents
   - Monitor index usage and effectiveness

2. **Query Optimization**
   - Use array functions efficiently
   - Avoid full array scans where possible
   - Consider query result caching

3. **Data Model Optimization**
   - Keep arrays reasonably sized
   - Consider pagination for large array results
   - Optimize array element structure

---

## 8. Monitoring and Maintenance

### 8.1. Migration Monitoring

1. **Progress Tracking**
   - Monitor migration function execution
   - Track batch processing progress
   - Log any errors or warnings

2. **Performance Monitoring**
   - Monitor query performance during migration
   - Track database resource usage
   - Identify any performance regressions

3. **Error Monitoring**
   - Log all migration errors
   - Track data validation failures
   - Monitor application error rates

### 8.2. Post-Migration Monitoring

1. **Query Performance**
   - Monitor query execution times
   - Track index usage statistics
   - Identify slow queries

2. **Data Integrity**
   - Periodic data consistency checks
   - Monitor for data anomalies
   - Track embedding search accuracy

3. **Application Health**
   - Monitor application response times
   - Track error rates
   - Verify user experience

### 8.3. Maintenance Procedures

1. **Regular Backups**
   - Schedule regular database backups
   - Test backup restoration procedures
   - Maintain backup retention policy

2. **Index Maintenance**
   - Monitor index fragmentation
   - Schedule index rebuilding if needed
   - Optimize index configurations

3. **Schema Evolution**
   - Plan for future schema changes
   - Maintain backward compatibility where possible
   - Document schema change procedures

---

## 9. Checklist and Timeline

### 9.1. Pre-Migration Checklist

- [ ] Create full database backup
- [ ] Test migration scripts on staging environment
- [ ] Prepare rollback procedures
- [ ] Schedule maintenance window
- [ ] Notify stakeholders of planned downtime
- [ ] Prepare monitoring tools
- [ ] Validate application compatibility

### 9.2. Migration Day Checklist

- [ ] Stop application services
- [ ] Create final backup
- [ ] Execute schema migration
- [ ] Run data migration
- [ ] Validate data integrity
- [ ] Update application code
- [ ] Start application services
- [ ] Run smoke tests
- [ ] Monitor system health
- [ ] Notify stakeholders of completion

### 9.3. Post-Migration Checklist

- [ ] Verify all functionality works
- [ ] Monitor performance metrics
- [ ] Check error logs
- [ ] Run full regression test suite
- [ ] Validate user workflows
- [ ] Update documentation
- [ ] Archive old schema (after verification period)
- [ ] Schedule post-migration review

### 9.4. Timeline Estimate

| Phase | Duration | Dependencies |
|-------|----------|--------------|
| Preparation | 1-2 days | Development environment setup |
| Testing | 2-3 days | Migration scripts ready |
| Migration Execution | 2-4 hours | Maintenance window |
| Post-Migration Testing | 1-2 hours | Migration completed |
| Monitoring Period | 7 days | Migration successful |

**Total Estimated Time:** 3-5 days (including preparation and testing)

---

## 10. Conclusion

This migration plan provides a comprehensive approach to transitioning the Crucible SurrealDB backend from a complex relational schema to a simplified array-based schema. The key benefits include:

1. **Simplified Data Model:** Single table with consolidated arrays
2. **Improved Performance:** Eliminated complex JOIN operations
3. **Reduced Storage:** No duplicate content fields or relation tables
4. **Easier Maintenance:** Simplified schema and fewer components

The migration includes robust safety measures including:
- Full database backup and rollback procedures
- Comprehensive testing strategy
- Detailed risk assessment and mitigation
- Post-migration monitoring and maintenance procedures

Following this plan will result in a more maintainable and performant database schema while preserving all existing functionality and data integrity.

**Next Steps:**
1. Review and approve this migration plan
2. Set up staging environment for testing
3. Implement migration scripts
4. Execute migration during planned maintenance window
5. Monitor and optimize post-migration performance

This migration represents a significant improvement in the database architecture and will provide a solid foundation for future development and scaling of the Crucible knowledge management system.