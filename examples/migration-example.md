# Migration Guide: Configuration System

**⚠️ DEPRECATED**: This guide is outdated and kept for historical reference only.

**IMPORTANT**:
- Environment variables are no longer used for configuration
- All configuration is now in TOML/YAML/JSON config files
- Only sensitive data (API keys, passwords) use environment variables
- The crucible-mcp-server no longer exists - use `crucible-cli` instead

---

This guide is for historical reference. For current configuration, see `examples/README.md`.

## Table of Contents

1. [Current Environment Variables](#current-environment-variables)
2. [Migration Steps](#migration-steps)
3. [Migration Examples](#migration-examples)
4. [Validation and Testing](#validation-and-testing)
5. [Troubleshooting](#troubleshooting)

## Current Environment Variables

### Current Configuration (from claude_desktop_config.example.json)

```json
{
  "mcpServers": {
    "crucible": {
      "command": "crucible-mcp-server",
      "args": [],
      "env": {
        "RUST_LOG": "info",
        "OBSIDIAN_VAULT_PATH": "/path/to/vault",

        "EMBEDDING_PROVIDER": "ollama",
        "EMBEDDING_ENDPOINT": "https://llama.example.com",
        "EMBEDDING_MODEL": "nomic-embed-text",
        "EMBEDDING_API_KEY": "sk-api-key-here",
        "EMBEDDING_TIMEOUT_SECS": "30",
        "EMBEDDING_MAX_RETRIES": "3",
        "EMBEDDING_BATCH_SIZE": "10"
      }
    }
  }
}
```

### Environment Variable Mapping

| Environment Variable | Configuration Path | Description |
|---------------------|-------------------|-------------|
| `EMBEDDING_PROVIDER` | `embedding_provider.type` | Provider type (ollama, openai, etc.) |
| `EMBEDDING_ENDPOINT` | `embedding_provider.api.base_url` | API base URL |
| `EMBEDDING_API_KEY` | `embedding_provider.api.key` | API authentication key |
| `EMBEDDING_MODEL` | `embedding_provider.model.name` | Model name |
| `EMBEDDING_TIMEOUT_SECS` | `embedding_provider.api.timeout_seconds` | Request timeout |
| `EMBEDDING_MAX_RETRIES` | `embedding_provider.api.retry_attempts` | Retry attempts |
| `EMBEDDING_BATCH_SIZE` | `embedding_provider.options.batch_size` | Batch processing size |
| `OBSIDIAN_VAULT_PATH` | `custom_settings.obsidian.vault_path` | Obsidian vault path |
| `RUST_LOG` | `logging.level` | Logging level |

## Migration Steps

### Step 1: Choose Configuration Format

The crucible-config system supports multiple formats:

- **YAML** (recommended for human readability)
- **TOML** (recommended for machine parsing)
- **JSON** (for compatibility)

### Step 2: Create Configuration File

Create a configuration file based on your current environment variables:

```bash
# For development
cp examples/development-config.yaml ./crucible-config.yaml

# For production
cp examples/production-config.yaml ./crucible-config.yaml

# For testing
cp examples/testing-config.toml ./crucible-config.toml
```

### Step 3: Map Environment Variables

Convert each environment variable to its corresponding configuration path:

#### Example: Ollama Configuration

**Before (Environment Variables):**
```bash
export EMBEDDING_PROVIDER=ollama
export EMBEDDING_ENDPOINT=https://llama.example.com
export EMBEDDING_MODEL=nomic-embed-text
export EMBEDDING_TIMEOUT_SECS=30
export EMBEDDING_MAX_RETRIES=3
export EMBEDDING_BATCH_SIZE=10
```

**After (YAML Configuration):**
```yaml
embedding_provider:
  type: "ollama"
  api:
    base_url: "https://llama.example.com"
    timeout_seconds: 30
    retry_attempts: 3
  model:
    name: "nomic-embed-text"
    max_tokens: 2048
  options:
    batch_size: 10
```

#### Example: OpenAI Configuration

**Before (Environment Variables):**
```bash
export EMBEDDING_PROVIDER=openai
export EMBEDDING_ENDPOINT=https://api.openai.com/v1
export EMBEDDING_API_KEY=sk-your-api-key-here
export EMBEDDING_MODEL=text-embedding-3-small
export EMBEDDING_TIMEOUT_SECS=30
export EMBEDDING_MAX_RETRIES=3
```

**After (YAML Configuration):**
```yaml
embedding_provider:
  type: "openai"
  api:
    base_url: "https://api.openai.com/v1"
    key: "sk-your-api-key-here"  # Or use environment variable
    timeout_seconds: 30
    retry_attempts: 3
  model:
    name: "text-embedding-3-small"
    dimensions: 1536
    max_tokens: 8192
```

### Step 4: Handle Sensitive Information

For sensitive data like API keys, you have several options:

#### Option 1: Environment Variables (Recommended)
```yaml
embedding_provider:
  api:
    key: null  # Will be read from OPENAI_API_KEY environment variable
```

#### Option 2: Environment Variable References
```yaml
embedding_provider:
  api:
    key: "${OPENAI_API_KEY}"  # Direct reference
```

#### Option 3: Encrypted Configuration
```yaml
embedding_provider:
  api:
    key: "encrypted:base64-encoded-encrypted-key"
```

### Step 5: Update Application Startup

Update your application to use the new configuration system:

**Before:**
```bash
RUST_LOG=info \
EMBEDDING_PROVIDER=ollama \
EMBEDDING_ENDPOINT=https://llama.example.com \
EMBEDDING_MODEL=nomic-embed-text \
crucible-mcp-server
```

**After:**
```bash
CRUCIBLE_CONFIG_PATH=./crucible-config.yaml \
crucible-mcp-server
```

## Migration Examples

### Example 1: Development Environment Migration

**Original Environment Setup:**
```bash
#!/bin/bash
# setup-dev.sh

export RUST_LOG=debug
export OBSIDIAN_VAULT_PATH=/home/user/Documents/vault
export EMBEDDING_PROVIDER=ollama
export EMBEDDING_ENDPOINT=http://localhost:11434
export EMBEDDING_MODEL=nomic-embed-text
export EMBEDDING_TIMEOUT_SECS=60
export EMBEDDING_MAX_RETRIES=2

crucible-mcp-server
```

**Migrated Configuration (crucible-dev.yaml):**
```yaml
# Development configuration migrated from environment variables
profile: "development"

profiles:
  development:
    name: "development"
    environment: "development"
    description: "Development environment migrated from env vars"

    embedding_provider:
      type: "ollama"
      api:
        base_url: "http://localhost:11434"
        timeout_seconds: 60
        retry_attempts: 2
      model:
        name: "nomic-embed-text"
        max_tokens: 2048

    logging:
      level: "debug"
      format: "text"
      file: false

    env_vars:
      RUST_LOG: "debug"
      OBSIDIAN_VAULT_PATH: "/home/user/Documents/vault"

    settings:
      obsidian:
        vault_path: "/home/user/Documents/vault"

# New startup command:
# CRUCIBLE_CONFIG_PATH=./crucible-dev.yaml crucible-mcp-server
```

### Example 2: Production Environment Migration

**Original Environment Setup:**
```bash
#!/bin/bash
# setup-prod.sh

export RUST_LOG=warn
export EMBEDDING_PROVIDER=openai
export EMBEDDING_ENDPOINT=https://api.openai.com/v1
export EMBEDDING_API_KEY="${OPENAI_API_KEY}"
export EMBEDDING_MODEL=text-embedding-3-small
export EMBEDDING_TIMEOUT_SECS=30
export EMBEDDING_MAX_RETRIES=3
export EMBEDDING_BATCH_SIZE=100

# Database settings
export DATABASE_URL="${DATABASE_URL}"
export DATABASE_MAX_CONNECTIONS=20
export DATABASE_TIMEOUT=30

crucible-mcp-server
```

**Migrated Configuration (crucible-prod.yaml):**
```yaml
# Production configuration migrated from environment variables
profile: "production"

profiles:
  production:
    name: "production"
    environment: "production"
    description: "Production environment migrated from env vars"

    embedding_provider:
      type: "openai"
      api:
        base_url: "https://api.openai.com/v1"
        key: null  # Read from OPENAI_API_KEY env var
        timeout_seconds: 30
        retry_attempts: 3
      model:
        name: "text-embedding-3-small"
        dimensions: 1536
        max_tokens: 8192
      options:
        batch_size: 100

    database:
      type: "postgres"
      url: null  # Read from DATABASE_URL env var
      max_connections: 20
      timeout_seconds: 30

    logging:
      level: "warn"
      format: "json"
      file: true
      file_path: "/var/log/crucible/production.log"

    env_vars:
      RUST_LOG: "warn"

    settings:
      production:
        track_api_usage: true
        cost_optimization: true

# New startup command:
# OPENAI_API_KEY=... \
# DATABASE_URL=... \
# CRUCIBLE_CONFIG_PATH=./crucible-prod.yaml \
# crucible-mcp-server
```

### Example 3: Testing Environment Migration

**Original Environment Setup:**
```bash
#!/bin/bash
# setup-test.sh

export RUST_LOG=error
export EMBEDDING_PROVIDER=custom
export EMBEDDING_ENDPOINT=http://localhost:11435
export EMBEDDING_MODEL=mock-embedding-model
export EMBEDDING_TIMEOUT_SECS=5
export EMBEDDING_MAX_RETRIES=1

export DATABASE_URL=":memory:"
export CRUCIBLE_TEST_MODE=true

crucible-mcp-server
```

**Migrated Configuration (crucible-test.toml):**
```toml
profile = "testing"

[profiles.testing]
name = "testing"
environment = "test"
description = "Testing environment migrated from env vars"

[profiles.testing.embedding_provider]
type = "custom"

[profiles.testing.embedding_provider.api]
base_url = "http://localhost:11435"
timeout_seconds = 5
retry_attempts = 1

[profiles.testing.embedding_provider.model]
name = "mock-embedding-model"
dimensions = 768
max_tokens = 1000

[profiles.testing.database]
type = "sqlite"
url = ":memory:"
max_connections = 1
timeout_seconds = 5

[profiles.testing.logging]
level = "error"
format = "text"
file = false

[profiles.testing.env_vars]
RUST_LOG = "error"
CRUCIBLE_TEST_MODE = "true"

# New startup command:
# CRUCIBLE_CONFIG_PATH=./crucible-test.toml crucible-mcp-server
```

## Validation and Testing

### Step 1: Configuration Validation

Use the built-in configuration validator:

```bash
# Validate YAML configuration
crucible config validate --config ./crucible-config.yaml

# Validate TOML configuration
crucible config validate --config ./crucible-config.toml

# Validate specific profile
crucible config validate --config ./crucible-config.yaml --profile production
```

### Step 2: Test Migration

Test the migrated configuration:

```bash
# Test loading configuration
crucible config test --config ./crucible-config.yaml

# Test with specific profile
CRUCIBLE_PROFILE=production crucible config test --config ./crucible-config.yaml

# Test embedding provider
crucible test embeddings --config ./crucible-config.yaml

# Test database connection
crucible test database --config ./crucible-config.yaml
```

### Step 3: Compare Behavior

Ensure the migrated configuration produces the same behavior:

```bash
# Run with old environment variables
export EMBEDDING_PROVIDER=ollama
export EMBEDDING_ENDPOINT=http://localhost:11434
export EMBEDDING_MODEL=nomic-embed-text
crucible-mcp-server --old-mode

# Run with new configuration
CRUCIBLE_CONFIG_PATH=./crucible-config.yaml crucible-mcp-server

# Compare logs and behavior
```

## Migration Script

Here's a helpful migration script:

```bash
#!/bin/bash
# migrate-config.sh

set -e

# Configuration
OLD_ENV_FILE=".env"
NEW_CONFIG_FILE="crucible-config.yaml"
PROFILE="${1:-development}"

echo "Migrating from environment variables to crucible-config..."
echo "Profile: $PROFILE"
echo "New config file: $NEW_CONFIG_FILE"

# Check if environment file exists
if [ ! -f "$OLD_ENV_FILE" ]; then
    echo "Error: Environment file $OLD_ENV_FILE not found"
    exit 1
fi

# Source environment variables
source "$OLD_ENV_FILE"

# Create migration script
cat > "$NEW_CONFIG_FILE" << EOF
# Auto-generated configuration from environment variables
# Generated on: $(date)
# Profile: $PROFILE

profile: "$PROFILE"

profiles:
  $PROFILE:
    name: "$PROFILE"
    environment: "$PROFILE"
    description: "Migrated from environment variables"
EOF

# Add embedding provider configuration
if [ -n "$EMBEDDING_PROVIDER" ]; then
    cat >> "$NEW_CONFIG_FILE" << EOF

    embedding_provider:
      type: "$EMBEDDING_PROVIDER"
EOF

    if [ -n "$EMBEDDING_ENDPOINT" ]; then
        cat >> "$NEW_CONFIG_FILE" << EOF
      api:
        base_url: "$EMBEDDING_ENDPOINT"
EOF
    fi

    if [ -n "$EMBEDDING_API_KEY" ]; then
        cat >> "$NEW_CONFIG_FILE" << EOF
        key: "$EMBEDDING_API_KEY"
EOF
    fi

    if [ -n "$EMBEDDING_TIMEOUT_SECS" ]; then
        cat >> "$NEW_CONFIG_FILE" << EOF
        timeout_seconds: $EMBEDDING_TIMEOUT_SECS
EOF
    fi

    if [ -n "$EMBEDDING_MAX_RETRIES" ]; then
        cat >> "$NEW_CONFIG_FILE" << EOF
        retry_attempts: $EMBEDDING_MAX_RETRIES
EOF
    fi

    if [ -n "$EMBEDDING_MODEL" ]; then
        cat >> "$NEW_CONFIG_FILE" << EOF
      model:
        name: "$EMBEDDING_MODEL"
EOF
    fi
fi

# Add logging configuration
if [ -n "$RUST_LOG" ]; then
    cat >> "$NEW_CONFIG_FILE" << EOF

    logging:
      level: "$RUST_LOG"
      format: "text"
      file: false
EOF
fi

# Add environment variables
cat >> "$NEW_CONFIG_FILE" << EOF

    env_vars:
EOF

if [ -n "$OBSIDIAN_VAULT_PATH" ]; then
    cat >> "$NEW_CONFIG_FILE" << EOF
      OBSIDIAN_VAULT_PATH: "$OBSIDIAN_VAULT_PATH"
EOF
fi

if [ -n "$RUST_LOG" ]; then
    cat >> "$NEW_CONFIG_FILE" << EOF
      RUST_LOG: "$RUST_LOG"
EOF
fi

echo "Migration completed successfully!"
echo "Configuration file created: $NEW_CONFIG_FILE"
echo ""
echo "Next steps:"
echo "1. Review the generated configuration file"
echo "2. Test the configuration: crucible config test --config $NEW_CONFIG_FILE"
echo "3. Update your startup scripts to use: CRUCIBLE_CONFIG_PATH=$NEW_CONFIG_FILE"
```

## Troubleshooting

### Common Issues and Solutions

#### 1. Configuration File Not Found

**Error:** `Configuration file not found: crucible-config.yaml`

**Solution:**
```bash
# Set the configuration path explicitly
export CRUCIBLE_CONFIG_PATH=/path/to/your/config.yaml

# Or use the command-line argument
crucible-mcp-server --config /path/to/your/config.yaml
```

#### 2. Invalid Configuration Format

**Error:** `YAML parsing error: ...`

**Solution:**
```bash
# Validate the configuration syntax
crucible config validate --config ./crucible-config.yaml

# Use a YAML linter
yamllint crucible-config.yaml
```

#### 3. Missing Required Fields

**Error:** `Missing configuration value: embedding_provider`

**Solution:**
```bash
# Check what fields are required
crucible config schema --show-required

# Use the example configuration as a template
cp examples/development-config.yaml ./crucible-config.yaml
```

#### 4. Environment Variable Not Working

**Error:** `API key not found`

**Solution:**
```bash
# Check if environment variable is set
echo $OPENAI_API_KEY

# Set environment variable before starting
export OPENAI_API_KEY=your-api-key

# Or reference it directly in configuration
# key: "\${OPENAI_API_KEY}"
```

#### 5. Profile Not Found

**Error:** `Profile 'staging' not found`

**Solution:**
```bash
# List available profiles
crucible config list-profiles --config ./crucible-config.yaml

# Set the correct profile
export CRUCIBLE_PROFILE=development

# Or update the configuration file
# profile: "development"
```

### Migration Checklist

- [ ] Create configuration file based on current environment variables
- [ ] Test configuration syntax with `crucible config validate`
- [ ] Verify all required fields are present
- [ ] Test application startup with new configuration
- [ ] Compare behavior with old environment variables
- [ ] Update deployment scripts and documentation
- [ ] Remove old environment variables from startup scripts
- [ ] Add configuration validation to CI/CD pipeline

### Support Resources

- **Documentation:** [Configuration System Documentation](./README.md)
- **Examples:** See the `examples/` directory for more configuration examples
- **Schema:** Use `crucible config schema` to see the full configuration schema
- **Validation:** Use `crucible config validate` to test your configuration

## Advanced Migration Topics

### Multi-Environment Configuration

You can use profiles to manage multiple environments:

```yaml
# crucible-config.yaml
profile: "${CRUCIBLE_ENV:-development}"

profiles:
  development:
    # Development settings
  staging:
    # Staging settings
  production:
    # Production settings
```

### Configuration Overrides

You can override specific settings using environment variables:

```bash
# Override specific settings
export CRUCIBLE_EMBEDDING_PROVIDER=openai
export CRUCIBLE_LOGGING_LEVEL=debug

# The configuration system will use these overrides
crucible-mcp-server --config ./crucible-config.yaml
```

### Configuration Inheritance

Profiles can inherit from other profiles:

```yaml
profiles:
  base:
    # Base configuration

  development:
    inherits: "base"
    # Development-specific overrides

  production:
    inherits: "base"
    # Production-specific overrides
```

This migration guide should help you successfully transition from environment variables to the new crucible-config system while maintaining all your existing functionality.