# ACP Agent Demo — Crucible chat with Claude Code
# Usage: vhs assets/acp-demo.tape

Output assets/acp-demo.gif

Require cru

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Firewatch"
Set WindowBar Colorful
Set WindowBarSize 40
Set Padding 20
Set TypingSpeed 60ms
Set PlaybackSpeed 4

Type "cru chat -a claude"
Hide
Ctrl+U
Type "cru chat --replay assets/fixtures/acp-demo.jsonl --replay-auto-exit 0"
Enter
Sleep 3s
Show
Sleep 52s
