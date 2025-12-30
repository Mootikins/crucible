//! Intermediate representation for graph queries.
//!
//! GraphIR is backend-agnostic and can be rendered to different targets
//! (SurrealQL, DuckDB SQL, etc.).

use serde::{Deserialize, Serialize};

/// Intermediate representation for graph queries.
///
/// Designed to be backend-agnostic (SurrealDB, DuckDB, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphIR {
    /// Starting point for the query
    pub source: QuerySource,

    /// Graph traversal pattern (nodes and edges)
    pub pattern: GraphPattern,

    /// What to return (projections)
    pub projections: Vec<Projection>,

    /// Post-traversal filters
    pub filters: Vec<Filter>,

    /// Remaining unparsed filter (jaq expression)
    pub post_filter: Option<String>,
}

impl Default for GraphIR {
    fn default() -> Self {
        Self {
            source: QuerySource::All,
            pattern: GraphPattern::default(),
            projections: Vec::new(),
            filters: Vec::new(),
            post_filter: None,
        }
    }
}

/// How to find the starting point
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QuerySource {
    /// Find entity by title
    ByTitle(String),
    /// Find entity by path
    ByPath(String),
    /// Find entity by ID
    ById(String),
    /// All entities (SELECT * FROM entities)
    All,
}

/// Graph traversal pattern
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphPattern {
    /// Sequence of nodes and edges in the pattern
    pub elements: Vec<PatternElement>,
}

/// Element in a graph pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternElement {
    /// A node in the pattern
    Node(NodePattern),
    /// An edge connecting nodes
    Edge(EdgePattern),
}

/// Node pattern: (alias:Label {prop: 'value'})
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodePattern {
    /// Optional alias for referencing in projections/filters
    pub alias: Option<String>,
    /// Node label (e.g., :Note, :Tag)
    pub label: Option<String>,
    /// Property constraints
    pub properties: Vec<PropertyMatch>,
}

/// Property match constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyMatch {
    /// Property name
    pub key: String,
    /// Comparison operator
    pub op: MatchOp,
    /// Value to match against
    pub value: serde_json::Value,
}

/// Match operators for property constraints
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchOp {
    /// Equals (=)
    Eq,
    /// Not equals (!=)
    Ne,
    /// Contains (for arrays/strings)
    Contains,
    /// Starts with (for strings)
    StartsWith,
    /// Ends with (for strings)
    EndsWith,
}

/// Edge pattern: -[:type]-> or <-[:type]- or -[:type]-
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EdgePattern {
    /// Optional alias for referencing
    pub alias: Option<String>,
    /// Edge type (e.g., wikilink, embed)
    pub edge_type: Option<String>,
    /// Direction of traversal
    pub direction: EdgeDirection,
    /// Quantifier for path patterns
    pub quantifier: Option<Quantifier>,
}

/// Direction of edge traversal
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeDirection {
    /// Outgoing: ->
    #[default]
    Out,
    /// Incoming: <-
    In,
    /// Both directions: <->
    Both,
    /// Undirected: -
    Undirected,
}

/// Quantifier for path patterns
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Quantifier {
    /// Zero or more: *
    ZeroOrMore,
    /// One or more: +
    OneOrMore,
    /// Exact count: {n}
    Exactly(usize),
    /// Range: {min, max}
    Range {
        /// Minimum path length
        min: usize,
        /// Maximum path length (None = unbounded)
        max: Option<usize>,
    },
}

/// Projection for what to return
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projection {
    /// Field or expression to project
    pub field: String,
    /// Optional alias
    pub alias: Option<String>,
}

/// Post-traversal filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// Field path to filter on
    pub field: String,
    /// Comparison operator
    pub op: MatchOp,
    /// Value to compare against
    pub value: serde_json::Value,
}

// ============================================================================
// Builder helpers
// ============================================================================

impl GraphIR {
    /// Create a new GraphIR with a title source
    pub fn find_by_title(title: impl Into<String>) -> Self {
        Self {
            source: QuerySource::ByTitle(title.into()),
            ..Default::default()
        }
    }

    /// Add an outlinks traversal
    pub fn outlinks(mut self, edge_type: impl Into<String>) -> Self {
        self.pattern
            .elements
            .push(PatternElement::Edge(EdgePattern {
                direction: EdgeDirection::Out,
                edge_type: Some(edge_type.into()),
                ..Default::default()
            }));
        self
    }

    /// Add an inlinks traversal
    pub fn inlinks(mut self, edge_type: impl Into<String>) -> Self {
        self.pattern
            .elements
            .push(PatternElement::Edge(EdgePattern {
                direction: EdgeDirection::In,
                edge_type: Some(edge_type.into()),
                ..Default::default()
            }));
        self
    }

    /// Add a bidirectional traversal (neighbors)
    pub fn neighbors(mut self, edge_type: impl Into<String>) -> Self {
        self.pattern
            .elements
            .push(PatternElement::Edge(EdgePattern {
                direction: EdgeDirection::Both,
                edge_type: Some(edge_type.into()),
                ..Default::default()
            }));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_ir_builder() {
        let ir = GraphIR::find_by_title("Index").outlinks("wikilink");

        assert_eq!(ir.source, QuerySource::ByTitle("Index".to_string()));
        assert_eq!(ir.pattern.elements.len(), 1);

        if let PatternElement::Edge(edge) = &ir.pattern.elements[0] {
            assert_eq!(edge.direction, EdgeDirection::Out);
            assert_eq!(edge.edge_type, Some("wikilink".to_string()));
        } else {
            panic!("Expected edge pattern");
        }
    }

    #[test]
    fn test_serialize_graph_ir() {
        let ir = GraphIR::find_by_title("Test");
        let json = serde_json::to_string(&ir).unwrap();
        let parsed: GraphIR = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.source, QuerySource::ByTitle("Test".to_string()));
    }
}
