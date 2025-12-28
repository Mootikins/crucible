//! Dependency Graph for Handler Ordering
//!
//! This module provides dependency graph construction and topological sorting
//! for event handlers. Handlers declare dependencies via `dependencies()`, and
//! this module ensures they execute in the correct order.
//!
//! ## Design
//!
//! The dependency graph:
//! - Validates handler dependencies at registration time
//! - Detects cycles (returns error instead of infinite loop)
//! - Computes stable topological order
//! - Supports dynamic handler registration/removal
//!
//! ## Usage
//!
//! ```rust
//! use crucible_core::events::dependency::DependencyGraph;
//!
//! let mut graph = DependencyGraph::new();
//! graph.add("persist", vec![]).unwrap();
//! graph.add("react", vec!["persist".to_string()]).unwrap();
//! graph.add("emit", vec!["react".to_string()]).unwrap();
//!
//! let order = graph.execution_order().unwrap();
//! assert_eq!(order, vec!["persist", "react", "emit"]);
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

/// Error types for dependency graph operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DependencyError {
    /// A cycle was detected in the dependency graph.
    #[error("Dependency cycle detected: {}", cycle.join(" -> "))]
    CycleDetected {
        /// Handlers involved in the cycle
        cycle: Vec<String>,
    },

    /// A handler declares a dependency that doesn't exist.
    #[error("Handler '{handler}' depends on unknown handler '{dependency}'")]
    UnknownDependency { handler: String, dependency: String },

    /// A handler with the same name already exists.
    #[error("Handler '{0}' already registered")]
    DuplicateHandler(String),

    /// Handler not found.
    #[error("Handler '{0}' not found")]
    HandlerNotFound(String),
}

/// Result type for dependency graph operations.
pub type DependencyResult<T> = Result<T, DependencyError>;

/// Node in the dependency graph.
#[derive(Debug, Clone)]
pub struct GraphNode {
    /// Handler name
    pub name: String,
    /// Names of handlers this one depends on
    pub depends_on: Vec<String>,
    /// Priority (for ordering within same dependency level)
    pub priority: i32,
}

impl GraphNode {
    /// Create a new graph node.
    pub fn new(name: impl Into<String>, depends_on: Vec<String>) -> Self {
        Self {
            name: name.into(),
            depends_on,
            priority: 50,
        }
    }

    /// Create a new graph node with priority.
    pub fn with_priority(name: impl Into<String>, depends_on: Vec<String>, priority: i32) -> Self {
        Self {
            name: name.into(),
            depends_on,
            priority,
        }
    }
}

/// Dependency graph for handler ordering.
///
/// Stores handler dependency relationships and computes topological order
/// for execution. Thread-safe for read operations; mutation requires
/// exclusive access.
///
/// ## Priority as Tiebreaker
///
/// When handlers have no dependency relationship, they are ordered by priority
/// (lower values run first). This provides deterministic ordering even for
/// independent handlers.
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// Nodes indexed by handler name
    nodes: HashMap<String, GraphNode>,
    /// Cached topological order (invalidated on mutation)
    cached_order: Option<Vec<String>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a handler to the graph.
    ///
    /// Does NOT validate dependencies yet - call `validate()` or
    /// `execution_order()` after adding all handlers.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        depends_on: Vec<String>,
    ) -> DependencyResult<()> {
        self.add_with_priority(name, depends_on, 50)
    }

    /// Add a handler with a specific priority.
    ///
    /// Priority is used as a tiebreaker when handlers have no dependency
    /// relationship. Lower values run first.
    pub fn add_with_priority(
        &mut self,
        name: impl Into<String>,
        depends_on: Vec<String>,
        priority: i32,
    ) -> DependencyResult<()> {
        let name = name.into();

        if self.nodes.contains_key(&name) {
            return Err(DependencyError::DuplicateHandler(name));
        }

        self.nodes.insert(
            name.clone(),
            GraphNode::with_priority(name, depends_on, priority),
        );
        self.cached_order = None; // Invalidate cache

        Ok(())
    }

    /// Remove a handler from the graph.
    ///
    /// Returns `Err` if handler doesn't exist.
    pub fn remove(&mut self, name: &str) -> DependencyResult<GraphNode> {
        self.cached_order = None; // Invalidate cache

        self.nodes
            .remove(name)
            .ok_or_else(|| DependencyError::HandlerNotFound(name.to_string()))
    }

    /// Check if a handler exists in the graph.
    pub fn contains(&self, name: &str) -> bool {
        self.nodes.contains_key(name)
    }

    /// Get a handler node by name.
    pub fn get(&self, name: &str) -> Option<&GraphNode> {
        self.nodes.get(name)
    }

    /// Get all handler names.
    pub fn handler_names(&self) -> impl Iterator<Item = &str> {
        self.nodes.keys().map(|s| s.as_str())
    }

    /// Get the number of handlers in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Validate that all dependencies exist.
    ///
    /// Returns `Err` if any handler depends on an unknown handler.
    pub fn validate_dependencies(&self) -> DependencyResult<()> {
        for node in self.nodes.values() {
            for dep in &node.depends_on {
                if !self.nodes.contains_key(dep) {
                    return Err(DependencyError::UnknownDependency {
                        handler: node.name.clone(),
                        dependency: dep.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Compute the topological execution order.
    ///
    /// Returns handler names in the order they should execute, where each
    /// handler runs only after all its dependencies have completed.
    ///
    /// Within each "level" (handlers with no pending dependencies), handlers
    /// are sorted by priority (lower first).
    ///
    /// # Errors
    ///
    /// - `DependencyError::CycleDetected` if there's a circular dependency
    /// - `DependencyError::UnknownDependency` if a dependency doesn't exist
    pub fn execution_order(&mut self) -> DependencyResult<Vec<String>> {
        // Return cached order if available
        if let Some(ref order) = self.cached_order {
            return Ok(order.clone());
        }

        // Validate dependencies first
        self.validate_dependencies()?;

        // Kahn's algorithm for topological sort
        let order = self.topological_sort()?;

        // Cache and return
        self.cached_order = Some(order.clone());
        Ok(order)
    }

    /// Get execution order without caching (for repeated calls during construction).
    pub fn execution_order_uncached(&self) -> DependencyResult<Vec<String>> {
        self.validate_dependencies()?;
        self.topological_sort()
    }

    /// Kahn's algorithm for topological sorting with priority tiebreaker.
    fn topological_sort(&self) -> DependencyResult<Vec<String>> {
        if self.nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Build adjacency list and in-degree map
        // Edge direction: dependency -> dependent (A depends on B means B -> A)
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

        // Initialize
        for name in self.nodes.keys() {
            in_degree.insert(name, 0);
            dependents.insert(name, Vec::new());
        }

        // Build graph
        for node in self.nodes.values() {
            for dep in &node.depends_on {
                // dep -> node.name (node depends on dep)
                dependents.get_mut(dep.as_str()).unwrap().push(&node.name);
                *in_degree.get_mut(node.name.as_str()).unwrap() += 1;
            }
        }

        // Start with nodes that have no dependencies, sorted by priority
        let mut queue: VecDeque<&str> = {
            let mut v: Vec<_> = in_degree
                .iter()
                .filter(|(_, &deg)| deg == 0)
                .map(|(&name, _)| name)
                .collect();
            // Sort by priority (lower first), then by name for determinism
            v.sort_by(|a, b| {
                let pa = self.nodes.get(*a).map(|n| n.priority).unwrap_or(50);
                let pb = self.nodes.get(*b).map(|n| n.priority).unwrap_or(50);
                pa.cmp(&pb).then_with(|| a.cmp(b))
            });
            v.into_iter().collect()
        };

        let mut result: Vec<String> = Vec::with_capacity(self.nodes.len());

        while let Some(name) = queue.pop_front() {
            result.push(name.to_string());

            // Get dependents sorted by priority then name for deterministic order
            let mut deps: Vec<&str> = dependents[name].clone();
            deps.sort_by(|a, b| {
                let pa = self.nodes.get(*a).map(|n| n.priority).unwrap_or(50);
                let pb = self.nodes.get(*b).map(|n| n.priority).unwrap_or(50);
                pa.cmp(&pb).then_with(|| a.cmp(b))
            });

            for dependent in deps {
                let deg = in_degree.get_mut(dependent).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    // Insert in priority order
                    let priority = self.nodes.get(dependent).map(|n| n.priority).unwrap_or(50);
                    let insert_pos = queue
                        .iter()
                        .position(|&n| {
                            let np = self.nodes.get(n).map(|node| node.priority).unwrap_or(50);
                            np > priority || (np == priority && n > dependent)
                        })
                        .unwrap_or(queue.len());
                    queue.insert(insert_pos, dependent);
                }
            }
        }

        // If we didn't process all nodes, there's a cycle
        if result.len() != self.nodes.len() {
            let cycle = self.find_cycle()?;
            return Err(DependencyError::CycleDetected { cycle });
        }

        Ok(result)
    }

    /// Find a cycle in the graph using DFS.
    ///
    /// Returns a list of handler names involved in the cycle.
    fn find_cycle(&self) -> DependencyResult<Vec<String>> {
        #[derive(Clone, Copy, PartialEq)]
        enum State {
            Unvisited,
            InProgress,
            Done,
        }

        let mut state: HashMap<&str, State> = self
            .nodes
            .keys()
            .map(|k| (k.as_str(), State::Unvisited))
            .collect();
        let mut path: Vec<&str> = Vec::new();

        fn dfs<'a>(
            node: &'a str,
            nodes: &'a HashMap<String, GraphNode>,
            state: &mut HashMap<&'a str, State>,
            path: &mut Vec<&'a str>,
        ) -> Option<Vec<String>> {
            state.insert(node, State::InProgress);
            path.push(node);

            if let Some(graph_node) = nodes.get(node) {
                for dep in &graph_node.depends_on {
                    match state.get(dep.as_str()) {
                        Some(State::InProgress) => {
                            // Found cycle - extract it
                            let cycle_start = path.iter().position(|&n| n == dep).unwrap();
                            let mut cycle: Vec<String> =
                                path[cycle_start..].iter().map(|s| s.to_string()).collect();
                            cycle.push(dep.clone()); // Complete the cycle
                            return Some(cycle);
                        }
                        Some(State::Unvisited) | None => {
                            if let Some(cycle) = dfs(dep, nodes, state, path) {
                                return Some(cycle);
                            }
                        }
                        Some(State::Done) => {}
                    }
                }
            }

            state.insert(node, State::Done);
            path.pop();
            None
        }

        // Try DFS from each unvisited node
        let node_names: Vec<&str> = self.nodes.keys().map(|s| s.as_str()).collect();
        for name in node_names {
            if state[name] == State::Unvisited {
                if let Some(cycle) = dfs(name, &self.nodes, &mut state, &mut path) {
                    return Ok(cycle);
                }
            }
        }

        // Shouldn't happen if called after topological_sort detected a cycle
        Ok(vec!["unknown cycle".to_string()])
    }

    /// Get the direct dependencies of a handler.
    pub fn dependencies_of(&self, name: &str) -> Option<&[String]> {
        self.nodes.get(name).map(|n| n.depends_on.as_slice())
    }

    /// Get all handlers that depend on the given handler.
    pub fn dependents_of(&self, name: &str) -> Vec<&str> {
        self.nodes
            .values()
            .filter(|node| node.depends_on.iter().any(|dep| dep == name))
            .map(|node| node.name.as_str())
            .collect()
    }

    /// Get transitive dependencies (all handlers that must run before this one).
    pub fn transitive_dependencies(&self, name: &str) -> DependencyResult<HashSet<String>> {
        let mut visited = HashSet::new();
        let mut result = HashSet::new();

        fn collect_deps(
            name: &str,
            nodes: &HashMap<String, GraphNode>,
            visited: &mut HashSet<String>,
            result: &mut HashSet<String>,
        ) {
            if visited.contains(name) {
                return;
            }
            visited.insert(name.to_string());

            if let Some(node) = nodes.get(name) {
                for dep in &node.depends_on {
                    result.insert(dep.clone());
                    collect_deps(dep, nodes, visited, result);
                }
            }
        }

        collect_deps(name, &self.nodes, &mut visited, &mut result);
        Ok(result)
    }

    /// Clear all handlers from the graph.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.cached_order = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let mut graph = DependencyGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);

        let order = graph.execution_order().unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_single_handler_no_deps() {
        let mut graph = DependencyGraph::new();
        graph.add("handler1", vec![]).unwrap();

        assert!(!graph.is_empty());
        assert_eq!(graph.len(), 1);
        assert!(graph.contains("handler1"));

        let order = graph.execution_order().unwrap();
        assert_eq!(order, vec!["handler1"]);
    }

    #[test]
    fn test_linear_chain() {
        let mut graph = DependencyGraph::new();

        // C depends on B depends on A
        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();
        graph.add("C", vec!["B".to_string()]).unwrap();

        let order = graph.execution_order().unwrap();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_diamond_dependency() {
        let mut graph = DependencyGraph::new();

        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();
        graph.add("C", vec!["A".to_string()]).unwrap();
        graph
            .add("D", vec!["B".to_string(), "C".to_string()])
            .unwrap();

        let order = graph.execution_order().unwrap();

        // A must come first, D must come last, B and C can be either order
        assert_eq!(order[0], "A");
        assert_eq!(order[3], "D");
        assert!(order[1..3].contains(&"B".to_string()));
        assert!(order[1..3].contains(&"C".to_string()));
    }

    #[test]
    fn test_priority_tiebreaker() {
        let mut graph = DependencyGraph::new();

        // A, B, C all independent - should be ordered by priority
        graph.add_with_priority("A", vec![], 50).unwrap();
        graph.add_with_priority("B", vec![], 10).unwrap(); // Lowest priority = first
        graph.add_with_priority("C", vec![], 30).unwrap();

        let order = graph.execution_order().unwrap();
        assert_eq!(order, vec!["B", "C", "A"]);
    }

    #[test]
    fn test_cycle_detection_simple() {
        let mut graph = DependencyGraph::new();

        // A -> B -> A (cycle)
        graph.add("A", vec!["B".to_string()]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();

        let result = graph.execution_order();
        assert!(matches!(result, Err(DependencyError::CycleDetected { .. })));

        if let Err(DependencyError::CycleDetected { cycle }) = result {
            // Cycle should contain A and B
            assert!(cycle.contains(&"A".to_string()));
            assert!(cycle.contains(&"B".to_string()));
        }
    }

    #[test]
    fn test_unknown_dependency() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec!["nonexistent".to_string()]).unwrap();

        let result = graph.execution_order();
        assert!(matches!(
            result,
            Err(DependencyError::UnknownDependency { .. })
        ));

        if let Err(DependencyError::UnknownDependency {
            handler,
            dependency,
        }) = result
        {
            assert_eq!(handler, "A");
            assert_eq!(dependency, "nonexistent");
        }
    }

    #[test]
    fn test_duplicate_handler() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec![]).unwrap();
        let result = graph.add("A", vec![]);

        assert!(matches!(result, Err(DependencyError::DuplicateHandler(_))));
    }

    #[test]
    fn test_remove_handler() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();

        assert!(graph.contains("A"));

        let node = graph.remove("A").unwrap();
        assert_eq!(node.name, "A");
        assert!(!graph.contains("A"));

        // B now has unknown dependency
        let result = graph.execution_order();
        assert!(matches!(
            result,
            Err(DependencyError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn test_dependencies_of() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();
        graph
            .add("C", vec!["A".to_string(), "B".to_string()])
            .unwrap();

        assert_eq!(graph.dependencies_of("A"), Some(&[][..]));
        assert_eq!(graph.dependencies_of("B"), Some(&["A".to_string()][..]));
        assert_eq!(
            graph.dependencies_of("C"),
            Some(&["A".to_string(), "B".to_string()][..])
        );
        assert_eq!(graph.dependencies_of("nonexistent"), None);
    }

    #[test]
    fn test_dependents_of() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();
        graph.add("C", vec!["A".to_string()]).unwrap();
        graph.add("D", vec!["B".to_string()]).unwrap();

        let mut deps = graph.dependents_of("A");
        deps.sort();
        assert_eq!(deps, vec!["B", "C"]);

        let deps = graph.dependents_of("B");
        assert_eq!(deps, vec!["D"]);

        let deps = graph.dependents_of("D");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_transitive_dependencies() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();
        graph.add("C", vec!["B".to_string()]).unwrap();
        graph.add("D", vec!["C".to_string()]).unwrap();

        let trans = graph.transitive_dependencies("D").unwrap();
        assert!(trans.contains("A"));
        assert!(trans.contains("B"));
        assert!(trans.contains("C"));
        assert!(!trans.contains("D"));
        assert_eq!(trans.len(), 3);
    }

    #[test]
    fn test_cached_order() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();

        // First call computes order
        let order1 = graph.execution_order().unwrap();

        // Second call uses cache
        let order2 = graph.execution_order().unwrap();

        assert_eq!(order1, order2);

        // Adding invalidates cache
        graph.add("C", vec!["B".to_string()]).unwrap();
        let order3 = graph.execution_order().unwrap();
        assert_eq!(order3, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_clear() {
        let mut graph = DependencyGraph::new();

        graph.add("A", vec![]).unwrap();
        graph.add("B", vec!["A".to_string()]).unwrap();

        assert_eq!(graph.len(), 2);

        graph.clear();

        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
        assert!(!graph.contains("A"));
    }

    #[test]
    fn test_complex_graph() {
        let mut graph = DependencyGraph::new();

        // Build a more complex realistic graph
        //
        //      log_start
        //          |
        //    +-----+-----+
        //    v           v
        // validate    parse
        //    |           |
        //    +-----+-----+
        //          v
        //       execute
        //          |
        //    +-----+-----+
        //    v           v
        // persist    notify
        //          |
        //          v
        //      log_end

        graph.add("log_start", vec![]).unwrap();
        graph
            .add("validate", vec!["log_start".to_string()])
            .unwrap();
        graph.add("parse", vec!["log_start".to_string()]).unwrap();
        graph
            .add("execute", vec!["validate".to_string(), "parse".to_string()])
            .unwrap();
        graph.add("persist", vec!["execute".to_string()]).unwrap();
        graph.add("notify", vec!["execute".to_string()]).unwrap();
        graph.add("log_end", vec!["notify".to_string()]).unwrap();

        let order = graph.execution_order().unwrap();

        // Verify constraints
        let pos = |name: &str| order.iter().position(|s| s == name).unwrap();

        assert!(pos("log_start") < pos("validate"));
        assert!(pos("log_start") < pos("parse"));
        assert!(pos("validate") < pos("execute"));
        assert!(pos("parse") < pos("execute"));
        assert!(pos("execute") < pos("persist"));
        assert!(pos("execute") < pos("notify"));
        assert!(pos("notify") < pos("log_end"));
    }
}
