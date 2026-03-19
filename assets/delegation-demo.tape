# Cross-Agent Delegation Demo — Claude delegating to OpenCode
# Usage: just demo delegation-demo

Output assets/delegation-demo.gif

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

Type "cru chat -a claude"
Hide
Ctrl+U
Type "/tmp/cru-demo-wrapper"
Enter
Sleep 3s
Show
Sleep 10s
