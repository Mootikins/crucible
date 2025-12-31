# Size-Aware System Prompt Templates Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make small models (<4B) reliable by providing size-aware system prompts and tool filtering.

**Architecture:** Add `ModelSize` enum and size detection to crucible-core, size-based prompt templates, wire `LayeredPromptBuilder` into agent factory, add read-only tool subset for small models.

**Tech Stack:** Rust, regex for size detection, existing LayeredPromptBuilder

---

## Task 1: Add ModelSize enum and detection to crucible-core

**Files:**
- Create: `crates/crucible-core/src/prompts/mod.rs`
- Create: `crates/crucible-core/src/prompts/size.rs`
- Modify: `crates/crucible-core/src/lib.rs` (add module export)

**Step 1: Write the failing test**

Create `crates/crucible-core/src/prompts/size.rs`:

```rust
//! Model size detection and classification

use regex::Regex;

/// Model size categories for prompt optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    /// < 4B parameters - needs explicit tool guidance
    Small,
    /// 4-30B parameters - standard prompting
    Medium,
    /// > 30B parameters - minimal prompting needed
    Large,
}

impl ModelSize {
    /// Detect model size from model name string
    ///
    /// Parses patterns like "granite-3b", "qwen3-4b", "llama-70b"
    pub fn from_model_name(name: &str) -> Self {
        let re = Regex::new(r"(\d+)[bB]").unwrap();
        if let Some(caps) = re.captures(name) {
            let size: u32 = caps[1].parse().unwrap_or(0);
            match size {
                0..=3 => ModelSize::Small,
                4..=30 => ModelSize::Medium,
                _ => ModelSize::Large,
            }
        } else {
            // Default to medium if can't detect
            ModelSize::Medium
        }
    }

    /// Check if this size needs read-only tools
    pub fn is_read_only(&self) -> bool {
        matches!(self, ModelSize::Small)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_models() {
        assert_eq!(ModelSize::from_model_name("granite-micro-3b-q6_k"), ModelSize::Small);
        assert_eq!(ModelSize::from_model_name("phi-2b"), ModelSize::Small);
        assert_eq!(ModelSize::from_model_name("tiny-1B"), ModelSize::Small);
    }

    #[test]
    fn test_medium_models() {
        assert_eq!(ModelSize::from_model_name("qwen3-4b-instruct"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("granite-tiny-7b"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("deepseek-r1-8b"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("qwen3-14b"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("gpt-oss-20b"), ModelSize::Medium);
    }

    #[test]
    fn test_large_models() {
        assert_eq!(ModelSize::from_model_name("qwen2.5-coder-32b"), ModelSize::Large);
        assert_eq!(ModelSize::from_model_name("llama-70b"), ModelSize::Large);
        assert_eq!(ModelSize::from_model_name("gpt-oss-120b"), ModelSize::Large);
    }

    #[test]
    fn test_unknown_defaults_to_medium() {
        assert_eq!(ModelSize::from_model_name("unknown-model"), ModelSize::Medium);
        assert_eq!(ModelSize::from_model_name("gpt-4o"), ModelSize::Medium);
    }

    #[test]
    fn test_is_read_only() {
        assert!(ModelSize::Small.is_read_only());
        assert!(!ModelSize::Medium.is_read_only());
        assert!(!ModelSize::Large.is_read_only());
    }
}
```

**Step 2: Create prompts module**

Create `crates/crucible-core/src/prompts/mod.rs`:

```rust
//! System prompt templates and model size detection

mod size;

pub use size::ModelSize;
```

**Step 3: Export from lib.rs**

Add to `crates/crucible-core/src/lib.rs`:

```rust
pub mod prompts;
```

**Step 4: Run tests**

```bash
cargo test -p crucible-core prompts::size -- --nocapture
```

Expected: All tests pass

**Step 5: Commit**

```bash
git add crates/crucible-core/src/prompts/
git add crates/crucible-core/src/lib.rs
git commit -m "feat(core): add ModelSize enum and size detection"
```

---

## Task 2: Add size-based prompt templates

**Files:**
- Create: `crates/crucible-core/src/prompts/templates.rs`
- Modify: `crates/crucible-core/src/prompts/mod.rs`

**Step 1: Create templates module**

Create `crates/crucible-core/src/prompts/templates.rs`:

```rust
//! Size-based system prompt templates

use super::ModelSize;

/// Base system prompt for small models (< 4B)
///
/// Explicit guidance to avoid tool loops
pub const SMALL_MODEL_PROMPT: &str = r#"You are a helpful assistant.

## When to Use Tools
- ONLY use tools when the user explicitly asks for file operations, searches, or commands
- For questions, math, JSON, formatting: respond directly WITHOUT tools
- Do NOT call tools for: definitions, explanations, code generation, data formatting

## Available Tools (use sparingly)
- read_file - Read file contents
- glob - Find files by pattern
- grep - Search file contents

When in doubt, respond directly without using tools."#;

/// Base system prompt for medium models (4-30B)
pub const MEDIUM_MODEL_PROMPT: &str = r#"You are a helpful assistant with access to workspace tools.

## Tool Usage
- Use tools when tasks require file operations or system interaction
- For simple questions and formatting: respond directly
- Available: read_file, write_file, edit_file, bash, glob, grep

Do NOT output XML-style tool tags - use native function calling format."#;

/// Base system prompt for large models (> 30B)
pub const LARGE_MODEL_PROMPT: &str = "You are a helpful assistant with workspace tools available.";

/// Get the appropriate base prompt for a model size
pub fn base_prompt_for_size(size: ModelSize) -> &'static str {
    match size {
        ModelSize::Small => SMALL_MODEL_PROMPT,
        ModelSize::Medium => MEDIUM_MODEL_PROMPT,
        ModelSize::Large => LARGE_MODEL_PROMPT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_prompt_has_explicit_guidance() {
        let prompt = base_prompt_for_size(ModelSize::Small);
        assert!(prompt.contains("ONLY use tools"));
        assert!(prompt.contains("Do NOT call tools"));
    }

    #[test]
    fn test_medium_prompt_lists_all_tools() {
        let prompt = base_prompt_for_size(ModelSize::Medium);
        assert!(prompt.contains("write_file"));
        assert!(prompt.contains("edit_file"));
        assert!(prompt.contains("bash"));
    }

    #[test]
    fn test_large_prompt_is_minimal() {
        let prompt = base_prompt_for_size(ModelSize::Large);
        assert!(prompt.len() < 100);
    }
}
```

**Step 2: Update mod.rs**

Update `crates/crucible-core/src/prompts/mod.rs`:

```rust
//! System prompt templates and model size detection

mod size;
mod templates;

pub use size::ModelSize;
pub use templates::{base_prompt_for_size, LARGE_MODEL_PROMPT, MEDIUM_MODEL_PROMPT, SMALL_MODEL_PROMPT};
```

**Step 3: Run tests**

```bash
cargo test -p crucible-core prompts -- --nocapture
```

Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/crucible-core/src/prompts/
git commit -m "feat(core): add size-based prompt templates"
```

---

## Task 3: Add read-only tools method to WorkspaceContext

**Files:**
- Modify: `crates/crucible-rig/src/workspace_tools.rs`

**Step 1: Add read_only_tools method**

After the `all_tools()` method in `WorkspaceContext`, add:

```rust
    /// Get read-only tools for small models
    ///
    /// Returns only: read_file, glob, grep
    /// Excludes write operations to reduce confusion
    pub fn read_only_tools(&self) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        vec![
            Box::new(ReadFileTool::new(self.clone())),
            Box::new(GlobTool::new(self.clone())),
            Box::new(GrepTool::new(self.clone())),
        ]
    }

    /// Get tools based on model size
    pub fn tools_for_size(&self, size: crucible_core::prompts::ModelSize) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        if size.is_read_only() {
            self.read_only_tools()
        } else {
            self.all_tools()
        }
    }
```

**Step 2: Add crucible-core dependency if needed**

Check `crates/crucible-rig/Cargo.toml` for crucible-core dependency. Add if missing:

```toml
crucible-core = { path = "../crucible-core" }
```

**Step 3: Run build**

```bash
cargo build -p crucible-rig
```

Expected: Build succeeds

**Step 4: Commit**

```bash
git add crates/crucible-rig/
git commit -m "feat(rig): add read-only tools for small models"
```

---

## Task 4: Add .rules file support to LayeredPromptBuilder

**Files:**
- Modify: `crates/crucible-context/src/layered_prompt.rs`

**Step 1: Add with_rules_file method**

After `with_agents_md` method, add:

```rust
    /// Load project rules from first matching file (Zed-compatible)
    ///
    /// Checks in order: .rules, .cursorrules, AGENTS.md, CLAUDE.md, .github/copilot-instructions.md
    /// Loads first match only (deduplication via first-match-wins)
    pub fn with_project_rules(mut self, dir: &Path) -> Self {
        let candidates = [
            ".rules",
            ".cursorrules",
            "AGENTS.md",
            "CLAUDE.md",
            ".github/copilot-instructions.md",
        ];

        for candidate in candidates {
            let path = dir.join(candidate);
            if let Ok(content) = fs::read_to_string(&path) {
                if !content.trim().is_empty() {
                    self.layers.insert(
                        "project_rules".to_string(),
                        PromptLayer::new(content, priorities::PROJECT),
                    );
                    break; // First match wins
                }
            }
        }
        self
    }
```

**Step 2: Add test**

```rust
    #[test]
    fn test_rules_file_first_match_wins() {
        let temp_dir = TempDir::new().unwrap();

        // Create both .rules and AGENTS.md
        let rules = temp_dir.path().join(".rules");
        let agents = temp_dir.path().join("AGENTS.md");

        std::fs::write(&rules, "Rules content").unwrap();
        std::fs::write(&agents, "Agents content").unwrap();

        let builder = LayeredPromptBuilder::new().with_project_rules(temp_dir.path());

        // .rules should win
        let layer = builder.get_layer("project_rules").unwrap();
        assert!(layer.contains("Rules content"));
        assert!(!layer.contains("Agents content"));
    }
```

**Step 3: Run tests**

```bash
cargo test -p crucible-context layered_prompt -- --nocapture
```

Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/crucible-context/
git commit -m "feat(context): add .rules file support with first-match-wins"
```

---

## Task 5: Wire LayeredPromptBuilder into agent factory

**Files:**
- Modify: `crates/crucible-cli/src/factories/agent.rs`

**Step 1: Add imports**

At top of file, add:

```rust
use crucible_context::LayeredPromptBuilder;
use crucible_core::prompts::{base_prompt_for_size, ModelSize};
```

**Step 2: Replace hardcoded system prompt**

In `create_internal_agent`, replace the hardcoded `system_prompt` with:

```rust
    // Detect model size and get appropriate prompt
    let model_size = ModelSize::from_model_name(&model);
    info!("Detected model size: {:?} for {}", model_size, model);

    // Build layered system prompt
    let mut prompt_builder = LayeredPromptBuilder::new();

    // Override base with size-appropriate prompt
    prompt_builder.add_layer(
        crucible_core::traits::priorities::BASE,
        "base",
        base_prompt_for_size(model_size).to_string(),
    );

    // Add project rules if workspace has them
    prompt_builder = prompt_builder.with_project_rules(&workspace_root);

    let system_prompt = prompt_builder.build();
```

**Step 3: Update tool selection**

Replace the tools creation with:

```rust
    // Get tools appropriate for model size
    let ctx = crucible_rig::WorkspaceContext::new(&workspace_root);
    let tools = ctx.tools_for_size(model_size);
```

**Step 4: Update build_agent_with_tools call**

The agent building needs to use the filtered tools. Check the `build_agent_with_tools` signature and update accordingly.

**Step 5: Add crucible-context dependency**

In `crates/crucible-cli/Cargo.toml`, add:

```toml
crucible-context = { path = "../crucible-context" }
```

**Step 6: Run build**

```bash
cargo build -p crucible-cli
```

Expected: Build succeeds

**Step 7: Commit**

```bash
git add crates/crucible-cli/
git commit -m "feat(cli): wire LayeredPromptBuilder and size-based tools into agent factory"
```

---

## Task 6: Integration test with VT script

**Files:**
- Existing: `tests/vt_basic_flows.sh`

**Step 1: Build release binary**

```bash
cargo build --release -p crucible-cli
```

**Step 2: Test with granite-micro-3b (small model)**

Temporarily set config to use granite-micro-3b:

```bash
sed -i 's/model = ".*"/model = "granite-micro-3b-q6_k"/' ~/.config/crucible/config.toml
```

**Step 3: Run VT full suite**

```bash
./tests/vt_basic_flows.sh full
```

Expected: All 6 tests pass (including JSON test that previously failed)

**Step 4: Restore config**

```bash
git checkout ~/.config/crucible/config.toml
# or manually restore the model setting
```

**Step 5: Commit VT results (optional)**

If tests pass, no commit needed. If changes were made to VT script, commit them.

---

## Task 7: Final cleanup and documentation

**Files:**
- Remove: `crates/crucible-core/src/prompts/base.md` (obsolete draft file)
- Update: `docs/Meta/plans/2024-12-30-prompt-templates-design.md` (mark as implemented)

**Step 1: Remove draft file**

```bash
rm crates/crucible-core/src/prompts/base.md
```

**Step 2: Update design doc status**

Change `**Status**: Draft` to `**Status**: Implemented`

**Step 3: Final commit**

```bash
git add -A
git commit -m "docs: mark prompt templates design as implemented"
```

---

## Success Criteria Checklist

- [ ] `ModelSize::from_model_name()` correctly categorizes models
- [ ] Small models get explicit "don't use tools" guidance
- [ ] Small models only get read-only tools (read_file, glob, grep)
- [ ] `.rules` files are loaded (first-match-wins)
- [ ] `LayeredPromptBuilder` is used for all internal agents
- [ ] granite-micro-3b passes VT full suite
- [ ] No regressions on medium/large models

---

**Plan complete and saved to `docs/Meta/plans/2024-12-30-prompt-templates-plan.md`.**

**Two execution options:**

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

**Which approach?**
