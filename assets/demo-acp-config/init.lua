-- Crucible ACP demo configuration.
--
-- Used to record the ACP and delegation demo fixtures. It points at `docs/`
-- as the kiln, which ships with the repository.
--
-- The config root is this DIRECTORY, so name this file to `cru`:
--
--     cru --config assets/demo-acp-config/init.lua session create --acp claude
--
-- Lua and not TOML: the daemon stopped reading `config.toml` in v0.30.0, so a
-- demo recorded against a TOML file ran on the defaults instead.
--
-- `CRUCIBLE_DEMO_ENDPOINT` names the OpenAI-compatible endpoint to record
-- against. The placeholder below keeps the file evaluable when it is unset.
cru.config.set({
    kiln_path = "./docs",
    llm = {
        default = "demo",
        providers = {
            demo = {
                type = "openai",
                endpoint = os.getenv("CRUCIBLE_DEMO_ENDPOINT") or "https://llm.example.com/v1",
                default_model = "glm-4.7-flash-iq4",
            },
        },
    },
    chat = {
        agent_preference = "crucible",
    },
    permissions = {
        default = "allow",
    },
    enrichment = {
        provider = {
            type = "fastembed",
            model = "BAAI/bge-small-en-v1.5",
            batch_size = 32,
        },
    },
    acp = {
        agents = {
            claude = {
                extends = "claude",
                -- The recording runs the agent against a throwaway config
                -- directory so the operator's own Claude login is untouched.
                env = {
                    CLAUDE_CONFIG_DIR = os.getenv("CRUCIBLE_DEMO_CLAUDE_DIR")
                        or "/tmp/crucible-demo-claude",
                },
                delegation = {
                    enabled = true,
                    max_depth = 1,
                    allowed_targets = { "cursor", "opencode" },
                },
            },
            cursor = { extends = "cursor" },
            opencode = { extends = "opencode" },
        },
    },
})
