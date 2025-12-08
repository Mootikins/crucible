
## 2025-12-08 - Progressive tool call display during streaming
Currently streaming accumulates all updates into StreamingState, then returns formatted output after completion. User wants progressive display as tools execute (show each tool call as it happens, not all at once at the end). Requires either: (1) callback/closure in send_prompt_with_streaming, (2) channel-based streaming, or (3) ratatui widget approach. Related to concurrent typing feature.
Context: Observed in cru chat - tool calls display all at once instead of progressively

## 2025-12-08 - Concurrent typing during streaming ticker
Save cursor position at ticker line, allow user to type at prompt while ticker updates above, jump back to update ticker then return to prompt position.
Context: Brainstorming diff display feature - deferred until ratatui migration

## 2025-12-08 - Pipeline as mailbox/channel coordinator (Actor-lite architecture)
Reframe pipeline crate as grouped mailbox manager: Read channels (queries), Write channels (mutations + events), Event channels (fan-out reactions). Components become actors with typed message passing. Benefits: clean separation (ACP sends WriteFile, CLI subscribes to FileChanged for diff display), same events feed web SSE, testable via message assertions. Consider if we hit coupling pain with 3+ consumers or need plugin extensibility.
Context: Discussed during diff display integration - deferred as YAGNI, simple return value approach chosen instead
