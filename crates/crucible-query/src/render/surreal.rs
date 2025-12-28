//! SurrealQL renderer.
//!
//! Renders GraphIR to SurrealQL for execution against SurrealDB.

use crate::error::RenderError;
use crate::ir::{EdgeDirection, Filter, GraphIR, MatchOp, PatternElement, QuerySource};
use crate::render::{QueryRenderer, RenderedQuery};
use serde_json::Value;
use std::collections::HashMap;

/// SurrealQL renderer with configurable table names.
pub struct SurrealRenderer {
    /// Table name for entities (notes)
    pub entity_table: String,
    /// Table name for relations (wikilinks)
    pub relation_table: String,
}

impl Default for SurrealRenderer {
    fn default() -> Self {
        Self {
            entity_table: "entities".to_string(),
            relation_table: "relations".to_string(),
        }
    }
}

impl SurrealRenderer {
    /// Create renderer with custom table names
    pub fn with_tables(entity_table: impl Into<String>, relation_table: impl Into<String>) -> Self {
        Self {
            entity_table: entity_table.into(),
            relation_table: relation_table.into(),
        }
    }

    /// Render filters as SurrealQL WHERE clause conditions
    fn render_filters(&self, filters: &[Filter], params: &mut HashMap<String, Value>) -> String {
        if filters.is_empty() {
            return String::new();
        }

        let conditions: Vec<String> = filters
            .iter()
            .enumerate()
            .map(|(i, f)| self.render_filter(f, i, params))
            .collect();

        format!(" AND {}", conditions.join(" AND "))
    }

    /// Render a single filter as a SurrealQL condition
    fn render_filter(&self, filter: &Filter, index: usize, params: &mut HashMap<String, Value>) -> String {
        let param_name = format!("filter_{}", index);

        match &filter.op {
            MatchOp::Eq => {
                if filter.value == Value::Null {
                    format!("{} IS NOT NULL", filter.field)
                } else {
                    params.insert(param_name.clone(), filter.value.clone());
                    format!("{} = ${}", filter.field, param_name)
                }
            }
            MatchOp::Ne => {
                if filter.value == Value::Null {
                    format!("{} IS NOT NULL", filter.field)
                } else {
                    params.insert(param_name.clone(), filter.value.clone());
                    format!("{} != ${}", filter.field, param_name)
                }
            }
            MatchOp::Contains => {
                params.insert(param_name.clone(), filter.value.clone());
                // In SurrealDB, check if value is in array
                format!("${} IN {}", param_name, filter.field)
            }
            MatchOp::StartsWith => {
                if let Value::String(s) = &filter.value {
                    params.insert(param_name.clone(), Value::String(format!("{}%", s)));
                    format!("{} LIKE ${}", filter.field, param_name)
                } else {
                    "true".to_string() // Fallback
                }
            }
            MatchOp::EndsWith => {
                if let Value::String(s) = &filter.value {
                    params.insert(param_name.clone(), Value::String(format!("%{}", s)));
                    format!("{} LIKE ${}", filter.field, param_name)
                } else {
                    "true".to_string() // Fallback
                }
            }
        }
    }
}

impl QueryRenderer for SurrealRenderer {
    fn name(&self) -> &str {
        "surrealql"
    }

    fn render(&self, ir: &GraphIR) -> Result<RenderedQuery, RenderError> {
        let mut params = HashMap::new();

        // Handle simple find (no traversal)
        if ir.pattern.elements.is_empty() {
            return match &ir.source {
                QuerySource::ByTitle(title) => {
                    params.insert("title".to_string(), Value::String(title.clone()));
                    let filter_clause = self.render_filters(&ir.filters, &mut params);
                    Ok(RenderedQuery {
                        sql: format!(
                            "SELECT * FROM {} WHERE title = $title{} LIMIT 1",
                            self.entity_table, filter_clause
                        ),
                        params,
                    })
                }
                QuerySource::All => {
                    let filter_clause = self.render_filters(&ir.filters, &mut params);
                    let where_clause = if filter_clause.is_empty() {
                        String::new()
                    } else {
                        // Remove leading " AND " since there's no prior WHERE
                        format!(" WHERE {}", filter_clause.trim_start_matches(" AND "))
                    };
                    Ok(RenderedQuery {
                        sql: format!("SELECT * FROM {}{}", self.entity_table, where_clause),
                        params,
                    })
                }
                _ => Err(RenderError::UnsupportedPattern),
            };
        }

        // Handle single edge traversal from title source
        if let QuerySource::ByTitle(title) = &ir.source {
            params.insert("title".to_string(), Value::String(title.clone()));

            if ir.pattern.elements.len() == 1 {
                if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
                    let edge_type = edge.edge_type.as_deref().unwrap_or("wikilink");

                    let sql = match edge.direction {
                        EdgeDirection::Out => {
                            format!(
                                r#"SELECT out FROM {relations}
                                WHERE `in`.title = $title
                                AND relation_type = "{edge_type}"
                                FETCH out"#,
                                relations = self.relation_table,
                                edge_type = edge_type,
                            )
                        }
                        EdgeDirection::In => {
                            format!(
                                r#"SELECT `in` FROM {relations}
                                WHERE out.title = $title
                                AND relation_type = "{edge_type}"
                                FETCH `in`"#,
                                relations = self.relation_table,
                                edge_type = edge_type,
                            )
                        }
                        EdgeDirection::Both => {
                            format!(
                                r#"array::concat(
                                    (SELECT out FROM {relations} WHERE `in`.title = $title AND relation_type = "{edge_type}" FETCH out),
                                    (SELECT `in` FROM {relations} WHERE out.title = $title AND relation_type = "{edge_type}" FETCH `in`)
                                )"#,
                                relations = self.relation_table,
                                edge_type = edge_type,
                            )
                        }
                        EdgeDirection::Undirected => {
                            // Same as Both for now
                            format!(
                                r#"array::concat(
                                    (SELECT out FROM {relations} WHERE `in`.title = $title AND relation_type = "{edge_type}" FETCH out),
                                    (SELECT `in` FROM {relations} WHERE out.title = $title AND relation_type = "{edge_type}" FETCH `in`)
                                )"#,
                                relations = self.relation_table,
                                edge_type = edge_type,
                            )
                        }
                    };

                    return Ok(RenderedQuery { sql, params });
                }
            }
        }

        // TODO: Handle more complex patterns in Step 4
        Err(RenderError::UnsupportedPattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{EdgePattern, GraphPattern};

    #[test]
    fn test_render_find_by_title() {
        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("SELECT * FROM entities"));
        assert!(result.sql.contains("WHERE title = $title"));
        assert_eq!(
            result.params.get("title"),
            Some(&Value::String("Index".to_string()))
        );
    }

    #[test]
    fn test_render_outlinks() {
        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            pattern: GraphPattern {
                elements: vec![PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Out,
                    edge_type: Some("wikilink".to_string()),
                    ..Default::default()
                })],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("SELECT out FROM relations"));
        assert!(result.sql.contains("FETCH out"));
        assert!(result.sql.contains("$title"));
    }

    #[test]
    fn test_render_inlinks() {
        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Project".to_string()),
            pattern: GraphPattern {
                elements: vec![PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::In,
                    edge_type: Some("wikilink".to_string()),
                    ..Default::default()
                })],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("SELECT `in` FROM relations"));
        assert!(result.sql.contains("FETCH `in`"));
    }

    #[test]
    fn test_render_neighbors() {
        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Hub".to_string()),
            pattern: GraphPattern {
                elements: vec![PatternElement::Edge(EdgePattern {
                    direction: EdgeDirection::Both,
                    edge_type: Some("wikilink".to_string()),
                    ..Default::default()
                })],
            },
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("array::concat"));
    }

    #[test]
    fn test_custom_tables() {
        let renderer = SurrealRenderer::with_tables("notes", "wikilinks");
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("FROM notes"));
    }

    // =========================================================================
    // Filter rendering tests
    // =========================================================================

    #[test]
    fn test_render_with_equality_filter() {
        use crate::ir::Filter;

        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            filters: vec![Filter {
                field: "status".to_string(),
                op: MatchOp::Eq,
                value: Value::String("active".to_string()),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("status = $filter_0"));
        assert_eq!(
            result.params.get("filter_0"),
            Some(&Value::String("active".to_string()))
        );
    }

    #[test]
    fn test_render_with_contains_filter() {
        use crate::ir::Filter;

        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            filters: vec![Filter {
                field: "tags".to_string(),
                op: MatchOp::Contains,
                value: Value::String("project".to_string()),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("$filter_0 IN tags"));
    }

    #[test]
    fn test_render_with_startswith_filter() {
        use crate::ir::Filter;

        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            filters: vec![Filter {
                field: "title".to_string(),
                op: MatchOp::StartsWith,
                value: Value::String("Chapter".to_string()),
            }],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("title LIKE $filter_0"));
        assert_eq!(
            result.params.get("filter_0"),
            Some(&Value::String("Chapter%".to_string()))
        );
    }

    #[test]
    fn test_render_with_multiple_filters() {
        use crate::ir::Filter;

        let renderer = SurrealRenderer::default();
        let ir = GraphIR {
            source: QuerySource::ByTitle("Index".to_string()),
            filters: vec![
                Filter {
                    field: "status".to_string(),
                    op: MatchOp::Eq,
                    value: Value::String("active".to_string()),
                },
                Filter {
                    field: "tags".to_string(),
                    op: MatchOp::Contains,
                    value: Value::String("project".to_string()),
                },
            ],
            ..Default::default()
        };

        let result = renderer.render(&ir).unwrap();

        assert!(result.sql.contains("status = $filter_0"));
        assert!(result.sql.contains("$filter_1 IN tags"));
        assert!(result.sql.contains(" AND "));
    }
}
