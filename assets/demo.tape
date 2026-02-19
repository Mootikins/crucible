Output assets/demo.gif

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
Set PlaybackSpeed 3

Type "cru chat --replay assets/fixtures/demo.jsonl --replay-auto-exit"
Enter
Wait+Screen /Replay/
Sleep 2s
