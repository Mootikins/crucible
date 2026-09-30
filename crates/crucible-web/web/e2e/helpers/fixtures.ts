// RawSession shape (what Axum returns, before mapSession() transforms it)
export const MOCK_SESSION = {
  session_id: 'test-session-001',
  type: 'chat' as const,
  // A registry NAME. A session's `kilns` stopped being paths; a path here
  // resolves to no kiln, so every surface that joins a session to its
  // directory (wikilink resolution, skills, the composer) silently got none.
  kilns: ['my-kiln'],
  workspace: '/home/user/project',
  state: 'active' as const,
  title: 'Test Session',
  agent_model: 'llama3.2',
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
};

// The single-session GET (session.get) shape, distinct from the list shape
// above: model/mode live in a nested `agent` object, there is NO top-level
// `agent_model`, and there's `continued_from` (see
// handle_session_get). Returning this from the GET mock keeps getSession()
// mapping bugs (e.g. reading model from the wrong level) from hiding.
export const MOCK_SESSION_DETAIL = {
  session_id: 'test-session-001',
  type: 'chat' as const,
  // A registry NAME. A session's `kilns` stopped being paths; a path here
  // resolves to no kiln, so every surface that joins a session to its
  // directory (wikilink resolution, skills, the composer) silently got none.
  kilns: ['my-kiln'],
  workspace: '/home/user/project',
  state: 'active' as const,
  title: 'Test Session',
  continued_from: null,
  agent: { model: 'llama3.2', mode: 'chat' },
  started_at: '2026-01-01T00:00:00Z',
};
export const MOCK_SESSION_2 = {
  session_id: 'test-session-002',
  type: 'chat' as const,
  // A registry NAME. A session's `kilns` stopped being paths; a path here
  // resolves to no kiln, so every surface that joins a session to its
  // directory (wikilink resolution, skills, the composer) silently got none.
  kilns: ['my-kiln'],
  workspace: '/home/user/project',
  state: 'active' as const,
  title: 'Second Session',
  agent_model: 'llama3.2',
  started_at: '2026-01-01T01:00:00Z',
  event_count: 5,
};

export const MOCK_PROVIDERS = {
  providers: [
    {
      name: 'ollama',
      provider_type: 'ollama',
      available: true,
      default_model: 'llama3.2',
      models: ['llama3.2', 'mistral'],
      endpoint: 'http://localhost:11434',
    },
  ],
};

// Real wire shape: kiln.list returns entries, not bare path strings.
export const MOCK_KILNS = {
  kilns: [
    {
      path: '/home/user/notes',
      name: 'my-kiln',
      registered: true,
      open: true,
      last_access_secs_ago: 0,
      git: false,
    },
  ],
};

/**
 * The files of the mock branch diff. `diff.get` answers them for any root,
 * and `diff.file` answers their texts.
 */
export const MOCK_DIFF_FILES = [
  {
    path: 'src/lib.rs',
    status: { kind: 'modified' },
    added: 1,
    removed: 1,
    binary: false,
    too_large: false,
  },
  {
    path: 'src/server.rs',
    status: { kind: 'modified' },
    added: 5,
    removed: 2,
    binary: false,
    too_large: false,
  },
  {
    path: 'README.md',
    status: { kind: 'added' },
    added: 3,
    removed: 0,
    binary: false,
    too_large: false,
  },
];

const SERVER_BASE = `use std::net::SocketAddr;
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::net::TcpListener;

/// The settings of one server.
pub struct Config {
    pub addr: SocketAddr,
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            timeout: Duration::from_secs(30),
        }
    }
}

/// Builds the routes of the server.
fn routes() -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/version", get(|| async { env!("CARGO_PKG_VERSION") }))
}

/// Serves the routes until the process stops.
pub async fn serve(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.addr).await?;
    axum::serve(listener, routes()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_port_is_3000() {
        assert_eq!(Config::default().addr.port(), 3000);
    }
}
`;

const SERVER_CURRENT = `use std::net::SocketAddr;
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::net::TcpListener;

/// The settings of one server.
pub struct Config {
    pub addr: SocketAddr,
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            timeout: Duration::from_secs(10),
        }
    }
}

/// Builds the routes of the server.
fn routes() -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/version", get(|| async { env!("CARGO_PKG_VERSION") }))
}

/// Serves the routes until the process stops.
pub async fn serve(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.addr).await?;
    tracing::info!(addr = %config.addr, "the server listens");
    axum::serve(listener, routes())
        .with_graceful_shutdown(shutdown(config.timeout))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_port_is_3000() {
        assert_eq!(Config::default().addr.port(), 3000);
    }
}
`;

/** The two texts of each mock diff file, by path. */
export const MOCK_DIFF_TEXTS: Record<
  string,
  { base_text: string | null; current_text: string | null }
> = {
  'src/lib.rs': { base_text: 'fn main() {}\n', current_text: 'fn main() { run(); }\n' },
  'README.md': { base_text: null, current_text: '# Project\n\nNew text.\n' },
  // About forty lines with two changes apart, so that the unchanged lines
  // between them fold.
  'src/server.rs': { base_text: SERVER_BASE, current_text: SERVER_CURRENT },
};

export const MOCK_CONFIG = {
  kiln_path: '/home/user/notes',
};

// What plugins published about themselves, keyed by contribution kind then
// plugin. The composer reads its isolation offer from here rather than from
// plugin config, so a second isolating plugin needs no frontend change.
/**
 * Target providers on both axes, as plugins publish them.
 *
 * The targets themselves are not here — they are enumerated on demand through
 * `targets_command`, which `MOCK_PLUGIN_COMMAND` answers.
 */
export const MOCK_PUBLICATIONS = {
  publications: {
    targets: {
      oci: { axis: 'runtime', label: 'Container', targets_command: 'oci.targets' },
      worktree: {
        axis: 'workspace',
        label: 'Worktree',
        targets_command: 'worktree.targets',
        resolve_command: 'worktree.resolve',
      },
    },
  },
};

/** What each provider's `targets_command` answers, by command name. */
export const MOCK_PLUGIN_TARGETS: Record<string, { targets: object[] }> = {
  'oci.targets': {
    targets: [
      { value: '', label: 'Default', hint: 'alpine:latest' },
      { value: 'rust', label: 'rust', hint: 'rust:1-bookworm' },
      { value: 'throwaway', label: 'throwaway' },
    ],
  },
  'worktree.targets': {
    targets: [
      { value: 'master', label: 'master', hint: 'current' },
      { value: 'feat/x', label: 'feat/x', hint: 'new worktree' },
    ],
  },
};

export const MOCK_PROJECT = {
  path: '/home/user/project',
  name: 'project',
  kilns: [{ path: '/home/user/notes', name: 'my-kiln' }],
  last_accessed: '2026-01-01T00:00:00Z',
};
