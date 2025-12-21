---
description: Windows-specific configuration, building, and troubleshooting
tags:
  - guide
  - windows
  - configuration
  - troubleshooting
---

# Windows Setup

This guide covers Windows-specific configuration and troubleshooting for Crucible.

## Configuration File Locations

Crucible uses a cross-platform configuration system that works consistently across Linux, macOS, and Windows.

### Main Configuration File

The primary configuration file location follows platform conventions:

- **Linux**: `~/.config/crucible/config.toml` (XDG Base Directory)
- **macOS**: `~/Library/Application Support/crucible/config.toml`
- **Windows**: `%APPDATA%\crucible\config.toml` (e.g., `C:\Users\YourName\AppData\Roaming\crucible\config.toml`)

> **Note:** On Windows, Crucible uses `%APPDATA%` (Roaming AppData) which is the standard Windows location for user configuration files that should roam with the user profile.

### Kiln-Specific Configuration

Each kiln (knowledge base) can have its own configuration:

- **Kiln config**: `KILN_ROOT/.crucible/` directory
  - Database: `KILN_ROOT/.crucible/kiln.db`
  - Kiln-specific tools: `KILN_ROOT/.crucible/tools/`
  - Kiln-specific agents: `KILN_ROOT/.crucible/agents/`
  - Kiln-specific hooks: `KILN_ROOT/.crucible/hooks/`
  - Kiln-specific events: `KILN_ROOT/.crucible/events/`

### Discovery Paths

Crucible searches for resources (tools, hooks, events, agents) in the following order:

1. **Kiln-specific** (highest priority):
   - `KILN_ROOT/.crucible/tools/`
   - `KILN_ROOT/.crucible/hooks/`
   - `KILN_ROOT/.crucible/events/`
   - `KILN_ROOT/.crucible/agents/`

2. **Global user directories**:
   - **Windows**: `%APPDATA%\crucible\{tools,hooks,events,agents}\`

3. **Additional paths** from `config.toml`:
   - `agent_directories` field
   - `discovery.tools.additional_paths`

### Creating Your Configuration File

To create your configuration file on Windows:

```powershell
# Create the config directory
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

### Environment Variables

Override configuration using environment variables:

| Variable | Description |
|----------|-------------|
| `CRUCIBLE_KILN_PATH` | Path to your kiln |
| `CRUCIBLE_EMBEDDING_URL` | Embedding provider API URL |
| `CRUCIBLE_EMBEDDING_MODEL` | Embedding model name |
| `CRUCIBLE_EMBEDDING_PROVIDER` | Provider type (fastembed, ollama, openai) |
| `CRUCIBLE_DATABASE_URL` | Database connection URL |
| `CRUCIBLE_LOG_LEVEL` | Logging level (off, error, warn, info, debug, trace) |

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

### Troubleshooting Runtime Mismatch

If you see `LNK2038` errors:

1. **Clean and rebuild** (most common fix):
   ```powershell
   cargo clean
   cargo build
   ```

2. **Verify `.cargo/config.toml` exists** and uses dynamic runtime (default)

3. **Rebuild dependencies from source** if needed:
   ```powershell
   cargo clean
   cargo build --verbose
   ```

4. **If the issue persists**, consider:
   - Using a different embedding provider (e.g., Ollama instead of FastEmbed)
   - Checking for updated dependency versions

## ONNX Runtime (FastEmbed) Issues

FastEmbed uses ONNX Runtime for local embedding generation. On Windows, this can cause C runtime library mismatches.

### Diagnostic Steps

1. **Run the diagnostic test:**
   ```powershell
   cargo test -p crucible-llm --features fastembed --test test_onnx_windows_diagnostics -- --nocapture
   ```

2. **Verify build configuration:**
   Ensure `.cargo/config.toml` contains:
   ```toml
   [target.'cfg(windows)']
   rustflags = ["-C", "target-feature=-crt-static"]
   ```

3. **Verify Visual C++ Redistributable:**
   Check for `msvcp140.dll` and `vcruntime140.dll` in `C:\Windows\System32\`

   If missing, install from: https://aka.ms/vs/17/release/vc_redist.x64.exe

### Alternative Providers

If ONNX Runtime issues persist, use alternative embedding providers:

- **Ollama**: Local or remote, requires Ollama server
- **llama.cpp**: GGUF models, excellent Windows support
- **OpenAI/Anthropic**: Cloud-based APIs

All providers work on Windows and can be configured in `config.toml`.

## Common Issues

### Missing DLLs

If you get "missing DLL" errors at runtime:

1. Install [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)
2. Ensure all dependencies are in your PATH
3. For ONNX Runtime, ensure `onnxruntime.dll` is accessible

### Long Path Issues

If you encounter path length errors:

1. Enable long path support in Windows (requires admin):
   ```powershell
   New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
     -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
   ```
2. Or use shorter paths for your kiln directory

### Path Handling

Windows paths are handled automatically:

- Forward slashes (`/`) are converted to backslashes (`\`) where needed
- UNC paths (`\\server\share`) are supported
- Long paths (260+ characters) require Windows 10+ with long path support enabled

## Testing on Windows

Tests are designed to work cross-platform:

- Path separators are normalized automatically
- Line endings (CRLF vs LF) are handled transparently
- File watching may have different timing on Windows

```powershell
# All tests
cargo test

# Specific crate
cargo test -p crucible-llm

# With output
cargo test -- --nocapture

# ONNX Runtime diagnostic test
cargo test -p crucible-llm --features fastembed --test test_onnx_windows_diagnostics -- --nocapture
```

## Performance Considerations

- **File watching**: Windows file system events may have different latency than Linux
- **Embedding models**: ONNX Runtime performance is similar across platforms
- **Database**: SurrealDB performance is consistent on Windows

## See Also

- [[Guides/Getting Started]] - General setup guide
- [[Help/Config/embedding]] - Embedding provider configuration
- [[Help/Config/storage]] - Database configuration
