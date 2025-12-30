# zellij-inbox

Agent inbox plugin for Zellij. Displays agents waiting for user input across multiple projects.

## Installation

### CLI

```bash
cargo install --path .
```

### Zellij Plugin

```bash
# Build WASM
cargo build --release --target wasm32-wasip1 --bin zellij-inbox-plugin

# Install
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij-inbox-plugin.wasm ~/.config/zellij/plugins/zellij_inbox.wasm
```

Add to your Zellij config (`~/.config/zellij/config.kdl`):

```kdl
keybinds {
    shared {
        // Alt+Shift+i (avoids conflict with Alt+i = move tab left)
        bind "Alt Shift i" {
            LaunchOrFocusPlugin "file:~/.config/zellij/plugins/zellij_inbox.wasm" {
                floating true
                move_to_focused_tab true  // Plugin follows you across tabs
            }
        }
    }
}
```

**Key options:**
- `floating true` - Opens as floating pane (press Esc to hide)
- `move_to_focused_tab true` - Plugin follows you when switching tabs

Alternative keybinds: `Ctrl Shift i`, `Alt n` (notifications), or use Zellij's plugin launcher (`Ctrl+o` → `w`).

## Usage

### CLI

```bash
# Add an item
zellij-inbox add "claude-code: Waiting for input" --pane 42 --project myproject

# Remove an item
zellij-inbox remove --pane 42

# List items
zellij-inbox list
zellij-inbox list --json

# Clear all
zellij-inbox clear
```

### Plugin Controls

- **j/k** or **↑/↓** - Navigate items
- **Enter** - Focus the selected pane
- **Esc** or **q** - Close the inbox

## Claude Code Integration

Add to `.claude/settings.json` (project) or `~/.claude/settings.json` (global):

```json
{
  "hooks": {
    "Notification": [
      {
        "matcher": "idle_prompt",
        "hooks": [
          {
            "type": "command",
            "command": "zellij-inbox add \"claude: Waiting for input\" --pane $ZELLIJ_PANE_ID --project $(basename \"$(git rev-parse --show-toplevel 2>/dev/null || echo $PWD)\")"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "zellij-inbox remove --pane $ZELLIJ_PANE_ID"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "zellij-inbox remove --pane $ZELLIJ_PANE_ID"
          }
        ]
      }
    ]
  }
}
```

### Hook Events

| Event | When it fires | Action |
|-------|--------------|--------|
| `Notification` (idle_prompt) | Claude is waiting for user input | Add to inbox |
| `UserPromptSubmit` | User submits a prompt | Remove from inbox |
| `Stop` | Claude finishes responding | Remove from inbox |

### Environment Variables

The hooks use these Zellij environment variables:
- `$ZELLIJ_PANE_ID` - Unique pane identifier
- `$ZELLIJ_SESSION_NAME` - Current session name (used for inbox file)

## File Format

Inbox stored at `~/.local/share/zellij-inbox/{session}.md`:

```markdown
## Waiting for Input

### crucible
- [ ] claude: Waiting for input [pane:: 42]

### k3s
- [ ] claude: Review this PR [pane:: 17]

## Background

### crucible
- [/] indexer: Processing files [pane:: 5]
```

## Development

```bash
# Run tests
cargo test -p zellij-inbox

# Build both CLI and plugin
cargo build --release -p zellij-inbox
cargo build --release -p zellij-inbox --bin zellij-inbox-plugin --target wasm32-wasip1
```

## License

MIT
