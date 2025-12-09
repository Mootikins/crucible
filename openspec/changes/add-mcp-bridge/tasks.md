## 1. Core Infrastructure

- [ ] 1.1 Define `Interceptor` trait with `before_call` and `after_call` methods
- [ ] 1.2 Define `InterceptorContext` struct with tool metadata and custom storage
- [ ] 1.3 Implement `InterceptorPipeline` that chains interceptors
- [ ] 1.4 Add unit tests for pipeline execution order and error handling

## 2. MCP Bridge Client

- [ ] 2.1 Create `UpstreamMcpClient` struct using rmcp client role
- [ ] 2.2 Implement stdio transport (spawn process + connect)
- [ ] 2.3 Implement HTTP+SSE transport
- [ ] 2.4 Add tool discovery via `tools/list` request
- [ ] 2.5 Handle `toolListChanged` notifications
- [ ] 2.6 Add integration test with mock MCP server

## 3. Tool Selector

- [ ] 3.1 Implement `ToolSelector` trait with filter/transform methods
- [ ] 3.2 Add whitelist filtering (`allowed_tools`)
- [ ] 3.3 Add blacklist filtering (`blocked_tools`)
- [ ] 3.4 Add namespace prefixing (`prefix`)
- [ ] 3.5 Add unit tests for selector logic

## 4. Built-in Interceptors

- [ ] 4.1 Move existing `filter_test_output` into `TestFilterInterceptor`
- [ ] 4.2 Create `ToonTransformInterceptor` using tq crate
- [ ] 4.3 Create `LlmEnrichmentInterceptor` with configurable provider/prompt
- [ ] 4.4 Create `EventEmitterInterceptor` that publishes to event bus
- [ ] 4.5 Add unit tests for each built-in interceptor

## 5. Rune Interceptor Support

- [ ] 5.1 Define Rune interceptor script format (exports `before_call`/`after_call`)
- [ ] 5.2 Create `RuneInterceptor` wrapper that compiles and executes scripts
- [ ] 5.3 Add interceptor discovery in `KILN/.crucible/interceptors/`
- [ ] 5.4 Add hot-reload support for interceptor scripts
- [ ] 5.5 Add integration test with sample Rune interceptor

## 6. Integration

- [ ] 6.1 Add `upstream_clients` to `ExtendedMcpServer`
- [ ] 6.2 Route tool calls through interceptor pipeline for all sources
- [ ] 6.3 Aggregate tools from Kiln + Just + Rune + upstream MCPs
- [ ] 6.4 Add configuration file support for bridge setup
- [ ] 6.5 Add CLI command to list upstream servers and their tools

## 7. Configuration

- [ ] 7.1 Define TOML schema for MCP bridge configuration
- [ ] 7.2 Add `[bridge]` section to crucible config
- [ ] 7.3 Support environment variable overrides
- [ ] 7.4 Add configuration validation at startup

## 8. Documentation

- [ ] 8.1 Document interceptor trait and pipeline usage
- [ ] 8.2 Document built-in interceptors and configuration
- [ ] 8.3 Add example Rune interceptor scripts
- [ ] 8.4 Document MCP bridge configuration options
