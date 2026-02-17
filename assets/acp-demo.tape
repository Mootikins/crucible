# ACP Agent Demo — Crucible spawning an external AI agent
# with full access to the knowledge graph via Agent Client Protocol.
#
# Requirements:
#   - Claude Code authenticated (OAuth via `claude` CLI)
#   - cru binary built and kiln processed
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

# Set PATH to ensure npx and cru-server are available
Type "export PATH=~/.nvm/versions/node/v24.7.0/bin:./crucible/target/debug:$PATH"
Enter
Sleep 1s

# Launch Crucible chat with Claude Code as ACP agent
# The agent gets MCP tools and Precognition context from the knowledge graph
Type "./crucible/target/debug/cru chat -a claude --no-process"
Enter
Sleep 10s

# Enable auto-approval of tool permissions for this session
Type ":set perm.autoconfirm_session true"
Enter
Sleep 2s

# Ask a question that exercises the protocol docs from the knowledge graph
# Precognition will inject ACP/MCP reference docs as context before Claude responds
Type "How does ACP differ from MCP, and how does Crucible use both?"
Enter
Sleep 120s

# Exit — first Ctrl+C shows "press again to quit", second confirms
Ctrl+C
Sleep 500ms
Ctrl+C
Sleep 5s
