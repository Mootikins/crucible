-- Crucible demo configuration.
--
-- Used by the VHS tape files for reproducible demo recordings. It points at
-- `docs/` as the kiln, which ships with the repository.
--
-- The config root is this DIRECTORY, so name this file to `cru`:
--
--     cru --config assets/demo-config/init.lua session create ...
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
    enrichment = {
        provider = {
            type = "fastembed",
            model = "BAAI/bge-small-en-v1.5",
            batch_size = 32,
        },
    },
})
