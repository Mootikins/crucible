# Testing the ACP CLI Integration

This guide walks through testing the Crucible CLI with ACP (Agent Client Protocol) integration.

## Prerequisites

1. **Install an ACP-compatible agent**:
   ```bash
   npm install -g @anthropic/claude-code
   ```

   Or use another compatible agent:
   - `gemini-cli`
   - `codex`

2. **Create a test kiln** (if you don't have one):
   ```bash
   mkdir -p test-kiln
   cd test-kiln

   # Create a sample note
   cat > "My First Note.md" << 'EOF'
   ---
   tags: [test, example]
   created: 2025-11-23
   ---

   # My First Note

   This is a test note for the Crucible ACP integration.

   See also: [[Another Note]]
   EOF

   cat > "Another Note.md" << 'EOF'
   # Another Note

   This note is linked from [[My First Note]].
   EOF
   ```

3. **Configure the test environment**:
   - Edit `config.test.toml` and set the `path` to your test kiln:
     ```toml
     [kiln]
     path = "/path/to/your/test-kiln"
     ```

## Building the CLI

```bash
# From the crucible repository root
cargo build --package crucible-cli --bin cru
```

## Testing Scenarios

### 1. Check Help Output

```bash
./target/debug/cru chat --help
```

Expected: Help text showing all options including `--agent`, `--no-context`, `--act`, etc.

### 2. One-Shot Query (Read-Only)

```bash
./target/debug/cru chat --config config.test.toml \
  --no-process \
  "What notes are in this kiln?"
```

Expected:
- CLI starts in plan mode (read-only)
- Agent discovers 10 Crucible tools
- Agent can list notes using `list_notes` tool
- Agent responds with note names

### 3. Interactive Mode (Read-Only)

```bash
./target/debug/cru chat --config config.test.toml --no-process
```

In the interactive session:
```
plan 📖 > What notes do you see?
plan 📖 > Read "My First Note"
plan 📖 > /exit
```

Expected:
- Visual mode indicator (plan 📖)
- Agent can use tools to list and read notes
- Clean exit on `/exit` command

### 4. Interactive Mode with Write Access

```bash
./target/debug/cru chat --config config.test.toml --no-process --act
```

In the interactive session:
```
act ✏️ > Create a new note called "Test Note" with content "This is a test"
act ✏️ > /plan
plan 📖 > Read "Test Note"
plan 📖 > /act
act ✏️ > Update "Test Note" to add a tag
act ✏️ > /exit
```

Expected:
- Mode switching works (/plan, /act)
- Agent can create and update notes in act mode
- Agent can read notes in both modes

### 5. Context Enrichment

```bash
# With context (default)
./target/debug/cru chat --config config.test.toml \
  --no-process \
  "Tell me about the note that mentions 'Another Note'"

# Without context
./target/debug/cru chat --config config.test.toml \
  --no-process \
  --no-context \
  "Tell me about the note that mentions 'Another Note'"
```

Expected:
- With context: Agent gets relevant note content prepended to query
- Without context: Agent only sees your query

### 6. Note Name Resolution

Test that the agent can reference notes in multiple ways:

```bash
./target/debug/cru chat --config config.test.toml --no-process
```

Then try:
```
plan 📖 > Read "My First Note"           # By name
plan 📖 > Read "[[My First Note]]"       # Wikilink format
plan 📖 > Read "My First Note.md"        # With extension
plan 📖 > Read "./My First Note.md"      # Relative path
```

Expected: All formats should work and return the same content

### 7. Tool Discovery

Verify all 10 tools are available:

```bash
./target/debug/cru chat --config config.test.toml --no-process
```

Ask the agent:
```
plan 📖 > What tools do you have access to?
```

Expected tools:
1. `read_note` - Read note content
2. `create_note` - Create new note
3. `update_note` - Update existing note
4. `delete_note` - Delete a note
5. `list_notes` - List all notes
6. `read_metadata` - Read note frontmatter
7. `semantic_search` - Vector similarity search
8. `text_search` - Keyword search
9. `property_search` - Search by metadata
10. `get_kiln_info` - Get kiln statistics

## Troubleshooting

### Agent Not Found

```
Error: No compatible ACP agent found.
```

**Solution**: Install Claude Code or specify a custom agent:
```bash
npm install -g @anthropic/claude-code
# OR
./target/debug/cru chat --agent /path/to/custom/agent
```

### Database Lock Error

```
Error: RocksDB lock collision
```

**Solution**: The database uses process IDs to prevent conflicts. If you see this:
1. Make sure no other `cru` processes are running
2. Delete the `.crucible/` directory in your kiln
3. Try again

### Tools Not Working

If the agent says tools aren't available:

1. Check that kiln path is set correctly in `config.test.toml`
2. Verify the kiln directory exists
3. Check logs with `--verbose` flag:
   ```bash
   ./target/debug/cru chat --config config.test.toml --verbose
   ```

### Context Enrichment Fails

```
Error: Context enrichment failed
```

This is normal if:
- Kiln is empty (no notes to search)
- Embedding service is unavailable

The CLI will fall back to the original query automatically.

## Expected Test Results

| Test | Status | Notes |
|------|--------|-------|
| CLI Compilation | ✅ | 3 warnings (unused imports) OK |
| Unit Tests | ✅ | 143 tests passing |
| Agent Discovery | ✅ | Finds claude-code if installed |
| Tool Registration | ✅ | All 10 tools available |
| Note Reading | ✅ | All three formats work |
| Interactive Mode | ✅ | Commands and signals work |
| Mode Switching | ✅ | /plan and /act work |
| Context Enrichment | ⏳ | Requires embedding service |
| End-to-End | ⏳ | Requires agent installation |

## Performance Benchmarks

Expected performance (approximate):

- **Startup time**: < 2 seconds
- **Tool registration**: < 500ms
- **Note read**: < 100ms
- **Agent response**: 1-5 seconds (depends on agent)
- **Context enrichment**: 200ms-2s (depends on embedding service)

## What's Next

After successful testing:

1. Performance optimization
2. Error recovery improvements
3. Add permission prompts for write operations
4. Implement session persistence
5. Add agent configuration presets

## Reporting Issues

If you encounter issues:

1. Run with `--verbose` to see detailed logs
2. Check `PROGRESS_REPORT.md` for known issues
3. Report bugs with:
   - CLI version (`./target/debug/cru --version`)
   - Agent version
   - Full error output
   - Steps to reproduce
