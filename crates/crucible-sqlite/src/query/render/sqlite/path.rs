use super::SqliteRenderer;
use crate::query::ir::{EdgeDirection, GraphIR, PatternElement, Quantifier};

impl SqliteRenderer {
    pub(super) fn extract_path_bounds(&self, ir: &GraphIR) -> (usize, Option<usize>) {
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

    pub(super) fn extract_edge_type<'a>(&self, ir: &'a GraphIR) -> Option<&'a str> {
        for element in &ir.pattern.elements {
            if let PatternElement::Edge(edge) = element {
                return edge.edge_type.as_deref();
            }
        }
        None
    }

    pub(super) fn extract_direction(&self, ir: &GraphIR) -> EdgeDirection {
        for element in &ir.pattern.elements {
            if let PatternElement::Edge(edge) = element {
                return edge.direction;
            }
        }
        EdgeDirection::Out
    }
}
