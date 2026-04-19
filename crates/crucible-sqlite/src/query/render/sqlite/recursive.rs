use super::SqliteRenderer;
use crate::query::error::RenderError;
use crate::query::ir::{EdgeDirection, EdgePattern, GraphIR, PatternElement, QuerySource};
use serde_json::Value;
use std::collections::HashMap;

impl SqliteRenderer {
    /// Check if the pattern needs a recursive CTE (has variable-length paths)
    pub(super) fn needs_recursion(&self, ir: &GraphIR) -> bool {
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

    /// Render a recursive query (variable-length paths)
    pub(super) fn render_recursive(
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
            _ => String::new(),
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
}
