# Quick Prompt Shell Integration Implementation Tasks

## 1. Shell Integration Script Generation

- [ ] 1.1 Create `crates/crucible-cli/src/commands/shell_integration.rs`
  - [ ] 1.1.1 Define `ShellIntegrationArgs` struct with shell type (zsh/bash)
  - [ ] 1.1.2 Implement `generate_zsh_script()` function
  - [ ] 1.1.3 Implement `generate_bash_script()` function
  - [ ] 1.1.4 Output script to stdout for `eval` sourcing
- [ ] 1.2 Add `shell-integration` subcommand to CLI
  - [ ] 1.2.1 Register command in `crates/crucible-cli/src/cli.rs`
  - [ ] 1.2.2 Export module in `crates/crucible-cli/src/commands/mod.rs`
  - [ ] 1.2.3 Add shell type argument (zsh|bash)
- [ ] 1.3 Implement zsh script generation
  - [ ] 1.3.1 Define `_crucible_prompt_mode` variable
  - [ ] 1.3.2 Create `_crucible_toggle_prompt_mode()` function
  - [ ] 1.3.3 Create ZLE widget `_crucible_tab_handler` for Tab key
  - [ ] 1.3.4 Create ZLE widget `_crucible_enter_handler` for Enter key
  - [ ] 1.3.5 Bind widgets to keys using `zle -N` and `bindkey`
  - [ ] 1.3.6 Update PS1 to show `[crucible]` prefix when mode is active
  - [ ] 1.3.7 Make script idempotent (check for existing bindings)
- [ ] 1.4 Implement bash script generation
  - [ ] 1.4.1 Define `_CRUCIBLE_PROMPT_MODE` variable
  - [ ] 1.4.2 Create `_crucible_toggle_prompt_mode()` function
  - [ ] 1.4.3 Create `_crucible_tab_handler()` function for Tab key
  - [ ] 1.4.4 Create `_crucible_enter_handler()` function for Enter key
  - [ ] 1.4.5 Bind functions using `bind` command
  - [ ] 1.4.6 Update PS1 to show `[crucible]` prefix when mode is active
  - [ ] 1.4.7 Make script idempotent
- [ ] 1.5 Test script generation
  - [ ] 1.5.1 Test zsh script output format
  - [ ] 1.5.2 Test bash script output format
  - [ ] 1.5.3 Verify scripts are valid shell syntax
  - [ ] 1.5.4 Test idempotency (sourcing multiple times)

## 2. Quick Prompt Command Infrastructure

- [ ] 2.1 Create `crates/crucible-cli/src/commands/quick_prompt.rs`
  - [ ] 2.1.1 Define `QuickPromptArgs` struct with input string
  - [ ] 2.1.2 Implement `execute_quick_prompt()` function
  - [ ] 2.1.3 Parse input and route to trigger registry
- [ ] 2.2 Add `quick-prompt` subcommand to CLI
  - [ ] 2.2.1 Register command in `crates/crucible-cli/src/cli.rs`
  - [ ] 2.2.2 Export module in `crates/crucible-cli/src/commands/mod.rs`
  - [ ] 2.2.3 Accept input string as argument
- [ ] 2.3 Create trigger registry module structure
  - [ ] 2.3.1 Create `crates/crucible-cli/src/quick_prompt/mod.rs`
  - [ ] 2.3.2 Create `crates/crucible-cli/src/quick_prompt/trigger_registry.rs`
  - [ ] 2.3.3 Create `crates/crucible-cli/src/quick_prompt/triggers/` directory
  - [ ] 2.3.4 Create `crates/crucible-cli/src/quick_prompt/triggers/mod.rs`

## 3. Trigger Registry Implementation

- [ ] 3.1 Implement `PromptTriggerRegistry` in `trigger_registry.rs`
  - [ ] 3.1.1 Define `TriggerHandler` trait with `handle()` method
  - [ ] 3.1.2 Define `PromptTriggerRegistry` struct
  - [ ] 3.1.3 Implement `register()` method for adding triggers
  - [ ] 3.1.4 Implement `match_and_route()` method for prefix matching
  - [ ] 3.1.5 Handle case where no prefix matches (fallback to agent)
- [ ] 3.2 Initialize built-in triggers
  - [ ] 3.2.1 Register `note:` trigger with note handler
  - [ ] 3.2.2 Register `agent:` trigger with agent handler
  - [ ] 3.2.3 Register `search:` trigger with search handler
  - [ ] 3.2.4 Create default registry instance

## 4. Trigger Handlers

- [ ] 4.1 Implement note creation trigger
  - [ ] 4.1.1 Create `crates/crucible-cli/src/quick_prompt/triggers/note.rs`
  - [ ] 4.1.2 Implement `NoteTriggerHandler` struct
  - [ ] 4.1.3 Implement `TriggerHandler` trait for note handler
  - [ ] 4.1.4 Extract content after `note:` prefix
  - [ ] 4.1.5 Create note using storage API
  - [ ] 4.1.6 Output confirmation message
- [ ] 4.2 Implement agent query trigger
  - [ ] 4.2.1 Create `crates/crucible-cli/src/quick_prompt/triggers/agent.rs`
  - [ ] 4.2.2 Implement `AgentTriggerHandler` struct
  - [ ] 4.2.3 Implement `TriggerHandler` trait for agent handler
  - [ ] 4.2.4 Extract content after `agent:` prefix (or use full input if no prefix)
  - [ ] 4.2.5 Send prompt to agent via ACP
  - [ ] 4.2.6 Stream response to stdout
- [ ] 4.3 Implement search trigger
  - [ ] 4.3.1 Create `crates/crucible-cli/src/quick_prompt/triggers/search.rs`
  - [ ] 4.3.2 Implement `SearchTriggerHandler` struct
  - [ ] 4.3.3 Implement `TriggerHandler` trait for search handler
  - [ ] 4.3.4 Extract content after `search:` prefix
  - [ ] 4.3.5 Perform semantic search using storage API
  - [ ] 4.3.6 Format and display results
- [ ] 4.4 Export trigger handlers
  - [ ] 4.4.1 Update `crates/crucible-cli/src/quick_prompt/triggers/mod.rs` to export handlers
  - [ ] 4.4.2 Update `crates/crucible-cli/src/quick_prompt/mod.rs` to export registry

## 5. Shell Integration Testing

- [ ] 5.1 Test zsh integration
  - [ ] 5.1.1 Source script in test zsh session
  - [ ] 5.1.2 Test Tab toggle at start of line
  - [ ] 5.1.3 Test Tab does not toggle in middle of line
  - [ ] 5.1.4 Test prompt indicator appears/disappears
  - [ ] 5.1.5 Test Enter in prompt mode routes to Crucible
  - [ ] 5.1.6 Test Enter in normal mode executes normally
- [ ] 5.2 Test bash integration
  - [ ] 5.2.1 Source script in test bash session
  - [ ] 5.2.2 Test Tab toggle at start of line
  - [ ] 5.2.3 Test Tab does not toggle in middle of line
  - [ ] 5.2.4 Test prompt indicator appears/disappears
  - [ ] 5.2.5 Test Enter in prompt mode routes to Crucible
  - [ ] 5.2.6 Test Enter in normal mode executes normally
- [ ] 5.3 Test trigger execution
  - [ ] 5.3.1 Test `note:` trigger creates note
  - [ ] 5.3.2 Test `agent:` trigger queries agent
  - [ ] 5.3.3 Test `search:` trigger performs search
  - [ ] 5.3.4 Test unknown prefix falls back to agent
  - [ ] 5.3.5 Test no prefix falls back to agent

## 6. Error Handling and Edge Cases

- [ ] 6.1 Handle Crucible CLI not in PATH
  - [ ] 6.1.1 Script checks if `crucible` command exists
  - [ ] 6.1.2 Display helpful error message if not found
- [ ] 6.2 Handle quick-prompt errors
  - [ ] 6.2.1 Display errors to stderr
  - [ ] 6.2.2 Return appropriate exit codes
  - [ ] 6.2.3 Shell integration handles errors gracefully
- [ ] 6.3 Handle edge cases
  - [ ] 6.3.1 Empty input handling
  - [ ] 6.3.2 Very long input handling
  - [ ] 6.3.3 Special characters in input
  - [ ] 6.3.4 Multiple consecutive spaces/tabs

## 7. Documentation

- [ ] 7.1 Add CLI help text for `shell-integration` command
- [ ] 7.2 Add CLI help text for `quick-prompt` command
- [ ] 7.3 Document installation in README or user guide
  - [ ] 7.3.1 Installation steps for zsh
  - [ ] 7.3.2 Installation steps for bash
  - [ ] 7.3.3 Usage examples
  - [ ] 7.3.4 Troubleshooting common issues
- [ ] 7.4 Document trigger system for future extension

## 8. Configuration (Future Enhancement)

- [ ] 8.1 Add trigger configuration to config file (Phase 2)
  - [ ] 8.1.1 Define config schema for custom triggers
  - [ ] 8.1.2 Load custom triggers from config
  - [ ] 8.1.3 Merge custom triggers with built-in triggers
- [ ] 8.2 Support custom keybindings (Phase 2)
  - [ ] 8.2.1 Allow configuration of toggle key
  - [ ] 8.2.2 Support custom keybindings per shell
