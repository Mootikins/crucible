# Tasks: Event & Hook Systems

## Phase 1: Core Event System (Week 1-2)

### Event Types & Schema
- [ ] Define `CrucibleEvent` enum in `crates/crucible-events/src/types.rs`
- [ ] Define `MemoryEvent`, `GraphEvent`, `FileEvent` enums
- [ ] Define `AgentEvent`, `FlowEvent`, `SystemEvent`, `HookEvent` enums
- [ ] Define `EventMetadata` struct with UUIDv7 IDs
- [ ] Add `EventSource` enum (User, Agent, System, External)
- [ ] Implement JSON serialization for all event types
- [ ] Write JSON Schema definitions for all events
- [ ] Add semver versioning to event schema
- [ ] Write tests for event serialization/deserialization

### Event Bus
- [ ] Implement `EventBus` in `crates/crucible-events/src/bus.rs`
- [ ] Add pub/sub pattern for event subscriptions
- [ ] Implement `EventFilter` for filtering events
- [ ] Add `subscribe(filter) -> Receiver<CrucibleEvent>` method
- [ ] Add `publish(event)` method
- [ ] Add broadcast channels for system-wide events
- [ ] Write tests for pub/sub functionality
- [ ] Write tests for event filtering
- [ ] Benchmark event delivery performance

### Event Store
- [ ] Implement `EventStore` in `crates/crucible-events/src/store.rs`
- [ ] Add append-only event log (backed by SurrealDB)
- [ ] Implement `EventIndexes` for fast queries
- [ ] Add index by event type
- [ ] Add index by actor
- [ ] Add index by correlation ID
- [ ] Add index by timestamp (BTreeMap)
- [ ] Implement `query(EventQuery) -> Vec<StoredEvent>` method
- [ ] Implement `replay(from: EventId) -> Stream<Event>` method
- [ ] Implement `trace(CorrelationId) -> Vec<Event>` method
- [ ] Write tests for event persistence
- [ ] Write tests for querying
- [ ] Write tests for replay
- [ ] Add event retention policy (configurable)

### Event Emission
- [ ] Add event emission to all memory operations (create, update, delete, link)
- [ ] Add event emission to graph operations
- [ ] Add event emission to file operations
- [ ] Add event emission to agent operations
- [ ] Add correlation ID propagation through call chains
- [ ] Write integration tests for event emission

## Phase 2: Hook System (Week 3-4)

### Hook Configuration
- [ ] Define `Hook` struct in `crates/crucible-hooks/src/config.rs`
- [ ] Define `HookTrigger`, `HookCondition`, `HookAction` types
- [ ] Define `HookOptions` (async, timeout, retry, debounce)
- [ ] Implement markdown + YAML parser for hook files
- [ ] Add hook validation (check Rune scripts exist, URLs valid, etc.)
- [ ] Write tests for hook parsing
- [ ] Write tests for hook validation

### Hook Registry
- [ ] Implement `HookRegistry` in `crates/crucible-hooks/src/registry.rs`
- [ ] Add `register(hook)` method
- [ ] Add `unregister(hook_id)` method
- [ ] Add `find_matching(event) -> Vec<Hook>` method
- [ ] Add `enable(hook_id)` and `disable(hook_id)` methods
- [ ] Implement hook discovery from directories
- [ ] Add hot-reload for hook file changes
- [ ] Write tests for registry operations

### Hook Executor
- [ ] Implement `HookExecutor` in `crates/crucible-hooks/src/executor.rs`
- [ ] Add pre-hook execution (blocking)
- [ ] Add post-hook execution (async)
- [ ] Implement Rune script execution
- [ ] Implement shell command execution
- [ ] Implement HTTP request execution
- [ ] Implement flow trigger execution
- [ ] Add timeout handling
- [ ] Add retry logic with exponential backoff
- [ ] Add debouncing logic
- [ ] Add rate limiting
- [ ] Emit `HookEvent` for all hook executions
- [ ] Write tests for each action type
- [ ] Write tests for retry/timeout/debounce

### Hook CLI
- [ ] Create `crates/crucible-cli/src/commands/hooks.rs`
- [ ] Implement `hook list` command
- [ ] Implement `hook info <id>` command
- [ ] Implement `hook enable <id>` command
- [ ] Implement `hook disable <id>` command
- [ ] Implement `hook test <id> --event <json>` command (dry-run)
- [ ] Implement `hook validate <path>` command
- [ ] Write CLI tests

## Phase 3: Webhook System (Week 5)

### Webhook Registry
- [ ] Implement `WebhookRegistry` in `crates/crucible-webhooks/src/registry.rs`
- [ ] Define `Webhook` struct with URL, filters, auth
- [ ] Add `register(webhook)` method
- [ ] Add `unregister(webhook_id)` method
- [ ] Add `find_matching(event) -> Vec<Webhook>` method
- [ ] Write tests for registry

### Webhook Delivery
- [ ] Implement `WebhookDelivery` in `crates/crucible-webhooks/src/delivery.rs`
- [ ] Add HTTP POST request with event payload
- [ ] Add custom headers (X-Crucible-Event-Id, X-Crucible-Event-Type, etc.)
- [ ] Implement retry logic (5xx errors only)
- [ ] Implement exponential backoff
- [ ] Add delivery tracking (success/failure)
- [ ] Add delivery history persistence
- [ ] Write tests for delivery
- [ ] Write tests for retry logic

### Webhook Authentication
- [ ] Implement `WebhookAuth` in `crates/crucible-webhooks/src/auth.rs`
- [ ] Add Bearer token auth
- [ ] Add HMAC signature generation (SHA256)
- [ ] Add HMAC signature verification
- [ ] Add Basic auth
- [ ] Add custom header auth
- [ ] Write tests for each auth method

### Webhook CLI
- [ ] Create `crates/crucible-cli/src/commands/webhooks.rs`
- [ ] Implement `webhook create <url> --events <filters>` command
- [ ] Implement `webhook list` command
- [ ] Implement `webhook delete <id>` command
- [ ] Implement `webhook test <id> --event <json>` command
- [ ] Implement `webhook deliveries <id>` command (show history)
- [ ] Write CLI tests

## Phase 4: HTTP REST API (Week 6-8)

### Server Setup
- [ ] Create `crates/crucible-api/src/server.rs`
- [ ] Set up Axum server
- [ ] Add CORS middleware
- [ ] Add request logging middleware
- [ ] Add error handling middleware
- [ ] Configure server from config file
- [ ] Write server startup tests

### API Routes - Memory
- [ ] Implement `POST /api/v1/memory` (create) in `crates/crucible-api/src/routes/memory.rs`
- [ ] Implement `GET /api/v1/memory/:id` (get)
- [ ] Implement `PUT /api/v1/memory/:id` (update)
- [ ] Implement `DELETE /api/v1/memory/:id` (delete)
- [ ] Implement `POST /api/v1/memory/query` (query)
- [ ] Implement `POST /api/v1/memory/:id/link` (link)
- [ ] Add request/response validation
- [ ] Write API tests

### API Routes - Graph
- [ ] Implement `GET /api/v1/graph/nodes/:id` in `crates/crucible-api/src/routes/graph.rs`
- [ ] Implement `GET /api/v1/graph/edges`
- [ ] Implement `POST /api/v1/graph/traverse`
- [ ] Implement `POST /api/v1/graph/paths`
- [ ] Implement `POST /api/v1/graph/communities`
- [ ] Write API tests

### API Routes - Agents
- [ ] Implement `POST /api/v1/agents` (register) in `crates/crucible-api/src/routes/agents.rs`
- [ ] Implement `GET /api/v1/agents/:id`
- [ ] Implement `DELETE /api/v1/agents/:id`
- [ ] Implement `POST /api/v1/agents/:id/actions` (execute)
- [ ] Write API tests

### API Routes - Flows
- [ ] Implement `POST /api/v1/flows/:name/trigger` in `crates/crucible-api/src/routes/flows.rs`
- [ ] Implement `GET /api/v1/flows/:id` (status)
- [ ] Implement `DELETE /api/v1/flows/:id` (cancel)
- [ ] Implement `GET /api/v1/flows/:id/events`
- [ ] Write API tests

### API Routes - Hooks & Webhooks
- [ ] Implement `POST /api/v1/hooks` in `crates/crucible-api/src/routes/hooks.rs`
- [ ] Implement `GET /api/v1/hooks`, `GET /api/v1/hooks/:id`
- [ ] Implement `PUT /api/v1/hooks/:id`, `DELETE /api/v1/hooks/:id`
- [ ] Implement `POST /api/v1/webhooks` in `crates/crucible-api/src/routes/webhooks.rs`
- [ ] Implement webhook CRUD operations
- [ ] Implement `GET /api/v1/webhooks/:id/deliveries`
- [ ] Write API tests

### API Routes - Events
- [ ] Implement `GET /api/v1/events` (list with pagination) in `crates/crucible-api/src/routes/events.rs`
- [ ] Implement `GET /api/v1/events/:id`
- [ ] Implement `POST /api/v1/events/query`
- [ ] Write API tests

### Authentication & Authorization
- [ ] Implement `AuthMiddleware` in `crates/crucible-api/src/auth.rs`
- [ ] Add API key authentication
- [ ] Add JWT authentication
- [ ] Add OAuth2 authentication (future)
- [ ] Implement `Permission` and `Resource` types
- [ ] Add permission checking to all routes
- [ ] Write auth tests

### Rate Limiting
- [ ] Implement `RateLimitMiddleware` in `crates/crucible-api/src/ratelimit.rs`
- [ ] Add per-IP rate limiting
- [ ] Add per-API-key rate limiting
- [ ] Add configurable limits
- [ ] Write rate limiting tests

### OpenAPI Spec
- [ ] Generate OpenAPI 3.0 spec in `crates/crucible-api/src/openapi.rs`
- [ ] Add spec annotations to all routes
- [ ] Serve spec at `/api/v1/openapi.json`
- [ ] Generate Swagger UI at `/api/v1/docs`
- [ ] Validate requests against spec
- [ ] Write spec generation tests

## Phase 5: WebSocket API (Week 9)

### WebSocket Server
- [ ] Implement `WebSocketServer` in `crates/crucible-api/src/websocket.rs`
- [ ] Add WebSocket endpoint at `/api/v1/events/stream`
- [ ] Implement connection handling
- [ ] Add ping/pong keep-alive
- [ ] Write WebSocket tests

### Subscription Management
- [ ] Implement `SubscriptionManager` in `crates/crucible-api/src/subscription.rs`
- [ ] Add `subscribe` action with event filters
- [ ] Add `unsubscribe` action
- [ ] Implement event filtering
- [ ] Implement per-connection event queues
- [ ] Write subscription tests

### WebSocket Protocol
- [ ] Define WebSocket message protocol (JSON)
- [ ] Implement event message format
- [ ] Implement ping/pong messages
- [ ] Implement error messages
- [ ] Write protocol tests

## Phase 6: Integration & Examples (Week 10-12)

### Example Hooks
- [ ] Create `examples/hooks/auto-link-related.md`
- [ ] Create `examples/hooks/auto-commit-edits.md`
- [ ] Create `examples/hooks/notify-important.md`
- [ ] Create `examples/hooks/trigger-research-flow.md`
- [ ] Create `examples/hooks/auto-tag-papers.md`
- [ ] Test all example hooks

### Example Bots
- [ ] Create Discord bot in `examples/bots/discord/`
- [ ] Implement !remember, !recall, !research commands
- [ ] Subscribe to Crucible events via WebSocket
- [ ] Create Telegram bot in `examples/bots/telegram/`
- [ ] Implement /ask, /remember commands
- [ ] Write bot setup documentation

### Python SDK
- [ ] Create Python SDK in `sdk/python/`
- [ ] Implement `CrucibleClient` class
- [ ] Add async/await support
- [ ] Add memory CRUD operations
- [ ] Add query operations
- [ ] Add event streaming
- [ ] Add webhook management
- [ ] Publish to PyPI
- [ ] Write SDK documentation

### Documentation
- [ ] Write event system user guide
- [ ] Write hook system user guide
- [ ] Write webhook setup guide
- [ ] Write API integration guide
- [ ] Write WebSocket guide
- [ ] Create architecture diagrams
- [ ] Create sequence diagrams
- [ ] Add examples to documentation

## Testing Checklist

- [ ] Unit tests for all event types
- [ ] Unit tests for event bus
- [ ] Unit tests for event store
- [ ] Unit tests for hook system
- [ ] Unit tests for webhook system
- [ ] Integration tests for HTTP API
- [ ] Integration tests for WebSocket
- [ ] End-to-end tests with example bots
- [ ] Performance benchmarks (event throughput)
- [ ] Load tests (concurrent webhooks)
- [ ] Security tests (auth, rate limiting)

## Documentation Checklist

- [ ] API documentation (rustdoc)
- [ ] OpenAPI specification
- [ ] User guides
- [ ] Developer guides
- [ ] Architecture decision records
- [ ] Examples
- [ ] Python SDK docs

## Success Metrics

- [ ] Event throughput >10,000 events/sec
- [ ] Hook execution latency <10ms
- [ ] Webhook delivery success rate >99%
- [ ] API response time p99 <100ms
- [ ] WebSocket message latency <10ms
- [ ] All tests pass
- [ ] Code coverage >80%
- [ ] OpenAPI spec validates
- [ ] Example bots work end-to-end
