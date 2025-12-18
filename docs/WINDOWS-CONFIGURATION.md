# Windows Configuration Guide

This guide covers Windows-specific configuration and troubleshooting for Crucible.

## C Runtime Library Configuration

On Windows, Rust uses the MSVC toolchain which requires consistent C runtime library linkage across all dependencies. You may encounter linker errors if dependencies are compiled with different runtime settings:

```
error LNK2038: mismatch detected for 'RuntimeLibrary': 
value 'MT_StaticRelease' doesn't match value 'MD_DynamicRelease'
```

### Solution

The project includes a `.cargo/config.toml` file that ensures consistent dynamic runtime linkage (the default on Windows). This matches most dependencies including:

- ONNX Runtime (`ort_sys`) - used by FastEmbed
- Most Rust crates on Windows

### Troubleshooting Runtime Mismatch Errors

If you see errors like:
```
error LNK2038: mismatch detected for 'RuntimeLibrary': 
value 'MT_StaticRelease' doesn't match value 'MD_DynamicRelease'
```

This typically happens when:
- `esaxx-rs` (used by `tokenizers`/`fastembed`) is compiled with static runtime
- `ort_sys` (ONNX Runtime) uses dynamic runtime

**Steps to resolve:**

1. **Clean and rebuild** (most common fix):
   ```powershell
   cargo clean
   cargo build
   ```

2. **Verify `.cargo/config.toml` exists** and uses dynamic runtime (default)

3. **Rebuild dependencies from source** if needed:
   ```powershell
   cargo clean
   cargo build --verbose  # Check which dependencies are being rebuilt
   ```

4. **If the issue persists**, you may need to:
   - Use a different embedding provider (e.g., Ollama instead of FastEmbed)
   - Rebuild problematic dependencies from source
   - Check if there are updated versions of dependencies that fix the issue

### If You Need Static Runtime

If you need to use static runtime instead (e.g., for standalone executables), you can modify `.cargo/config.toml`:

```toml
[target.'cfg(windows)']
rustflags = ["-C", "target-feature=+crt-static"]
```

**Warning:** This may cause linker errors with dependencies that expect dynamic runtime, such as `ort_sys`. You may need to rebuild those dependencies or use alternative providers.

## Configuration File Locations

Crucible uses a cross-platform configuration system that works consistently across Linux, macOS, and Windows.

### Main Configuration File

The primary configuration file location follows platform conventions:

- **Linux**: `~/.config/crucible/config.toml` (XDG Base Directory)
- **macOS**: `~/Library/Application Support/crucible/config.toml`
- **Windows**: `%APPDATA%\crucible\config.toml` (e.g., `C:\Users\YourName\AppData\Roaming\crucible\config.toml`)

**Note:** On Windows, Crucible uses `%APPDATA%` (Roaming AppData) which is the standard Windows location for user configuration files that should roam with the user profile. This follows Windows conventions rather than using a Unix-style `~/.config/` path.

### Kiln-Specific Configuration

Each kiln (knowledge base) can have its own configuration:

- **Kiln config**: `KILN_ROOT/.crucible/` directory
  - Database: `KILN_ROOT/.crucible/kiln.db`
  - Kiln-specific tools: `KILN_ROOT/.crucible/tools/`
  - Kiln-specific agents: `KILN_ROOT/.crucible/agents/`
  - Kiln-specific hooks: `KILN_ROOT/.crucible/hooks/`
  - Kiln-specific events: `KILN_ROOT/.crucible/events/`

**Future:** Workspace-level configuration may be added in `WORKSPACE_ROOT/.crucible/`

### Discovery Paths

Crucible searches for resources (tools, hooks, events, agents) in the following order:

1. **Kiln-specific** (highest priority):
   - `KILN_ROOT/.crucible/tools/`
   - `KILN_ROOT/.crucible/hooks/`
   - `KILN_ROOT/.crucible/events/`
   - `KILN_ROOT/.crucible/agents/`

2. **Global user directories** (these are configuration, not data):
   - **Tools**: 
     - Linux: `~/.config/crucible/tools/`
     - Windows: `%APPDATA%\crucible\tools\` (e.g., `C:\Users\YourName\AppData\Roaming\crucible\tools\`)
     - macOS: `~/Library/Application Support/crucible/tools/`
   - **Hooks**: 
     - Linux: `~/.config/crucible/hooks/`
     - Windows: `%APPDATA%\crucible\hooks\` (e.g., `C:\Users\YourName\AppData\Roaming\crucible\hooks\`)
     - macOS: `~/Library/Application Support/crucible/hooks/`
   - **Events**: 
     - Linux: `~/.config/crucible/events/`
     - Windows: `%APPDATA%\crucible\events\` (e.g., `C:\Users\YourName\AppData\Roaming\crucible\events\`)
     - macOS: `~/Library/Application Support/crucible/events/`
   - **Agents**: 
     - Linux: `~/.config/crucible/agents/`
     - Windows: `%APPDATA%\crucible\agents\` (e.g., `C:\Users\YourName\AppData\Roaming\crucible\agents\`)
     - macOS: `~/Library/Application Support/crucible/agents/`

3. **Additional paths** from `config.toml`:
   - `agent_directories` field
   - `discovery.tools.additional_paths`
   - `discovery.hooks.additional_paths`
   - `discovery.events.additional_paths`

### Data Directories

- **Embedding models cache**: 
  - Linux: `~/.local/share/crucible/embedding-models`
  - macOS: `~/Library/Application Support/crucible/embedding-models`
  - Windows: `%LOCALAPPDATA%\crucible\embedding-models` (e.g., `C:\Users\YourName\AppData\Local\crucible\embedding-models`)

### Creating Your Configuration File

To create your configuration file on Windows:

```powershell
# Create the config directory (uses %APPDATA% which is Roaming AppData)
New-Item -ItemType Directory -Force -Path "$env:APPDATA\crucible"

# Create a basic config file
@"
kiln_path = "C:\Users\YourName\Documents\my-kiln"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
batch_size = 16
"@ | Out-File -FilePath "$env:APPDATA\crucible\config.toml" -Encoding utf8
```

Or manually create the file at: `C:\Users\YourName\AppData\Roaming\crucible\config.toml`

### Database Location

The database is stored in each kiln's `.crucible` directory:

- **Location**: `KILN_ROOT/.crucible/kiln.db`
- **Example**: If your kiln is at `C:\Users\YourName\Documents\my-kiln`, the database will be at `C:\Users\YourName\Documents\my-kiln\.crucible\kiln.db`

This keeps the database with the kiln, making it easy to backup or move kilns.

### Environment Variables

You can override configuration using environment variables:

- `CRUCIBLE_KILN_PATH` - Path to your kiln (Obsidian vault)
- `CRUCIBLE_EMBEDDING_URL` - Embedding provider API URL
- `CRUCIBLE_EMBEDDING_MODEL` - Embedding model name
- `CRUCIBLE_EMBEDDING_PROVIDER` - Provider type (fastembed, ollama, openai)
- `CRUCIBLE_DATABASE_URL` - Database connection URL
- `CRUCIBLE_SERVER_HOST` - Server hostname
- `CRUCIBLE_SERVER_PORT` - Server port
- `CRUCIBLE_LOG_LEVEL` - Logging level (off, error, warn, info, debug, trace)

## Path Handling

Windows paths are handled automatically:

- Forward slashes (`/`) are converted to backslashes (`\`) where needed
- UNC paths (`\\server\share`) are supported
- Long paths (260+ characters) require Windows 10+ with long path support enabled

## Building on Windows

### Prerequisites

1. **Rust toolchain**: Install from [rustup.rs](https://rustup.rs/)
   - Select "x86_64-pc-windows-msvc" (default)
   - Or "x86_64-pc-windows-gnu" if you prefer MinGW

2. **Visual Studio Build Tools** (for MSVC toolchain):
   - Install "Desktop development with C++" workload
   - Or install Visual Studio Community with C++ support

3. **Git**: For cloning the repository

### Build Commands

```powershell
# Build all crates (debug)
cargo build

# Build release
cargo build --release

# Run tests
cargo test

# Run specific test
cargo test -p crucible-llm --test test_backend_comparison
```

### Common Issues

#### Linker Errors

If you see C runtime mismatch errors, ensure `.cargo/config.toml` exists and is configured correctly (see above).

#### Missing DLLs

If you get "missing DLL" errors at runtime:

1. Install [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)
2. Ensure all dependencies are in your PATH
3. For ONNX Runtime, ensure `onnxruntime.dll` is accessible

#### Long Path Issues

If you encounter path length errors:

1. Enable long path support in Windows (requires admin):
   ```powershell
   New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
     -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
   ```
2. Or use shorter paths for your kiln directory

## Testing on Windows

Tests are designed to work cross-platform, but some may have Windows-specific behavior:

- Path separators are normalized automatically
- Line endings (CRLF vs LF) are handled transparently
- File watching may have different timing on Windows

### Running Tests

```powershell
# All tests
cargo test

# Specific crate
cargo test -p crucible-llm

# With output
cargo test -- --nocapture

# Specific test
cargo test test_backend_comparison
```

## Performance Considerations

- **File watching**: Windows file system events may have different latency than Linux
- **Embedding models**: ONNX Runtime performance is similar across platforms
- **Database**: SurrealDB performance is consistent on Windows

## Getting Help

If you encounter Windows-specific issues:

1. Check this guide first
2. Review error messages carefully (Windows errors are often descriptive)
3. Check that all prerequisites are installed
4. Verify `.cargo/config.toml` is configured correctly
5. Open an issue on GitHub with:
   - Windows version
   - Rust version (`rustc --version`)
   - Full error message
   - Steps to reproduce
