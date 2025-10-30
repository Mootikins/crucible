# Crucible CLI Operation Modes

This document describes the different operational modes of the Crucible CLI and important design decisions.

## CLI Modes

### 1. Command Mode (Default for Subcommands)
When you run `cru` with a subcommand, it executes that specific command and exits.

**Examples:**
```bash
cru search "my query"
cru semantic "AI concepts"
cru stats
cru note create "My Note" --tags "work,ideas"
```

### 2. Interactive REPL Mode (Default when no subcommand)
When you run `cru` without arguments, it starts an interactive Read-Eval-Print Loop.

**Usage:**
```bash
cru                    # Starts interactive REPL
```

**Features:**
- Line editing with history (via reedline)
- Syntax highlighting for SurrealQL
- Tab completion
- Built-in commands (`:tools`, `:run`, `:help`, `:quit`)
- Direct SurrealQL query execution
- **Requires a TTY** (terminal device)

### 3. Non-Interactive Mode (For Testing/Scripting)
When you run `cru` with the `--non-interactive` flag, it reads commands from stdin without requiring a TTY.

**Usage:**
```bash
echo ":tools" | cru --non-interactive
cru --non-interactive < commands.txt
```

**Features:**
- Reads commands line-by-line from stdin
- No terminal requirement (works in pipes, CI/CD, tests)
- Flushes output after each command
- Same command processing as interactive mode
- Exits on `:quit`, `:q`, or `:exit`

**Use Cases:**
- Automated testing
- CI/CD pipelines
- Shell scripts
- Non-TTY environments

## Configuration

### File-Based Configuration Only (Since v0.2.0)

**IMPORTANT:** Crucible CLI uses **file-based configuration only**. Environment variables are **NOT** supported.

Configuration is loaded from:
1. `~/.config/crucible/config.toml` (default)
2. Custom path via `--config` flag

**Removed Environment Variables:**
The following environment variables were removed in v0.2.0 and are **no longer read**:
- `OBSIDIAN_KILN_PATH` - Use `config.toml` `kiln.path` instead
- `EMBEDDING_MODEL` - Use `config.toml` `embedding.model` instead
- `EMBEDDING_ENDPOINT` - Use `config.toml` `embedding.endpoint` instead
- `CRUCIBLE_DB_PATH` - Use `--db-path` flag or `config.toml` instead
- `CRUCIBLE_TEST_MODE` - No longer used

**Why File-Based Only?**
- Explicit configuration management
- No implicit environment variable precedence
- Easier to track and version control
- Clearer for users and tests
- Reduces configuration bugs from stale environment

**Configuration Example:**
```toml
[kiln]
path = "/home/user/my-kiln"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
endpoint = "https://api.example.com"  # Optional for remote providers

[database]
path = "~/.crucible/kiln.db"
```

## Testing

### Integration Tests

REPL tests use `--non-interactive` mode:

```rust
let mut child = Command::new(cli_path)
    .arg("--non-interactive")
    .arg("--db-path")
    .arg(&db_path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;
```

This allows tests to:
- Run without a TTY
- Send commands via stdin
- Capture output via stdout
- Avoid blocking on terminal operations

### Test Configuration

Tests should:
1. Create temporary config files (not use env vars)
2. Use `--non-interactive` for REPL tests
3. Specify explicit `--db-path` and `--tool-dir`
4. Clean up temporary files after execution

**Example:**
```rust
let config_content = format!(
    r#"
[kiln]
path = "{}"

[embedding]
provider = "mock"
model = "test-model"
"#,
    kiln_path.display()
);

let config_path = temp_dir.join("config.toml");
std::fs::write(&config_path, config_content)?;

Command::new("cru")
    .arg("--config")
    .arg(&config_path)
    .arg("--non-interactive")
    // ...
```

## Migration from Environment Variables

If you were using environment variables before v0.2.0, migrate to config files:

### Old Way (No Longer Works):
```bash
export OBSIDIAN_KILN_PATH="/home/user/my-kiln"
export EMBEDDING_MODEL="BAAI/bge-small-en-v1.5"
cru semantic "query"
```

### New Way:
```bash
# Create ~/.config/crucible/config.toml with:
cat > ~/.config/crucible/config.toml <<EOF
[kiln]
path = "/home/user/my-kiln"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
EOF

# Then run:
cru semantic "query"
```

Or use command-line flags:
```bash
cru --db-path /custom/path.db semantic "query"
```

## Architecture Notes

### REPL Implementation

- **Interactive Mode:** Uses `reedline` library for line editing
  - Provides history, completion, highlighting
  - Requires TTY for terminal control
  - Method: `Repl::run()`

- **Non-Interactive Mode:** Uses `std::io::stdin()` directly
  - Line-by-line reading without terminal control
  - Works in pipes and non-TTY environments
  - Method: `Repl::run_non_interactive()`

### Why Two Modes?

1. **Interactive Mode** provides a rich user experience with:
   - Command history (persistent across sessions)
   - Tab completion
   - Syntax highlighting
   - Line editing (Emacs/Vi keybindings)

2. **Non-Interactive Mode** enables:
   - Automated testing without mock terminals
   - CI/CD integration
   - Shell scripting
   - Programmatic control

Both modes share the same command processing logic, ensuring consistent behavior.

## See Also

- `crates/crucible-cli/src/commands/repl/mod.rs` - REPL implementation
- `crates/crucible-cli/src/config.rs` - Configuration loading
- `crates/crucible-cli/tests/repl_end_to_end_tests.rs` - Test examples
