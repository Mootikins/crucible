// ============================================================================
// Crucible + LadybugDB - Rust API Sketch
// ============================================================================
// This is a design sketch, not compilable code.
// Shows how Crucible's storage layer would look with LadybugDB.
// ============================================================================

use lbug::{Connection, Database, QueryResult, SystemConfig, Value};
use std::path::Path;

// ============================================================================
// Database Setup
// ============================================================================

pub struct LadybugStorage {
    db: Database,
}

impl LadybugStorage {
    /// Create or open a Crucible kiln database
    pub fn open(kiln_path: &Path) -> Result<Self, Error> {
        let db_path = kiln_path.join(".crucible/graph.db");
        let db = Database::new(&db_path, SystemConfig::default())?;

        let storage = Self { db };
        storage.ensure_schema()?;
        Ok(storage)
    }

    /// Initialize schema if not exists
    fn ensure_schema(&self) -> Result<(), Error> {
        let conn = self.connection()?;

        // Check if schema exists
        let result = conn.query("CALL show_tables() RETURN *")?;
        if result.is_empty() {
            conn.query(include_str!("schema.cypher"))?;
        }
        Ok(())
    }

    pub fn connection(&self) -> Result<Connection, Error> {
        Connection::new(&self.db)
    }
}

// ============================================================================
// Note Operations
// ============================================================================

impl LadybugStorage {
    /// Upsert a note (create or update)
    pub fn upsert_note(&self, note: &ParsedNote) -> Result<(), Error> {
        let conn = self.connection()?;

        conn.query_with_params(
            r#"
            MERGE (n:Note {path: $path})
            SET n.title = $title,
                n.content = $content,
                n.file_hash = $file_hash,
                n.modified_at = $modified_at,
                n.folder = $folder,
                n.status = $status
            "#,
            &[
                ("path", note.path.as_str().into()),
                ("title", note.title.clone().into()),
                ("content", note.content.as_str().into()),
                ("file_hash", note.file_hash.as_str().into()),
                ("modified_at", note.modified_at.into()),
                ("folder", note.folder().into()),
                ("status", note.status().into()),
            ],
        )?;

        Ok(())
    }

    /// Get a note by path
    pub fn get_note(&self, path: &str) -> Result<Option<Note>, Error> {
        let conn = self.connection()?;

        let mut result = conn.query_with_params(
            "MATCH (n:Note {path: $path}) RETURN n",
            &[("path", path.into())],
        )?;

        if let Some(row) = result.next()? {
            Ok(Some(row.get::<Note>("n")?))
        } else {
            Ok(None)
        }
    }

    /// Delete a note and all its relationships
    pub fn delete_note(&self, path: &str) -> Result<(), Error> {
        let conn = self.connection()?;

        // Cypher DETACH DELETE removes node and all relationships
        conn.query_with_params(
            "MATCH (n:Note {path: $path}) DETACH DELETE n",
            &[("path", path.into())],
        )?;

        Ok(())
    }
}

// ============================================================================
// Link Operations (Graph-native!)
// ============================================================================

impl LadybugStorage {
    /// Create wikilink relationship
    pub fn create_link(
        &self,
        source_path: &str,
        target_path: &str,
        link: &Wikilink,
    ) -> Result<(), Error> {
        let conn = self.connection()?;

        conn.query_with_params(
            r#"
            MATCH (source:Note {path: $source_path})
            MATCH (target:Note {path: $target_path})
            CREATE (source)-[:LINKS_TO {
                link_text: $link_text,
                display_alias: $alias,
                context: $context,
                position: $position
            }]->(target)
            "#,
            &[
                ("source_path", source_path.into()),
                ("target_path", target_path.into()),
                ("link_text", link.target.as_str().into()),
                ("alias", link.alias.clone().into()),
                ("context", link.context.clone().into()),
                ("position", (link.position as i64).into()),
            ],
        )?;

        Ok(())
    }

    /// Get outgoing links from a note
    pub fn get_outlinks(&self, path: &str) -> Result<Vec<LinkInfo>, Error> {
        let conn = self.connection()?;

        let result = conn.query_with_params(
            r#"
            MATCH (source:Note {path: $path})-[l:LINKS_TO]->(target:Note)
            RETURN target.path AS target_path,
                   target.title AS target_title,
                   l.link_text AS link_text,
                   l.context AS context
            "#,
            &[("path", path.into())],
        )?;

        result.map(|row| LinkInfo {
            target_path: row.get("target_path")?,
            target_title: row.get("target_title")?,
            link_text: row.get("link_text")?,
            context: row.get("context")?,
        }).collect()
    }

    /// Get backlinks to a note
    pub fn get_backlinks(&self, path: &str) -> Result<Vec<LinkInfo>, Error> {
        let conn = self.connection()?;

        let result = conn.query_with_params(
            r#"
            MATCH (source:Note)-[l:LINKS_TO]->(target:Note {path: $path})
            RETURN source.path AS source_path,
                   source.title AS source_title,
                   l.link_text AS link_text,
                   l.context AS context
            "#,
            &[("path", path.into())],
        )?;

        result.map(|row| LinkInfo {
            source_path: row.get("source_path")?,
            source_title: row.get("source_title")?,
            link_text: row.get("link_text")?,
            context: row.get("context")?,
        }).collect()
    }

    /// Get N-hop neighborhood
    pub fn get_neighborhood(&self, path: &str, max_hops: u32) -> Result<Vec<String>, Error> {
        let conn = self.connection()?;

        // Cypher variable-length paths - clean!
        let query = format!(
            r#"
            MATCH (n:Note {{path: $path}})-[:LINKS_TO*1..{}]-(related:Note)
            RETURN DISTINCT related.path AS path
            "#,
            max_hops
        );

        let result = conn.query_with_params(&query, &[("path", path.into())])?;
        result.map(|row| row.get::<String>("path")).collect()
    }
}

// ============================================================================
// Block Operations
// ============================================================================

impl LadybugStorage {
    /// Store blocks for a note (replaces existing)
    pub fn store_blocks(&self, note_path: &str, blocks: &[ParsedBlock]) -> Result<(), Error> {
        let conn = self.connection()?;

        // Delete existing blocks for this note
        conn.query_with_params(
            r#"
            MATCH (n:Note {path: $path})-[:HAS_BLOCK]->(b:Block)
            DETACH DELETE b
            "#,
            &[("path", note_path.into())],
        )?;

        // Create new blocks with relationships
        for (idx, block) in blocks.iter().enumerate() {
            let block_id = format!("{}#{}", note_path, idx);

            conn.query_with_params(
                r#"
                MATCH (n:Note {path: $note_path})
                CREATE (b:Block {
                    id: $block_id,
                    block_type: $block_type,
                    content: $content,
                    content_hash: $content_hash,
                    heading_level: $heading_level,
                    start_line: $start_line,
                    end_line: $end_line
                })
                CREATE (n)-[:HAS_BLOCK {
                    block_index: $block_index,
                    section_path: $section_path
                }]->(b)
                "#,
                &[
                    ("note_path", note_path.into()),
                    ("block_id", block_id.into()),
                    ("block_type", block.block_type.as_str().into()),
                    ("content", block.content.as_str().into()),
                    ("content_hash", block.content_hash.as_str().into()),
                    ("heading_level", block.heading_level.into()),
                    ("start_line", (block.start_line as i64).into()),
                    ("end_line", (block.end_line as i64).into()),
                    ("block_index", (idx as i64).into()),
                    ("section_path", block.section_path.clone().into()),
                ],
            )?;
        }

        Ok(())
    }
}

// ============================================================================
// Tag Operations
// ============================================================================

impl LadybugStorage {
    /// Ensure tag exists (with hierarchy)
    pub fn ensure_tag(&self, tag_path: &str) -> Result<(), Error> {
        let conn = self.connection()?;

        let parts: Vec<&str> = tag_path.split('/').collect();
        let depth = parts.len() - 1;
        let name = parts.last().unwrap();

        // Create tag
        conn.query_with_params(
            r#"
            MERGE (t:Tag {name: $name})
            SET t.path = $path, t.depth = $depth
            "#,
            &[
                ("name", (*name).into()),
                ("path", tag_path.into()),
                ("depth", (depth as i64).into()),
            ],
        )?;

        // Create parent relationship if nested
        if parts.len() > 1 {
            let parent_path = parts[..parts.len() - 1].join("/");
            let parent_name = parts[parts.len() - 2];

            conn.query_with_params(
                r#"
                MATCH (child:Tag {name: $child_name})
                MATCH (parent:Tag {name: $parent_name})
                MERGE (child)-[:TAG_PARENT]->(parent)
                "#,
                &[
                    ("child_name", (*name).into()),
                    ("parent_name", parent_name.into()),
                ],
            )?;
        }

        Ok(())
    }

    /// Tag a note
    pub fn tag_note(&self, note_path: &str, tag_name: &str, source: &str) -> Result<(), Error> {
        let conn = self.connection()?;

        conn.query_with_params(
            r#"
            MATCH (n:Note {path: $note_path})
            MATCH (t:Tag {name: $tag_name})
            MERGE (n)-[:TAGGED_WITH {source: $source}]->(t)
            "#,
            &[
                ("note_path", note_path.into()),
                ("tag_name", tag_name.into()),
                ("source", source.into()),
            ],
        )?;

        Ok(())
    }

    /// Find notes by tag (including children)
    pub fn find_by_tag(&self, tag_path_prefix: &str) -> Result<Vec<String>, Error> {
        let conn = self.connection()?;

        let result = conn.query_with_params(
            r#"
            MATCH (n:Note)-[:TAGGED_WITH]->(t:Tag)
            WHERE t.path STARTS WITH $prefix
            RETURN DISTINCT n.path AS path
            "#,
            &[("prefix", tag_path_prefix.into())],
        )?;

        result.map(|row| row.get::<String>("path")).collect()
    }
}

// ============================================================================
// Semantic Search
// ============================================================================

impl LadybugStorage {
    /// Store embedding for a block
    pub fn store_embedding(
        &self,
        entity_type: &str,
        entity_id: &str,
        model: &str,
        model_version: &str,
        vector: &[f32],
        content_hash: &str,
    ) -> Result<(), Error> {
        let conn = self.connection()?;

        let id = format!("{}:{}:{}", entity_type, entity_id, model);

        conn.query_with_params(
            r#"
            MERGE (e:Embedding {id: $id})
            SET e.entity_type = $entity_type,
                e.entity_id = $entity_id,
                e.model = $model,
                e.model_version = $model_version,
                e.vector = $vector,
                e.content_hash = $content_hash,
                e.created_at = timestamp()
            "#,
            &[
                ("id", id.into()),
                ("entity_type", entity_type.into()),
                ("entity_id", entity_id.into()),
                ("model", model.into()),
                ("model_version", model_version.into()),
                ("vector", Value::List(vector.iter().map(|&f| f.into()).collect())),
                ("content_hash", content_hash.into()),
            ],
        )?;

        Ok(())
    }

    /// Vector similarity search
    pub fn semantic_search(
        &self,
        query_vector: &[f32],
        entity_type: &str,
        limit: usize,
    ) -> Result<Vec<SemanticResult>, Error> {
        let conn = self.connection()?;

        // LadybugDB supports vector search via index
        let result = conn.query_with_params(
            r#"
            CALL vector_search(Embedding, vector, $query_vector, $limit)
            YIELD node, score
            WHERE node.entity_type = $entity_type
            RETURN node.entity_id AS id, score
            "#,
            &[
                ("query_vector", Value::List(query_vector.iter().map(|&f| f.into()).collect())),
                ("limit", (limit as i64).into()),
                ("entity_type", entity_type.into()),
            ],
        )?;

        result.map(|row| SemanticResult {
            entity_id: row.get("id")?,
            score: row.get("score")?,
        }).collect()
    }
}

// ============================================================================
// Graph Analytics
// ============================================================================

impl LadybugStorage {
    /// Find orphan notes (no links in or out)
    pub fn find_orphans(&self) -> Result<Vec<String>, Error> {
        let conn = self.connection()?;

        let result = conn.query(
            r#"
            MATCH (n:Note)
            WHERE NOT (n)-[:LINKS_TO]-() AND NOT ()-[:LINKS_TO]->(n)
            RETURN n.path AS path
            "#,
        )?;

        result.map(|row| row.get::<String>("path")).collect()
    }

    /// Get most connected notes
    pub fn most_connected(&self, limit: usize) -> Result<Vec<(String, u64)>, Error> {
        let conn = self.connection()?;

        let result = conn.query_with_params(
            r#"
            MATCH (n:Note)-[l:LINKS_TO]-()
            RETURN n.path AS path, COUNT(l) AS connections
            ORDER BY connections DESC
            LIMIT $limit
            "#,
            &[("limit", (limit as i64).into())],
        )?;

        result.map(|row| {
            let path: String = row.get("path")?;
            let connections: i64 = row.get("connections")?;
            Ok((path, connections as u64))
        }).collect()
    }

    /// Find clusters (connected components)
    pub fn find_clusters(&self) -> Result<Vec<Vec<String>>, Error> {
        let conn = self.connection()?;

        // LadybugDB/Kuzu has built-in graph algorithms
        let result = conn.query(
            r#"
            CALL weakly_connected_components(Note, LINKS_TO)
            YIELD node, componentId
            RETURN node.path AS path, componentId
            ORDER BY componentId
            "#,
        )?;

        // Group by component
        let mut clusters: std::collections::HashMap<i64, Vec<String>> = Default::default();
        for row in result {
            let path: String = row.get("path")?;
            let component: i64 = row.get("componentId")?;
            clusters.entry(component).or_default().push(path);
        }

        Ok(clusters.into_values().collect())
    }
}

// ============================================================================
// Migration Helper
// ============================================================================

impl LadybugStorage {
    /// Import from SurrealDB export (JSON lines)
    pub fn import_from_surrealdb(&self, export_path: &Path) -> Result<ImportStats, Error> {
        let conn = self.connection()?;
        let mut stats = ImportStats::default();

        // Read notes
        let notes_file = export_path.join("notes.jsonl");
        for line in BufReader::new(File::open(notes_file)?).lines() {
            let note: SurrealNote = serde_json::from_str(&line?)?;

            conn.query_with_params(
                r#"
                CREATE (n:Note {
                    path: $path,
                    title: $title,
                    content: $content,
                    file_hash: $file_hash,
                    modified_at: $modified_at
                })
                "#,
                &[
                    ("path", note.path.into()),
                    ("title", note.title.into()),
                    ("content", note.content.into()),
                    ("file_hash", note.file_hash.into()),
                    ("modified_at", note.modified_at.into()),
                ],
            )?;
            stats.notes += 1;
        }

        // Read wikilinks
        let links_file = export_path.join("wikilinks.jsonl");
        for line in BufReader::new(File::open(links_file)?).lines() {
            let link: SurrealWikilink = serde_json::from_str(&line?)?;

            conn.query_with_params(
                r#"
                MATCH (source:Note {path: $source})
                MATCH (target:Note {path: $target})
                CREATE (source)-[:LINKS_TO {
                    link_text: $link_text,
                    position: $position
                }]->(target)
                "#,
                &[
                    ("source", link.source_path.into()),
                    ("target", link.target_path.into()),
                    ("link_text", link.link_text.into()),
                    ("position", link.position.into()),
                ],
            )?;
            stats.links += 1;
        }

        Ok(stats)
    }
}

// ============================================================================
// Type Definitions
// ============================================================================

pub struct LinkInfo {
    pub source_path: Option<String>,
    pub target_path: Option<String>,
    pub source_title: Option<String>,
    pub target_title: Option<String>,
    pub link_text: String,
    pub context: Option<String>,
}

pub struct SemanticResult {
    pub entity_id: String,
    pub score: f32,
}

#[derive(Default)]
pub struct ImportStats {
    pub notes: usize,
    pub links: usize,
    pub blocks: usize,
    pub tags: usize,
    pub embeddings: usize,
}

// ============================================================================
// Comparison: SurrealDB vs LadybugDB API
// ============================================================================
//
// | Operation          | SurrealDB                          | LadybugDB                    |
// |--------------------|------------------------------------|-----------------------------|
// | Get backlinks      | SELECT * FROM wikilink WHERE out=X | MATCH ()-[l]->(n) RETURN    |
// | 2-hop neighborhood | Complex recursive query            | -[:LINKS_TO*1..2]-          |
// | Find orphans       | Multiple queries + app logic       | Single MATCH with NOT       |
// | Connected components| Not built-in                      | CALL weakly_connected_components() |
// | Vector search      | vector::distance::cosine()         | CALL vector_search()        |
//
// Key benefits:
// - Graph traversal is declarative, not imperative
// - Variable-length paths (*1..N) are trivial
// - Built-in graph algorithms
// - LLM can generate correct queries more often
// ============================================================================
