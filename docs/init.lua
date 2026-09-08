-- =============================================================================
-- Crucible Reference Configuration
-- =============================================================================
-- Location: docs/init.lua
--
-- This is a reference for the shape of Crucible's config file. Copy the parts
-- you need into your own file; this file is documentation, not a config the
-- tool loads.
--
-- Where the real config lives:
--   Linux    ~/.config/crucible/init.lua
--   macOS    ~/Library/Application Support/crucible/init.lua
--   Windows  %APPDATA%\crucible\init.lua
--
-- `init.luau` is read in preference to `init.lua`; a directory that holds both
-- is refused. Override the root with `cru -C <path>` or $CRUCIBLE_CONFIG_DIR.
--
-- The daemon evaluates this file once, at boot, before it loads plugins. Any
-- line may set any key, and the last write wins. `cru.config.set` deep-merges:
-- tables merge key by key, arrays and scalars replace. Put `__replace = true`
-- inside a table to replace it whole.
--
-- Values are code. Read an environment variable with `os.getenv("VAR")`, a
-- file with `io.open(path)`, and build a table with a loop.
--
-- A kiln's own `.crucible/kiln.toml` holds only the kiln's display name, and
-- `.crucible/project.toml` holds project metadata and security policy. Neither
-- takes the keys below.
--
-- Environment variables Crucible reads:
--   CRUCIBLE_CONFIG_DIR  - the config root, where this file lives
--   CRUCIBLE_KILN        - kiln path, when no --kiln flag and no ancestor
--                          .crucible/ directory is found
--   CRUCIBLE_HOME        - daemon data root (default ~/.crucible)
--   CRUCIBLE_SOCKET      - daemon socket path
--   CRUCIBLE_RUNTIME     - runtime root (plugins, themes, skills)
--   CRUCIBLE_PLUGIN_PATH - extra plugin search paths
--   CRUCIBLE_LOG_FILE    - log file path
--
-- Priority (highest to lowest):
--   1. CLI flags (--embedding-url, --embedding-model, ...)
--   2. This file
--   3. settings.json, which the settings UI writes
--   4. A plugin's declared defaults
--   5. Defaults
-- =============================================================================

cru.config.set({
    -- =========================================================================
    -- Kiln registry
    -- =========================================================================
    -- `kilns` maps a name to a path, or to a table with options. When it is
    -- present, the legacy top-level `kiln_path` is ignored.

    default_kiln = "docs",

    kilns = {
        docs = ".",

        -- Full form, with options:
        -- work = { path = "~/work/notes", lazy = true },  -- not opened at start
    },

    -- Legacy single-kiln shorthand, still honoured when `kilns` is empty:
    -- kiln_path = "~/notes",

    -- Where `cru chat` stores sessions, if not the default kiln:
    -- session_kiln = "~/sessions",

    -- Daemon data root (project registry, default session storage, home kiln).
    -- Defaults to $CRUCIBLE_HOME, else ~/.crucible.
    -- data_home = "~/.crucible",

    -- Extra directories to search for agent cards, beyond the built-in
    -- locations. Deprecated; prefer `runtimepath`, which serves every asset
    -- kind.
    -- agent_directories = { "~/shared-agents" },

    -- Extra roots for plugins, themes, skills and agent cards. This is the
    -- list of *additional* roots; the well-known ones always apply.
    -- runtimepath = { "/opt/crucible/runtime" },

    -- =========================================================================
    -- projects - Project registry
    -- =========================================================================
    -- Bind a code repository to one or more kilns. The daemon auto-opens a
    -- project's kilns when a session starts inside it. `cru init` registers
    -- these with the daemon instead, which needs no config line at all.

    -- projects = {
    --     crucible = { path = "~/crucible", kilns = { "docs" }, default_kiln = "docs" },
    -- },

    -- =========================================================================
    -- llm - LLM providers
    -- =========================================================================
    -- Named provider instances. `default` names the key used for chat.
    -- See Help/Config/llm.md.

    llm = {
        default = "local",
        providers = {
            ["local"] = {
                type = "ollama",
                endpoint = "http://localhost:11434",
                default_model = "llama3.2",
            },

            -- cloud = {
            --     type = "openai",
            --     default_model = "gpt-4o",
            --     api_key = os.getenv("OPENAI_API_KEY"),
            -- },

            -- claude = {
            --     type = "anthropic",
            --     default_model = "claude-sonnet-5",
            --     api_key = os.getenv("ANTHROPIC_API_KEY"),
            -- },
        },

        -- Specialty -> model mapping for agent cards that declare `specialty:`.
        -- models = { reasoning = "openai/o1", coder = "qwen2.5-coder" },
    },

    -- =========================================================================
    -- enrichment - Embeddings and the enrichment pipeline
    -- =========================================================================
    -- Without this table the daemon SKIPS embedding generation entirely and
    -- semantic search returns nothing.
    --
    -- The older flat `embedding` table is REJECTED — a config containing it
    -- fails to load. See Help/Config/embedding.md for every provider's fields.

    enrichment = {
        provider = {
            -- fastembed | ollama | openai | mock. cohere/vertexai/custom/burn
            -- were removed: a config that names one fails at load.
            type = "fastembed",
            model = "BAAI/bge-small-en-v1.5",
            -- For fastembed, `model`, `cache_dir` and `batch_size` are all
            -- read. The vector dimension comes from the model. The
            -- `dimensions`, `retry_attempts` and `headers` knobs were
            -- removed: no provider read them.
        },

        -- Pipeline tuning. `worker_count`, `batch_size` and `timeout_ms` were
        -- removed: nothing read them.
        pipeline = {
            max_precognition_chars = 3000,  -- char budget for precognition snippets
        },
    },

    -- =========================================================================
    -- chat - Chat defaults
    -- =========================================================================
    -- Defaults for Crucible's own agent. The provider is chosen under `llm`,
    -- not here — a `provider` key in this table is REJECTED.

    chat = {
        show_thinking = false,
        show_diffs = true,
        agent_preference = "crucible",  -- "crucible" (default) or "acp"
        -- model = "llama3.2",
        -- endpoint = "http://localhost:11434",
    },

    -- =========================================================================
    -- acp - External agents over the Agent Client Protocol
    -- =========================================================================
    -- See Help/Config/acp.md for every field, including agent profiles,
    -- delegation, and per-agent permissions.

    acp = {
        streaming_timeout_minutes = 15,  -- time allowed for one complete response
        -- default_agent = "claude",     -- omit to auto-discover

        -- agents = {
        --     ["my-claude"] = {
        --         extends = "claude",
        --         env = { ANTHROPIC_BASE_URL = "http://localhost:4000" },
        --     },
        -- },
    },

    -- =========================================================================
    -- cli - CLI behaviour
    -- =========================================================================
    -- Verbosity comes from the -v flag, not from config.

    cli = {
        highlighting = {
            enabled = true,
            theme = "base16-ocean.dark",
        },
    },

    -- =========================================================================
    -- context - Project rules files
    -- =========================================================================
    -- Files searched from git root down to the workspace directory and loaded
    -- hierarchically. See Help/Rules Files.md.

    -- context = {
    --     rules_files = { "AGENTS.md", "CLAUDE.md", ".rules", ".github/copilot-instructions.md" },
    -- },

    -- =========================================================================
    -- Storage
    -- =========================================================================
    -- SQLite lives at <kiln>/.crucible/crucible-sqlite.db and holds everything,
    -- embeddings included (a leftover crucible-vectors.lance/ directory from an
    -- older version is unused and safe to delete). There is no `storage` key.

    -- =========================================================================
    -- permissions - Tool access control
    -- =========================================================================
    -- See Help/Config/permissions.md for pattern syntax.

    -- permissions = {
    --     default = "ask",                                -- allow | deny | ask
    --     allow = { "read_note:*", "semantic_search:*" },
    --     deny = { "bash:rm *" },
    --     ask = { "write_file:*" },
    -- },

    -- =========================================================================
    -- mcp - Upstream MCP servers
    -- =========================================================================
    -- Aggregate an external MCP server's tools under a prefix.
    -- See Help/Config/mcp.md.

    -- mcp = {
    --     servers = {
    --         {
    --             name = "github",
    --             prefix = "gh_",
    --             auto_reconnect = true,
    --             timeout_secs = 30,
    --             allowed_tools = { "search_*", "get_*" },
    --             blocked_tools = { "delete_*" },
    --             transport = {
    --                 type = "stdio",
    --                 command = "npx",
    --                 args = { "-y", "@modelcontextprotocol/server-github" },
    --                 env = { GITHUB_TOKEN = os.getenv("GITHUB_TOKEN") },
    --             },
    --         },
    --     },
    -- },

    -- =========================================================================
    -- web - Browser UI
    -- =========================================================================
    -- See Help/Config/web.md.
    --
    --   registration_roots  OPTIONAL confinement for the web UI's "add project"
    --                       button. Empty ({}, the default) means any ordinary
    --                       directory registers — the floor (filesystem root,
    --                       your home dir, credential stores, config tree) is
    --                       the only gate. Set it to confine registration to an
    --                       explicit list instead. A project root is also a read
    --                       scope for /api/file/raw, which is why the floor
    --                       exists.
    --
    -- `allowed_hosts` is FAIL-CLOSED and empty by default, with no command-line
    -- override:
    --
    --   allowed_hosts       Extra Host authorities this server answers to, on
    --                       top of localhost/127.0.0.1/[::1] on `port` and
    --                       whatever `host` names. Empty ({}) means "derive from
    --                       the bind address", NOT "allow anything" — so
    --                       reaching the box by an mDNS name (node7.local) or
    --                       through a reverse proxy is a 403 until the name is
    --                       listed here. An entry without a port matches both
    --                       bare and on `port`; an entry with a port matches
    --                       that port only. No globs: a leading dot
    --                       (".example.com") is the wildcard, and it matches the
    --                       apex plus exactly ONE label under it
    --                       (app.example.com, never a.b.example.com). A
    --                       malformed entry — a glob, a bare ".", or a public
    --                       suffix such as ".com" or ".local" — makes `cru web`
    --                       REFUSE TO START rather than be dropped with a
    --                       warning.
    --
    -- Webhook sender secrets are NOT in this file. POST /api/webhook/{name}
    -- requires an HMAC signature and is closed until you create
    -- ~/.config/crucible/webhooks.toml with `[webhooks.<name>] secret = "..."`.
    -- See Help/Config/web.md, section Webhooks.
    --
    -- Two more web controls have no config key at all:
    --   CRUCIBLE_WEB_ALLOW_LOOPBACK_ENDPOINTS=1   env var on the `cru web`
    --       process. A session endpoint pointing at the server's own loopback
    --       (a local Ollama on http://localhost:11434) is allowed when `host`
    --       is a loopback bind — 127.x, ::1 or "localhost" — and refused as
    --       SSRF otherwise. This variable force-allows it on a LAN/wildcard
    --       bind too; it only ever adds permission, and unlocks loopback only.
    --   CRUCIBLE_CORS_ORIGINS                     comma-separated extra CORS
    --       origins.

    -- web = {
    --     port = 3000,
    --     host = "127.0.0.1",
    --     api_key = "",             -- "" disables auth entirely
    --     remote_shell = false,
    --     registration_roots = {},  -- empty = any ordinary dir; set to confine
    --     allowed_hosts = {},       -- e.g. { "node7.local", ".crucible.example.com" }
    -- },

    -- =========================================================================
    -- workspace - Where checkouts live
    -- =========================================================================
    -- `root_dir` is the default workspace directory, where `scm.clone` writes.
    --
    -- Set `discover = true` to make the daemon register every git repository
    -- that is a DIRECT child of `root_dir` when it starts, so the web root
    -- picker lists them without a manual registration for each. It is off by
    -- default because a registered root is also a web read/write scope:
    -- `/api/file/raw` serves everything under it and the save route writes to
    -- it. The scan applies only the daemon floor, so it admits a checkout that
    -- `POST /api/project/register` refuses — a symlink into `~/.config`, or a
    -- dotfiles repo holding `.ssh`. Turn it on only when every child of
    -- `root_dir` is ordinary work.
    --
    -- `session_scratch_dir` is unrelated: a session started with no project
    -- gets a private `<session_scratch_dir>/<session_id>` directory that lives
    -- and dies with it.

    -- workspace = {
    --     root_dir = "~/Projects",
    --     discover = false,
    --     session_scratch_dir = "~/.crucible/workspaces",
    -- },

    -- =========================================================================
    -- server - Daemon server settings
    -- =========================================================================
    -- `auto_archive_hours` is the whole table. `host` and `port` were removed —
    -- the daemon binds a Unix socket, and the web server's address is `web`. So
    -- were the TLS and request-limit keys, which were never wired to anything.
    -- This table refuses an unknown key, and says which one.

    -- server = { auto_archive_hours = 72 },

    -- =========================================================================
    -- logging
    -- =========================================================================
    -- `level` is the whole table. `format`, `console`, `file`, `file_path`,
    -- `rotation`, `max_file_size`, `max_files`, `component_levels`,
    -- `timestamps`, `target` and `ansi` were removed: they parsed, validated,
    -- and reached nothing. Use `RUST_LOG` for per-module levels.

    -- logging = {
    --     level = "info",  -- off | error | warn | info | debug | trace
    -- },

    -- =========================================================================
    -- schedules - Recurring Lua snippets
    -- =========================================================================
    -- `cru.schedule` is the native spelling; this table is the declarative one.

    -- schedules = {
    --     {
    --         name = "nightly reindex",
    --         every = "1d",  -- "1d", "1h", "30m", "5s", or bare seconds
    --         action = "lua:cru.log('tick')",
    --         enabled = true,
    --     },
    -- },

    -- =========================================================================
    -- plugins - Per-plugin settings, and git-hosted declarations
    -- =========================================================================
    -- Free-form tables read by the plugin of the same name. The reserved
    -- `declare` table names plugins the daemon clones and loads at every boot.

    -- plugins = {
    --     oci = { runtime = "podman" },
    --     declare = {
    --         greeter = "user/greeter",
    --         review = { url = "someone/review", pin = "v1.2" },
    --     },
    -- },
})

-- =============================================================================
-- Beyond config keys
-- =============================================================================
-- The same file registers hooks, modes, session defaults and runtime paths.
-- See Help/Lua/Configuration.md.
--
-- cru.on("pre_tool_call", function(ctx) end)
-- cru.defaults.model = "claude-sonnet-5"
-- cru.rtp.append("~/team-kit")
--
-- Split a long config with `require`: a module in ~/.config/crucible/lua/
-- resolves by name, so `require("my.mod")` reads lua/my/mod.lua.
