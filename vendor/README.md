# Vendored Dependencies

This directory contains local patches for upstream crates with bugs or missing features.

## markdown-it

**Source:** https://github.com/rlidwka/markdown-it.rs
**Upstream version:** 0.6.1
**Reason:** Upstream is semi-abandoned with unmerged panic fixes

### Patches Applied

1. **emph_pair.rs underflow fixes** (upstream issue #48)
   - `map.1 - map.0` -> `map.1.saturating_sub(map.0)`
   - `state.pos -= token_len` -> `state.pos = state.pos.saturating_sub(token_len)`
   - `end - marker_len` -> `end.saturating_sub(marker_len)`

   These prevent panics when emphasis markers span across list item lines:
   ```markdown
   - _foo
     bar_
   ```

2. **emph_pair.rs backtrack fix** (same upstream issue)
   - The `saturating_sub` in patch 1 stopped the underflow panic but left
     `state.pos` at 0, and the tokenizer then added the token length back.
     The position landed at an arbitrary offset, and the next inline rule
     sliced the string there — panicking inside a multi-byte character.
   - `state.pos` indexes the de-indented, joined list-item buffer; the source
     map the backtrack reads holds document offsets. They agree for ordinary
     inline text and disagree when an emphasis pair spans list-item lines.
   - The rewind now applies only when it lands on a real char boundary of that
     buffer. Otherwise the rule advances plainly by the marker length, trading
     one note's source map for a parse that finishes.
   - Regression: `emphasis_across_list_item_lines_before_a_multibyte_char_parses`
     in `crates/crucible-core/src/parser/basic_markdown_it.rs`.
   - Why it mattered: `BasicMarkdownItExtension::parse` wraps `md.parse` in
     `catch_unwind`, but the release profile sets `panic = "abort"`, so the
     guard cannot run in a shipped build. One docs note aborted the whole
     daemon mid-index.

### Updating

To pull in upstream changes:

```bash
cd vendor/markdown-it
git init  # if needed
git remote add upstream https://github.com/rlidwka/markdown-it.rs
git fetch upstream
git diff upstream/master -- src/  # review changes
# Apply any new fixes manually, preserving our patches
```

### Cargo Configuration

The workspace `Cargo.toml` uses `[patch.crates-io]` to substitute this local copy:

```toml
[patch.crates-io]
markdown-it = { path = "vendor/markdown-it" }
```

The `vendor/markdown-it` directory is excluded from the workspace via `workspace.exclude`.
