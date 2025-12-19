//! Test fixtures for TOON LLM evaluation
//!
//! Provides JSON test data at varying complexity levels.

use serde_json::{json, Value};

/// Complexity level for test fixtures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Complexity {
    /// Flat object with primitives
    Primitives,
    /// Nested objects 2-3 levels deep
    Nested,
    /// Simple homogeneous arrays
    SimpleArrays,
    /// Arrays of uniform objects (tabular)
    TabularArrays,
    /// Mixed/heterogeneous arrays
    MixedArrays,
    /// Real-world MCP/tool responses
    RealWorld,
    /// Edge cases: quoting, escaping, special values
    EdgeCases,
}

impl Complexity {
    /// All complexity levels in order
    pub fn all() -> &'static [Complexity] {
        &[
            Complexity::Primitives,
            Complexity::Nested,
            Complexity::SimpleArrays,
            Complexity::TabularArrays,
            Complexity::MixedArrays,
            Complexity::RealWorld,
            Complexity::EdgeCases,
        ]
    }

    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Complexity::Primitives => "primitives",
            Complexity::Nested => "nested",
            Complexity::SimpleArrays => "simple_arrays",
            Complexity::TabularArrays => "tabular_arrays",
            Complexity::MixedArrays => "mixed_arrays",
            Complexity::RealWorld => "real_world",
            Complexity::EdgeCases => "edge_cases",
        }
    }
}

/// A test fixture with JSON data and optional expected TOON
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Unique identifier
    pub id: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Complexity level
    pub complexity: Complexity,
    /// The JSON data
    pub json: Value,
    /// Expected TOON output (if known/canonical)
    pub expected_toon: Option<&'static str>,
}

/// Get all test fixtures
pub fn all_fixtures() -> Vec<Fixture> {
    let mut fixtures = Vec::new();
    fixtures.extend(primitives_fixtures());
    fixtures.extend(nested_fixtures());
    fixtures.extend(simple_array_fixtures());
    fixtures.extend(tabular_array_fixtures());
    fixtures.extend(mixed_array_fixtures());
    fixtures.extend(real_world_fixtures());
    fixtures.extend(edge_case_fixtures());
    fixtures
}

/// Get fixtures by complexity level
pub fn fixtures_by_complexity(complexity: Complexity) -> Vec<Fixture> {
    all_fixtures()
        .into_iter()
        .filter(|f| f.complexity == complexity)
        .collect()
}

// --- Level 1: Primitives & flat objects ---

fn primitives_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            id: "prim_simple",
            description: "Simple flat object with string, number, boolean",
            complexity: Complexity::Primitives,
            json: json!({
                "name": "Ada",
                "age": 30,
                "active": true
            }),
            expected_toon: Some("name: Ada\nage: 30\nactive: true"),
        },
        Fixture {
            id: "prim_null",
            description: "Object with null value",
            complexity: Complexity::Primitives,
            json: json!({
                "name": "Bob",
                "email": null
            }),
            expected_toon: Some("name: Bob\nemail: null"),
        },
        Fixture {
            id: "prim_numbers",
            description: "Various number formats",
            complexity: Complexity::Primitives,
            json: json!({
                "integer": 42,
                "negative": -17,
                "decimal": std::f64::consts::PI,
                "zero": 0
            }),
            expected_toon: None, // Order may vary
        },
        Fixture {
            id: "prim_empty",
            description: "Empty object",
            complexity: Complexity::Primitives,
            json: json!({}),
            expected_toon: Some(""),
        },
    ]
}

// --- Level 2: Nested objects ---

fn nested_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            id: "nest_simple",
            description: "Single level nesting",
            complexity: Complexity::Nested,
            json: json!({
                "user": {
                    "name": "Ada",
                    "id": 123
                }
            }),
            expected_toon: Some("user:\n  name: Ada\n  id: 123"),
        },
        Fixture {
            id: "nest_deep",
            description: "Three levels deep",
            complexity: Complexity::Nested,
            json: json!({
                "config": {
                    "database": {
                        "connection": {
                            "host": "localhost",
                            "port": 5432
                        }
                    }
                }
            }),
            expected_toon: None,
        },
        Fixture {
            id: "nest_siblings",
            description: "Multiple nested siblings",
            complexity: Complexity::Nested,
            json: json!({
                "user": {
                    "name": "Ada"
                },
                "settings": {
                    "theme": "dark"
                }
            }),
            expected_toon: None,
        },
    ]
}

// --- Level 3: Simple arrays ---

fn simple_array_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            id: "arr_strings",
            description: "Array of strings",
            complexity: Complexity::SimpleArrays,
            json: json!({
                "tags": ["rust", "llm", "toon"]
            }),
            expected_toon: Some("tags[3]: rust,llm,toon"),
        },
        Fixture {
            id: "arr_numbers",
            description: "Array of numbers",
            complexity: Complexity::SimpleArrays,
            json: json!({
                "scores": [95, 87, 92, 78]
            }),
            expected_toon: Some("scores[4]: 95,87,92,78"),
        },
        Fixture {
            id: "arr_empty",
            description: "Empty array",
            complexity: Complexity::SimpleArrays,
            json: json!({
                "items": []
            }),
            expected_toon: Some("items[0]:"),
        },
        Fixture {
            id: "arr_single",
            description: "Single element array",
            complexity: Complexity::SimpleArrays,
            json: json!({
                "only": ["one"]
            }),
            expected_toon: Some("only[1]: one"),
        },
    ]
}

// --- Level 4: Tabular arrays (TOON's sweet spot) ---

fn tabular_array_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            id: "tab_users",
            description: "Classic user table",
            complexity: Complexity::TabularArrays,
            json: json!({
                "users": [
                    {"id": 1, "name": "Alice", "role": "admin"},
                    {"id": 2, "name": "Bob", "role": "user"},
                    {"id": 3, "name": "Carol", "role": "user"}
                ]
            }),
            expected_toon: Some(
                "users[3]{id,name,role}:\n  1,Alice,admin\n  2,Bob,user\n  3,Carol,user",
            ),
        },
        Fixture {
            id: "tab_products",
            description: "Product inventory",
            complexity: Complexity::TabularArrays,
            json: json!({
                "products": [
                    {"sku": "A001", "name": "Widget", "price": 9.99, "qty": 100},
                    {"sku": "B002", "name": "Gadget", "price": 19.99, "qty": 50}
                ]
            }),
            expected_toon: None,
        },
        Fixture {
            id: "tab_single_row",
            description: "Tabular with single row",
            complexity: Complexity::TabularArrays,
            json: json!({
                "items": [
                    {"id": 1, "value": "only"}
                ]
            }),
            expected_toon: Some("items[1]{id,value}:\n  1,only"),
        },
    ]
}

// --- Level 5: Mixed arrays ---

fn mixed_array_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            id: "mix_types",
            description: "Array with different types",
            complexity: Complexity::MixedArrays,
            json: json!({
                "mixed": [1, "text", true, null]
            }),
            expected_toon: None, // Mixed arrays use list syntax
        },
        Fixture {
            id: "mix_nested",
            description: "Array with nested objects and arrays",
            complexity: Complexity::MixedArrays,
            json: json!({
                "items": [
                    {"type": "object", "data": {"x": 1}},
                    [1, 2, 3],
                    "plain string"
                ]
            }),
            expected_toon: None,
        },
        Fixture {
            id: "mix_heterogeneous_objects",
            description: "Objects with different fields",
            complexity: Complexity::MixedArrays,
            json: json!({
                "records": [
                    {"name": "Alice", "age": 30},
                    {"name": "Bob", "email": "bob@example.com"},
                    {"title": "Manager", "dept": "Sales"}
                ]
            }),
            expected_toon: None,
        },
    ]
}

// --- Level 6: Real-world MCP/tool responses ---

fn real_world_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            id: "rw_search_results",
            description: "Search tool response",
            complexity: Complexity::RealWorld,
            json: json!({
                "tool": "semantic_search",
                "query": "rust async patterns",
                "results": [
                    {"title": "Async Rust Book", "score": 0.95, "path": "/docs/async.md"},
                    {"title": "Tokio Tutorial", "score": 0.87, "path": "/docs/tokio.md"}
                ],
                "metadata": {
                    "total_matches": 42,
                    "search_time_ms": 15
                }
            }),
            expected_toon: None,
        },
        Fixture {
            id: "rw_function_call",
            description: "LLM function call response",
            complexity: Complexity::RealWorld,
            json: json!({
                "function": "get_weather",
                "arguments": {
                    "location": "San Francisco",
                    "units": "celsius"
                },
                "result": {
                    "temperature": 18,
                    "conditions": "partly cloudy",
                    "humidity": 65
                }
            }),
            expected_toon: None,
        },
        Fixture {
            id: "rw_mcp_tool_list",
            description: "MCP tool listing",
            complexity: Complexity::RealWorld,
            json: json!({
                "tools": [
                    {
                        "name": "read_file",
                        "description": "Read contents of a file",
                        "parameters": {
                            "path": {"type": "string", "required": true}
                        }
                    },
                    {
                        "name": "search",
                        "description": "Search for text",
                        "parameters": {
                            "query": {"type": "string", "required": true},
                            "limit": {"type": "integer", "required": false}
                        }
                    }
                ]
            }),
            expected_toon: None,
        },
    ]
}

// --- Level 7: Edge cases ---

fn edge_case_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            id: "edge_quoting",
            description: "Values requiring quotes",
            complexity: Complexity::EdgeCases,
            json: json!({
                "path": "C:\\Users\\Admin",
                "greeting": "hello, world",
                "empty": "",
                "numeric_string": "123"
            }),
            expected_toon: None,
        },
        Fixture {
            id: "edge_reserved",
            description: "Reserved words as strings",
            complexity: Complexity::EdgeCases,
            json: json!({
                "status": "true",
                "value": "null",
                "flag": "false"
            }),
            expected_toon: None, // Must be quoted
        },
        Fixture {
            id: "edge_special_chars",
            description: "Special characters in values",
            complexity: Complexity::EdgeCases,
            json: json!({
                "formula": "a + b = c",
                "json_like": "{\"nested\": true}",
                "array_like": "[1, 2, 3]"
            }),
            expected_toon: None,
        },
        Fixture {
            id: "edge_whitespace",
            description: "Values with whitespace",
            complexity: Complexity::EdgeCases,
            json: json!({
                "leading": " space",
                "trailing": "space ",
                "multiline": "line1\nline2"
            }),
            expected_toon: None,
        },
        Fixture {
            id: "edge_unicode",
            description: "Unicode content",
            complexity: Complexity::EdgeCases,
            json: json!({
                "emoji": "Hello ",
                "chinese": "",
                "arabic": ""
            }),
            expected_toon: None,
        },
    ]
}

/// A query test case - TOON data with questions and expected answers
#[derive(Debug, Clone)]
pub struct QueryFixture {
    /// Unique identifier
    pub id: &'static str,
    /// Description of what this tests
    pub description: &'static str,
    /// The TOON data (can be large)
    pub toon: &'static str,
    /// Questions to ask about the data
    pub questions: Vec<QueryQuestion>,
}

#[derive(Debug, Clone)]
pub struct QueryQuestion {
    /// The question to ask
    pub question: &'static str,
    /// Expected answer (or key phrases that should appear)
    pub expected: Vec<&'static str>,
    /// Whether the answer should contain ALL expected phrases or ANY
    pub match_all: bool,
}

/// Get query test fixtures - realistic tool output scenarios
pub fn query_fixtures() -> Vec<QueryFixture> {
    vec![
        QueryFixture {
            id: "query_search_results",
            description: "Search results from semantic search tool",
            toon: r#"tool: semantic_search
query: rust error handling patterns
total_results: 47
results[5]{title,path,score,snippet}:
  Error Handling in Rust,/docs/error-handling.md,0.95,Using Result and Option types for safe error handling
  Custom Error Types,/docs/custom-errors.md,0.91,Implementing std::error::Error trait for custom types
  The ? Operator,/docs/question-mark.md,0.88,Propagating errors with the ? operator
  anyhow vs thiserror,/blog/error-crates.md,0.82,Comparing popular error handling crates
  Panic vs Result,/docs/panic-vs-result.md,0.79,When to panic and when to return Result
metadata:
  search_time_ms: 23
  index_version: 3"#,
            questions: vec![
                QueryQuestion {
                    question: "What is the highest scoring result?",
                    expected: vec!["Error Handling in Rust", "0.95"],
                    match_all: false,
                },
                QueryQuestion {
                    question: "How many total results were found?",
                    expected: vec!["47"],
                    match_all: true,
                },
                QueryQuestion {
                    question: "Which result discusses the ? operator?",
                    expected: vec!["question-mark", "? Operator"],
                    match_all: false,
                },
                QueryQuestion {
                    question: "How long did the search take?",
                    expected: vec!["23", "ms"],
                    match_all: false,
                },
            ],
        },
        QueryFixture {
            id: "query_file_listing",
            description: "Directory listing from file tool",
            toon: r#"path: /home/user/project/src
total_files: 12
total_size_kb: 245
files[8]{name,size_kb,modified,type}:
  main.rs,15,2024-12-01T10:30:00Z,rust
  lib.rs,42,2024-12-05T14:22:00Z,rust
  config.rs,8,2024-11-28T09:15:00Z,rust
  utils/mod.rs,3,2024-11-20T11:00:00Z,rust
  utils/helpers.rs,28,2024-12-03T16:45:00Z,rust
  tests/mod.rs,5,2024-11-25T08:30:00Z,rust
  tests/integration.rs,67,2024-12-04T12:00:00Z,rust
  tests/unit.rs,34,2024-12-02T17:20:00Z,rust
directories[2]: utils,tests"#,
            questions: vec![
                QueryQuestion {
                    question: "What is the largest file?",
                    expected: vec!["integration.rs", "67"],
                    match_all: false,
                },
                QueryQuestion {
                    question: "Which file was most recently modified?",
                    expected: vec!["lib.rs", "2024-12-05"],
                    match_all: false,
                },
                QueryQuestion {
                    question: "What subdirectories exist?",
                    expected: vec!["utils", "tests"],
                    match_all: true,
                },
                QueryQuestion {
                    question: "How many files are in the tests directory?",
                    expected: vec!["3", "three"],
                    match_all: false,
                },
            ],
        },
        QueryFixture {
            id: "query_api_response",
            description: "API response with nested data",
            toon: r#"status: success
request_id: req_abc123
data:
  users[3]{id,name,email,role,active}:
    1,Alice Smith,alice@example.com,admin,true
    2,Bob Jones,bob@example.com,developer,true
    3,Carol White,carol@example.com,developer,false
  permissions:
    admin[4]: read,write,delete,manage_users
    developer[2]: read,write
  quotas:
    storage_gb: 100
    api_calls_per_day: 10000
    max_file_size_mb: 50
pagination:
  page: 1
  per_page: 10
  total: 3"#,
            questions: vec![
                QueryQuestion {
                    question: "Who is the admin user?",
                    expected: vec!["Alice", "alice"],
                    match_all: false,
                },
                QueryQuestion {
                    question: "Which user is inactive?",
                    expected: vec!["Carol", "carol", "false"],
                    match_all: false,
                },
                QueryQuestion {
                    question: "What permissions do developers have?",
                    expected: vec!["read", "write"],
                    match_all: true,
                },
                QueryQuestion {
                    question: "What is the storage quota?",
                    expected: vec!["100", "GB", "gb"],
                    match_all: false,
                },
            ],
        },
        QueryFixture {
            id: "query_git_log",
            description: "Git log output",
            toon: r#"branch: main
commits[5]{hash,author,date,message}:
  a1b2c3d,Alice,2024-12-05,Fix memory leak in parser
  e4f5g6h,Bob,2024-12-04,Add support for TOON format
  i7j8k9l,Alice,2024-12-03,Refactor error handling
  m0n1o2p,Carol,2024-12-02,Update dependencies
  q3r4s5t,Bob,2024-12-01,Initial commit
stats:
  total_commits: 127
  contributors: 3
  lines_added: 4521
  lines_deleted: 1203"#,
            questions: vec![
                QueryQuestion {
                    question: "Who made the most recent commit?",
                    expected: vec!["Alice"],
                    match_all: true,
                },
                QueryQuestion {
                    question: "What was Bob's commit about on December 4th?",
                    expected: vec!["TOON", "support", "format"],
                    match_all: false,
                },
                QueryQuestion {
                    question: "How many total commits are there?",
                    expected: vec!["127"],
                    match_all: true,
                },
                QueryQuestion {
                    question: "How many lines were added overall?",
                    expected: vec!["4521"],
                    match_all: true,
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_fixtures_have_unique_ids() {
        let fixtures = all_fixtures();
        let mut ids: Vec<_> = fixtures.iter().map(|f| f.id).collect();
        ids.sort();
        let original_len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "Duplicate fixture IDs found");
    }

    #[test]
    fn test_fixtures_cover_all_complexity_levels() {
        for complexity in Complexity::all() {
            let fixtures = fixtures_by_complexity(*complexity);
            assert!(
                !fixtures.is_empty(),
                "No fixtures for complexity level {:?}",
                complexity
            );
        }
    }
}
