# Troubleshooting Guide

> **Status**: Active Troubleshooting Documentation
> **Version**: 1.0.0
> **Date**: 2025-10-23
> **Purpose**: Common issues and solutions for Crucible users and developers

## Table of Contents

- [Installation Issues](#installation-issues)
- [Build and Compilation Issues](#build-and-compilation-issues)
- [Runtime Issues](#runtime-issues)
- [Database Issues](#database-issues)
- [CLI Issues](#cli-issues)
- [Performance Issues](#performance-issues)
- [Development Issues](#development-issues)
- [Getting Help](#getting-help)

## Installation Issues

### Rust Installation Problems

**Issue**: `command not found: cargo` or `rustc not found`

**Solution**:
```bash
# Install Rust using rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version
```

**Issue**: Permission denied during installation

**Solution**:
```bash
# Use a user-local installation without sudo
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Or fix permissions for cargo home
sudo chown -R $USER:$(id -gn $USER) ~/.cargo
```

### Node.js Installation Problems

**Issue**: Node.js version too old

**Solution**:
```bash
# Install Node.js 18 or later
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install nodejs

# Verify version
node --version  # Should be 18.0.0 or later
```

### Dependency Installation Failures

**Issue**: `error: failed to compile` during `cargo build`

**Common Solutions**:

1. **Update Rust toolchain**:
   ```bash
   rustup update
   rustup component add clippy rustfmt
   ```

2. **Clean and rebuild**:
   ```bash
   cargo clean
   cargo build
   ```

3. **Check for missing system dependencies**:
   ```bash
   # Ubuntu/Debian
   sudo apt update
   sudo apt install build-essential pkg-config libssl-dev

   # macOS
   xcode-select --install
   brew install openssl
   ```

## Build and Compilation Issues

### Compilation Errors in crucible-tools

**Issue**: Multiple compilation errors in `crucible-tools`

**Current Status**: Known issue with 175 compilation errors

**Temporary Solution**:
```bash
# Skip building problematic crates for now
cargo build -p crucible-cli
cargo build -p crucible-core
cargo build -p crucible-services

# Build specific working components
cargo run -p crucible-cli -- --help
```

**Permanent Solution**: This issue is being tracked and will be resolved in the next release.

### Memory Compilation Errors

**Issue**: Out of memory errors during compilation

**Solution**:
```bash
# Limit parallel jobs
export CARGO_BUILD_JOBS=2
cargo build

# Or use single thread
cargo build -j 1
```

### Linker Errors

**Issue**: `linker` not found or linking errors

**Solution**:
```bash
# Ubuntu/Debian
sudo apt install build-essential

# macOS
xcode-select --install

# Use LLD linker for faster compilation
export RUSTFLAGS="-C link-arg=-fuse-ld=lld"
```

## Runtime Issues

### Database Connection Issues

**Issue**: `Database connection failed` or `Unable to connect to database`

**Solutions**:

1. **Check database paths**:
   ```bash
   crucible-cli --db-path /path/to/database
   ```

2. **Create database directory**:
   ```bash
   mkdir -p ~/.local/share/crucible/db
   ```

3. **Check permissions**:
   ```bash
   ls -la ~/.local/share/crucible/
   chmod -R 755 ~/.local/share/crucible/
   ```

### Kiln Processor Failures

**Issue**: `Processor failed to start` or semantic search reports missing embeddings

**Solutions**:

1. **Check processor status**:
   ```bash
   crucible-cli process status
   ```

2. **Run the processor with verbose output**:
   ```bash
   RUST_LOG=debug crucible-cli process start --wait
   ```

3. **Review recent processor output**:
   ```bash
   RUST_LOG=trace crucible-cli process restart --wait --force
   ```

### Configuration Issues

**Issue**: `Configuration file not found` or `Invalid configuration`

**Solutions**:

1. **Create default configuration**:
   ```bash
   mkdir -p ~/.config/crucible
   crucible-cli --help > ~/.config/crucible/config.toml
   ```

2. **Check configuration syntax**:
   ```bash
   crucible-cli config --validate
   ```

3. **Use environment variables**:
   ```bash
   export CRUCIBLE_KILN_PATH=/path/to/kiln
   export CRUCIBLE_LOG_LEVEL=debug
   ```

## Database Issues

### SurrealDB Connection Problems

**Issue**: `Cannot connect to SurrealDB`

**Solutions**:

1. **Check if SurrealDB is running**:
   ```bash
   ps aux | grep surreal
   ```

2. **Start SurrealDB manually**:
   ```bash
   surreal start memory --user root --pass root
   ```

3. **Check connection URL**:
   ```bash
   crucible-cli --db-url "ws://localhost:8000"
   ```

### Database Corruption

**Issue**: `Database file corrupted` or `Unable to read database`

**Solutions**:

1. **Backup current database**:
   ```bash
   cp ~/.local/share/crucible/db/* ~/crucible-db-backup/
   ```

2. **Recreate database**:
   ```bash
   rm ~/.local/share/crucible/db/*
   crucible-cli index --rebuild
   ```

3. **Import from backup**:
   ```bash
   crucible-cli import --from ~/crucible-db-backup/
   ```

## CLI Issues

### Command Not Found

**Issue**: `crucible-cli: command not found`

**Solutions**:

1. **Build and install**:
   ```bash
   cargo build -p crucible-cli
   cargo install --path crates/crucible-cli
   ```

2. **Add to PATH**:
   ```bash
   echo 'export PATH="$PATH:~/.cargo/bin"' >> ~/.bashrc
   source ~/.bashrc
   ```

3. **Run from source**:
   ```bash
   cargo run -p crucible-cli -- [arguments]
   ```

### REPL Issues

**Issue**: REPL not starting or freezing

**Solutions**:

1. **Check terminal compatibility**:
   ```bash
   export TERM=xterm-256color
   crucible-cli
   ```

2. **Disable fancy features**:
   ```bash
   crucible-cli --no-color
   ```

3. **Use basic mode**:
   ```bash
   crucible-cli --basic
   ```

### Search Not Working

**Issue**: `Search returned no results` or `Search failed`

**Solutions**:

1. **Rebuild index**:
   ```bash
   crucible-cli index --rebuild
   ```

2. **Check kiln path**:
   ```bash
   crucible-cli --kiln-path /path/to/kiln search "query"
   ```

3. **Check file permissions**:
   ```bash
   ls -la /path/to/kiln
   chmod -R 644 /path/to/kiln/*
   ```

## Performance Issues

### Slow Search Performance

**Issue**: Search operations taking too long

**Solutions**:

1. **Rebuild search index**:
   ```bash
   crucible-cli index --force-rebuild
   ```

2. **Limit search scope**:
   ```bash
   crucible-cli search "query" --limit 10
   ```

3. **Use specific search type**:
   ```bash
   crucible-cli fuzzy "query" --no-content
   ```

### High Memory Usage

**Issue**: Process using too much memory

**Solutions**:

1. **Limit concurrent operations**:
   ```bash
   crucible-cli --max-concurrent 2
   ```

2. **Clear cache**:
   ```bash
   crucible-cli cache --clear
   ```

3. **Reduce indexing scope**:
   ```bash
   crucible-cli index --exclude "*.tmp,*.log"
   ```

### Slow Startup

**Issue**: Application taking too long to start

**Solutions**:

1. **Disable auto-indexing**:
   ```bash
   crucible-cli --no-auto-index
   ```

2. **Use smaller kiln**:
   ```bash
   crucible-cli --kiln-path ~/small-kiln
   ```

3. **Check for corrupted files**:
   ```bash
   crucible-cli check --kiln
   ```

## Development Issues

### Test Failures

**Issue**: Tests failing during `cargo test`

**Common Solutions**:

1. **Update dependencies**:
   ```bash
   cargo update
   ```

2. **Clean test cache**:
   ```bash
   cargo clean
   cargo test
   ```

3. **Run specific test**:
   ```bash
   cargo test test_name
   ```

4. **Run with different features**:
   ```bash
   cargo test --no-default-features
   ```

### IDE Integration Issues

**Issue**: Rust analyzer not working properly

**Solutions**:

1. **Restart Rust analyzer**:
   ```bash
   # In VS Code: Ctrl+Shift+P -> "Rust Analyzer: Reload workspace"
   ```

2. **Update Rust analyzer**:
   ```bash
   # In VS Code: Check for updates to Rust analyzer extension
   ```

3. **Check toolchain**:
   ```bash
   rustup default stable
   rustup component add rust-src
   ```

### Documentation Build Issues

**Issue**: `cargo doc` failing

**Solutions**:

1. **Build without dependencies**:
   ```bash
   cargo doc --no-deps
   ```

2. **Build specific crate**:
   ```bash
   cargo doc -p crucible-cli
   ```

3. **Open in browser**:
   ```bash
   cargo doc --open
   ```

## Getting Help

### Check Logs

For detailed error information, check the logs:

```bash
# Enable verbose logging
crucible-cli --verbose

# Check kiln processor output
RUST_LOG=debug crucible-cli process start --wait

# Tail application logs
tail -f ~/.local/share/crucible/logs/crucible.log
```

### Report Issues

When reporting issues, include:

1. **System Information**:
   ```bash
   uname -a
   rustc --version
   cargo --version
   ```

2. **Error Messages**: Full error output

3. **Steps to Reproduce**: Detailed reproduction steps

4. **Configuration**: Your configuration (remove sensitive data)

### Community Support

- **GitHub Issues**: Report bugs and request features
- **GitHub Discussions**: Ask questions and share experiences
- **Documentation**: Check [API Documentation](./API_DOCUMENTATION.md) and [CLI Reference](./CLI_REFERENCE.md)

### Common Debug Commands

```bash
# Check processor status
crucible-cli process status

# Show current configuration
crucible-cli config show

# Run a semantic search
crucible-cli semantic "debugging checklist" --show-scores

# Display kiln statistics
crucible-cli stats

# Show version information
crucible-cli --version
```

---

## Quick Reference

### Most Common Issues

| Issue | Quick Fix |
|-------|-----------|
| `command not found: cargo` | Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` |
| Compilation errors | `cargo clean && cargo build` |
| Database connection failed | Check permissions and paths |
| Semantic search returns nothing | `crucible-cli process start --wait` |
| CLI command not found | `cargo install --path crates/crucible-cli` |

### Useful Commands

```bash
# Full system reset (developer use only)
cargo clean
rm -rf ~/.local/share/crucible/

# Debug mode
RUST_LOG=debug crucible-cli --verbose

# Check everything
crucible-cli process status
crucible-cli config show
crucible-cli stats
```

For additional help, please check the [main documentation](../README.md) or [create an issue](https://github.com/matthewkrohn/crucible/issues).
