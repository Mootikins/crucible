-- Named LLM provider instances.
--
-- One `llm` table holds every provider you can reach, each under its own
-- name, and `default` names the one chat uses. Switch providers by changing
-- that one string, or per session with `cru chat --provider <name>`.
--
-- Read every key from the environment rather than writing it here: this file
-- is Lua, so `os.getenv` is all the reference syntax there is.

cru.config.set({
    llm = {
        -- The provider chat uses when no flag names another.
        default = "local",

        providers = {
            -- A local Ollama instance.
            ["local"] = {
                type = "ollama",
                endpoint = "http://localhost:11434",
                default_model = "llama3.2",
                timeout_secs = 120,
            },

            -- Cloud OpenAI.
            cloud = {
                type = "openai",
                api_key = os.getenv("OPENAI_API_KEY"),
                endpoint = "https://api.openai.com/v1",  -- optional; this is the default
                default_model = "gpt-4o",
            },

            -- A remote Ollama, on another machine.
            remote = {
                type = "ollama",
                endpoint = "https://llm.example.com",
                default_model = "qwen-110b-q4",
                timeout_secs = 300,
            },

            -- Anthropic Claude.
            anthropic = {
                type = "anthropic",
                api_key = os.getenv("ANTHROPIC_API_KEY"),
                default_model = "claude-sonnet-5",
            },

            -- OpenRouter, which fronts many APIs. Name a model as
            -- `provider/model`.
            openrouter = {
                type = "openrouter",
                api_key = os.getenv("OPENROUTER_API_KEY"),
                default_model = "openai/gpt-4o",
            },

            -- The Z.AI GLM Coding Plan. Available models: GLM-5, GLM-4.7,
            -- GLM-4.5-Air.
            ["zai-coding"] = {
                type = "zai",
                endpoint = "https://api.z.ai/api/coding/paas/v4",
                api_key = os.getenv("GLM_AUTH_TOKEN"),
                default_model = "GLM-4.7",
            },
        },
    },
})

-- With no `llm` table at all, chat falls back to the `chat` table:
--
--   cru.config.set({ chat = { model = "llama3.2", endpoint = "http://localhost:11434" } })
