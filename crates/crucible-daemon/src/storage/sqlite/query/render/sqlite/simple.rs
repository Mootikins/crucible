use super::SqliteRenderer;
use crate::storage::sqlite::query::error::RenderError;
use crate::storage::sqlite::query::ir::{EdgeDirection, GraphIR, PatternElement, QuerySource};
use crate::storage::sqlite::query::render::RenderedQuery;
use serde_json::Value;
use std::collections::HashMap;

impl SqliteRenderer {
    /// Render a simple query (no variable-length paths)
    pub(super) fn render_simple(
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

                    // If no node has been seen yet, create an implicit source node
                    if node_aliases.is_empty() {
                        node_aliases.push("n0".to_string());
                    }
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

    pub(super) fn render_simple_lookup(
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
