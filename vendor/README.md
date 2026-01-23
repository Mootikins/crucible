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
