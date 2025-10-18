# Example Configuration Files

This document lists all the example configuration files created for the crucible-config system.

## Configuration Files

| File | Format | Environment | Provider | Database | Purpose |
|------|--------|-------------|----------|----------|---------|
| `development-config.yaml` | YAML | Development | Ollama | SQLite | Local development with debug features |
| `testing-config.toml` | TOML | Testing | Custom (mock) | In-memory | CI/CD and automated testing |
| `production-config.yaml` | YAML | Production | OpenAI | PostgreSQL | Production deployment with security |
| `minimal-config.json` | JSON | Minimal | Ollama | SQLite | Quick start and basic setup |

## Documentation Files

| File | Purpose |
|------|---------|
| `README.md` | Comprehensive documentation and usage guide |
| `migration-example.md` | Step-by-step migration from environment variables |
| `FILES.md` | This file - overview of all example files |

## Utility Files

| File | Purpose |
|------|---------|
| `.gitignore` | Git ignore rules for configuration files |
| `validate-configs.sh` | Validation script for all configuration files |
| `validate-examples.sh` | Original validation script (more complex) |

## File Sizes and Complexity

- **development-config.yaml**: ~5.5KB - Comprehensive development setup
- **testing-config.toml**: ~8.2KB - Detailed testing configuration
- **production-config.yaml**: ~14KB - Enterprise production setup
- **minimal-config.json**: ~1.5KB - Simple starter configuration
- **README.md**: ~14KB - Extensive documentation
- **migration-example.md**: ~16KB - Detailed migration guide

## Quick Reference

### Use Development Config
```bash
cp examples/development-config.yaml ./crucible-config.yaml
crucible-mcp-server --config ./crucible-config.yaml
```

### Use Testing Config
```bash
cp examples/testing-config.toml ./crucible-config.toml
CRUCIBLE_PROFILE=testing crucible-mcp-server --config ./crucible-config.toml
```

### Use Production Config
```bash
cp examples/production-config.yaml ./crucible-config.yaml
export OPENAI_API_KEY=your-key
export DATABASE_URL=your-db-url
crucible-mcp-server --config ./crucible-config.yaml
```

### Use Minimal Config
```bash
cp examples/minimal-config.json ./crucible-config.json
crucible-mcp-server --config ./crucible-config.json
```

### Validate All Configs
```bash
cd examples
./validate-configs.sh
```

## Configuration Features Demonstrated

### Multi-Format Support
- YAML: Human-readable, great for complex configurations
- TOML: Machine-parsable, excellent for structured data
- JSON: Universal format, good for programmatic generation

### Environment Profiles
- **Development**: Local services, debug logging, relaxed security
- **Testing**: Mock services, minimal resources, deterministic responses
- **Production**: Enterprise services, security, monitoring, compliance
- **Minimal**: Basic setup for quick starts

### Provider Configurations
- **Ollama**: Local, free, no API key required
- **OpenAI**: Production-ready, requires API key
- **Custom/Mock**: Testing, deterministic responses

### Database Configurations
- **SQLite**: Local file-based, good for development
- **PostgreSQL**: Production-ready, connection pooling
- **In-memory**: Fast testing, no persistence

### Security Features
- Environment variable references for sensitive data
- SSL/TLS configuration
- Authentication and authorization
- Rate limiting and security headers
- Audit logging and compliance

### Advanced Features
- Profile inheritance and overrides
- Custom settings and options
- Monitoring and observability
- Backup and recovery procedures
- Multi-region deployment support

## Best Practices Demonstrated

1. **Security**: Never hardcode API keys or sensitive data
2. **Environment separation**: Different configs for different environments
3. **Validation**: All configs are syntactically valid and tested
4. **Documentation**: Comprehensive comments and explanations
5. **Flexibility**: Support for multiple providers and databases
6. **Scalability**: Production configs handle enterprise requirements
7. **Testing**: Mock services for reliable automated testing
8. **Migration**: Clear path from environment variables

These examples provide a complete foundation for using the crucible-config system in any environment, from local development to production deployment.