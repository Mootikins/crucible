## ADDED Requirements

### Requirement: Web Server

The system SHALL provide an HTTP server for browser-based access to chat functionality.

The server SHALL be started via `cru serve` command.

The server SHALL accept `--port` flag to configure listening port (default: 3000).

The server SHALL accept `--web-dir` flag to override static asset directory.

#### Scenario: Start web server with defaults

- **GIVEN** no web server is running
- **WHEN** user executes `cru serve`
- **THEN** server starts on port 3000
- **AND** server logs "Listening on http://localhost:3000"

#### Scenario: Start web server with custom port

- **GIVEN** no web server is running
- **WHEN** user executes `cru serve --port 8080`
- **THEN** server starts on port 8080
- **AND** server logs "Listening on http://localhost:8080"

#### Scenario: Start web server with custom asset directory

- **GIVEN** no web server is running
- **AND** directory `./my-ui/dist` exists with valid web assets
- **WHEN** user executes `cru serve --web-dir ./my-ui/dist`
- **THEN** server serves static files from `./my-ui/dist`

---

### Requirement: Chat SSE Endpoint

The system SHALL provide an SSE (Server-Sent Events) endpoint for streaming chat responses.

The endpoint SHALL be accessible at `POST /api/chat`.

The endpoint SHALL accept JSON request body with `message` field.

The endpoint SHALL return `Content-Type: text/event-stream`.

#### Scenario: Send chat message and receive streaming response

- **GIVEN** web server is running
- **AND** ACP agent (Claude) is configured
- **WHEN** client sends POST to `/api/chat` with body `{"message": "Hello"}`
- **THEN** server returns status 200
- **AND** response Content-Type is `text/event-stream`
- **AND** server streams `event: token` events as tokens arrive
- **AND** server sends `event: message_complete` when response is finished

#### Scenario: SSE token event format

- **GIVEN** agent is generating response
- **WHEN** a token is received from agent
- **THEN** server emits SSE event:
  ```
  event: token
  data: {"content": "<token_text>"}
  ```

#### Scenario: SSE message complete event format

- **GIVEN** agent has finished generating response
- **WHEN** response is complete
- **THEN** server emits SSE event:
  ```
  event: message_complete
  data: {"id": "<message_id>", "content": "<full_response>"}
  ```

#### Scenario: SSE error event format

- **GIVEN** an error occurs during chat
- **WHEN** error is encountered
- **THEN** server emits SSE event:
  ```
  event: error
  data: {"code": "<error_code>", "message": "<error_description>"}
  ```

---

### Requirement: Static Asset Serving

The system SHALL serve a single-page application (SPA) for the chat interface.

In release builds, assets SHALL be embedded in the binary.

In debug builds, assets SHALL be served from the filesystem.

The `--web-dir` flag SHALL override both behaviors.

#### Scenario: Serve embedded assets in release build

- **GIVEN** server is running in release mode
- **AND** no `--web-dir` flag provided
- **WHEN** client requests `GET /`
- **THEN** server returns embedded `index.html`
- **AND** all referenced assets (JS, CSS) are served from embedded files

#### Scenario: Serve filesystem assets in debug build

- **GIVEN** server is running in debug mode
- **AND** no `--web-dir` flag provided
- **AND** `web/dist/index.html` exists
- **WHEN** client requests `GET /`
- **THEN** server returns `web/dist/index.html` from filesystem

#### Scenario: SPA routing fallback

- **GIVEN** server is running
- **WHEN** client requests `GET /chat` (or any non-API path)
- **THEN** server returns `index.html` (SPA handles routing)

---

### Requirement: Actor-Based Architecture

The system SHALL use actix actors for internal communication.

WebHostActor SHALL manage HTTP connections and SSE streams.

ChatActor SHALL manage conversation state and ACP communication.

Actors SHALL communicate via typed messages.

#### Scenario: Message flow from browser to agent

- **GIVEN** browser is connected via SSE
- **WHEN** browser sends chat message via POST
- **THEN** WebHostActor receives HTTP request
- **AND** WebHostActor sends typed message to ChatActor
- **AND** ChatActor sends message to ACP agent
- **AND** ChatActor emits token events back to WebHostActor
- **AND** WebHostActor streams tokens to browser via SSE

#### Scenario: Multiple browser connections

- **GIVEN** ChatActor is running
- **WHEN** two browsers connect and send messages
- **THEN** each browser receives only its own conversation's events
- **AND** conversations are isolated
