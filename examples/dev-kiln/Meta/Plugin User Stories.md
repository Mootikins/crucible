---
title: Plugin System User Stories
status: design
created: 2024-12-21
tags:
  - plugins
  - user-stories
  - design
description: User stories for Crucible's plugin system across three personas and two maturity levels
---

# Plugin System User Stories

User stories for Crucible's plugin/extension system, organized by persona and maturity level.

## Design Context

**Extension Languages (near-term):** Rune, possibly Luau
**Future Extensions:** WASM, dynamic libraries

**MVP vs Full-Featured:**
- **MVP** = Core functionality works, text-based interfaces
- **Full** = Polished experience, visual tools (JSON canvas)

---

## Personas

| Persona | Motivation |
|---------|-----------|
| **Non-Technical** | "I want my vault to do things for me without learning to code" |
| **Light Technical** | "I want to customize how Crucible works for my workflow" |
| **Software Engineer** | "I want to extend Crucible's capabilities and share with others" |

### Non-Technical Definition
- No coding experience
- Uses templates and natural language
- Needs safe guardrails (dry-run, undo)

### Light Technical Definition
- Power user of tools (Obsidian, Notion, Dataview)
- Comfortable editing code, not writing from scratch
- Can modify existing scripts, understands variables/functions

### Software Engineer Definition
- Reads API docs and builds from scratch
- Understands async, error handling, testing
- Contributes plugins to community registry

---

## Non-Technical User Stories

### MVP

#### NT-MVP-1: Template Installation
> As a non-technical user, I want to browse a list of pre-made plugin templates so I can add functionality without writing code.

*Example:* "Daily Digest" template that surfaces notes modified yesterday.

#### NT-MVP-2: Fill-in-the-Blanks Configuration
> As a non-technical user, I want to customize a template by answering simple questions (folder names, tag preferences) so it works for my vault structure.

*Example:* Template asks "Which folder contains your projects?" and fills in the path.

#### NT-MVP-3: Natural Language Plugin Generation
> As a non-technical user, I want to describe what I want in plain English and have Crucible generate a working plugin so I don't need to understand code.

*Example:* "Show me notes tagged #reading that I haven't touched in 2 weeks"

#### NT-MVP-4: Safe Preview (Dry-Run)
> As a non-technical user, I want to see what a plugin *would* do before it changes anything so I feel safe experimenting.

#### NT-MVP-5: Undo/Revert
> As a non-technical user, I want to undo changes a plugin made so I'm not afraid of breaking my vault.

### Full-Featured

#### NT-FULL-1: Plugin Discovery & Ratings
> As a non-technical user, I want to browse community-rated plugins with reviews so I can find trusted solutions for common needs.

*Example:* "Top 10 plugins for academics" with install counts and ratings.

#### NT-FULL-2: Visual Workflow Builder (JSON Canvas)
> As a non-technical user, I want to build multi-step automations by connecting boxes on a canvas so I can create workflows visually.

*Example:* Drag "Find notes by tag" → "Filter by date" → "Create summary note" and connect them.

#### NT-FULL-3: Friendly Error Messages
> As a non-technical user, when something goes wrong, I want plain-English explanations with suggested fixes so I can resolve issues myself.

*Example:* "This plugin tried to read 'Projects/Active' but that folder doesn't exist. Did you mean 'Projects/Current'?"

#### NT-FULL-4: Plugin Health Dashboard
> As a non-technical user, I want to see which plugins ran, when, and whether they succeeded so I know my automations are working.

#### NT-FULL-5: Scheduled Automations
> As a non-technical user, I want plugins to run automatically on a schedule so I get daily digests without manual effort.

*Example:* "Run every morning at 8am" → get a note with yesterday's activity.

#### NT-FULL-6: One-Click Sharing
> As a non-technical user, I want to share my customized plugin as a template so others can benefit from my setup.

---

## Light Technical User Stories

### MVP

#### LT-MVP-1: Modify Existing Scripts
> As a light technical user, I want to open a plugin's source and tweak values (folder paths, tag names, thresholds) so I can adapt it to my needs.

*Example:* Change `let days_threshold = 7` to `14` in a "stale notes" plugin.

#### LT-MVP-2: Copy-Paste Snippets
> As a light technical user, I want to find working code snippets in the docs that I can paste and modify so I don't start from scratch.

*Example:* "Here's a snippet that finds all notes with a tag" — copy, change the tag, done.

#### LT-MVP-3: Combine Template Pieces
> As a light technical user, I want to merge parts of two templates into one script so I can create custom combinations.

*Example:* Take the "filter by folder" logic from one template and "update frontmatter" from another.

#### LT-MVP-4: Understand What Went Wrong
> As a light technical user, when a script fails, I want error messages that point to the line and explain the issue so I can fix it myself.

*Example:* "Line 23: `note.tags` is None — try `note.tags.unwrap_or([])`"

#### LT-MVP-5: Test Changes Safely
> As a light technical user, I want to run my modified script in dry-run mode and see exactly what it would change so I can iterate confidently.

### Full-Featured

#### LT-FULL-1: Inline Documentation on Hover
> As a light technical user, I want to hover over Crucible API functions and see what they do with examples so I don't have to leave my editor.

*Example:* Hover over `crucible::search_by_tags` → see signature, description, and sample usage.

#### LT-FULL-2: Script Playground/REPL
> As a light technical user, I want an interactive playground where I can test snippets against my vault and see results immediately so I can experiment quickly.

*Example:* Type `crucible::search_by_tags(["project"])` → see matching notes live.

#### LT-FULL-3: Diff Preview Before Apply
> As a light technical user, I want to see a side-by-side diff of what my script will change before I commit so I can catch mistakes.

*Example:* "These 3 notes will have frontmatter updated: [diff view]"

#### LT-FULL-4: Version History for Scripts
> As a light technical user, I want my script changes versioned automatically so I can revert if I break something.

#### LT-FULL-5: AI-Assisted Debugging
> As a light technical user, when my script doesn't work, I want to ask "why isn't this working?" and get contextual help based on my code and error.

*Example:* "Your loop variable `note` shadows the outer `note` — rename one of them."

#### LT-FULL-6: Fork & Customize Community Plugins
> As a light technical user, I want to fork a community plugin into my vault and customize it while still seeing upstream updates.

*Example:* "Original plugin updated — view changes and merge?"

---

## Software Engineer User Stories

### MVP

#### SE-MVP-1: Full API Access
> As a plugin author, I want comprehensive API documentation with types, error conditions, and edge cases so I can build robust plugins.

*Example:* Docs show that `read_note()` returns `Result<Note, Error>` with possible `NotFound`, `ParseError`, `PermissionDenied`.

#### SE-MVP-2: Event Hook System
> As a plugin author, I want to subscribe to system events (note saved, tool invoked, etc.) with priority ordering so my plugin can react to vault changes.

*Example:* Hook into `note:before_save` at priority `-50` to validate content before it's written.

#### SE-MVP-3: Define Custom Tools
> As a plugin author, I want to expose functions as MCP tools with typed parameters so agents can invoke my plugin's capabilities.

*Example:* `#[tool(name = "summarize_folder")]` makes the function callable via MCP.

#### SE-MVP-4: Structured Error Handling
> As a plugin author, I want to use Result types with proper error propagation so my plugins fail gracefully with actionable messages.

#### SE-MVP-5: Local Testing Workflow
> As a plugin author, I want to run tests against a fixture vault and verify behavior before publishing so I ship working code.

*Example:* `cru test my-plugin.rn --vault ./test-vault`

### Full-Featured

#### SE-FULL-1: Plugin Manifest & Metadata
> As a plugin author, I want to declare dependencies, permissions, activation events, and configuration schema in a manifest so the system can manage my plugin properly.

*Example:* `manifest.toml` declares `requires = ["crucible >= 0.5"]`, `permissions = ["read", "write"]`.

#### SE-FULL-2: Capability-Based Sandboxing
> As a plugin author, I want to declare what my plugin needs (read content, write content, network access) so users can trust my plugin's scope.

*Example:* "This plugin requests: read notes in `Projects/`, no write access, no network."

#### SE-FULL-3: State Machines for Workflows
> As a plugin author, I want to define multi-step workflows with states, transitions, and guards so I can build complex processes like review pipelines.

*Example:* Draft → Pending Review → Approved/Rejected with guard `can_approve()`.

#### SE-FULL-4: Generation Pipeline Control
> As a plugin author, I want to define LLM generation pipelines with context assembly, chaining, and parallel execution so I can orchestrate complex AI tasks.

*Example:* Semantic search → outline → parallel expand sections → synthesize.

#### SE-FULL-5: Publish to Registry
> As a plugin author, I want to publish my plugin to a community registry with versioning so others can discover and install it.

*Example:* `cru plugin publish` → appears in registry with changelog.

#### SE-FULL-6: Analytics & Feedback
> As a plugin author, I want to see install counts, error rates, and user feedback so I can improve my plugin over time.

#### SE-FULL-7: Cross-Plugin Communication
> As a plugin author, I want to emit and subscribe to custom events so plugins can compose without tight coupling.

*Example:* My plugin emits `custom:extraction_complete`, another plugin listens and acts.

---

## Capability Matrix

| Capability | NT-MVP | NT-Full | LT-MVP | LT-Full | SE-MVP | SE-Full |
|------------|:------:|:-------:|:------:|:-------:|:------:|:-------:|
| Install templates | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Configure via questions | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Natural language generation | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Dry-run preview | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Modify script source | — | — | ✓ | ✓ | ✓ | ✓ |
| Combine snippets | — | — | ✓ | ✓ | ✓ | ✓ |
| Visual canvas builder | — | ✓ | — | ✓ | — | ✓ |
| Plugin registry/ratings | — | ✓ | — | ✓ | — | ✓ |
| Event hooks | — | — | — | — | ✓ | ✓ |
| Custom MCP tools | — | — | — | — | ✓ | ✓ |
| State machine workflows | — | — | — | — | — | ✓ |
| Generation pipelines | — | — | — | — | — | ✓ |
| Publish to registry | — | — | — | — | — | ✓ |

---

## Related Documents

- [[Help/Rune/Language Basics]] — Rune syntax fundamentals
- [[Help/Rune/Crucible API]] — API reference for scripts
- [[Help/Rune/Tool Definition]] — Creating MCP-callable tools
- [[Help/Rune/Event Types]] — Hook event reference
- [[Help/Extending/Creating Plugins]] — Plugin authoring guide

---

## Open Questions

1. **Template format**: How are templates packaged? Single file with frontmatter config, or folder with manifest?
2. **NL generation guardrails**: What constraints prevent NL-generated plugins from doing harmful things?
3. **Registry hosting**: Self-hosted, GitHub-based, or dedicated service?
4. **Canvas format**: Use Obsidian's JSON Canvas spec or define our own?
5. **Workflow DAG builder**: Markdown-to-DAG — what's the syntax? YAML? Custom DSL?
