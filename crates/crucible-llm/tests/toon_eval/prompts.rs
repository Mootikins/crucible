//! Modular prompt system for TOON LLM evaluation
//!
//! Provides toggleable prompt components that combine into test configurations.

use std::fmt;

/// Individual prompt components that can be combined
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptComponent {
    // Format explanation
    /// ABNF grammar excerpt from TOON spec
    SpecGrammar,
    /// Quoting, escaping, indentation rules
    SpecRules,

    // Examples (graduated complexity)
    /// Simple flat object JSON→TOON pair
    ExampleSimple,
    /// Nested object example
    ExampleNested,
    /// Array with tabular {field,list}: syntax
    ExampleTabular,
    /// Complex mixed arrays
    ExampleMixed,

    // Task framing
    /// "Convert this JSON to TOON format"
    TaskJsonToToon,
    /// "Convert this TOON to JSON format"
    TaskToonToJson,
    /// "Answer this question about the TOON data"
    TaskToonQuery,
}

impl PromptComponent {
    /// Get the prompt text for this component
    pub fn text(&self) -> &'static str {
        match self {
            // --- Spec components ---
            PromptComponent::SpecGrammar => SPEC_GRAMMAR,
            PromptComponent::SpecRules => SPEC_RULES,

            // --- Example components ---
            PromptComponent::ExampleSimple => EXAMPLE_SIMPLE,
            PromptComponent::ExampleNested => EXAMPLE_NESTED,
            PromptComponent::ExampleTabular => EXAMPLE_TABULAR,
            PromptComponent::ExampleMixed => EXAMPLE_MIXED,

            // --- Task components ---
            PromptComponent::TaskJsonToToon => TASK_JSON_TO_TOON,
            PromptComponent::TaskToonToJson => TASK_TOON_TO_JSON,
            PromptComponent::TaskToonQuery => TASK_TOON_QUERY,
        }
    }
}

/// Pre-defined prompt configurations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptConfig {
    /// Task only, no spec or examples
    ZeroShot,
    /// Task + grammar/rules, no examples
    SpecOnly,
    /// Task + n examples, no spec
    FewShot(usize),
    /// Task + spec + n examples
    SpecPlusFewShot(usize),
    /// Everything included
    Full,
    /// Custom combination
    Custom(Vec<PromptComponent>),
}

impl fmt::Display for PromptConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PromptConfig::ZeroShot => write!(f, "zero_shot"),
            PromptConfig::SpecOnly => write!(f, "spec_only"),
            PromptConfig::FewShot(n) => write!(f, "few_shot_{}", n),
            PromptConfig::SpecPlusFewShot(n) => write!(f, "spec_plus_few_shot_{}", n),
            PromptConfig::Full => write!(f, "full"),
            PromptConfig::Custom(_) => write!(f, "custom"),
        }
    }
}

impl PromptConfig {
    /// Get all standard configurations for testing
    pub fn all_standard() -> Vec<PromptConfig> {
        vec![
            PromptConfig::ZeroShot,
            PromptConfig::SpecOnly,
            PromptConfig::FewShot(1),
            PromptConfig::FewShot(2),
            PromptConfig::FewShot(3),
            PromptConfig::SpecPlusFewShot(1),
            PromptConfig::SpecPlusFewShot(2),
            PromptConfig::SpecPlusFewShot(3),
            PromptConfig::Full,
        ]
    }

    /// Get quick configurations for smoke testing
    pub fn quick() -> Vec<PromptConfig> {
        vec![
            PromptConfig::ZeroShot,
            PromptConfig::FewShot(1),
            PromptConfig::FewShot(2),
            PromptConfig::Full,
        ]
    }

    /// Get configurations focused on example variation
    pub fn example_variations() -> Vec<PromptConfig> {
        vec![
            PromptConfig::ZeroShot,
            PromptConfig::FewShot(1),
            PromptConfig::FewShot(2),
            PromptConfig::FewShot(3),
            PromptConfig::Full,
        ]
    }

    /// Get the components for this configuration (excluding task)
    pub fn components(&self) -> Vec<PromptComponent> {
        match self {
            PromptConfig::ZeroShot => vec![],
            PromptConfig::SpecOnly => vec![PromptComponent::SpecGrammar, PromptComponent::SpecRules],
            PromptConfig::FewShot(n) => examples_for_count(*n),
            PromptConfig::SpecPlusFewShot(n) => {
                let mut components =
                    vec![PromptComponent::SpecGrammar, PromptComponent::SpecRules];
                components.extend(examples_for_count(*n));
                components
            }
            PromptConfig::Full => vec![
                PromptComponent::SpecGrammar,
                PromptComponent::SpecRules,
                PromptComponent::ExampleSimple,
                PromptComponent::ExampleNested,
                PromptComponent::ExampleTabular,
                PromptComponent::ExampleMixed,
            ],
            PromptConfig::Custom(components) => components.clone(),
        }
    }
}

/// Get example components for a given count
fn examples_for_count(n: usize) -> Vec<PromptComponent> {
    let all_examples = [
        PromptComponent::ExampleSimple,
        PromptComponent::ExampleNested,
        PromptComponent::ExampleTabular,
        PromptComponent::ExampleMixed,
    ];
    all_examples.into_iter().take(n).collect()
}

/// Direction of conversion for the test
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConversionDirection {
    /// JSON → TOON (LLM writes TOON)
    JsonToToon,
    /// TOON → JSON (LLM reads TOON)
    ToonToJson,
}

impl fmt::Display for ConversionDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionDirection::JsonToToon => write!(f, "json_to_toon"),
            ConversionDirection::ToonToJson => write!(f, "toon_to_json"),
        }
    }
}

/// Build a complete prompt for a test case
pub fn build_prompt(
    config: &PromptConfig,
    direction: ConversionDirection,
    input_data: &str,
) -> String {
    let mut parts = Vec::new();

    // Add context components
    for component in config.components() {
        parts.push(component.text());
    }

    // Add task instruction
    let task = match direction {
        ConversionDirection::JsonToToon => PromptComponent::TaskJsonToToon,
        ConversionDirection::ToonToJson => PromptComponent::TaskToonToJson,
    };
    parts.push(task.text());

    // Add the input data
    parts.push(input_data);

    parts.join("\n\n")
}

// =============================================================================
// Prompt Text Constants
// =============================================================================

const SPEC_GRAMMAR: &str = r#"## TOON Format Grammar

TOON (Token-Oriented Object Notation) encodes JSON data compactly.

Key syntax:
- Objects use `key: value` pairs, one per line
- Nested objects are indented (2 spaces per level)
- Arrays declare size: `items[3]: a,b,c`
- Tabular arrays (uniform objects): `users[2]{id,name}:` followed by rows
- Primitives: numbers, true, false, null are unquoted
- Strings are unquoted unless they need escaping"#;

const SPEC_RULES: &str = r#"## TOON Quoting and Escaping Rules

Strings MUST be quoted with double quotes if they contain:
- Empty string: ""
- Leading/trailing whitespace
- Reserved literals: true, false, null
- Numbers or number-like patterns
- Special characters: : " \ [ ] { } , newline, tab
- The active delimiter (comma by default)

Valid escape sequences (only these):
- \\ → backslash
- \" → double quote
- \n → newline
- \r → carriage return
- \t → tab

Indentation:
- Use exactly 2 spaces per nesting level
- No tabs for indentation
- One space after colon in key-value pairs"#;

const EXAMPLE_SIMPLE: &str = r#"## Example: Simple Object

JSON:
```json
{"name": "Ada", "age": 30, "active": true}
```

TOON:
```toon
name: Ada
age: 30
active: true
```"#;

const EXAMPLE_NESTED: &str = r#"## Example: Nested Object

JSON:
```json
{"user": {"profile": {"name": "Ada", "role": "admin"}}}
```

TOON:
```toon
user:
  profile:
    name: Ada
    role: admin
```"#;

const EXAMPLE_TABULAR: &str = r#"## Example: Tabular Array

JSON:
```json
{"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}
```

TOON (tabular format for uniform objects):
```toon
users[2]{id,name}:
  1,Alice
  2,Bob
```

The header `[2]{id,name}` declares: 2 elements, each with fields id and name.
Each row contains comma-separated values in field order."#;

const EXAMPLE_MIXED: &str = r#"## Example: Mixed Array

JSON:
```json
{"items": [1, "text", {"nested": true}]}
```

TOON (list syntax for non-uniform arrays):
```toon
items[3]:
  - 1
  - text
  - nested: true
```

Mixed arrays use `- ` prefix for each item."#;

const TASK_JSON_TO_TOON: &str = r#"## Task

Convert the following JSON to TOON format. Output ONLY the TOON, no explanation.

JSON to convert:"#;

const TASK_TOON_TO_JSON: &str = r#"## Task

Convert the following TOON to JSON format. Output ONLY valid JSON, no explanation.

TOON to convert:"#;

const TASK_TOON_QUERY: &str = r#"## Task

Answer the question about the following TOON data. Be concise and precise.

TOON data:"#;

/// Build a query prompt for TOON comprehension
pub fn build_query_prompt(
    config: &PromptConfig,
    toon_data: &str,
    question: &str,
) -> String {
    let mut parts = Vec::new();

    // Add context components
    for component in config.components() {
        parts.push(component.text().to_string());
    }

    // Add task instruction
    parts.push(TASK_TOON_QUERY.to_string());

    // Add the TOON data
    parts.push(format!("```\n{}\n```", toon_data));

    // Add the question
    parts.push(format!("\nQuestion: {}", question));

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_config_display() {
        assert_eq!(PromptConfig::ZeroShot.to_string(), "zero_shot");
        assert_eq!(PromptConfig::FewShot(3).to_string(), "few_shot_3");
        assert_eq!(
            PromptConfig::SpecPlusFewShot(2).to_string(),
            "spec_plus_few_shot_2"
        );
    }

    #[test]
    fn test_components_count() {
        assert!(PromptConfig::ZeroShot.components().is_empty());
        assert_eq!(PromptConfig::SpecOnly.components().len(), 2);
        assert_eq!(PromptConfig::FewShot(3).components().len(), 3);
        assert_eq!(PromptConfig::Full.components().len(), 6);
    }

    #[test]
    fn test_build_prompt_includes_input() {
        let prompt = build_prompt(
            &PromptConfig::ZeroShot,
            ConversionDirection::JsonToToon,
            r#"{"test": 1}"#,
        );
        assert!(prompt.contains(r#"{"test": 1}"#));
        assert!(prompt.contains("Convert the following JSON to TOON"));
    }

    #[test]
    fn test_build_prompt_includes_examples() {
        let prompt = build_prompt(
            &PromptConfig::FewShot(2),
            ConversionDirection::JsonToToon,
            "{}",
        );
        assert!(prompt.contains("Example: Simple Object"));
        assert!(prompt.contains("Example: Nested Object"));
        assert!(!prompt.contains("Example: Tabular Array")); // Only 2 examples
    }
}
