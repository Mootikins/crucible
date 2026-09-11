//! Every documented `cru.config.set` example must evaluate.
//!
//! The TOML sibling (`crucible-core/tests/docs_config.rs`) proved the value
//! of the gate: a docs example nobody executes is a docs example that
//! drifts. Now that the configuration reference teaches Lua, its blocks get
//! the same treatment — each one runs through
//! [`crucible_lua::evaluate_config_source`], the same store-and-evaluate
//! construction as the daemon's boot, and must produce a config that
//! extracts.
//!
//! # Which fence is a config example
//!
//! The TOML gate can take every `toml` fence, because every one of them is
//! config. A `lua` fence is usually plugin code: of the 216 under these two
//! roots, 14 configure anything. So a block is a config example when it
//! WRITES config — when it calls `cru.config.set`. Selecting on the call
//! rather than on a label the author must remember is what lets a config
//! example written in any document under these roots be checked on the day
//! it is written.
//!
//! A fence a doc deliberately does not want validated (it demonstrates a
//! failure, or reads a file that only exists on a user's machine) carries
//! the same marker the TOML gate uses, on the line above:
//! `<!-- crucible:not-config -->`.
//!
//! Run with: `cargo test -p crucible-lua --test docs_lua_config -- --ignored`
//! (part of `just lint docs`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use walkdir::WalkDir;

/// Marker that opts a single following fence out of the check.
///
/// The bare token, not the whole HTML comment, so a marker may carry its
/// reason inline the way the TOML gate's already do
/// (`<!-- crucible:not-config — this block is webhooks.toml -->`). The two
/// gates read the same spelling; an annotated marker that one honoured and
/// the other ignored is a silently unchecked block.
const NOT_CONFIG_MARKER: &str = "crucible:not-config";

/// The docs roots whose `lua` fences are configuration examples.
///
/// These are the two roots the TOML gate walks
/// (`crucible-core/tests/docs_config.rs`). One file was not enough: the TOML
/// gate loses its blocks as `config.toml` goes away, so the Lua gate has to
/// cover the same prose or the pair stops proving anything.
const DOC_ROOTS: &[&str] = &["docs/Help", "docs/Guides"];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/crucible-lua has a workspace root two levels up")
        .to_path_buf()
}

/// Every authored, committed `.md` file under `DOC_ROOTS`, sorted.
///
/// The two filters match `crucible-core/tests/common/docs_kiln.rs`, which a
/// test binary in another crate cannot import. `.crucible/` holds session
/// notes the daemon writes when somebody chats in this kiln, so they are
/// generated rather than authored. A file git does not have in its index is
/// not part of any commit, so a scratch draft in `docs/` must not turn this
/// gate red.
fn markdown_files() -> Vec<PathBuf> {
    let workspace = workspace_root();
    let mut files: Vec<PathBuf> = DOC_ROOTS
        .iter()
        .flat_map(|dir| WalkDir::new(workspace.join(dir)).into_iter().flatten())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .filter(|e| !e.path().components().any(|c| c.as_os_str() == ".crucible"))
        .filter(|e| is_committable(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    files
}

/// Whether git has this path in its index — tracked, or newly staged.
///
/// Falls back to accepting everything when git cannot answer (no git, or an
/// unpacked source tarball), rather than silently checking nothing.
fn is_committable(path: &Path) -> bool {
    indexed_paths().is_none_or(|indexed| indexed.contains(path))
}

fn indexed_paths() -> Option<&'static HashSet<PathBuf>> {
    static INDEXED: OnceLock<Option<HashSet<PathBuf>>> = OnceLock::new();
    INDEXED
        .get_or_init(|| {
            let workspace = workspace_root();
            let output = Command::new("git")
                .args(["-C", workspace.to_str()?, "ls-files", "-z"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            Some(
                String::from_utf8_lossy(&output.stdout)
                    .split('\0')
                    .filter(|line| !line.is_empty())
                    .map(|rel| workspace.join(rel))
                    .collect(),
            )
        })
        .as_ref()
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
                    let is_lua = info.split(|c: char| c.is_whitespace() || c == ',').next()
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

/// The door a config example goes through. A `lua` fence that never calls it
/// is plugin code, statusline layout or module prose — none of which the
/// config loader can be asked to evaluate.
const CONFIG_WRITE_CALL: &str = "cru.config.set";

/// The `lua` fences in `content` that write configuration.
fn config_examples(content: &str) -> Vec<LuaBlock> {
    extract_lua_blocks(content)
        .into_iter()
        .filter(|block| block.body.contains(CONFIG_WRITE_CALL))
        .collect()
}

/// Evaluate one block the way the daemon boots, then name every key the
/// config types do not claim.
///
/// The second half is not decoration. No struct in the config tree sets
/// `deny_unknown_fields`, so `cru.config.set{ invented_key = true }` merges
/// into the store and extracts clean: the block runs, the gate goes green,
/// and the doc teaches a key that does nothing. `serde_ignored` over the
/// resulting store value is what turns that back into a failure.
fn evaluate(body: &str) -> Result<(), String> {
    crucible_lua::evaluate_config_source(body).map_err(|e| e.to_string())?;
    let value =
        crucible_lua::get_app_config().ok_or("the store holds no config after evaluation")?;

    let mut ignored = Vec::new();
    serde_ignored::deserialize::<_, _, crucible_core::config::CliAppConfig>(value, |path| {
        ignored.push(path.to_string())
    })
    .map_err(|e| e.to_string())?;

    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!("no such config key(s): {}", ignored.join(", ")))
    }
}

#[test]
#[ignore = "requires: dev kiln — evaluates every documented config example through the config boot"]
fn docs_lua_blocks_evaluate_as_config() {
    let root = workspace_root();
    let files = markdown_files();
    assert!(
        !files.is_empty(),
        "no markdown found under {DOC_ROOTS:?} — root discovery is broken"
    );

    let mut checked = 0usize;
    let mut failures = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        // Repo-relative, so a failure can be pasted straight into an editor.
        let rel = path.strip_prefix(&root).unwrap_or(path).display();

        for block in config_examples(&content) {
            checked += 1;
            if let Err(e) = evaluate(&block.body) {
                failures.push(format!("{rel}:{} — {e}", block.line));
            }
        }
    }

    assert!(
        checked > 0,
        "no config examples found — the extractor or the docs moved"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} documented Lua config blocks do not evaluate:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `docs/init.lua` is the annotated reference config the docs point readers
/// at, and the single largest piece of config prose in the repo. It is a
/// whole file rather than a fenced block, so the sweep above never sees it.
///
/// It replaced `docs/Config.toml`, which the TOML gate checked the same way.
#[test]
#[ignore = "requires: dev kiln — evaluates the reference config through the config boot"]
fn the_reference_config_evaluates() {
    let reference = workspace_root().join("docs").join("init.lua");
    let body = std::fs::read_to_string(&reference).expect("read docs/init.lua");

    if let Err(e) = evaluate(&body) {
        panic!("docs/init.lua does not evaluate as config: {e}");
    }
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

/// Selection is the half of this gate that can silently check nothing: a rule
/// that took every `lua` fence would drag plugin code through the config
/// loader, and one that took none would stay green forever.
#[test]
fn only_the_fences_that_write_config_are_selected() {
    let content = "\
```lua
cru.config.set({ cli = { highlighting = { enabled = true } } })
```

```lua
cru.tool.register({ name = \"greet\", handler = function() end })
```

<!-- crucible:not-config -->
```lua
cru.config.set({ demonstrates = \"a failure\" })
```
";
    let blocks = config_examples(content);
    assert_eq!(
        blocks.len(),
        1,
        "{:?}",
        blocks.iter().map(|b| b.line).collect::<Vec<_>>()
    );
    assert_eq!(blocks[0].line, 1);
}
