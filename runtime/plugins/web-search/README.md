# web-search

A `web_search` tool for Crucible agents, over a provider chain you configure.

The chain is walked in order, a provider whose config is absent is skipped, and
the first one that answers wins. Every provider adapts to the same normalised
result, so the model sees one shape regardless of who answered:

```json
{
  "query": "rust prompt caching",
  "provider": "searxng",
  "results": [
    {
      "title": "Prompt caching",
      "url": "https://…",
      "snippet": "…",
      "score": 2.67,
      "engines": ["duckduckgo", "google"]
    }
  ],
  "degraded": ["brave", "startpage"]
}
```

`score` and `engines` appear only where the provider reports them (SearXNG
does; cross-engine agreement is real relevance signal). `degraded` names
providers or engines that failed *without* failing the search — it is always
present, empty when the provider has no way to report partial failure, because
absence must never be a signal.

Everything else the providers return is dropped. A SearXNG hit alone carries 23
fields; forwarding `audio_src`, `thumbnail`, `positions` and friends to a model
is paying tokens for noise.

---

## The default chain sends your queries to DuckDuckGo

Read this before enabling anything.

The shipped default is `["searxng", "ddg"]`. With no `searxng_url` set — the
out-of-the-box state — **searxng is skipped and every query goes to
`lite.duckduckgo.com`**, a third party. There is no configuration in which
Crucible searches the web without something outside your machine seeing the
query; the only question is who.

Two things follow:

- Consent is the **existing tool permission gate**. `web_search` is not on the
  daemon's safe-tool allowlist, so the first call in a session prompts, and the
  prompt's "always allow" is the accept-and-persist. There is no separate
  first-use notice, and if you add a blanket `allow` rule for it you have opted
  out of being asked.
- If you do not want that, either host a SearXNG instance (below) or set
  `providers = []`, which disables search entirely.

The `ddg` adapter also **scrapes HTML** — DuckDuckGo lite has no API. It will
break when that page changes. It is written so breakage is loud: a parse that
finds nothing is an error naming the markup landmark that went missing, never
an empty result set, because "the web has nothing on this" is a wrong answer a
model would believe.

Crucible identifies itself honestly (`crucible/…` User-Agent, no browser
spoofing). An operator who wants to block or rate-limit it can, and that is
correct behaviour.

---

## Configuration

Either `[plugins.web-search]` in `config.toml`:

```toml
[plugins.web-search]
providers   = ["searxng", "ddg"]        # order = preference; [] disables search
searxng_url = "http://localhost:8888"   # unset → searxng is skipped
timeout     = 15                        # seconds, per provider
```

or from your `init.lua`, which runs after plugins load and therefore wins:

```lua
require("web-search").setup({
  providers   = { "searxng", "ddg" },
  searxng_url = "http://localhost:8888",
})
```

| Key | Type | Default | Meaning |
|---|---|---|---|
| `providers` | list of strings | `["searxng", "ddg"]` | The chain, in preference order. `[]` disables search. A bare string is read as a one-element chain. |
| `searxng_url` | string | *(none)* | Base URL of a SearXNG instance. Unset → `searxng` is skipped. |
| `timeout` | number | `15` | Per-provider HTTP timeout in seconds. The chain moves on rather than hanging. |
| `exa_api_key` | secret | *(none)* | Optional Exa key. See below — put it in the environment, not here. |

Resolution order, highest first: `$CRUCIBLE_WEB_SEARCH_*` for secrets →
`setup{}` → `[plugins.web-search]` → the defaults above.

### Secrets

Credentials resolve from the environment first:

```sh
export CRUCIBLE_WEB_SEARCH_EXA_API_KEY=…
```

The name is `CRUCIBLE_WEB_SEARCH_` + the config key, upper-cased. Env comes
before config so a key can stay out of a file that gets committed. There is no
Lua binding to a keyring; do not put a raw key in `init.lua`.

---

## Providers

### `searxng` — bring your own instance

`GET {searxng_url}/search?q=…&format=json`. Best results, and the only option
where the query does not leave a machine you control.

**The JSON API is off by default.** SearXNG ships with `search.formats: [html]`,
and asking for `format=json` without changing that gets you an HTML error page.
Add it to the instance's `settings.yml` and restart:

```yaml
search:
  formats:
    - html
    - json
```

If you skip this, the provider says so by name rather than reporting a parse
failure — that mistake is universal enough to be worth its own error message.

There is **no default instance and no shipped list of public ones**. Nine
public instances were tested; seven returned 429/403/timeout and the two that
returned HTTP 200 served anti-bot challenges. Public instances defend the JSON
endpoint precisely because it is the automation-friendly one. A pool of
instances *you* know to be good is a fine thing to configure; a shipped list
would be a feature that fails most of the time and, worse, intermittently.

### `ddg` — DuckDuckGo lite, keyless

`POST https://lite.duckduckgo.com/lite/`. No key, no account, no container —
which is why it is the out-of-box default. See the warning above: it is a third
party and it is scraping.

### `exa` — keyless, opt-in, not in the default chain

`https://mcp.exa.ai/mcp` is an HTTPS endpoint that speaks JSON-RPC and answers
`tools/call` anonymously. Enable it by adding `"exa"` to `providers`.

This is **not** an MCP server registration: no server config, no process
lifecycle, no tool-namespace entry, nothing in `cru mcp list`. It is one HTTP
POST behind one provider function, indistinguishable from the others at the
config surface.

It is kept out of the default chain because Exa sees the raw query, not because
it needs a credential — a key is optional and only buys rate limit.

---

## The tool

```
web_search(query, max_results?, provider?)
```

- `query` — required.
- `max_results` — default 8, capped at 25. The real bound is a ~6KB serialised
  payload: tool output over ~10KB is spilled to a file and replaced by a path
  reference, and the threshold is measured on the JSON-*escaped* string, so
  content spills nearer 7–9KB. Search results have to arrive inline, so the
  payload is trimmed from the tail to stay under.
- `provider` — restrict this call to one provider. It can only **narrow** the
  configured chain, never widen it: the model chooses tool arguments, and if it
  could name a provider you had not enabled, `providers` would stop being the
  complete list of who sees your queries.

### When everything fails

The tool returns an error naming **every provider it tried and why each one
failed**, skips included:

```
web_search found nothing for "rust prompt caching": every provider failed.
  searxng — skipped: no `searxng_url` is configured. Point it at an instance you host; …
  ddg — failed: ddg: POST https://lite.duckduckgo.com/lite/ failed: HTTP 429
Change the chain with `[plugins.web-search].providers` or `require("web-search").setup{ providers = { … } }`.
```

Tool errors reach the model. "Search failed" would send it into a retry loop —
it could not tell that retrying is pointless because the one configured
provider was never set up, and could not tell you what to fix.

### What you see versus what the model sees

The model gets the normalised rows. You additionally get a one-line summary via
the `tool:display_complete` hook:

```
searxng · 8 results · brave, startpage unavailable
```

Two channels, already separated in the daemon. The provider is named in both,
so a network call is never invisible.

---

## Layout

| Path | What |
|---|---|
| `plugin.yaml` | Manifest and the declaration of record for the config keys. |
| `init.lua` | The tool, the chain, `setup()`, and the display hook. |
| `lua/config.lua` | Config resolution, including the secret-from-environment step. |
| `lua/contract.lua` | The normalised result shape. Every provider ends here; nothing else constructs a result. |
| `lua/providers/*.lua` | One adapter per provider: transport and parsing only. |
| `tests/` | The suite, run by `cru plugin test runtime/plugins/web-search` and by CI. |

## Adding a provider

1. Write `lua/providers/<name>.lua`. Do the transport and the parsing, then end
   in `contract.normalise` — do not construct the payload yourself. Return
   `payload` or `nil, err`; never a bare `nil`, because the chain has to be able
   to say what failed.
2. Add an entry to `PROVIDERS` in `init.lua`: where to `require` it, a `needs`
   function returning why it should be skipped when its config is absent, and
   the option table to hand it.
3. Declare any new config key in `plugin.yaml` and `lua/config.lua`; mark
   credentials `secret: true` and add them to `SECRETS`.
4. Add `tests/<name>_test.lua`, driven off a recorded fixture and the stdlib
   HTTP mock. No network in the suite.
