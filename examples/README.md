# Crucible Configuration Examples

This directory contains comprehensive example configuration files for the new crucible-config system. These examples demonstrate the full capabilities of the configuration system across different environments and use cases.

## Overview

The crucible-config system supports:
- **Multiple formats**: YAML, TOML, and JSON
- **Environment profiles**: Development, testing, staging, and production
- **Provider configurations**: OpenAI, Ollama, Cohere, and custom providers
- **Security features**: Environment variable references, encryption support
- **Multi-format support**: Choose the format that works best for your workflow

## Configuration Files

### 1. Development Configuration (`development-config.yaml`)

**Purpose**: Local development with Ollama and debug features
**Format**: YAML
**Provider**: Ollama (local)
**Database**: SQLite (local file)

**Features**:
- Local Ollama integration
- Debug logging and verbose output
- Hot reload capabilities
- Development-friendly security settings
- CORS enabled for frontend development

**Usage**:
```bash
# Use the development configuration
CRUCIBLE_CONFIG_PATH=./examples/development-config.yaml crucible-mcp-server

# Or switch to testing profile within the config
CRUCIBLE_PROFILE=testing crucible-mcp-server --config ./examples/development-config.yaml
```

**Key Configuration Sections**:
- `profiles.development.embedding_provider`: Ollama configuration
- `profiles.development.database`: SQLite with development settings
- `profiles.development.logging`: Debug-level console logging
- `profiles.development.server`: Development server with CORS

### 2. Testing Configuration (`testing-config.toml`)

**Purpose**: CI/CD pipelines and automated testing
**Format**: TOML
**Provider**: Custom (mock)
**Database**: In-memory SQLite

**Features**:
- Mock services for reliable testing
- Minimal resource usage
- Fast execution
- Deterministic responses
- Integration test support

**Usage**:
```bash
# Run tests with mocking
CRUCIBLE_CONFIG_PATH=./examples/testing-config.toml cargo test

# Run integration tests
CRUCIBLE_PROFILE=integration cargo test integration_tests

# CI/CD pipeline usage
export CRUCIBLE_CI_MODE=true
export CRUCIBLE_CONFIG_PATH=./examples/testing-config.toml
```

**Key Configuration Sections**:
- `profiles.testing.embedding_provider`: Mock embedding service
- `profiles.testing.database`: In-memory database
- `profiles.testing.logging`: Error-only logging
- `ci_cd`: CI/CD specific settings

### 3. Production Configuration (`production-config.yaml`)

**Purpose**: Production deployment with enterprise features
**Format**: YAML
**Provider**: OpenAI
**Database**: PostgreSQL

**Features**:
- Enterprise-grade security
- Performance optimization
- Comprehensive monitoring
- Backup and recovery
- Compliance features (GDPR, audit logging)
- SSL/TLS configuration

**Usage**:
```bash
# Production deployment with environment variables
export OPENAI_API_KEY=your-openai-api-key
export DATABASE_URL=postgres://user:pass@host:port/db
export SSL_CERT_PATH=/etc/ssl/certs/crucible.crt
export SSL_KEY_PATH=/etc/ssl/private/crucible.key
export JWT_SECRET=your-jwt-secret

export CRUCIBLE_CONFIG_PATH=./examples/production-config.yaml
crucible-mcp-server
```

**Key Configuration Sections**:
- `profiles.production.embedding_provider`: OpenAI with cost optimization
- `profiles.production.database`: PostgreSQL with connection pooling
- `profiles.production.server`: HTTPS with security headers
- `profiles.production.logging`: Structured JSON logging
- `custom_settings.security`: Comprehensive security configuration
- `custom_settings.monitoring`: Metrics and alerting

### 4. Migration Guide (`migration-example.md`)

**Purpose**: Step-by-step migration from environment variables
**Format**: Markdown documentation

**Features**:
- Detailed migration steps
- Environment variable mapping
- Migration scripts
- Troubleshooting guide
- Validation procedures

## Quick Start Guide

### 1. Choose Your Environment

**For Local Development**:
```bash
cp examples/development-config.yaml ./crucible-config.yaml
```

**For Testing/CI**:
```bash
cp examples/testing-config.toml ./crucible-config.toml
```

**For Production**:
```bash
cp examples/production-config.yaml ./crucible-config.yaml
# Then customize with your production values
```

### 2. Configure Environment Variables

**Development** (minimal setup):
```bash
export OBSIDIAN_VAULT_PATH=/path/to/your/vault
```

**Testing** (CI/CD):
```bash
export CRUCIBLE_CI_MODE=true
export CRUCIBLE_TEST_MODE=true
```

**Production** (required):
```bash
export OPENAI_API_KEY=your-production-api-key
export DATABASE_URL=your-production-database-url
export SSL_CERT_PATH=/path/to/ssl/cert
export SSL_KEY_PATH=/path/to/ssl/key
export JWT_SECRET=your-jwt-secret
```

### 3. Start the Application

```bash
# Using configuration file
CRUCIBLE_CONFIG_PATH=./crucible-config.yaml crucible-mcp-server

# Or specify profile
CRUCIBLE_PROFILE=production crucible-mcp-server --config ./crucible-config.yaml
```

## Configuration System Features

### Profile System

Profiles allow you to maintain different configurations for different environments:

```yaml
profiles:
  development:
    # Development-specific settings
  testing:
    # Testing-specific settings
  production:
    # Production-specific settings
```

### Environment Variable References

Keep sensitive information out of configuration files:

```yaml
embedding_provider:
  api:
    key: null  # Read from OPENAI_API_KEY environment variable

# Or direct reference
key: "${OPENAI_API_KEY}"
```

### Configuration Inheritance

Profiles can inherit from base configurations:

```yaml
profiles:
  base:
    # Common settings
    logging:
      format: "json"

  development:
    inherits: "base"
    logging:
      level: "debug"  # Override specific setting
```

### Multi-Format Support

Choose the format that works best for your workflow:

**YAML** (recommended for readability):
```yaml
embedding_provider:
  type: "openai"
  api:
    timeout_seconds: 30
```

**TOML** (recommended for machine parsing):
```toml
[embedding_provider]
type = "openai"

[embedding_provider.api]
timeout_seconds = 30
```

**JSON** (for compatibility):
```json
{
  "embedding_provider": {
    "type": "openai",
    "api": {
      "timeout_seconds": 30
    }
  }
}
```

## Advanced Usage Examples

### Custom Provider Configuration

```yaml
profiles:
  custom:
    embedding_provider:
      type: "custom"
      api:
        base_url: "https://my-custom-provider.com/v1"
        key: "${CUSTOM_API_KEY}"
        headers:
          Authorization: "Bearer ${CUSTOM_API_KEY}"
          User-Agent: "Crucible/1.0.0"
      model:
        name: "my-custom-model"
        dimensions: 1024
      options:
        custom_param: "value"
        another_param: 123
```

### Database Configuration Variations

**SQLite with custom options**:
```yaml
database:
  type: "sqlite"
  url: "sqlite:./custom.db"
  options:
    journal_mode: "WAL"
    synchronous: "NORMAL"
    cache_size: 2000
    temp_store: "MEMORY"
```

**PostgreSQL with connection pooling**:
```yaml
database:
  type: "postgres"
  url: "postgres://user:pass@host:port/db"
  max_connections: 20
  timeout_seconds: 30
  options:
    ssl_mode: "require"
    application_name: "crucible_prod"
    statement_timeout: 30000
```

### Security Configuration

**Production security setup**:
```yaml
profiles:
  production:
    settings:
      security:
        require_auth: true
        jwt_secret: null  # Read from JWT_SECRET env var
        session_timeout: 3600
        max_login_attempts: 5
        password_policy:
          min_length: 12
          require_uppercase: true
          require_symbols: true
        rate_limiting:
          enabled: true
          requests_per_minute: 100
```

### Monitoring and Observability

**Comprehensive monitoring setup**:
```yaml
profiles:
  production:
    settings:
      monitoring:
        metrics_enabled: true
        prometheus_endpoint: "/metrics"
        health_check_endpoint: "/health"
        tracing_enabled: true
        tracing_sampling_rate: 0.1
        alerting:
          enabled: true
          thresholds:
            error_rate_percent: 5
            response_time_ms: 5000
            memory_usage_percent: 85
```

## Validation and Testing

### Validate Configuration Syntax

```bash
# Validate YAML configuration
crucible config validate --config ./crucible-config.yaml

# Validate TOML configuration
crucible config validate --config ./crucible-config.toml

# Validate specific profile
crucible config validate --config ./crucible-config.yaml --profile production
```

### Test Configuration Loading

```bash
# Test configuration loading
crucible config test --config ./crucible-config.yaml

# Test with environment override
CRUCIBLE_PROFILE=testing crucible config test --config ./crucible-config.yaml
```

### Show Configuration Schema

```bash
# Show full configuration schema
crucible config schema

# Show schema for specific section
crucible config schema --section embedding_provider

# Show required fields only
crucible config schema --required-only
```

## Environment-Specific Best Practices

### Development Environment

- Use local services (Ollama, SQLite)
- Enable debug logging and features
- Use relaxed security settings
- Enable hot reload and auto-restart
- Use console logging for immediate feedback

### Testing Environment

- Use mock services for deterministic testing
- Use in-memory databases for speed
- Minimize logging output
- Enable test-specific features
- Configure timeouts for CI/CD constraints

### Production Environment

- Use managed services (OpenAI, PostgreSQL)
- Enable comprehensive security
- Use structured logging for log aggregation
- Configure monitoring and alerting
- Implement backup and recovery procedures
- Follow compliance requirements

## Common Configuration Patterns

### Multi-Team Development

```yaml
profiles:
  team-a-dev:
    inherits: "development"
    env_vars:
      TEAM: "team-a"
      FEATURE_FLAGS: "new-ui,experimental-api"

  team-b-dev:
    inherits: "development"
    env_vars:
      TEAM: "team-b"
      FEATURE_FLAGS: "legacy-mode"
```

### Feature Flag Configuration

```yaml
profiles:
  development:
    settings:
      features:
        experimental_features: true
        new_ui_enabled: true
        legacy_api_support: false

  production:
    settings:
      features:
        experimental_features: false
        new_ui_enabled: false
        legacy_api_support: true
```

### Multi-Region Deployment

```yaml
profiles:
  production-us:
    inherits: "production"
    database:
      url: "${DATABASE_URL_US}"
    server:
      region: "us-east-1"

  production-eu:
    inherits: "production"
    database:
      url: "${DATABASE_URL_EU}"
    server:
      region: "eu-west-1"
```

## Troubleshooting

### Common Issues

1. **Configuration not found**
   ```bash
   export CRUCIBLE_CONFIG_PATH=/absolute/path/to/config.yaml
   ```

2. **Invalid syntax**
   ```bash
   crucible config validate --config ./config.yaml
   ```

3. **Missing environment variables**
   ```bash
   env | grep CRUCIBLE
   env | grep OPENAI_API_KEY
   ```

4. **Profile not found**
   ```bash
   crucible config list-profiles --config ./config.yaml
   ```

### Debug Configuration Loading

```bash
# Enable configuration debugging
RUST_LOG=crucible_config=debug crucible-mcp-server

# Show effective configuration
crucible config show --config ./config.yaml

# Show profile inheritance
crucible config show --config ./config.yaml --with-inheritance
```

## Contributing

When contributing configuration examples:

1. **Use clear, descriptive names** for configurations
2. **Add comprehensive comments** explaining each section
3. **Include environment variable references** for sensitive data
4. **Provide usage examples** in comments
5. **Follow the established patterns** for consistency
6. **Test configurations** before submitting

### Configuration Example Template

```yaml
# Configuration Name
# Purpose: Brief description of what this configuration is for
# Environment: development/testing/production
# Provider: ollama/openai/custom
# Database: sqlite/postgres/memory

# Active profile
profile: "profile-name"

# Profile configurations
profiles:
  profile-name:
    name: "profile-name"
    description: "Detailed description of this profile"
    environment: "development"

    # Configuration sections with comments
    embedding_provider:
      # Explain the provider choice and settings
      type: "provider-type"

    # ... other configuration sections

    # Environment variables specific to this profile
    env_vars:
      VAR_NAME: "value"

    # Custom settings
    settings:
      custom_section:
        setting: value
```

## Additional Resources

- **Migration Guide**: See `migration-example.md` for detailed migration instructions
- **Configuration Schema**: Use `crucible config schema` to see all available options
- **Validation Tools**: Use `crucible config validate` to test your configurations
- **API Documentation**: See the main project documentation for API reference

## Support

For questions or issues with these configuration examples:

1. Check the troubleshooting section above
2. Validate your configuration with the built-in tools
3. Review the migration guide for common migration issues
4. Consult the main project documentation
5. Open an issue in the project repository

These examples should provide a solid foundation for configuring Crucible in any environment, from local development to production deployment.