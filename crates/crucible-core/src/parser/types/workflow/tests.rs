//! Unit tests for the workflow parser.
//!
//! Split out from `workflow.rs` to keep the impl file under the 1500-line
//! decomposition ceiling. All helpers are intentionally scoped to this
//! module (via `super::*`).

use super::*;
use crate::parser::types::{Frontmatter, FrontmatterFormat, ParsedNote};
use std::path::PathBuf;

fn parse_workflow(source: &str) -> Option<WorkflowDoc> {
    parse_workflow_at(source, "test.md")
}

fn parse_workflow_at(source: &str, path: &str) -> Option<WorkflowDoc> {
    let (fm, _) = split_frontmatter(source);
    let mut note = ParsedNote::new(PathBuf::from(path));
    note.frontmatter = fm;
    WorkflowDoc::from_parsed(&note, source)
}

fn split_frontmatter(source: &str) -> (Option<Frontmatter>, String) {
    if let Some(rest) = source.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let yaml = &rest[..end];
            let body = rest[end + "\n---\n".len()..].to_string();
            return (
                Some(Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml)),
                body,
            );
        }
    }
    (None, source.to_string())
}

// ---- from_parsed / frontmatter gate ----

#[test]
fn non_workflow_note_returns_none() {
    let source = "---\ntype: note\n---\n# Hi\n";
    assert!(parse_workflow(source).is_none());
}

#[test]
fn note_without_frontmatter_returns_none() {
    let source = "# Hi\n";
    assert!(parse_workflow(source).is_none());
}

#[test]
fn note_without_type_field_returns_none() {
    let source = "---\ntitle: X\n---\n## Step\n";
    assert!(parse_workflow(source).is_none());
}

#[test]
fn workflow_type_is_case_insensitive() {
    let source = "---\ntype: Workflow\n---\n## Step\n";
    assert!(parse_workflow(source).is_some());
}

#[test]
fn empty_workflow_returns_some_with_no_steps() {
    let source = "---\ntype: workflow\n---\n";
    let wf = parse_workflow(source).expect("parses");
    assert!(wf.steps.is_empty());
    assert!(wf.goals.is_empty());
    assert!(wf.validations.is_empty());
}

#[test]
fn title_from_frontmatter() {
    let source = "---\ntype: workflow\ntitle: Deploy Feature\n---\n";
    let wf = parse_workflow(source).expect("parses");
    assert_eq!(wf.title, "Deploy Feature");
}

#[test]
fn title_falls_back_to_filename() {
    let source = "---\ntype: workflow\n---\n";
    let wf = parse_workflow_at(source, "deploy-feature.md").expect("parses");
    assert_eq!(wf.title, "deploy-feature");
}

// ---- heading suffix extraction (table-driven) ----

#[test]
fn heading_suffix_plain_title() {
    let (title, agent, output, attrs) = parse_heading_suffix("Simple Step");
    assert_eq!(title, "Simple Step");
    assert_eq!(agent, None);
    assert_eq!(output, None);
    assert!(attrs.is_empty());
}

#[test]
fn heading_suffix_agent_only() {
    let (title, agent, _, _) = parse_heading_suffix("Design @architect");
    assert_eq!(title, "Design");
    assert_eq!(agent, Some("architect".to_string()));
}

#[test]
fn heading_suffix_output_only() {
    let (title, _, output, _) = parse_heading_suffix("Parse -> config");
    assert_eq!(title, "Parse");
    assert_eq!(output, Some("config".to_string()));
}

#[test]
fn heading_suffix_full_combo() {
    let (title, agent, output, attrs) = parse_heading_suffix("Step @a -> b [type:: fan]");
    assert_eq!(title, "Step");
    assert_eq!(agent, Some("a".to_string()));
    assert_eq!(output, Some("b".to_string()));
    assert_eq!(attrs.get("type").map(String::as_str), Some("fan"));
}

#[test]
fn heading_suffix_version_arrow_not_matched() {
    // 2.0 is not a bare ident, so -> must not be extracted.
    let (title, _, output, _) = parse_heading_suffix("Migration: 1.0 -> 2.0");
    assert_eq!(title, "Migration: 1.0 -> 2.0");
    assert_eq!(output, None);
}

#[test]
fn heading_suffix_middle_at_not_matched() {
    // @example.com is in the middle, not trailing; must remain in title.
    let (title, agent, _, _) = parse_heading_suffix("Parse RFC 5322 @example.com addresses");
    assert_eq!(title, "Parse RFC 5322 @example.com addresses");
    assert_eq!(agent, None);
}

#[test]
fn heading_suffix_trailing_backticks_fine() {
    let (title, agent, _, _) = parse_heading_suffix("Implement `foo()` @dev");
    assert_eq!(title, "Implement `foo()`");
    assert_eq!(agent, Some("dev".to_string()));
}

#[test]
fn heading_suffix_metadata_only() {
    let (title, _, _, attrs) = parse_heading_suffix("[type:: fan]");
    assert_eq!(title, "");
    assert_eq!(attrs.get("type").map(String::as_str), Some("fan"));
}

#[test]
fn heading_suffix_multiple_attrs() {
    let (_, _, _, attrs) = parse_heading_suffix("Step [priority:: high] [owner:: alice]");
    assert_eq!(attrs.get("priority").map(String::as_str), Some("high"));
    assert_eq!(attrs.get("owner").map(String::as_str), Some("alice"));
}

#[test]
fn heading_suffix_only_agent() {
    let (title, agent, _, _) = parse_heading_suffix("@only-agent");
    assert_eq!(title, "");
    assert_eq!(agent, Some("only-agent".to_string()));
}

// ---- tree structure ----

#[test]
fn tree_flat_siblings() {
    let source = "---\ntype: workflow\n---\n## A\n## B\n## C\n";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps.len(), 3);
    assert!(wf.steps.iter().all(|s| s.children.is_empty()));
}

#[test]
fn tree_parent_with_children() {
    let source = "\
---
type: workflow
---
## Deploy
### Staging
### Production
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps.len(), 1);
    assert_eq!(wf.steps[0].title, "Deploy");
    assert_eq!(wf.steps[0].children.len(), 2);
    assert_eq!(wf.steps[0].children[0].title, "Staging");
    assert_eq!(wf.steps[0].children[1].title, "Production");
}

#[test]
fn tree_level_skip_still_nests() {
    // Level-2 then level-4 — level-4 still a child (we don't enforce
    // strict increments).
    let source = "---\ntype: workflow\n---\n## A\n#### Deep\n## B\n";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps.len(), 2);
    assert_eq!(wf.steps[0].children.len(), 1);
    assert_eq!(wf.steps[0].children[0].title, "Deep");
}

#[test]
fn level_one_heading_treated_as_step() {
    // We don't try to be clever about "document title" — if they use `#`
    // as a step, it's a step. The common pattern is `## Step` anyway.
    let source = "---\ntype: workflow\n---\n# Whole Workflow\n## Sub\n";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps.len(), 1);
    assert_eq!(wf.steps[0].title, "Whole Workflow");
    assert_eq!(wf.steps[0].children.len(), 1);
}

#[test]
fn headings_inside_code_fence_ignored() {
    let source = "\
---
type: workflow
---
## Real Step

```markdown
## Fake Step
```

## Another Real
";
    let wf = parse_workflow(source).unwrap();
    let titles: Vec<_> = wf.steps.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, vec!["Real Step", "Another Real"]);
}

// ---- goals ----

#[test]
fn goals_extracted_as_task_items() {
    let source = "\
---
type: workflow
---
## Goals

- [ ] Ship CSV export
- [x] Ship JSON export
- [ ] Cancelable for large files

## Implement
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.goals.len(), 3);
    assert_eq!(wf.goals[0].content, "Ship CSV export");
    assert_eq!(wf.goals[0].status, CheckboxStatus::Pending);
    assert_eq!(wf.goals[1].status, CheckboxStatus::Done);
    // Goals heading is NOT in the steps tree.
    assert_eq!(wf.steps.len(), 1);
    assert_eq!(wf.steps[0].title, "Implement");
}

#[test]
fn goals_with_inline_metadata() {
    let source = "\
---
type: workflow
---
## Goals

- [ ] First goal [id:: g1]
- [ ] Second goal [id:: g2] [priority:: high]
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.goals[0].id, "g1");
    assert_eq!(wf.goals[1].id, "g2");
    assert_eq!(
        wf.goals[1]
            .metadata
            .get("priority")
            .and_then(|m| m.as_string()),
        Some("high")
    );
}

#[test]
fn goals_ignores_non_task_bullets() {
    let source = "\
---
type: workflow
---
## Goals

- [ ] Real goal
- Just a bullet, not a goal
- [x] Another goal
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.goals.len(), 2);
}

#[test]
fn nested_goals_heading_not_promoted() {
    let source = "\
---
type: workflow
---
## Implement

### Goals

- [ ] Not a top-level goal
";
    let wf = parse_workflow(source).unwrap();
    assert!(wf.goals.is_empty());
    // Nested `### Goals` stays in the tree as a child step.
    assert_eq!(wf.steps.len(), 1);
    assert_eq!(wf.steps[0].children.len(), 1);
}

#[test]
fn duplicate_goals_last_wins_with_warning() {
    let source = "\
---
type: workflow
---
## Goals

- [ ] First-section goal

## Goals

- [ ] Second-section goal
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.goals.len(), 1);
    assert_eq!(wf.goals[0].content, "Second-section goal");
    assert_eq!(wf.warnings.len(), 1);
    assert!(matches!(
        wf.warnings[0],
        WorkflowParseWarning::DuplicateGoalsSection { .. }
    ));
}

#[test]
fn task_list_not_under_goals_stays_in_step() {
    let source = "\
---
type: workflow
---
## Implement

- [ ] This is a step-level task, NOT a goal
";
    let wf = parse_workflow(source).unwrap();
    assert!(wf.goals.is_empty());
}

// ---- validation ----

#[test]
fn validation_entries_extract_commands() {
    let source = "\
---
type: workflow
---
## Validation

- `cargo test` passes
- `cargo clippy --all-targets` clean
- Manual: CSV download under 2s for 10k rows
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.validations.len(), 3);
    assert_eq!(wf.validations[0].command.as_deref(), Some("cargo test"));
    assert_eq!(wf.validations[0].description, "passes");
    assert_eq!(
        wf.validations[1].command.as_deref(),
        Some("cargo clippy --all-targets")
    );
    assert_eq!(wf.validations[1].description, "clean");
    assert_eq!(wf.validations[2].command, None);
    assert_eq!(
        wf.validations[2].description,
        "Manual: CSV download under 2s for 10k rows"
    );
}

#[test]
fn validation_multiple_code_spans_treated_as_prose() {
    let source = "\
---
type: workflow
---
## Validation

- Run `cargo test` then `cargo clippy`
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.validations.len(), 1);
    assert_eq!(wf.validations[0].command, None);
}

#[test]
fn validation_bare_command_falls_back_to_description() {
    let source = "\
---
type: workflow
---
## Validation

- `cargo fmt --check`
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.validations.len(), 1);
    // Fallback rule: description holds the command text, no command field.
    assert_eq!(wf.validations[0].description, "cargo fmt --check");
    assert_eq!(wf.validations[0].command, None);
}

#[test]
fn validation_task_style_items_still_parse() {
    let source = "\
---
type: workflow
---
## Validation

- [ ] `cargo test` passes
- [x] Performance acceptable
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.validations.len(), 2);
    assert_eq!(wf.validations[0].command.as_deref(), Some("cargo test"));
    assert_eq!(wf.validations[1].description, "Performance acceptable");
}

#[test]
fn validation_missing_section_is_empty() {
    let source = "---\ntype: workflow\n---\n## Step\n";
    let wf = parse_workflow(source).unwrap();
    assert!(wf.validations.is_empty());
}

#[test]
fn validation_without_goals_still_parses() {
    let source = "\
---
type: workflow
---
## Validation

- `cargo test` passes
";
    let wf = parse_workflow(source).unwrap();
    assert!(wf.goals.is_empty());
    assert_eq!(wf.validations.len(), 1);
}

#[test]
fn duplicate_validation_last_wins_with_warning() {
    let source = "\
---
type: workflow
---
## Validation

- First section

## Validation

- Second section
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.validations.len(), 1);
    assert_eq!(wf.validations[0].description, "Second section");
    assert!(wf
        .warnings
        .iter()
        .any(|w| matches!(w, WorkflowParseWarning::DuplicateValidationSection { .. })));
}

#[test]
fn nested_validation_heading_not_promoted() {
    let source = "\
---
type: workflow
---
## Implement

### Validation

- `cargo test` passes
";
    let wf = parse_workflow(source).unwrap();
    assert!(wf.validations.is_empty());
}

// ---- gates ----

#[test]
fn gate_inside_step_body() {
    let source = "\
---
type: workflow
---
## Deploy

> [!gate]
> Requires ops sign-off
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps.len(), 1);
    assert_eq!(wf.steps[0].gates.len(), 1);
    assert!(wf.steps[0].gates[0].content.contains("ops sign-off"));
    assert!(wf.preamble_gates.is_empty());
}

#[test]
fn gate_with_title() {
    let source = "\
---
type: workflow
---
## Deploy

> [!gate] Ops approval
> Body
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps[0].gates[0].title.as_deref(), Some("Ops approval"));
}

#[test]
fn gate_before_first_step_is_preamble() {
    let source = "\
---
type: workflow
---
> [!gate]
> Whole-workflow prior approval

## Do Thing
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.preamble_gates.len(), 1);
    assert_eq!(wf.steps[0].gates.len(), 0);
}

#[test]
fn gate_belongs_to_parent_not_child() {
    let source = "\
---
type: workflow
---
## Deploy

> [!gate]
> Approval required

### Staging
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps[0].gates.len(), 1);
    assert_eq!(wf.steps[0].children[0].gates.len(), 0);
}

#[test]
fn non_gate_callouts_ignored() {
    let source = "\
---
type: workflow
---
## Step

> [!note]
> Just a note
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps[0].gates.len(), 0);
}

// ---- step bodies and output references ----

#[test]
fn step_body_captures_markdown_between_headings() {
    let source = "\
---
type: workflow
---
## Analyze

First, read the config.
Then, summarize findings.

## Implement
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps[0].title, "Analyze");
    assert!(wf.steps[0].body.contains("read the config"));
    assert!(wf.steps[0].body.contains("summarize findings"));
    assert!(!wf.steps[0].body.contains("## Implement"));
}

#[test]
fn output_suffix_populates_step_output() {
    let source = "---\ntype: workflow\n---\n## Parse -> config\n";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps[0].output.as_deref(), Some("config"));
    assert_eq!(wf.steps[0].title, "Parse");
}

// ---- parallel markers ----

#[test]
fn ampersand_prefix_marks_step_parallel_and_is_stripped() {
    let source = "\
---
type: workflow
---
## &Build frontend
## &Build backend
## Run tests
";
    let wf = parse_workflow(source).unwrap();
    assert!(wf.steps[0].parallel);
    assert_eq!(wf.steps[0].title, "Build frontend");
    assert!(wf.steps[1].parallel);
    assert_eq!(wf.steps[1].title, "Build backend");
    assert!(!wf.steps[2].parallel);
    assert_eq!(wf.steps[2].title, "Run tests");
}

#[test]
fn parallel_heading_suffix_marks_child_steps_parallel() {
    let source = "\
---
type: workflow
---
## Build Artifacts (parallel)
### Build frontend
### Build backend
## Test
";
    let wf = parse_workflow(source).unwrap();
    let section = &wf.steps[0];
    assert_eq!(section.title, "Build Artifacts");
    assert!(!section.parallel, "the section itself stays sequential");
    assert_eq!(section.children.len(), 2);
    assert!(section.children.iter().all(|c| c.parallel));
    assert!(!wf.steps[1].parallel);
}

#[test]
fn parallel_suffix_is_case_insensitive() {
    for suffix in ["(parallel)", "(Parallel)", "(PARALLEL)"] {
        let source = format!("---\ntype: workflow\n---\n## Build {suffix}\n### A\n### B\n");
        let wf = parse_workflow(&source).unwrap();
        assert_eq!(wf.steps[0].title, "Build", "suffix {suffix} stripped");
        assert!(
            wf.steps[0].children.iter().all(|c| c.parallel),
            "suffix {suffix} marks children"
        );
    }
}

#[test]
fn ampersand_prefix_composes_with_agent_and_output_suffixes() {
    let source = "---\ntype: workflow\n---\n## &Build @builder -> artifact\n";
    let wf = parse_workflow(source).unwrap();
    let step = &wf.steps[0];
    assert!(step.parallel);
    assert_eq!(step.title, "Build");
    assert_eq!(step.agent.as_deref(), Some("builder"));
    assert_eq!(step.output.as_deref(), Some("artifact"));
}

#[test]
fn plain_ampersand_inside_title_is_not_a_marker() {
    let source = "---\ntype: workflow\n---\n## Fix A & B\n";
    let wf = parse_workflow(source).unwrap();
    assert!(!wf.steps[0].parallel);
    assert_eq!(wf.steps[0].title, "Fix A & B");
}

#[test]
fn parallel_suffix_on_step_without_children_is_stripped_harmlessly() {
    let source = "---\ntype: workflow\n---\n## Solo (parallel)\n";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps[0].title, "Solo");
    assert!(!wf.steps[0].parallel);
    assert!(wf.steps[0].children.is_empty());
}

#[test]
fn grandchildren_of_parallel_section_stay_sequential() {
    let source = "\
---
type: workflow
---
## Build (parallel)
### Frontend
#### Bundle assets
### Backend
";
    let wf = parse_workflow(source).unwrap();
    let section = &wf.steps[0];
    assert!(section.children[0].parallel);
    assert!(section.children[1].parallel);
    assert!(
        !section.children[0].children[0].parallel,
        "only direct children of a (parallel) section are marked"
    );
}

#[test]
fn parallel_marker_inside_word_is_not_stripped() {
    // "(parallel)" only counts as a marker at the very end of the title.
    let source = "---\ntype: workflow\n---\n## Run (parallel) builds\n";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps[0].title, "Run (parallel) builds");
}

// ---- fixture-style full examples ----

#[test]
fn deploy_example_from_docs_parses() {
    let source = "\
---
type: workflow
title: Deploy New Feature
---

## Goals

- [ ] Users can export data in CSV format
- [ ] Export respects active filters
- [ ] Large exports don't block the UI

## Plan the Implementation

Analyze requirements and identify affected components.

## Implement Changes @developer

Make code changes following existing patterns.

## Review and Deploy

> [!gate]
> Requires sign-off before production deployment

### Code Review @reviewer

### Deploy to Staging

### Deploy to Production
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.title, "Deploy New Feature");
    assert_eq!(wf.goals.len(), 3);
    assert_eq!(wf.steps.len(), 3);
    assert_eq!(wf.steps[1].agent.as_deref(), Some("developer"));
    let deploy = &wf.steps[2];
    assert_eq!(deploy.title, "Review and Deploy");
    assert_eq!(deploy.gates.len(), 1);
    assert_eq!(deploy.children.len(), 3);
    assert_eq!(deploy.children[0].agent.as_deref(), Some("reviewer"));
}

#[test]
fn data_flow_example_parses() {
    let source = "\
---
type: workflow
title: Data Flow
---

## Parse Configuration -> config

Read config.

## Validate Schema -> validated_config

Validate **config**.

## Generate Output

Use **validated_config**.
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps.len(), 3);
    assert_eq!(wf.steps[0].output.as_deref(), Some("config"));
    assert_eq!(wf.steps[1].output.as_deref(), Some("validated_config"));
    assert_eq!(wf.steps[2].output, None);
}

#[test]
fn iter_steps_depth_first() {
    let source = "\
---
type: workflow
---
## A
### A1
### A2
## B
### B1
";
    let wf = parse_workflow(source).unwrap();
    let titles: Vec<_> = wf.iter_steps().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, vec!["A", "A1", "A2", "B", "B1"]);
}

// ---- regression tests from code review ----

#[test]
fn gate_inside_code_fence_is_not_attached() {
    // Docs often contain fenced examples that *show* gate syntax. Those
    // must not create real gates on the enclosing step.
    let source = "\
---
type: workflow
---
## Step With Example

Here's how to write a gate:

```markdown
> [!gate]
> This is an example in docs
```

Real step content.
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.steps.len(), 1);
    assert_eq!(wf.steps[0].gates.len(), 0);
    assert!(wf.preamble_gates.is_empty());
}

#[test]
fn heading_with_bracketed_ticket_and_metadata_parses_correctly() {
    // The global `extract_inline_metadata` regex uses `[^:]+` for the
    // key, which greedily consumes the first `]` in a heading like
    // `## Impl [TICKET-123] @dev [priority:: high]` and produces a
    // nonsense key. The workflow parser uses a stricter regex that
    // doesn't cross `]` boundaries.
    let (title, agent, _output, attrs) =
        parse_heading_suffix("Implement [TICKET-123] @dev [priority:: high]");
    assert_eq!(
        attrs.get("priority").map(String::as_str),
        Some("high"),
        "priority attribute should be extracted cleanly"
    );
    assert_eq!(attrs.len(), 1, "no spurious attributes: {:?}", attrs);
    assert_eq!(agent.as_deref(), Some("dev"));
    assert_eq!(title, "Implement [TICKET-123]");
}

#[test]
fn goals_with_bracketed_ticket_and_metadata_parse_correctly() {
    let source = "\
---
type: workflow
---
## Goals

- [ ] Fix [TICKET-123] auth bug [id:: g1]
- [ ] Ship [TICKET-456] feature [id:: g2] [priority:: high]
";
    let wf = parse_workflow(source).unwrap();
    assert_eq!(wf.goals.len(), 2);
    assert_eq!(wf.goals[0].id, "g1");
    assert!(
        wf.goals[0].content.contains("TICKET-123"),
        "content preserves ticket reference: {:?}",
        wf.goals[0].content
    );
    assert_eq!(wf.goals[1].id, "g2");
    assert_eq!(
        wf.goals[1]
            .metadata
            .get("priority")
            .and_then(|m| m.as_string()),
        Some("high")
    );
}

#[test]
fn heading_inside_tilde_fence_ignored() {
    // Previously we only handled backtick fences; tilde fences are also
    // legal CommonMark and should mask their contents.
    let source = "\
---
type: workflow
---
## Real Step

~~~md
## Fake Step Inside Tilde Fence
~~~

## Another Real
";
    let wf = parse_workflow(source).unwrap();
    let titles: Vec<_> = wf.steps.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, vec!["Real Step", "Another Real"]);
}
