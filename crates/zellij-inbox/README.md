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
cargo build --release --target wasm32-wasip1 --lib

# Install
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij_inbox.wasm ~/.config/zellij/plugins/
```

Add to your Zellij config:

```kdl
keybinds {
    shared {
        bind "Alt i" {
            LaunchOrFocusPlugin "file:~/.config/zellij/plugins/zellij_inbox.wasm" {
                floating true
            }
        }
    }
}
```

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

### Hook Integration (Claude Code)

Add to `.claude/settings.json`:

```json
{
  "hooks": {
    "Notification": [{
      "command": "zellij-inbox add \"claude-code: $CLAUDE_NOTIFICATION\" --pane $ZELLIJ_PANE_ID --project $(basename $(git rev-parse --show-toplevel 2>/dev/null) || basename $PWD)"
    }],
    "UserPromptSubmit": [{
      "command": "zellij-inbox remove --pane $ZELLIJ_PANE_ID"
    }]
  }
}
```

## File Format

Inbox stored at `~/.local/share/zellij-inbox/{session}.md`:

```markdown
## Waiting for Input

### crucible
- [ ] claude-code: Auth question [pane:: 42]

## Background

### crucible
- [/] indexer: Processing files [pane:: 5]
```

## License

MIT
