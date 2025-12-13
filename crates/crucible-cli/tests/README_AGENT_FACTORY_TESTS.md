# Agent Factory Tests

This directory contains comprehensive tests for the agent factory system that creates internal agents with different LLM provider configurations.

## Test Files

### `agent_factory_integration.rs`

Tests the AgentInitParams builder pattern and basic factory behavior:

- **Builder Pattern Tests** (7 tests)
  - Parameter construction and defaults
  - Builder chaining and method overrides
  - Optional helper methods
  - Edge cases (empty strings, boundary values)

- **Type Tests** (4 tests)
  - AgentType enum behavior
  - Copy trait semantics
  - Debug formatting
  - Type equality

- **Basic Factory Tests** (5 tests)
  - Invalid provider handling
  - Read-only mode toggling
  - Max context token configuration

**Total: 16 tests**

### `agent_factory_config_tests.rs`

Tests comprehensive configuration scenarios for agent creation:

- **Configuration Defaults Tests** (2 tests)
  - Default config values
  - Custom config overrides

- **Named Provider Configuration Tests** (4 tests)
  - Single provider setup
  - Multiple provider management
  - Provider lookup and defaults
  - Error handling for missing providers

- **Provider Type Tests** (4 tests)
  - Ollama provider defaults
  - OpenAI provider defaults
  - Anthropic provider defaults
  - Custom value overrides

- **Agent Creation Tests** (5 tests)
  - Default config agent creation
  - Nonexistent provider error handling
  - Custom model name propagation
  - Named provider selection
  - Configuration validation

- **Model Name Propagation Tests** (3 tests)
  - Model from chat config
  - Model fallback to defaults
  - Model from named provider

- **Context Token Configuration Tests** (2 tests)
  - Explicit max context tokens
  - Default context token handling

- **Edge Cases and Error Handling** (5 tests)
  - Empty LLM config
  - Provider variant testing
  - Temperature boundary values
  - Max tokens boundary values
  - Timeout boundary values

- **API Key Configuration Tests** (3 tests)
  - API key from environment variable
  - Missing environment variable handling
  - No API key configured

- **Integration-style Configuration Tests** (3 tests)
  - Realistic Ollama setup
  - Realistic OpenAI setup
  - Multi-provider configuration

**Total: 31 tests**

## Test Coverage

### What's Tested

1. **Configuration Parsing and Validation**
   - All provider types (Ollama, OpenAI, Anthropic)
   - Named provider system
   - Chat config fallback
   - Environment variable API keys
   - Default value handling

2. **Agent Creation**
   - Internal agent factory
   - Provider selection by name
   - Model name propagation
   - Context token limits
   - Error handling for invalid configs

3. **Builder Pattern**
   - AgentInitParams construction
   - Method chaining
   - Optional parameters
   - Value overrides

4. **Configuration Components**
   - LlmConfig with named providers
   - ChatConfig for simple use cases
   - LlmProviderConfig for individual providers
   - CliAppConfig integration

5. **Error Handling**
   - Nonexistent provider names
   - Missing API keys
   - Invalid default providers
   - Connection vs configuration errors

### What's NOT Tested

The following are intentionally not tested as they require external services:

1. **Actual LLM Communication**
   - Tests mock or stub external API calls
   - Real Ollama/OpenAI/Anthropic connections require running services
   - Tests verify config validity, not runtime behavior

2. **Tool Executor Wiring**
   - Tool executor is provided as Box<dyn ToolExecutor>
   - Testing actual tool execution requires integration tests
   - Config tests focus on configuration acceptance

3. **ACP Agent Creation**
   - ACP agents require external agent discovery
   - Tested separately in ACP-specific test suites

## Running Tests

```bash
# Run all agent factory tests
cargo test -p crucible-cli --test agent_factory_config_tests --test agent_factory_integration

# Run only config tests
cargo test -p crucible-cli --test agent_factory_config_tests

# Run only integration tests
cargo test -p crucible-cli --test agent_factory_integration

# Run specific test
cargo test -p crucible-cli --test agent_factory_config_tests test_create_internal_agent_with_named_provider
```

## Test Strategy

### Configuration-First Testing

Tests focus on validating that configurations are:
1. Parsed correctly from structs
2. Provide sensible defaults
3. Allow custom overrides
4. Fail with clear error messages

### Graceful Degradation

Tests accept two outcomes for agent creation:
1. **Success**: Configuration is valid and service is available
2. **Connection Error**: Configuration is valid but service is unavailable

Tests explicitly fail if:
- Configuration is rejected as invalid
- Error messages are unclear or misleading

### Boundary Value Testing

Tests verify extreme but valid values:
- Zero and very large token limits
- Minimum and maximum temperature values
- Very short and very long timeout values

### Realistic Scenarios

Integration-style tests demonstrate:
- Real-world Ollama local setup
- Production OpenAI configuration
- Multi-environment provider management

## Related Code

- **Factory Implementation**: `/home/moot/crucible/crates/crucible-cli/src/factories/agent.rs`
- **Config Components**: `/home/moot/crucible/crates/crucible-config/src/components/`
- **LLM Providers**: `/home/moot/crucible/crates/crucible-llm/src/`
- **Agent Handle**: `/home/moot/crucible/crates/crucible-agents/src/handle.rs`

## Adding New Tests

When adding new provider types or configuration options:

1. Add provider default tests (endpoint, model, etc.)
2. Add realistic configuration examples
3. Add error handling tests for invalid configs
4. Update this README with new test categories
5. Ensure tests accept both success and connection errors

## Test Philosophy

These tests follow the principle:

> **Test the configuration, not the external service.**

Tests validate that:
- User configurations are accepted and parsed correctly
- Defaults are sensible and well-documented
- Error messages guide users to correct configuration
- The factory creates agents with the right settings

Tests do NOT validate:
- Whether Ollama responds correctly to requests
- Whether OpenAI API keys are valid
- Whether network connections succeed

This allows tests to run in CI/CD without external dependencies while still providing confidence in the configuration system.
