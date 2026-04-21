use super::SqliteRenderer;
use crate::storage::sqlite::query::ir::Projection;

impl SqliteRenderer {
    pub(super) fn render_projection(&self, projection: &Projection) -> String {
        match &projection.alias {
            Some(alias) => format!("{} AS {}", projection.field, alias),
            _ => projection.field.clone(),
        }
    }

    pub(super) fn build_select_clause(
        &self,
        projections: &[Projection],
        node_aliases: &[String],
    ) -> String {
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

    pub(super) fn build_from_clause(&self, node_aliases: &[String], joins: &[String]) -> String {
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
