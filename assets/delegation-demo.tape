# Cross-Agent Delegation Demo — Claude delegating to OpenCode
# Requirements: Claude Code and opencode both available
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

Type "cru chat -a claude"
Hide
Ctrl+U
Type "cru chat --replay assets/fixtures/delegation-demo.jsonl --replay-auto-exit 0"
Enter
Sleep 3s
Show
Sleep 16s
