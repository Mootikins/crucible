//! Integration tests that parse on-disk workflow fixtures end-to-end.

use crucible_core::parser::types::{Frontmatter, FrontmatterFormat, ParsedNote, WorkflowDoc};
use std::path::PathBuf;

fn parse_fixture(name: &str) -> WorkflowDoc {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/workflows")
        .join(name);
    let source = std::fs::read_to_string(&path).expect("fixture readable");

    let fm = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---\n").map(|i| rest[..i].to_string()))
        .map(|yaml| Frontmatter::new(yaml, FrontmatterFormat::Yaml));

    let mut note = ParsedNote::new(path.clone());
    note.frontmatter = fm;

    WorkflowDoc::from_parsed(&note, &source).expect("frontmatter declares workflow")
}

#[test]
fn basic_fixture_has_expected_shape() {
    let wf = parse_fixture("basic.md");

    assert_eq!(wf.title, "Basic Fixture Workflow");
    assert_eq!(
        wf.description.as_deref(),
        Some("Covers goals, validation, gates, data flow, and nested steps.",)
    );

    assert_eq!(wf.goals.len(), 2);
    assert_eq!(wf.validations.len(), 3);
    assert_eq!(
        wf.validations[0].command.as_deref(),
        Some("cargo test --workspace"),
    );

    assert_eq!(wf.preamble_gates.len(), 1);

    assert_eq!(wf.steps.len(), 3);
    assert_eq!(wf.steps[0].output.as_deref(), Some("plan"));
    assert_eq!(wf.steps[1].agent.as_deref(), Some("developer"));
    assert_eq!(wf.steps[1].children.len(), 2);

    let deploy = &wf.steps[2];
    assert_eq!(
        deploy.attributes.get("type").map(String::as_str),
        Some("fan")
    );
    assert_eq!(deploy.gates.len(), 1);
    assert_eq!(deploy.children.len(), 2);
}
