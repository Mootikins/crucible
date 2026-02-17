# ACP Agent Demo — Crucible spawning an external AI agent
# with full access to the knowledge graph via Agent Context Protocol.
#
# Requirements:
#   - ANTHROPIC_API_KEY set (for Claude Code agent)
#   - claude-code-acp installed: npm install -g @zed-industries/claude-code-acp
#   - cru binary built
#
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
Set TypingSpeed 50ms
Set PlaybackSpeed 2

# Launch Crucible chat with Claude Code as ACP agent
# The agent gets MCP tools: semantic_search, get_outlinks, create_note, etc.
Type "cru chat -a claude --no-process"
Enter
Sleep 8s

# Ask a question that exercises the knowledge graph
Type "What notes link to Wikilinks and how are they connected?"
Enter
Sleep 45s

# Clean exit
Ctrl+C
Sleep 2s
