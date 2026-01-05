//! SQLite query renderer.
//!
//! Renders GraphIR to SQLite SQL with:
//! - Recursive CTEs for variable-length paths
//! - JOINs for fixed-length patterns
//! - Parameter binding for values

use crate::error::RenderError;
use crate::ir::{
    EdgeDirection, EdgePattern, Filter, GraphIR, MatchOp, PatternElement, Projection, Quantifier,
    QuerySource,
};
use crate::render::{QueryRenderer, RenderedQuery};
use serde_json::Value;
use std::collections::HashMap;

/// SQLite renderer with configurable table and column names.
///
/// Assumes the following schema by default:
/// ```sql
/// CREATE TABLE notes (
///     path TEXT PRIMARY KEY,
///     title TEXT,
///     content TEXT,
///     file_hash TEXT NOT NULL
/// );
///
/// CREATE TABLE edges (
///     source TEXT NOT NULL,
///     target TEXT NOT NULL,
///     type TEXT NOT NULL,
///     PRIMARY KEY (source, target, type)
/// );
/// ```
///
/// Column names can be customized via [`Self::with_schema`] or [`Self::for_crucible_eav`].
pub struct SqliteRenderer {
    /// Table name for notes
    pub notes_table: String,
    /// Table name for edges
    pub edges_table: String,
    /// Column name for source entity (default: "source")
    pub source_column: String,
    /// Column name for target entity (default: "target")
    pub target_column: String,
    /// Column name for edge type (default: "type")
    pub type_column: String,
}

impl Default for SqliteRenderer {
    fn default() -> Self {
        Self {
            notes_table: "notes".to_string(),
            edges_table: "edges".to_string(),
            source_column: "source".to_string(),
            target_column: "target".to_string(),
            type_column: "type".to_string(),
        }
    }
}

impl SqliteRenderer {
    /// Create renderer with custom table names (uses default column names)
    pub fn with_tables(notes: impl Into<String>, edges: impl Into<String>) -> Self {
        Self {
            notes_table: notes.into(),
            edges_table: edges.into(),
            source_column: "source".to_string(),
            target_column: "target".to_string(),
            type_column: "type".to_string(),
        }
    }

    /// Create renderer with custom table and column names
    pub fn with_schema(
        notes_table: impl Into<String>,
        edges_table: impl Into<String>,
        source_column: impl Into<String>,
        target_column: impl Into<String>,
        type_column: impl Into<String>,
    ) -> Self {
        Self {
            notes_table: notes_table.into(),
            edges_table: edges_table.into(),
            source_column: source_column.into(),
            target_column: target_column.into(),
            type_column: type_column.into(),
        }
    }

    /// Create renderer for Crucible's EAV schema
    pub fn for_crucible_eav() -> Self {
        Self::with_schema(
            "entities",
            "relations",
            "from_entity_id",
            "to_entity_id",
            "relation_type",
        )
    }

    /// Check if the pattern needs a recursive CTE (has variable-length paths)
    fn needs_recursion(&self, ir: &GraphIR) -> bool {
        ir.pattern.elements.iter().any(|e| {
            matches!(
                e,
                PatternElement::Edge(EdgePattern {
                    quantifier: Some(_),
                    ..
                })
            )
        })
    }

    /// Render a simple query (no variable-length paths)
    fn render_simple(
        &self,
        ir: &GraphIR,
        params: &mut HashMap<String, Value>,
    ) -> Result<String, RenderError> {
        let mut joins = Vec::new();
        let mut conditions = Vec::new();
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

                    // Add node property conditions
                    for prop in &node.properties {
                        let param_name = format!("prop_{}_{}", alias, prop.key);
                        params.insert(param_name.clone(), prop.value.clone());
                        conditions.push(format!("{}.{} = :{}", alias, prop.key, param_name));
                    }
                }
                PatternElement::Edge(edge) => {
                    let edge_alias = format!("e{}", edge_idx);

                    let prev_node = &node_aliases[node_aliases.len() - 1];

                    // Build JOIN based on direction
                    let join = match edge.direction {
                        EdgeDirection::Out => {
                            format!(
                                "JOIN {} {} ON {}.{} = {}.path",
                                self.edges_table,
                                edge_alias,
                                edge_alias,
                                self.source_column,
                                prev_node
                            )
                        }
                        EdgeDirection::In => {
                            format!(
                                "JOIN {} {} ON {}.{} = {}.path",
                                self.edges_table,
                                edge_alias,
                                edge_alias,
                                self.target_column,
                                prev_node
                            )
                        }
                        EdgeDirection::Both | EdgeDirection::Undirected => {
                            format!(
                                "JOIN {} {} ON ({}.{} = {}.path OR {}.{} = {}.path)",
                                self.edges_table,
                                edge_alias,
                                edge_alias,
                                self.source_column,
                                prev_node,
                                edge_alias,
                                self.target_column,
                                prev_node
                            )
                        }
                    };
                    joins.push(join);

                    // Edge type filter - parameterized to prevent SQL injection
                    if let Some(etype) = &edge.edge_type {
                        let param_name = format!("edge_type_{}", edge_idx);
                        params.insert(param_name.clone(), Value::String(etype.clone()));
                        conditions.push(format!(
                            "{}.{} = :{}",
                            edge_alias, self.type_column, param_name
                        ));
                    }

                    edge_idx += 1;
                }
            }
        }

        // Handle source condition
        match &ir.source {
            QuerySource::ByTitle(title) => {
                params.insert("source_title".to_string(), Value::String(title.clone()));
                if let Some(first_alias) = node_aliases.first() {
                    conditions.push(format!("{}.title = :source_title", first_alias));
                }
            }
            QuerySource::ByPath(path) => {
                params.insert("source_path".to_string(), Value::String(path.clone()));
                if let Some(first_alias) = node_aliases.first() {
                    conditions.push(format!("{}.path = :source_path", first_alias));
                }
            }
            QuerySource::ById(id) => {
                params.insert("source_id".to_string(), Value::String(id.clone()));
                if let Some(first_alias) = node_aliases.first() {
                    conditions.push(format!("{}.path = :source_id", first_alias));
                }
            }
            QuerySource::All => {}
        }

        // Add IR filters
        for (i, filter) in ir.filters.iter().enumerate() {
            conditions.push(self.render_filter(filter, i, params)?);
        }

        // Build SELECT clause
        let select_fields = self.build_select_clause(&ir.projections, &node_aliases);

        // Build FROM clause with JOINs
        let from_clause = self.build_from_clause(&node_aliases, &joins);

        // Assemble query
        let mut sql = format!("SELECT {}\nFROM {}", select_fields, from_clause);

        if !conditions.is_empty() {
            sql.push_str(&format!("\nWHERE {}", conditions.join("\n  AND ")));
        }

        Ok(sql)
    }

    /// Render a recursive query (variable-length paths)
    fn render_recursive(
        &self,
        ir: &GraphIR,
        params: &mut HashMap<String, Value>,
    ) -> Result<String, RenderError> {
        // Extract path bounds and edge info
        let (min_depth, max_depth) = self.extract_path_bounds(ir);
        let edge_type = self.extract_edge_type(ir);
        let direction = self.extract_direction(ir);

        // Get source
        let source_path = match &ir.source {
            QuerySource::ByPath(p) => {
                params.insert("source".to_string(), Value::String(p.clone()));
                ":source".to_string()
            }
            QuerySource::ByTitle(t) => {
                params.insert("source".to_string(), Value::String(t.clone()));
                format!(
                    "(SELECT path FROM {} WHERE title = :source)",
                    self.notes_table
                )
            }
            _ => return Err(RenderError::MissingSource),
        };

        // Direction-specific join conditions
        let direction_condition = match direction {
            EdgeDirection::Out => format!("e.{} = t.path", self.source_column),
            EdgeDirection::In => format!("e.{} = t.path", self.target_column),
            EdgeDirection::Both | EdgeDirection::Undirected => {
                format!(
                    "(e.{} = t.path OR e.{} = t.path)",
                    self.source_column, self.target_column
                )
            }
        };

        // Next node expression
        let next_node = match direction {
            EdgeDirection::Out => format!("e.{}", self.target_column),
            EdgeDirection::In => format!("e.{}", self.source_column),
            EdgeDirection::Both | EdgeDirection::Undirected => {
                format!(
                    "CASE WHEN e.{} = t.path THEN e.{} ELSE e.{} END",
                    self.source_column, self.target_column, self.source_column
                )
            }
        };

        // Edge type filter - parameterized to prevent SQL injection
        let edge_filter = match edge_type {
            Some(t) => {
                params.insert("edge_type".to_string(), Value::String(t.to_string()));
                format!(" AND e.{} = :edge_type", self.type_column)
            }
            None => String::new(),
        };

        // Depth limit
        let depth_check = max_depth
            .map(|max| format!(" AND t.depth < {}", max))
            .unwrap_or_default();

        // Build projections for final SELECT
        let node_alias = ir.pattern.elements.iter().find_map(|e| {
            if let PatternElement::Node(n) = e {
                if n.alias.is_some() {
                    return n.alias.clone();
                }
            }
            None
        });
        let target_alias = node_alias.unwrap_or_else(|| "n".to_string());

        let select_fields = if ir.projections.is_empty() {
            format!("{}.*", target_alias)
        } else {
            ir.projections
                .iter()
                .map(|p| self.render_projection(p))
                .collect::<Vec<_>>()
                .join(", ")
        };

        // For ZeroOrMore (min_depth=0), include source as valid 0-hop match
        // For min_depth > 0, exclude source since we want actual traversals
        let exclude_source = if min_depth > 0 {
            format!("\n  AND t.path != {}", source_path)
        } else {
            String::new()
        };

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
    WHERE instr(',' || t.visited || ',', ',' || {next_node} || ',') = 0  -- cycle prevention
        {depth_check}
)
SELECT DISTINCT {select_fields}
FROM {notes} {target_alias}
JOIN traverse t ON {target_alias}.path = t.path
WHERE t.depth >= {min_depth}{exclude_source}"#,
            source_path = source_path,
            next_node = next_node,
            edges = self.edges_table,
            notes = self.notes_table,
            direction_condition = direction_condition,
            edge_filter = edge_filter,
            depth_check = depth_check,
            min_depth = min_depth,
            exclude_source = exclude_source,
            target_alias = target_alias,
            select_fields = select_fields,
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
        (1, Some(1))
    }

    fn extract_edge_type<'a>(&self, ir: &'a GraphIR) -> Option<&'a str> {
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

    /// Escape SQL LIKE metacharacters (%, _) in a pattern
    fn escape_like_pattern(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    }

    fn render_filter(
        &self,
        filter: &Filter,
        index: usize,
        params: &mut HashMap<String, Value>,
    ) -> Result<String, RenderError> {
        let param_name = format!("filter_{}", index);

        match &filter.op {
            MatchOp::Eq => {
                if filter.value == Value::Null {
                    Ok(format!("{} IS NULL", filter.field))
                } else {
                    params.insert(param_name.clone(), filter.value.clone());
                    Ok(format!("{} = :{}", filter.field, param_name))
                }
            }
            MatchOp::Ne => {
                if filter.value == Value::Null {
                    Ok(format!("{} IS NOT NULL", filter.field))
                } else {
                    params.insert(param_name.clone(), filter.value.clone());
                    Ok(format!("{} != :{}", filter.field, param_name))
                }
            }
            MatchOp::Contains => {
                if let Value::String(s) = &filter.value {
                    let escaped = Self::escape_like_pattern(s);
                    params.insert(param_name.clone(), Value::String(format!("%{}%", escaped)));
                    Ok(format!(
                        "{} LIKE :{} ESCAPE '\\\\'",
                        filter.field, param_name
                    ))
                } else {
                    Err(RenderError::UnsupportedFilter {
                        message: format!("CONTAINS requires string value, got {:?}", filter.value),
                    })
                }
            }
            MatchOp::StartsWith => {
                if let Value::String(s) = &filter.value {
                    let escaped = Self::escape_like_pattern(s);
                    params.insert(param_name.clone(), Value::String(format!("{}%", escaped)));
                    Ok(format!(
                        "{} LIKE :{} ESCAPE '\\\\'",
                        filter.field, param_name
                    ))
                } else {
                    Err(RenderError::UnsupportedFilter {
                        message: format!(
                            "STARTS WITH requires string value, got {:?}",
                            filter.value
                        ),
                    })
                }
            }
            MatchOp::EndsWith => {
                if let Value::String(s) = &filter.value {
                    let escaped = Self::escape_like_pattern(s);
                    params.insert(param_name.clone(), Value::String(format!("%{}", escaped)));
                    Ok(format!(
                        "{} LIKE :{} ESCAPE '\\\\'",
                        filter.field, param_name
                    ))
                } else {
                    Err(RenderError::UnsupportedFilter {
                        message: format!("ENDS WITH requires string value, got {:?}", filter.value),
                    })
                }
            }
        }
    }

    fn render_projection(&self, projection: &Projection) -> String {
        match &projection.alias {
            Some(alias) => format!("{} AS {}", projection.field, alias),
            None => projection.field.clone(),
        }
    }

    fn build_select_clause(&self, projections: &[Projection], node_aliases: &[String]) -> String {
        if projections.is_empty() {
            node_aliases
                .first()
                .map(|a| format!("{}.*", a))
                .unwrap_or_else(|| "*".to_string())
        } else {
            projections
                .iter()
                .map(|p| self.render_projection(p))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn build_from_clause(&self, node_aliases: &[String], joins: &[String]) -> String {
        if node_aliases.is_empty() {
            self.notes_table.clone()
        } else {
            let base = format!("{} {}", self.notes_table, node_aliases.first().unwrap());
            if joins.is_empty() {
                base
            } else {
                let mut full_from = base;

                for (i, join) in joins.iter().enumerate() {
                    full_from.push_str(&format!("\n{}", join));

                    // Join the next node table
                    if i + 1 < node_aliases.len() {
                        let next_alias = &node_aliases[i + 1];
                        let edge_alias = format!("e{}", i);
                        full_from.push_str(&format!(
                            "\nJOIN {} {} ON {}.path = CASE \
                             WHEN {}.{} = {}.path THEN {}.{} \
                             ELSE {}.{} END",
                            self.notes_table,
                            next_alias,
                            next_alias,
                            edge_alias,
                            self.source_column,
                            node_aliases[i],
                            edge_alias,
                            self.target_column,
                            edge_alias,
                            self.source_column
                        ));
                    }
                }
                full_from
            }
        }
    }
}

impl QueryRenderer for SqliteRenderer {
    fn name(&self) -> &str {
        "sqlite"
    }

    fn render(&self, ir: &GraphIR) -> Result<RenderedQuery, RenderError> {
        let mut params = HashMap::new();

        // Handle empty pattern (simple lookup)
        if ir.pattern.elements.is_empty() {
            return self.render_simple_lookup(ir, &mut params);
        }

        // Check if we need recursive CTE
        let sql = if self.needs_recursion(ir) {
            self.render_recursive(ir, &mut params)?
        } else {
            self.render_simple(ir, &mut params)?
        };

        Ok(RenderedQuery { sql, params })
    }
}

impl SqliteRenderer {
    fn render_simple_lookup(
        &self,
        ir: &GraphIR,
        params: &mut HashMap<String, Value>,
    ) -> Result<RenderedQuery, RenderError> {
        let mut conditions = Vec::new();

        match &ir.source {
            QuerySource::ByTitle(title) => {
                params.insert("title".to_string(), Value::String(title.clone()));
                conditions.push("title = :title".to_string());
            }
            QuerySource::ByPath(path) => {
                params.insert("path".to_string(), Value::String(path.clone()));
                conditions.push("path = :path".to_string());
            }
            QuerySource::ById(id) => {
                params.insert("id".to_string(), Value::String(id.clone()));
                conditions.push("path = :id".to_string());
            }
            QuerySource::All => {}
        }

        // Add IR filters
        for (i, filter) in ir.filters.iter().enumerate() {
            conditions.push(self.render_filter(filter, i, params)?);
        }

        // Build SELECT fields
        let select_fields = if ir.projections.is_empty() {
            "*".to_string()
        } else {
            ir.projections
                .iter()
                .map(|p| self.render_projection(p))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let sql = if conditions.is_empty() {
            format!("SELECT {} FROM {}", select_fields, self.notes_table)
        } else {
            format!(
                "SELECT {} FROM {}\nWHERE {}",
                select_fields,
                self.notes_table,
                conditions.join("\n  AND ")
            )
        };

        Ok(RenderedQuery {
            sql,
            params: params.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{GraphPattern, NodePattern};

    // =========================================================================
    // Simple lookup tests
    // =========================================================================

    #[test]
    fn test_render_select_all() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR::default();

        let result = renderer.render(&ir).unwrap();

        assert_eq!(result.sql, "SELECT * FROM notes");
    }

    #[test]
    fn test_render_find_by_title() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("WHERE title = :title"));
        assert_eq!(
            result.params.get("title"),
            Some(&Value::String("Index".to_string()))
        );
    }

    #[test]
    fn test_render_find_by_path() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByPath("notes/index.md".to_string()),
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("WHERE path = :path"));
    }

    // =========================================================================
    // Simple edge pattern tests
    // =========================================================================

    #[test]
    fn test_render_outlinks() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern {
                        alias: Some("a".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern {
                        alias: Some("b".to_string()),
                        ..Default::default()
                    }),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("SELECT a.*"));
        assert!(result.sql.contains("JOIN edges e0 ON e0.source = a.path"));
        assert!(result.sql.contains("e0.type = :edge_type_0"));
        assert_eq!(
            result.params.get("edge_type_0"),
            Some(&Value::String("wikilink".to_string()))
        );
    }

    #[test]
    fn test_render_inlinks() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Project".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern {
                        alias: Some("a".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::In,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern {
                        alias: Some("b".to_string()),
                        ..Default::default()
                    }),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("JOIN edges e0 ON e0.target = a.path"));
    }

    #[test]
    fn test_render_bidirectional() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Hub".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern {
                        alias: Some("a".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Both,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern {
                        alias: Some("b".to_string()),
                        ..Default::default()
                    }),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result
            .sql
            .contains("e0.source = a.path OR e0.target = a.path"));
    }

    // =========================================================================
    // Recursive query tests
    // =========================================================================

    #[test]
    fn test_render_variable_length_path() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByPath("index.md".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern {
                        alias: Some("a".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        edge_type: Some("LINKS_TO".to_string()),
                        quantifier: Some(Quantifier::Range {
                            min: 1,
                            max: Some(3),
                        }),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern {
                        alias: Some("b".to_string()),
                        ..Default::default()
                    }),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("WITH RECURSIVE traverse"));
        assert!(result.sql.contains("t.depth < 3"));
        assert!(result.sql.contains("t.depth >= 1"));
        assert!(result.sql.contains("e.type = :edge_type"));
        assert_eq!(
            result.params.get("edge_type"),
            Some(&Value::String("LINKS_TO".to_string()))
        );
    }

    #[test]
    fn test_render_star_quantifier() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByPath("index.md".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern::default()),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        quantifier: Some(Quantifier::ZeroOrMore),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern::default()),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("WITH RECURSIVE traverse"));
        assert!(result.sql.contains("t.depth >= 0"));
        // ZeroOrMore includes source as valid 0-hop match
        assert!(
            !result.sql.contains("t.path != "),
            "ZeroOrMore should not exclude source node"
        );
    }

    #[test]
    fn test_render_plus_quantifier() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByPath("index.md".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern::default()),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        quantifier: Some(Quantifier::OneOrMore),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern::default()),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("WITH RECURSIVE traverse"));
        assert!(result.sql.contains("t.depth >= 1"));
        // OneOrMore excludes source since we want actual traversals
        assert!(
            result.sql.contains("t.path != "),
            "OneOrMore should exclude source node"
        );
    }

    #[test]
    fn test_recursive_requires_source() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::All, // No explicit source
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern::default()),
                    PatternElement::Edge(EdgePattern {
                        quantifier: Some(Quantifier::ZeroOrMore),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern::default()),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir);

        assert!(matches!(result, Err(RenderError::MissingSource)));
    }

    // =========================================================================
    // Filter tests
    // =========================================================================

    #[test]
    fn test_render_with_filter() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::All,
            filters: vec![Filter {
                field: "folder".to_string(),
                op: MatchOp::Eq,
                value: Value::String("Projects".to_string()),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("folder = :filter_0"));
        assert_eq!(
            result.params.get("filter_0"),
            Some(&Value::String("Projects".to_string()))
        );
    }

    #[test]
    fn test_render_with_contains_filter() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::All,
            filters: vec![Filter {
                field: "title".to_string(),
                op: MatchOp::Contains,
                value: Value::String("API".to_string()),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("title LIKE :filter_0"));
        assert_eq!(
            result.params.get("filter_0"),
            Some(&Value::String("%API%".to_string()))
        );
    }

    #[test]
    fn test_render_with_starts_with_filter() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::All,
            filters: vec![Filter {
                field: "path".to_string(),
                op: MatchOp::StartsWith,
                value: Value::String("docs/".to_string()),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("path LIKE :filter_0"));
        assert_eq!(
            result.params.get("filter_0"),
            Some(&Value::String("docs/%".to_string()))
        );
    }

    #[test]
    fn test_render_contains_with_non_string_fails() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::All,
            filters: vec![Filter {
                field: "count".to_string(),
                op: MatchOp::Contains,
                value: Value::Number(42.into()),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir);
        assert!(matches!(result, Err(RenderError::UnsupportedFilter { .. })));
    }

    #[test]
    fn test_render_starts_with_non_string_fails() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::All,
            filters: vec![Filter {
                field: "status".to_string(),
                op: MatchOp::StartsWith,
                value: Value::Bool(true),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir);
        assert!(matches!(result, Err(RenderError::UnsupportedFilter { .. })));
    }

    // =========================================================================
    // Projection tests
    // =========================================================================

    #[test]
    fn test_render_with_projections() {
        let renderer = SqliteRenderer::default();
        let ir = GraphIR {
            source: QuerySource::All,
            projections: vec![
                Projection {
                    field: "path".to_string(),
                    alias: None,
                },
                Projection {
                    field: "title".to_string(),
                    alias: Some("name".to_string()),
                },
            ],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("SELECT path, title AS name"));
    }

    // =========================================================================
    // Custom table names
    // =========================================================================

    #[test]
    fn test_custom_tables() {
        let renderer = SqliteRenderer::with_tables("documents", "links");
        let ir = GraphIR::default();

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("FROM documents"));
    }

    // =========================================================================
    // Custom column names
    // =========================================================================

    #[test]
    fn test_crucible_eav_schema() {
        let renderer = SqliteRenderer::for_crucible_eav();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern {
                        alias: Some("a".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        edge_type: Some("wikilink".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern {
                        alias: Some("b".to_string()),
                        ..Default::default()
                    }),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("FROM entities"));
        assert!(result.sql.contains("JOIN relations"));
        assert!(result.sql.contains("from_entity_id"));
        assert!(result.sql.contains("relation_type"));
    }

    #[test]
    fn test_custom_columns_with_schema() {
        let renderer = SqliteRenderer::with_schema("nodes", "edges", "src", "dst", "edge_kind");
        let ir = GraphIR {
            source: QuerySource::ByTitle("Test".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern {
                        alias: Some("a".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        edge_type: Some("link".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern {
                        alias: Some("b".to_string()),
                        ..Default::default()
                    }),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("FROM nodes"));
        assert!(result.sql.contains("JOIN edges"));
        assert!(result.sql.contains("e0.src"));
        assert!(result.sql.contains("e0.edge_kind"));
    }

    #[test]
    fn test_crucible_eav_recursive() {
        let renderer = SqliteRenderer::for_crucible_eav();
        let ir = GraphIR {
            source: QuerySource::ByPath("index.md".to_string()),
            pattern: GraphPattern {
                elements: vec![
                    PatternElement::Node(NodePattern {
                        alias: Some("a".to_string()),
                        ..Default::default()
                    }),
                    PatternElement::Edge(EdgePattern {
                        direction: EdgeDirection::Out,
                        edge_type: Some("wikilink".to_string()),
                        quantifier: Some(Quantifier::OneOrMore),
                        ..Default::default()
                    }),
                    PatternElement::Node(NodePattern {
                        alias: Some("b".to_string()),
                        ..Default::default()
                    }),
                ],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        // Verify recursive CTE uses custom column names
        assert!(result.sql.contains("WITH RECURSIVE traverse"));
        assert!(result.sql.contains("JOIN relations"));
        assert!(result.sql.contains("e.from_entity_id"));
        assert!(result.sql.contains("e.to_entity_id"));
        assert!(result.sql.contains("e.relation_type"));
    }
}
