# Cross-Agent Delegation Demo — Claude delegating to Cursor
# Requirements: Claude Code and cursor-acp both available
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
Set TypingSpeed 60ms
Set PlaybackSpeed 4

Type "cru chat -a claude -C assets/demo-acp-config.toml --no-process --set perm.autoconfirm_session"
Enter
Sleep 10s

Type "Use the delegate_session tool to ask cursor to explain what the Agent Client Protocol is based on the codebase."
Enter
Sleep 50s

Ctrl+C
Sleep 500ms
Ctrl+C
Sleep 3s
