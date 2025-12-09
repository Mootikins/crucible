
## 2025-12-08 - Progressive tool call display during streaming
Currently streaming accumulates all updates into StreamingState, then returns formatted output after completion. User wants progressive display as tools execute (show each tool call as it happens, not all at once at the end). Requires either: (1) callback/closure in send_prompt_with_streaming, (2) channel-based streaming, or (3) ratatui widget approach. Related to concurrent typing feature.
Context: Observed in cru chat - tool calls display all at once instead of progressively

## 2025-12-08 - Concurrent typing during streaming ticker
Save cursor position at ticker line, allow user to type at prompt while ticker updates above, jump back to update ticker then return to prompt position.
Context: Brainstorming diff display feature - deferred until ratatui migration

## 2025-12-08 - Pipeline as mailbox/channel coordinator (Actor-lite architecture)
Reframe pipeline crate as grouped mailbox manager: Read channels (queries), Write channels (mutations + events), Event channels (fan-out reactions). Components become actors with typed message passing. Benefits: clean separation (ACP sends WriteFile, CLI subscribes to FileChanged for diff display), same events feed web SSE, testable via message assertions. Consider if we hit coupling pain with 3+ consumers or need plugin extensibility.
Context: Discussed during diff display integration - deferred as YAGNI, simple return value approach chosen instead

## 2025-12-08 - Domain Memory System for Agents (from Anthropic insights)
Build persistent, structured domain memory for agents instead of relying on LLM context. Key components: explicit goal list/backlog, state tracking (passing/failing/attempted/reverted), scaffolding (how to run/test/extend), progress logs. Two-agent pattern: initializer (creates domain memory from prompt, no memory needed) + worker (amnesiac, reads memory, makes atomic progress, updates memory, exits). Magic is in memory/harness, not personality. Schemas and rituals are domain-specific (code: features.json, tests, git logs; research: hypothesis backlog, experiment registry).
Context: Video transcript on Anthropic's agent findings - generalized agents are amnesiacs without domain memory

## 2025-12-08 - Actor system or ECS for Crucible architecture
Recurring pattern: many features (streaming display, tool categorization, event fan-out) would benefit from actor-style message passing or ECS (Entity Component System) architecture. ECS might fit better than traditional actors - entities (tools, notes, agents) with components (category, embeddings, state) and systems (categorizer, searcher, executor) that operate on component queries. Worth investigating: is ECS a better fit than actors for this domain?
Context: Discussing recipe categorization, realized this is the 3rd time actor-like patterns have come up

## 2025-12-08 - Rune event system (not "integration points")
Frame Rune script directories as event handlers, not integration hooks. Folder structure = event types. Scripts subscribe by existing in the folder. Eventually becomes full event system with typed event payloads. Naming: `~/.crucible/runes/events/recipe_discovered/`, `events/tool_executed/`, etc. This is the glue between actor/ECS systems and user-scriptable behavior.
Context: Designing recipe categorization, realized it's really just event handling

## 2025-12-09 - Break out Just MCP server to separate repo
Create standalone just-mcp repo (in ~/just-mcp) with the Just recipe → MCP tool functionality. Iterate separately from Crucible. Crucible then connects to it as an upstream MCP via the bridge/gateway. Cleaner separation, reusable by others.
Context: Designing MCP gateway - Just MCP is a good standalone tool

## 2025-12-09 - Workflow hierarchies as nested maps/todos
Workflow events need more granularity than flat `workflow:step`. Consider `workflow:phase:step` hierarchy, or abstract to nested lists/maps for defining todos. Tools encode workflow, reducing state-space for agents - give them unambiguous tools that cover 90% of cases instead of full BASH. Agents become closer to stateless.
Context: MCP bridge proposal - thinking about how workflows interact with events
