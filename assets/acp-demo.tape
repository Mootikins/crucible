# ACP Agent Demo — Crucible chat with Claude Code
# Usage: vhs assets/acp-demo.tape

Output assets/acp-demo.gif

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

Type "cru chat -a claude --no-process --set perm.autoconfirm_session"
Enter
Sleep 10s

Type "How does ACP differ from MCP, and how does Crucible use both?"
Enter
Sleep 60s

Ctrl+C
Sleep 500ms
Ctrl+C
Sleep 3s
