# Daily Notes Plugin

A journaling plugin for Crucible that creates and manages daily notes.

## Features

- **daily_create** - Create a daily note for today or a specific date
- **daily_open** - Open today's note, creating if it doesn't exist
- **daily_list** - List recent daily notes
- **/daily** - Quick command for daily note access

## Installation

Copy this plugin to your Crucible plugins directory:

```bash
cp -r daily-notes ~/.config/crucible/plugins/
# or
cp -r daily-notes ~/your-kiln/plugins/
```

## Usage

### Tools (for agents)

```lua
-- Create today's note
daily_create({})

-- Create note for specific date
daily_create({ date = "2025-01-15" })

-- Open today's note (create if needed)
daily_open({})

-- List last 7 days
daily_list({ days = 7 })
```

### Command (for users)

```
/daily              # Open today's note
/daily today        # Same as above
/daily yesterday    # Open yesterday's note
/daily 2025-01-15   # Open specific date
/daily list         # Show recent notes
```

## Configuration

In your `plugin.yaml`:

```yaml
config:
  properties:
    folder:
      type: string
      default: "Journal"
    template:
      type: string
      default: ""
    date_format:
      type: string
      default: "%Y-%m-%d"
```

### Template Variables

If you specify a template file, these variables are replaced:
- `{{date}}` - The date string
- `{{title}}` - Same as date

## Output Structure

```
your-kiln/
└── Journal/
    ├── 2025-01-18.md
    ├── 2025-01-19.md
    └── 2025-01-20.md
```

## Default Note Template

```markdown
# 2025-01-20

## Notes

## Tasks

- [ ] 
```

## Capabilities Required

- `filesystem` - Create and read note files
- `kiln` - Access to kiln for note storage
