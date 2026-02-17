# Delegation Demo — Crucible spawning a background session to delegate research
# with inline progress display in the TUI.
#
# Requirements:
#   - Claude Code authenticated (OAuth via `claude` CLI)
#   - cru binary built and kiln processed
#   - Delegation feature enabled in config
#
# Usage: vhs assets/delegation-demo.tape

Output assets/delegation-demo.gif

Require cru

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Set WindowBarSize 40
Set Padding 20
Set TypingSpeed 50ms
Set PlaybackSpeed 2

# Launch Crucible chat with Claude Code as ACP agent
# Delegation feature is enabled by default in the config
Type "cru chat -a claude --no-process --set perm.autoconfirm_session"
Enter
Sleep 10s

# Ask Claude to research a topic and delegate the analysis to a background session
# This triggers the delegate_session tool, which spawns a child session
# The TUI shows a delegation spinner inline while the background work completes
Type "Research authentication patterns in the codebase and delegate the analysis to a background session"
Enter
Sleep 5s

# Wait for delegation to spawn (shows spinner)
Sleep 10s

# Wait for delegation to complete (spinner → checkmark)
Sleep 15s

# Wait for result to be displayed in the chat
Sleep 10s

# Exit — first Ctrl+C shows "press again to quit", second confirms
Ctrl+C
Sleep 500ms
Ctrl+C
Sleep 5s
