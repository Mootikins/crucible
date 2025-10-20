// Input parsing and routing
//
// Determines whether input is a command or query and routes appropriately

use super::command::Command;
use super::error::ReplError;

/// Parsed input type
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Built-in command (starts with ':')
    Command(Command),

    /// SurrealQL query (anything else)
    Query(String),

    /// Empty input (whitespace only)
    Empty,
}

impl Input {
    /// Parse raw input string
    ///
    /// # Routing Logic
    /// - Lines starting with ':' are commands
    /// - Everything else is treated as SurrealQL
    /// - Empty/whitespace lines are ignored
    ///
    /// This simple routing avoids ambiguity and parser complexity.
    pub fn parse(input: &str) -> Result<Self, ReplError> {
        let trimmed = input.trim();

        // Empty input
        if trimmed.is_empty() {
            return Ok(Input::Empty);
        }

        // Command (starts with ':')
        if trimmed.starts_with(':') {
            let cmd = Command::parse(trimmed)?;
            return Ok(Input::Command(cmd));
        }

        // Everything else is a query
        Ok(Input::Query(trimmed.to_string()))
    }

    /// Check if input is empty
    pub fn is_empty(&self) -> bool {
        matches!(self, Input::Empty)
    }

    /// Check if input is a command
    pub fn is_command(&self) -> bool {
        matches!(self, Input::Command(_))
    }

    /// Check if input is a query
    pub fn is_query(&self) -> bool {
        matches!(self, Input::Query(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        assert!(matches!(Input::parse("").unwrap(), Input::Empty));
        assert!(matches!(Input::parse("   ").unwrap(), Input::Empty));
        assert!(matches!(Input::parse("\n").unwrap(), Input::Empty));
        assert!(matches!(Input::parse("\t").unwrap(), Input::Empty));
    }

    #[test]
    fn test_command_routing() {
        let input = Input::parse(":tools").unwrap();
        assert!(input.is_command());
        assert!(!input.is_query());
        assert!(!input.is_empty());

        match input {
            Input::Command(cmd) => {
                assert_eq!(cmd, Command::ListTools);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_query_routing() {
        let input = Input::parse("SELECT * FROM notes").unwrap();
        assert!(input.is_query());
        assert!(!input.is_command());
        assert!(!input.is_empty());

        match input {
            Input::Query(query) => {
                assert_eq!(query, "SELECT * FROM notes");
            }
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_multiline_query() {
        let input = r#"
            SELECT
                path,
                title,
                tags
            FROM notes
            WHERE tags CONTAINS '#project'
        "#;

        let parsed = Input::parse(input).unwrap();
        assert!(parsed.is_query());

        match parsed {
            Input::Query(query) => {
                // Should preserve the query content (trimmed)
                assert!(query.contains("SELECT"));
                assert!(query.contains("FROM notes"));
            }
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_whitespace_handling() {
        // Leading/trailing whitespace should be trimmed
        assert!(matches!(
            Input::parse("  :tools  ").unwrap(),
            Input::Command(_)
        ));

        assert!(matches!(
            Input::parse("  SELECT * FROM notes  ").unwrap(),
            Input::Query(_)
        ));
    }

    #[test]
    fn test_command_vs_query_distinction() {
        // Command
        assert!(Input::parse(":run tool").unwrap().is_command());

        // Query with colon but not at start
        let query = "SELECT * FROM notes WHERE path = 'foo:bar'";
        assert!(Input::parse(query).unwrap().is_query());

        // Query that might look like command
        let query = "-- :comment\nSELECT * FROM notes";
        assert!(Input::parse(query).unwrap().is_query());
    }

    #[test]
    fn test_surrealql_examples() {
        // Various SurrealQL patterns
        let queries = vec![
            "SELECT * FROM notes;",
            "CREATE note:test SET title = 'Test'",
            "UPDATE note:123 SET tags += '#new'",
            "DELETE note:456",
            "SELECT ->links->note FROM notes WHERE id = note:789",
            "LET $var = 'value'; SELECT * FROM notes WHERE path = $var",
            "BEGIN TRANSACTION; SELECT * FROM notes; COMMIT;",
        ];

        for query in queries {
            let parsed = Input::parse(query).unwrap();
            assert!(parsed.is_query(), "Failed for: {}", query);
        }
    }

    #[test]
    fn test_error_propagation() {
        // Invalid command should propagate error
        let result = Input::parse(":invalid_command");
        assert!(result.is_err());

        // Unknown command should propagate error
        let result = Input::parse(":xyz");
        assert!(result.is_err());
    }
}
