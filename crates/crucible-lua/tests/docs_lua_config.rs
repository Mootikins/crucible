//! Every `lua` fence in the configuration docs must evaluate.
//!
//! The TOML sibling (`crucible-core/tests/docs_config.rs`) proved the value
//! of the gate: a docs example nobody executes is a docs example that
//! drifts. Now that the configuration reference teaches Lua, its blocks get
//! the same treatment — each one runs through
//! [`crucible_lua::evaluate_config_source`], the same store-and-evaluate
//! construction as the daemon's boot, and must produce a config that
//! extracts.
//!
//! A fence a doc deliberately does not want validated (it demonstrates a
//! failure, or reads a file that only exists on a user's machine) carries
//! the same marker the TOML gate uses, on the line above:
//! `<!-- crucible:not-config -->`.
//!
//! Run with: `cargo test -p crucible-lua --test docs_lua_config -- --ignored`
//! (part of `just lint docs`).

use std::path::PathBuf;

const NOT_CONFIG_MARKER: &str = "<!-- crucible:not-config -->";

/// The docs files whose `lua` fences are configuration examples.
const DOC_FILES: &[&str] = &["docs/Help/Configuration.md"];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/crucible-lua has a workspace root two levels up")
        .to_path_buf()
}

struct LuaBlock {
    line: usize,
    body: String,
}

/// The `lua`-tagged fenced blocks in `content`, skipping any fence directly
/// under the not-config marker. Mirrors the TOML extractor's rules: the
/// marker applies to the next fence and nothing else, and any other prose
/// between the two clears it.
fn extract_lua_blocks(content: &str) -> Vec<LuaBlock> {
    struct Open {
        line: usize,
        ticks: usize,
        indent: usize,
        is_lua: bool,
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
                    if fence.is_lua {
                        blocks.push(LuaBlock {
                            line: fence.line,
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
                    let is_lua = info
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .next()
                        == Some("lua")
                        && !marked;
                    marked = false;
                    open = Some(Open {
                        line: idx + 1,
                        ticks,
                        indent,
                        is_lua,
                        body: Vec::new(),
                    });
                } else if !trimmed.is_empty() {
                    marked = trimmed.contains(NOT_CONFIG_MARKER);
                }
            }
        }
    }

    blocks
}

#[test]
#[ignore = "requires: dev kiln — evaluates every Lua block in docs/ through the config boot"]
fn docs_lua_blocks_evaluate_as_config() {
    let root = workspace_root();
    let mut checked = 0usize;
    let mut failures = Vec::new();

    for rel in DOC_FILES {
        let path = root.join(rel);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        for block in extract_lua_blocks(&content) {
            checked += 1;
            if let Err(e) = crucible_lua::evaluate_config_source(&block.body) {
                failures.push(format!("{rel}:{} — {e}", block.line));
            }
        }
    }

    assert!(
        checked > 0,
        "no lua blocks found — the extractor or the docs moved"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} documented Lua config blocks do not evaluate:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The extractor itself: lua fences are taken, other languages are not, and
/// the marker skips exactly one block.
#[test]
fn extracts_lua_fences_and_honours_the_marker() {
    let content = "\
```lua
cru.config.set({})
```

```toml
a = 1
```

<!-- crucible:not-config -->
```lua
error('demonstrates a failure')
```

```lua
-- kept
```
";
    let blocks = extract_lua_blocks(content);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].line, 1);
    assert!(blocks[1].body.contains("kept"));
}
