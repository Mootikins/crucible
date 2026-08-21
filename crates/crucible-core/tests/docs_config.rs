//! Config-truth test for the documentation kiln.
//!
//! Every ```` ```toml ```` block under `docs/Help/**` and `docs/Guides/**` is fed
//! to the real config loader ([`CliAppConfig::load`]) and then deserialized a
//! second time with every ignored key recorded. The invariant: **nothing in the
//! user-facing docs teaches config the loader rejects, or config it silently
//! throws away.**
//!
//! Run with: `cargo test -p crucible-core --test docs_config -- --ignored`
//! (or `just lint docs`, which is what the CI `docs` job invokes).
//!
//! # Why the second pass
//!
//! No struct in the config tree carries `#[serde(deny_unknown_fields)]`, so the
//! loader accepts `batch_size = 100` under a provider that has no such field —
//! it deserializes clean and is dropped on the floor. Loading alone therefore
//! proves only that a block is syntactically TOML and free of the two
//! hand-coded legacy rejections; it does not prove a single documented field
//! name is real. `serde_ignored` closes that: every key the config types do not
//! claim is reported with the `file:line` of the block that taught it.
//!
//! # Which config a block is
//!
//! A fence labelled ```` ```toml title=".crucible/project.toml" ```` is checked
//! against [`ProjectConfig`], everything else against `CliAppConfig`. See
//! [`ConfigKind`].
//!
//! # Why whole-config, not fragment parsing
//!
//! A doc snippet is almost always partial — a bare `[acp.agents.my-claude]`
//! stanza, three lines of `[llm.providers.*]`. Every field of `CliAppConfig` is
//! `#[serde(default)]`, so a partial stanza *is* a valid whole config. If a
//! fragment does not load, the doc is teaching a shape the loader refuses, and
//! that is precisely the defect this test exists to catch.
//!
//! # Excluding a block
//!
//! Some blocks illustrate config the loader is *supposed* to reject (a
//! migration note showing the legacy key you must remove). Mark those in the
//! document, immediately above the fence:
//!
//! ````text
//! <!-- crucible:not-config -->
//! ```toml
//! [embedding]           # legacy — rejected since 0.2
//! ```
//! ````
//!
//! An in-document marker rather than a list in this file: the reason lives next
//! to the block, and moving or renaming the doc cannot desynchronise it.

mod common;

use common::docs_kiln::{docs_root, markdown_files, workspace_root};

/// Directories under the docs kiln whose TOML must load. `docs/Meta/**` is
/// design and planning material, not instructions to a user, so it is out of
/// scope.
const DOC_ROOTS: &[&str] = &["docs/Help", "docs/Guides"];

/// Marker that opts a single following fence out of the check.
const NOT_CONFIG_MARKER: &str = "crucible:not-config";

/// Which config file a block is teaching.
///
/// Docs name it in the fence info string
/// (```` ```toml title=".crucible/project.toml" ````). Without this the four
/// `project.toml` blocks in the kiln were fed to `CliAppConfig` and "failed" for
/// being a perfectly valid project config.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ConfigKind {
    /// The global `config.toml`, loaded by [`CliAppConfig::load`].
    Cli,
    /// A project's `.crucible/project.toml`.
    Project,
}

impl ConfigKind {
    fn of(info: &str) -> Self {
        if info.contains("project.toml") {
            Self::Project
        } else {
            Self::Cli
        }
    }
}

/// A fenced ```` ```toml ```` block.
#[derive(Debug, PartialEq, Eq)]
struct TomlBlock {
    /// 1-indexed line of the opening fence.
    line: usize,
    kind: ConfigKind,
    body: String,
}

/// Extract every ```` ```toml ```` block from markdown.
///
/// Fence handling follows CommonMark closely enough for the kiln: a block is
/// closed only by a fence of at least as many backticks with no info string, so
/// a ```` ```markdown ```` block that *contains* a ```` ```toml ```` fence is
/// one block, not two. Indented fences (inside list items) have the opening
/// fence's indentation stripped from their body.
fn extract_toml_blocks(content: &str) -> Vec<TomlBlock> {
    /// The fence currently being read.
    struct Open {
        line: usize,
        ticks: usize,
        indent: usize,
        kind: Option<ConfigKind>,
        body: Vec<String>,
    }

    let mut blocks = Vec::new();
    let mut open: Option<Open> = None;
    let mut marked = false;

    for (idx, raw) in content.lines().enumerate() {
        let indent = raw.len() - raw.trim_start().len();
        let trimmed = raw.trim_start();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        let info = trimmed[ticks..].trim();

        match open.as_mut() {
            Some(fence) => {
                if ticks >= fence.ticks && info.is_empty() {
                    if let Some(kind) = fence.kind {
                        blocks.push(TomlBlock {
                            line: fence.line,
                            kind,
                            body: fence.body.join("\n"),
                        });
                    }
                    open = None;
                } else {
                    let dedented = raw.strip_prefix(&" ".repeat(fence.indent)).unwrap_or(raw);
                    fence.body.push(dedented.to_string());
                }
            }
            None => {
                if ticks >= 3 {
                    let is_toml = info.split(|c: char| c.is_whitespace() || c == ',').next()
                        == Some("toml")
                        && !marked;
                    marked = false;
                    open = Some(Open {
                        line: idx + 1,
                        ticks,
                        indent,
                        kind: is_toml.then(|| ConfigKind::of(info)),
                        body: Vec::new(),
                    });
                } else if !trimmed.is_empty() {
                    // The marker applies to the next fence and nothing else;
                    // any other prose between the two clears it.
                    marked = trimmed.contains(NOT_CONFIG_MARKER);
                }
            }
        }
    }

    blocks
}

/// Keys in `body` that `T` does not claim, in dotted form
/// (`enrichment.provider.batch_size`). `Err` if the block cannot be
/// deserialized into `T` at all.
fn ignored_keys<T: serde::de::DeserializeOwned>(body: &str) -> Result<Vec<String>, String> {
    let value: toml::Value = toml::from_str(body).map_err(|e| e.to_string())?;
    let mut ignored = Vec::new();
    serde_ignored::deserialize::<_, _, T>(value, |path| ignored.push(path.to_string()))
        .map_err(|e| e.to_string())?;
    Ok(ignored)
}

/// Check every TOML block in one document, returning a `file:line`-prefixed
/// message per defect. `config_path` is a scratch file the loader is pointed
/// at; its directory also anchors `{file:…}` reference resolution.
fn load_failures(
    display: &str,
    content: &str,
    config_path: &std::path::Path,
) -> (usize, Vec<String>) {
    use crucible_core::config::{CliAppConfig, ProjectConfig};

    let blocks = extract_toml_blocks(content);
    let mut failures = Vec::new();

    for block in &blocks {
        let at = format!("{display}:{}", block.line);
        let quoted = format!("\n--- block ---\n{}\n-------------", block.body.trim_end());

        // `CliAppConfig` alone has a path-based loader, and it is the one that
        // carries the legacy-key rejections; `ProjectConfig` is checked by
        // deserialization only.
        let keys = match block.kind {
            ConfigKind::Cli => {
                std::fs::write(config_path, &block.body).expect("write scratch config");
                if let Err(e) = CliAppConfig::load(Some(config_path.to_path_buf()), None, None) {
                    failures.push(format!("{at}: {e}{quoted}"));
                    continue;
                }
                ignored_keys::<CliAppConfig>(&block.body)
            }
            ConfigKind::Project => ignored_keys::<ProjectConfig>(&block.body),
        };

        match keys {
            Ok(keys) if keys.is_empty() => {}
            Ok(keys) => failures.push(format!(
                "{at}: no such config key(s): {}{quoted}",
                keys.join(", ")
            )),
            Err(e) => failures.push(format!("{at}: {e}{quoted}")),
        }
    }

    (blocks.len(), failures)
}

// ============================================================================
// TEST: Every documented TOML block survives the real config loader
// ============================================================================

#[test]
#[ignore = "requires: dev kiln — loads every TOML block in docs/ through the config loader"]
fn docs_toml_blocks_load_as_config() {
    let files = markdown_files(DOC_ROOTS);
    assert!(
        !files.is_empty(),
        "no markdown found under {DOC_ROOTS:?} — root discovery is broken"
    );

    let workspace = workspace_root();
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");

    let mut failures = Vec::new();
    let mut checked = 0;

    for file in &files {
        let content = std::fs::read_to_string(file).expect("read doc");
        // Repo-relative, so a failure can be pasted straight into an editor.
        let display = file.strip_prefix(&workspace).unwrap_or(file).display();
        let (n, mut file_failures) = load_failures(&display.to_string(), &content, &config_path);
        checked += n;
        failures.append(&mut file_failures);
    }

    assert!(
        checked > 0,
        "no ```toml blocks found — the extractor is broken, not the docs"
    );

    if !failures.is_empty() {
        panic!(
            "DOC CONFIG VALIDATION FAILED ({}/{checked} blocks rejected or carrying keys no config type claims)\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }

    println!("All {checked} TOML blocks in {DOC_ROOTS:?} load as config");
}

/// `docs/Config.toml` is the annotated reference config the docs point readers
/// at (`docs/Help/Configuration.md` cites it as the home of `[workspace]`,
/// `[server]`, `[[schedules]]`, `[plugins.*]` and `runtimepath`). It is a whole
/// file rather than a fenced block, so the sweep above never sees it — and it
/// is the single largest piece of config prose in the repo.
#[test]
#[ignore = "requires: dev kiln — loads the reference config through the config loader"]
fn the_reference_config_loads() {
    use crucible_core::config::CliAppConfig;

    let reference = docs_root().join("Config.toml");
    let body = std::fs::read_to_string(&reference).expect("read docs/Config.toml");

    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(&config_path, &body).expect("write scratch config");

    if let Err(e) = CliAppConfig::load(Some(config_path), None, None) {
        panic!("docs/Config.toml is rejected by CliAppConfig::load: {e}");
    }

    match ignored_keys::<CliAppConfig>(&body) {
        Ok(keys) if keys.is_empty() => {}
        Ok(keys) => panic!(
            "docs/Config.toml documents {} key(s) no config type claims:\n{}",
            keys.len(),
            keys.join("\n")
        ),
        Err(e) => panic!("docs/Config.toml could not be deserialized: {e}"),
    }
}

// ============================================================================
// Helper tests — the extractor is the part that can silently check nothing
// ============================================================================

/// The suite would be worthless if `load_failures` could not fail. `[embedding]`
/// is a key the loader explicitly rejects (`cli_app.rs`), so a doc containing it
/// must be named with its exact line.
#[test]
fn a_rejected_block_is_reported_with_its_file_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let content = "\
prose

```toml
[llm]
default = \"local\"
```

more prose

```toml
[embedding]
model = \"nomic-embed-text\"
```
";
    let (checked, failures) = load_failures("Doc.md", content, &dir.path().join("config.toml"));

    assert_eq!(checked, 2);
    assert_eq!(failures.len(), 1, "only the legacy block should fail");
    assert!(
        failures[0].starts_with("Doc.md:10:"),
        "failure must name file and line, got: {}",
        failures[0]
    );
    assert!(failures[0].contains("[embedding]"), "{}", failures[0]);
}

/// The loader accepts unknown keys silently — no config struct sets
/// `deny_unknown_fields` — so without the `serde_ignored` pass a doc could
/// invent any field name it liked and stay green.
#[test]
fn a_block_teaching_a_field_that_does_not_exist_is_reported_with_its_file_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let content = "\
```toml
[llm]
default = \"local\"
invented_field = 100
```
";
    let (checked, failures) = load_failures("Doc.md", content, &dir.path().join("config.toml"));

    assert_eq!(checked, 1);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].starts_with("Doc.md:1:") && failures[0].contains("llm.invented_field"),
        "{}",
        failures[0]
    );
}

/// `.crucible/project.toml` is a different shape from `config.toml` — its
/// `kilns` is an array of attachments, not a name-to-path map. Feeding it to
/// `CliAppConfig` reports a correct block as broken.
#[test]
fn a_project_toml_block_is_checked_against_the_project_config() {
    let dir = tempfile::tempdir().unwrap();
    let content = "\
```toml title=\".crucible/project.toml\"
[[kilns]]
path = \"~/notes/work\"
data_classification = \"internal\"
```

```toml title=\".crucible/project.toml\"
[security.shell]
invented_field = true
```
";
    let blocks = extract_toml_blocks(content);
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|b| b.kind == ConfigKind::Project));

    let (checked, failures) = load_failures("Doc.md", content, &dir.path().join("config.toml"));
    assert_eq!(checked, 2);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].starts_with("Doc.md:7:")
            && failures[0].contains("security.shell.invented_field"),
        "{}",
        failures[0]
    );
}

#[test]
fn extracts_toml_fences_and_skips_other_languages() {
    let content = "\
intro
```toml
a = 1
```
```bash
cru chat
```
```toml
b = 2
```
";
    let blocks = extract_toml_blocks(content);
    assert_eq!(
        blocks,
        vec![
            TomlBlock {
                line: 2,
                kind: ConfigKind::Cli,
                body: "a = 1".into()
            },
            TomlBlock {
                line: 8,
                kind: ConfigKind::Cli,
                body: "b = 2".into()
            },
        ]
    );
}

#[test]
fn a_toml_fence_nested_in_an_outer_fence_is_not_extracted() {
    let content = "\
````markdown
```toml
a = 1
```
````
";
    assert!(extract_toml_blocks(content).is_empty());
}

#[test]
fn an_indented_fence_has_its_indentation_stripped() {
    let content = "\
- step one:

  ```toml
  [llm]
  default = \"local\"
  ```
";
    let blocks = extract_toml_blocks(content);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].body, "[llm]\ndefault = \"local\"");
}

#[test]
fn the_marker_excludes_only_the_block_immediately_below_it() {
    let content = "\
<!-- crucible:not-config -->
```toml
[embedding]
```

```toml
b = 2
```
";
    let blocks = extract_toml_blocks(content);
    assert_eq!(
        blocks,
        vec![TomlBlock {
            line: 6,
            kind: ConfigKind::Cli,
            body: "b = 2".into()
        }]
    );
}
