//! SQLite query renderer for crucible-query.
//!
//! Converts GraphIR to SQLite SQL with:
//! - Recursive CTEs for variable-length paths
//! - JOINs for fixed-length patterns
//! - sqlite-vec integration for vector search
//!
//! This is a SKETCH - shows the approach, not fully compilable.

use crate::ir::{
    EdgeDirection, EdgePattern, Filter, GraphIR, MatchOp, NodePattern, PatternElement,
    Projection, Quantifier, QuerySource,
};
use crate::render::{QueryRenderer, RenderError, RenderedQuery};

// ============================================================================
// SQLite Schema Assumptions
// ============================================================================
//
// CREATE TABLE notes (
//     path TEXT PRIMARY KEY,
//     title TEXT,
//     content TEXT,
//     file_hash TEXT NOT NULL,
//     folder TEXT GENERATED ALWAYS AS (substr(path, 1, instr(path, '/') - 1))
// );
//
// CREATE TABLE edges (
//     source TEXT NOT NULL,
//     target TEXT NOT NULL,
//     type TEXT NOT NULL,
//     properties TEXT,  -- JSON
//     PRIMARY KEY (source, target, type)
// );
//
// CREATE TABLE blocks (
//     id TEXT PRIMARY KEY,
//     note_path TEXT NOT NULL,
//     block_type TEXT,
//     content TEXT,
//     content_hash TEXT
// );
//
// CREATE VIRTUAL TABLE embeddings USING vec0(
//     id TEXT PRIMARY KEY,
//     embedding FLOAT[384]
// );

pub struct SqliteRenderer {
    /// Table name for notes
    notes_table: String,
    /// Table name for edges
    edges_table: String,
}

impl Default for SqliteRenderer {
    fn default() -> Self {
        Self {
            notes_table: "notes".to_string(),
            edges_table: "edges".to_string(),
        }
    }
}

impl QueryRenderer for SqliteRenderer {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn render(&self, ir: &GraphIR) -> Result<RenderedQuery, RenderError> {
        // Check if we need recursive CTE (variable-length paths)
        let needs_recursion = ir.pattern.elements.iter().any(|e| {
            matches!(
                e,
                PatternElement::Edge(EdgePattern {
                    quantifier: Some(_),
                    ..
                })
            )
        });

        let sql = if needs_recursion {
            self.render_recursive(ir)?
        } else {
            self.render_simple(ir)?
        };

        Ok(RenderedQuery {
            query: sql,
            params: extract_params(ir),
        })
    }
}

impl SqliteRenderer {
    // ========================================================================
    // Simple queries (no variable-length paths)
    // ========================================================================

    fn render_simple(&self, ir: &GraphIR) -> Result<String, RenderError> {
        let mut sql = String::new();
        let mut joins = Vec::new();
        let mut conditions = Vec::new();

        // Track aliases for nodes
        let mut node_aliases: Vec<String> = Vec::new();
        let mut edge_idx = 0;

        // Process pattern elements
        for element in &ir.pattern.elements {
            match element {
                PatternElement::Node(node) => {
                    let alias = node
                        .alias
                        .clone()
                        .unwrap_or_else(|| format!("n{}", node_aliases.len()));
                    node_aliases.push(alias.clone());

                    // Add node conditions
                    for prop in &node.properties {
                        conditions.push(format!(
                            "{}.{} {} {}",
                            alias,
                            prop.key,
                            op_to_sql(&prop.op),
                            value_to_sql(&prop.value)
                        ));
                    }
                }
                PatternElement::Edge(edge) => {
                    let edge_alias = format!("e{}", edge_idx);
                    edge_idx += 1;

                    let prev_node = &node_aliases[node_aliases.len() - 1];
                    // Next node will be added in the next iteration

                    // Build JOIN based on direction
                    let join = match edge.direction {
                        EdgeDirection::Out => {
                            format!(
                                "JOIN {} {} ON {}.source = {}.path",
                                self.edges_table, edge_alias, edge_alias, prev_node
                            )
                        }
                        EdgeDirection::In => {
                            format!(
                                "JOIN {} {} ON {}.target = {}.path",
                                self.edges_table, edge_alias, edge_alias, prev_node
                            )
                        }
                        EdgeDirection::Both | EdgeDirection::Undirected => {
                            format!(
                                "JOIN {} {} ON ({}.source = {}.path OR {}.target = {}.path)",
                                self.edges_table, edge_alias, edge_alias, prev_node, edge_alias,
                                prev_node
                            )
                        }
                    };
                    joins.push(join);

                    // Edge type filter
                    if let Some(etype) = &edge.edge_type {
                        conditions.push(format!("{}.type = '{}'", edge_alias, etype));
                    }
                }
            }
        }

        // Build SELECT clause
        let select_fields = if ir.projections.is_empty() {
            node_aliases
                .first()
                .map(|a| format!("{}.*", a))
                .unwrap_or_else(|| "*".to_string())
        } else {
            ir.projections
                .iter()
                .map(|p| match &p.alias {
                    Some(alias) => format!("{} AS {}", p.field, alias),
                    None => p.field.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        // Build FROM clause with JOINs
        let from_clause = if node_aliases.is_empty() {
            self.notes_table.clone()
        } else {
            let base = format!(
                "{} {}",
                self.notes_table,
                node_aliases.first().unwrap()
            );
            if joins.is_empty() {
                base
            } else {
                // Need to join additional note tables for multi-node patterns
                let mut full_from = base;
                for (i, join) in joins.iter().enumerate() {
                    full_from.push_str(&format!("\n{}", join));
                    // Join the next node table
                    if i + 1 < node_aliases.len() {
                        let next_alias = &node_aliases[i + 1];
                        let edge_alias = format!("e{}", i);
                        // Connect edge to next node based on direction
                        full_from.push_str(&format!(
                            "\nJOIN {} {} ON {}.path = CASE \
                             WHEN {}.source = {}.path THEN {}.target \
                             ELSE {}.source END",
                            self.notes_table,
                            next_alias,
                            next_alias,
                            edge_alias,
                            node_aliases[i],
                            edge_alias,
                            edge_alias
                        ));
                    }
                }
                full_from
            }
        };

        // Add WHERE filters
        for filter in &ir.filters {
            conditions.push(format!(
                "{} {} {}",
                filter.field,
                op_to_sql(&filter.op),
                value_to_sql(&filter.value)
            ));
        }

        // Assemble query
        sql.push_str(&format!("SELECT {}\nFROM {}", select_fields, from_clause));

        if !conditions.is_empty() {
            sql.push_str(&format!("\nWHERE {}", conditions.join("\n  AND ")));
        }

        Ok(sql)
    }

    // ========================================================================
    // Recursive queries (variable-length paths)
    // ========================================================================

    fn render_recursive(&self, ir: &GraphIR) -> Result<String, RenderError> {
        // Extract the quantifier info
        let (min_depth, max_depth) = self.extract_path_bounds(ir);
        let edge_type = self.extract_edge_type(ir);
        let direction = self.extract_direction(ir);

        // Get source node path
        let source_path = match &ir.source {
            QuerySource::ByPath(p) => format!("'{}'", p),
            QuerySource::ByTitle(t) => {
                format!("(SELECT path FROM {} WHERE title = '{}')", self.notes_table, t)
            }
            _ => return Err(RenderError::MissingSource),
        };

        // Build recursive CTE
        let direction_condition = match direction {
            EdgeDirection::Out => "e.source = t.path",
            EdgeDirection::In => "e.target = t.path",
            EdgeDirection::Both | EdgeDirection::Undirected => {
                "(e.source = t.path OR e.target = t.path)"
            }
        };

        let next_node = match direction {
            EdgeDirection::Out => "e.target",
            EdgeDirection::In => "e.source",
            EdgeDirection::Both | EdgeDirection::Undirected => {
                "CASE WHEN e.source = t.path THEN e.target ELSE e.source END"
            }
        };

        let edge_filter = edge_type
            .map(|t| format!(" AND e.type = '{}'", t))
            .unwrap_or_default();

        let depth_check = max_depth
            .map(|max| format!(" AND t.depth < {}", max))
            .unwrap_or_default();

        let sql = format!(
            r#"WITH RECURSIVE traverse(path, depth, visited) AS (
    -- Base case: starting node
    SELECT {source_path}, 0, {source_path}

    UNION ALL

    -- Recursive case: follow edges
    SELECT
        {next_node},
        t.depth + 1,
        t.visited || ',' || {next_node}
    FROM traverse t
    JOIN {edges} e ON {direction_condition}{edge_filter}
    WHERE t.visited NOT LIKE '%' || {next_node} || '%'  -- cycle prevention
        {depth_check}
)
SELECT DISTINCT n.*
FROM {notes} n
JOIN traverse t ON n.path = t.path
WHERE t.depth >= {min_depth}
  AND t.path != {source_path}"#,
            source_path = source_path,
            next_node = next_node,
            edges = self.edges_table,
            notes = self.notes_table,
            direction_condition = direction_condition,
            edge_filter = edge_filter,
            depth_check = depth_check,
            min_depth = min_depth,
        );

        Ok(sql)
    }

    fn extract_path_bounds(&self, ir: &GraphIR) -> (usize, Option<usize>) {
        for element in &ir.pattern.elements {
            if let PatternElement::Edge(edge) = element {
                if let Some(q) = &edge.quantifier {
                    return match q {
                        Quantifier::ZeroOrMore => (0, None),
                        Quantifier::OneOrMore => (1, None),
                        Quantifier::Exactly(n) => (*n, Some(*n)),
                        Quantifier::Range { min, max } => (*min, *max),
                    };
                }
            }
        }
        (1, Some(1)) // Default: exactly one hop
    }

    fn extract_edge_type(&self, ir: &GraphIR) -> Option<&str> {
        for element in &ir.pattern.elements {
            if let PatternElement::Edge(edge) = element {
                return edge.edge_type.as_deref();
            }
        }
        None
    }

    fn extract_direction(&self, ir: &GraphIR) -> EdgeDirection {
        for element in &ir.pattern.elements {
            if let PatternElement::Edge(edge) = element {
                return edge.direction;
            }
        }
        EdgeDirection::Out
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn op_to_sql(op: &MatchOp) -> &'static str {
    match op {
        MatchOp::Eq => "=",
        MatchOp::Ne => "!=",
        MatchOp::Contains => "LIKE",
        MatchOp::StartsWith => "LIKE",
        MatchOp::EndsWith => "LIKE",
    }
}

fn value_to_sql(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => {
            if s.starts_with('$') {
                // Parameter placeholder - keep as-is for binding
                s.clone()
            } else {
                format!("'{}'", s.replace('\'', "''"))
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        serde_json::Value::Null => "NULL".to_string(),
        _ => "NULL".to_string(),
    }
}

fn extract_params(ir: &GraphIR) -> Vec<(String, serde_json::Value)> {
    let mut params = Vec::new();

    // Extract from node properties
    for element in &ir.pattern.elements {
        if let PatternElement::Node(node) = element {
            for prop in &node.properties {
                if let serde_json::Value::String(s) = &prop.value {
                    if s.starts_with('$') {
                        params.push((s[1..].to_string(), serde_json::Value::Null));
                    }
                }
            }
        }
    }

    // Extract from filters
    for filter in &ir.filters {
        if let serde_json::Value::String(s) = &filter.value {
            if s.starts_with('$') {
                params.push((s[1..].to_string(), serde_json::Value::Null));
            }
        }
    }

    params
}

// ============================================================================
// Example outputs
// ============================================================================

#[cfg(test)]
mod examples {
    // Input: MATCH (n:Note {path: 'foo.md'}) RETURN n
    // Output:
    // SELECT n.*
    // FROM notes n
    // WHERE n.path = 'foo.md'

    // Input: MATCH (a:Note {path: 'foo.md'})-[:LINKS_TO]->(b:Note) RETURN b.path
    // Output:
    // SELECT b.path
    // FROM notes a
    // JOIN edges e0 ON e0.source = a.path
    // JOIN notes b ON b.path = e0.target
    // WHERE a.path = 'foo.md'
    //   AND e0.type = 'LINKS_TO'

    // Input: MATCH (a:Note {path: 'foo.md'})-[:LINKS_TO*1..3]-(b) RETURN b
    // Output:
    // WITH RECURSIVE traverse(path, depth, visited) AS (
    //     SELECT 'foo.md', 0, 'foo.md'
    //     UNION ALL
    //     SELECT
    //         CASE WHEN e.source = t.path THEN e.target ELSE e.source END,
    //         t.depth + 1,
    //         t.visited || ',' || CASE WHEN e.source = t.path THEN e.target ELSE e.source END
    //     FROM traverse t
    //     JOIN edges e ON (e.source = t.path OR e.target = t.path) AND e.type = 'LINKS_TO'
    //     WHERE t.visited NOT LIKE '%' || CASE WHEN e.source = t.path THEN e.target ELSE e.source END || '%'
    //         AND t.depth < 3
    // )
    // SELECT DISTINCT n.*
    // FROM notes n
    // JOIN traverse t ON n.path = t.path
    // WHERE t.depth >= 1
    //   AND t.path != 'foo.md'
}

// ============================================================================
// Vector search helper (uses sqlite-vec)
// ============================================================================

impl SqliteRenderer {
    /// Render a vector similarity search
    pub fn render_vector_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        entity_type: Option<&str>,
    ) -> String {
        let type_filter = entity_type
            .map(|t| format!(" AND e.id LIKE '{}:%'", t))
            .unwrap_or_default();

        format!(
            r#"SELECT
    e.id,
    vec_distance_cosine(e.embedding, vec_f32('[{}]')) as distance
FROM embeddings e
WHERE e.embedding MATCH vec_f32('[{}]')
  AND k = {}{type_filter}
ORDER BY distance
LIMIT {}"#,
            query_embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(","),
            query_embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(","),
            limit,
            limit,
        )
    }
}
