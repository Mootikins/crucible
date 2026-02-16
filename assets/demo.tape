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
Set TypingSpeed 100ms
Set PlaybackSpeed 3

Type "cru stats -C assets/demo-config.toml --no-process"
Enter
Sleep 4s

Type "cru chat -C assets/demo-config.toml --internal --local --no-process"
Enter
Sleep 5s

Type "What are wikilinks and how does the knowledge graph work?"
Enter
Sleep 60s

Ctrl+C
Sleep 2s
