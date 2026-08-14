---
title: Discord
description: Run a Crucible agent as a Discord bot with the bundled discord plugin
status: implemented
tags:
  - extending
  - plugins
  - messaging
  - discord
aliases:
  - Discord Plugin
  - Discord Bot
---

# Discord

The bundled `discord` plugin connects a Crucible session to a Discord bot. It
holds one Gateway WebSocket, routes the messages it is allowed to answer into a
per-channel agent session, and streams the reply back as Discord messages.

It is the reference implementation for a plugin that *is* a service: everything
here is Lua on the daemon's plugin VM — `cru.ws`, `cru.http`, `cru.sessions` —
with no Discord-specific Rust. See [[Help/Extending/Creating Plugins]] for the
plugin structure and [[Help/Plugins/Lua Runtime API]] for the APIs it calls.

> [!info] It ships loaded and answers nobody
> The plugin is bundled, so it loads on every daemon. That is safe because it
> is inert in three independent ways until you configure it: it does not dial
> Discord without `auto_connect` **and** a token, it answers no user and no
> guild until an allowlist names them, and it refuses to create a session
> without a `kiln`. A fresh install with a valid bot token still answers no
> one. This is intentional — see [Who the bot answers](#who-the-bot-answers).

## Setup

**1. Create the bot.** At <https://discord.com/developers/applications>, create
an application, add a Bot, and copy its token. Under *Bot → Privileged Gateway
Intents*, enable **Message Content Intent** — without it Discord delivers
messages with an empty `content` and the bot silently answers nothing. Invite
the bot to your server with the `bot` scope and the *Send Messages* and *Read
Message History* permissions.

**2. Configure Crucible.** In `~/.config/crucible/config.toml`:

```toml
[plugins.discord]
bot_token = "..."            # or leave unset and export DISCORD_BOT_TOKEN
auto_connect = true
kiln = "/home/you/kiln"      # required; see below
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
allowed_users = ["123456789012345678"]    # your Discord user id
allowed_guilds = ["987654321098765432"]   # servers the bot may answer in
```

**3. Restart the daemon.**

```
cru daemon restart
```

`cru daemon logs` should show `Discord bot ready: <name> (N guilds)`. If it
shows `auto_connect is false` or `no bot_token configured`, the gateway was
deliberately not started.

You can also connect on demand without `auto_connect`. The plugin registers a
`discord` command, invoked from the TUI (or the web palette) as a slash:

```
/discord connect      # blocks until a clean disconnect or exhausted retries
/discord status       # connected?, gateway session id, active agent sessions
/discord disconnect
```

With no subcommand, `/discord` reports status. (The plugin's own status text
says `:discord`; plugin commands dispatch on `/`.)

## Who the bot answers

Two allowlists decide, and they are checked **above every other routing rule**:

- `allowed_users` — Discord user ids the bot answers **in DMs**.
- `allowed_guilds` — guild (server) ids the bot answers **in**.

**Both default to empty, and empty means nobody — not everybody.** A bot that
is online, tokenised and invited will ignore every message until one of these
lists names someone. This is deliberate: the DM branch used to return "answer"
unconditionally, above the `respond_to` check, so no configuration value could
close it and anyone who could DM the bot could spend the operator's API key.
Fail-closed is the only defensible default for a surface a stranger can reach.

Ids arrive from the Gateway as strings but are routinely written unquoted in
TOML, so both sides are compared as strings — `["123"]` and `[123]` both match.

Once a guild is allowed, `respond_to` decides *which* messages within it.
`respond_to` does not apply to DMs: a listed user's DM is always answered.

To find an id, enable *Settings → Advanced → Developer Mode* in Discord, then
right-click a user or server and *Copy ID*.


## What the bot may do

The allowlists decide **who gets an answer**. `access` decides **what that
answer may do**, so one bot instance can read for a server and read *and* write
for you.

```toml
[plugins.discord]
approvers = ["123456789012345678"]    # who answers an `ask` prompt

[plugins.discord.access]
"user:123456789012345678" = "write"   # your own DMs
"role:135792468013579246" = "ask"     # moderators may write, once approved
"guild:987654321098765432" = "read"   # a server that may look, not touch
default = "read"
```

- **`read`** (the default, and what you get with no `access` block) — the agent
  may read files and notes and run kiln searches, with no prompt.
- **`ask`** — reads freely, but the agent must get a **y** before each write.
  Prompts arrive as a callout naming the tool:

  ```
  > ⚠️ **write_file** wants to run:
  > ```
  > docs/Notes/scratch.md
  > ```
  > Reply **y** to allow, **n** to deny (optionally `n, reason`).
  ```

  No answer within 60 seconds denies, and so does a second request arriving
  while that one is still unanswered. Who answers it is
  [`approvers`](#who-answers-an-ask-prompt).
- **`write`** — the read tools plus `write_file`, `edit_file`, `multi_edit`,
  `create_note` and `update_note`, with no prompt.

Reads and writes are bounded by the session's kilns — `kiln` plus anything in
`kilns` — not by the filesystem. Point `kiln` somewhere you are content for the
bot to touch.

A message takes **its sender's own tier**, and the room's only when that sender
has no key of their own: `user:` first, then `role:` (any guild role they
hold), then `guild:`, then `default`, first match wins. That is safe because
sessions are keyed per speaker — a per-account grant reaches nobody else,
because nobody else is in that session. A `guild:` key is a floor for the
unnamed rather than a ceiling on the named.

`bash` is in neither tier. Its blast radius is not bounded by the session's
kilns, so granting it is deliberate: set `tool_policy` explicitly, which
replaces the tier for every session.

> [!NOTE]
> Outside the `ask` tier a Discord turn runs non-interactively, so a tool that
> is not granted here is *denied* rather than queued — nothing will prompt, and
> the agent gets a tool error. `ask` is the exception.

### Who answers an `ask` prompt

`approvers` is a list of user ids. The **first** is sent the prompt in a DM,
and **only their reply resolves it** — so the request itself may come from
anywhere, because the room it came from never sees the prompt:

```toml
[plugins.discord]
approvers = ["123456789012345678"]
```

The prompt names the requester and the room they asked from, because the
approver is deciding for an account they cannot otherwise see:

```
> @someone in #general asked.
> ⚠️ **write_file** wants to run:
> ```
> docs/Notes/scratch.md
> ```
> Reply **y** to allow, **n** to deny (optionally `n, reason`).
```

The requester is told their request is waiting, and gets the answer when it
comes. If the DM cannot be opened, or the prompt cannot be sent, the request is
**denied** rather than handed back to the requester to approve.

**One request at a time.** A reply is a bare **y**, with nothing in it naming
the request it answers — so only one prompt may be outstanding in a channel at
once, and a second request arriving while the first is unanswered is denied
immediately and told to try again. That matters most with an approver, since
every requester's prompt lands in the same DM: without the rule, a **y** meant
for one request could resolve another, from a different person in a different
room.

With `approvers` **unset**, the requester answers their own prompt, in the
channel they asked in. That is only offered when their tier came from a `user:`
or `role:` key — a grant that named an account. A `guild:` or `default` grant
of `ask` degrades to `read` instead, because the principal it describes is
"anyone in the room", which is also who would be answering.

For a personal bot, put **your own id** in `approvers`. Nothing special
happens: you are simply both parties, and the prompt arrives in your DMs
wherever you asked from.

## Every option

All keys live under `[plugins.discord]`.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Master switch. `false` stops the plugin loading at all — see [Turning it off](#turning-it-off). |
| `bot_token` | `""` | Bot token. Falls back to the `DISCORD_BOT_TOKEN` environment variable when empty. |
| `auto_connect` | `false` | Dial the Gateway when the plugin loads. With it false the plugin loads inert and you connect with `/discord connect`. |
| `intents` | `37889` | Gateway intents bitmask — `GUILDS` + `GUILD_MESSAGES` + `DIRECT_MESSAGES` + `MESSAGE_CONTENT`. Change it only if you know why. |
| `allowed_users` | `[]` | User ids answered in DMs. **Empty means nobody.** |
| `allowed_guilds` | `[]` | Guild ids answered in. **Empty means nobody.** |
| `access` | `{}` | Capability per identity — see [What the bot may do](#what-the-bot-may-do). |
| `approvers` | `[]` | User ids that answer `ask` prompts, in order; the first is DMed. Empty means the requester answers their own — see [Who answers an `ask` prompt](#who-answers-an-ask-prompt). |
| `tool_policy` | `{}` | Replaces the access tiers wholesale. The escape hatch for granting a tool the tiers withhold. |
| `respond_to` | `"mentions"` | Within an allowed guild: `mentions`, `prefix`, `both`, or `all`. |
| `command_prefix` | `""` | Text prefix for `respond_to = "prefix"`/`"both"`, e.g. `"!"`. Empty disables prefix matching. |
| `quota_turns_per_day` | `50` | Agent turns each user may spend per UTC day. |
| `kiln` | — | **Required.** Path to the kiln every Discord session writes to. |
| `kilns` | `[]` | Additional kiln paths the session may *read*. See [Citations](#citations-and-the-precognition-prerequisite). |
| `provider` | — | **Required.** LLM provider for Discord sessions. |
| `model` | — | **Required.** Model id. |
| `agent_type` | `"internal"` | Agent implementation: `internal` or `acp`. Leave it alone unless you have a reason. |
| `system_prompt` | Discord-shaped default | Overrides the built-in prompt entirely, including its citation sentence. Ignored when `agent_card` is set. |
| `provider_key` | — | Named provider credential instead of the default. |
| `agent_card` | — | [[Help/Extending/Agent Cards|Agent card]] giving the internal agent its persona. See [Running on an agent card](#running-on-an-agent-card). |
| `agent_name` | — | ACP profile to launch. Requires `agent_type = "acp"`; refused on an internal agent. |

### Running on an agent card

`agent_card` names a card the daemon resolves at session create, against the
kiln you configured — the card's prompt, model and MCP servers instead of
`system_prompt` and `model`. `cru agents list` shows what a kiln can see.

The access tier still decides what the session may *do*: the tier's tool policy
is applied over the card's own `tools:` block, never under it. A card cannot
widen a `read` sender to `write`.

The card is named at create and nowhere else, because `session.configure_agent`
writes the whole agent — configuring one afterwards would replace the card's
prompt and model with the plugin's own. That is also why `agent_name` is refused
on an internal agent: it names an ACP *profile*, resolves no card, and used to
be set anyway.

### `kiln` is required, not defaulted

Without it, `cru.sessions.create` falls back to the daemon's data root and the
session's reflection proposals land under `~/.crucible/.crucible/proposals/`,
where `cru proposals list` never looks. The plugin refuses to create a session
rather than write there, and logs
`no kiln configured — set [plugins.discord] kiln`.

Point `kiln` at the same path as your top-level `kiln_path` if you want
`cru proposals list` to find Discord's proposals without changing directory.

### Turn quota

`quota_turns_per_day` caps **turns, not tokens**, per user per UTC day. Tokens
would be the better unit and are the wrong one to use: usage is recorded only
when the provider reports it, and an ACP agent may report none — so a token
quota reads zero and fails *open* the moment the configured agent changes. A
turn counter cannot read zero.

The user who crosses the cap gets exactly one reply naming it; every message
after that is dropped silently, because replying to each message of a flood is
itself Discord REST traffic during exactly the scenario the cap exists for.
Counters live in memory and reset on daemon restart.

## Sessions

One Crucible session per **speaker per channel**, reused while it stays warm:

- **DMs** — 24 hours of inactivity.
- **Guild channels** — 15 minutes of inactivity.
- Sessions idle for 2 hours are ended outright by a periodic sweep.

Two people talking to the bot in one channel get a session each, so their
contexts stay apart and a tier or a quota charge belongs to whoever spoke. A DM
channel already holds exactly one account, so nothing changes there.

**Replying to one of the bot's messages continues the session that produced
it**, even when it was answering somebody else — that is how you join a thread
another person started. Two exceptions: a replier whose tier differs from the
one that session was built with gets their own session instead, because a
session's tool grants are fixed when it is created; and the index of the bot's
messages lives in memory, so a reply to something said before the daemon
restarted starts a new session rather than resuming the old one.

### DM sessions survive a daemon restart — if the kiln is registered

DM sessions are written to `~/.crucible/plugin-state/discord/sessions.json` and
read back when the plugin loads, so a conversation in a DM continues across a
`cru daemon restart` rather than starting over. Channel sessions are not
written: they are gone in fifteen minutes anyway, and remembering them would
mean a file write on every channel message.

**The kiln must belong to a registered project for this to work.** Reviving a
session id means finding the kiln that holds its transcript, and a restarted
daemon can only look inside the kilns it has opened — which are the kilns of
the projects in `[projects.*]`. Setting `[plugins.discord] kiln` opens nothing;
it only tells the plugin where to create sessions. Register it:

```toml
# ~/.config/crucible/config.toml

[kilns]
discord = "/home/you/kiln"

[projects.discord]
path = "/home/you/kiln"
kilns = ["discord"]
```

Running `cru init` inside the kiln directory does the same thing.

Without it the id is remembered and the revival fails, which costs one message:
a new session is created and the conversation starts fresh.

Two further limits. The index of the bot's own messages is not persisted, so a
reply to something the bot said before the restart starts a new session (see
above). And a DM session already past its 24-hour window when the daemon comes
back is dropped rather than revived.

A second message that arrives while the agent is still working on the first is
refused by the daemon (one concurrent turn per session) and answered with
*"I'm still working on your previous message."* There is no queue.

## Citations, and the precognition prerequisite

The default system prompt ends with:

> When kiln notes were provided to you, name the note titles you drew on at the
> end of your reply.

There is no citation renderer and no link resolution — [[Help/Concepts/Precognition]]
already injects the retrieved notes as a system block containing each note's
title and similarity score, so the model has the titles in context and this
sentence asks it to name them.

The sentence is **conditional on purpose.** Precognition only injects on the
*first* user message of a session, while a guild channel session is reused for
15 minutes. From message two onward there are no notes in context, and an
imperative "always cite your sources" would make the model invent titles rather
than admit it had none. Overriding `system_prompt` drops this sentence along
with the rest of the default; re-add it if you want citations.

> [!warning] Prerequisites before you can observe a citation
> Precognition is on by default, but it retrieves nothing without an embedding
> provider and an indexed kiln — and **connected kilns are skipped entirely
> when there is no enrichment config**, or when the embedding model does not
> match the primary kiln's
> (`crates/crucible-daemon/src/agent_manager/precognition/mod.rs`). So:
>
> 1. Configure `[enrichment.provider]` — see [[Help/Config/embedding]].
> 2. Index every kiln the session touches, both `kiln` and each entry in
>    `kilns`, with that same model: `cru process <path>`.
> 3. Ask a question whose answer is in an indexed note, as the **first**
>    message of a fresh session.
>
> An unindexed kiln retrieves nothing, and a kiln listed in `kilns` but indexed
> with a different embedding model is dropped with a warning in
> `cru daemon logs` rather than an error at the user.

## Discord turns are non-interactive outside `ask`

Except on the `ask` tier, a plugin-created session runs its turns with
`is_interactive = false`. The permission engine converts an `Ask` decision to
`Deny` when a turn is not interactive, and the tool call returns an error before
any interaction request is emitted. **A rule that would have prompted instead
denies.**

This is not a limitation to work around — it is the point. Permissions are
keyed on `(session_id, permission_id)` and nothing in the daemon knows who is
entitled to answer, so a y/n matched against the Discord author id that
triggered the prompt is a chat-room username with no Crucible principal behind
it, in a channel anyone can be invited to. That is what
[`approvers`](#who-answers-an-ask-prompt) supplies: an account the *operator*
named in config, prompted in its own DM. Only a turn that has one — or a
requester a `user:`/`role:` key named — runs interactively at all.

If you want a tool available to the Discord bot, `allow` it explicitly in your
permission rules. See [[Help/Concepts/Permission Precedence]].

## Reloading: the daemon, not the plugin

**Do not use `plugin.reload` (or the web UI's reload button, or the plugin file
watcher) on `discord`.** Reload re-spawns the plugin's declared services, and
nothing cancels the already-running one: the old `gateway.connect` task keeps
its socket. You end up with two live WSS connections on one token, and every
message is answered twice.

To pick up a change, restart the daemon:

```
cru daemon restart
```

If you have already double-spawned, a restart is also the fix.

## Gateway reliability

`gateway.connect` retries with exponential backoff — 10 attempts, 1 s base,
60 s cap — on any failure that is not a deliberate `/discord disconnect`.
Heartbeats are tracked explicitly: a socket whose heartbeat goes unacknowledged
is dropped and redialled rather than left half-alive. Discord's `RESUME` is
used where the gateway offers it.

After the retry budget is exhausted the gateway stays down. `/discord status`
reports it, and `/discord connect` or a daemon restart brings it back.

## Turning it off

If you are running the bot where other people can reach it, read
[[Help/Operations/Discord Bot Runbook]] before you need it — it covers the three
ways to stop the bot and which one fits, how to rotate both of its secrets, and
what it can actually reach.

Add `enabled = false` **to the `[plugins.discord]` section you already have**
in `~/.config/crucible/config.toml`, leaving the rest of it in place:

```toml
[plugins.discord]
enabled = false              # <- the only line you add
bot_token = "..."
auto_connect = true
kiln = "/home/you/kiln"
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
```

Then:

```
cru daemon restart
```

The plugin is disabled between discovery and load, so its `init.lua` never
runs, no service spawns, and no socket opens. `plugin.reload` and the file
watcher cannot bring it back — reload bails for a disabled plugin.

> [!danger] Add the key to the section you already have
> Do **not** append a second `[plugins.discord]` header. A working install
> already has one, and a duplicate TOML table is a parse error that takes the
> *whole* config file down — the daemon then starts with no plugin config at
> all, which looks like a successful kill switch and is not. If your config
> genuinely has no `[plugins.discord]` section, the plugin was already inert.

Editing `enabled:` in the plugin's own `plugin.yaml` does **not** work durably:
the bundled runtime tree is re-extracted whenever the binary's version or tree
hash changes, reverting your edit. Config is the only durable lever.

## What is retained

- **Every Discord message routed to the bot, and every reply**, in the
  session transcript at `<kiln>/.crucible/sessions/<session-id>/session.jsonl`.
  Message content is stored verbatim. Nothing expires it automatically;
  `cru session cleanup --older-than <days>` is the tool, and `--dry-run` shows
  what it would remove.
- **Message content is sent to your configured `provider`**, along with any
  kiln notes precognition retrieved. Whatever a Discord user types reaches that
  third party under your API key.
- **Reflection proposals** at `<kiln>/.crucible/proposals/*.md` — outside the
  index until you accept them, at which point they become ordinary kiln notes
  and are embedded and searchable.
- **The DM half of the sender→session map**, at
  `~/.crucible/plugin-state/discord/sessions.json`: a Discord channel id, the
  sender's account id, a Crucible session id, an access tier and a timestamp
  per open DM. No message content. Delete the file to forget every DM
  conversation; the next message starts a fresh session.
- **In memory only, lost on daemon restart:** the channel half of that map, the
  index of the bot's own messages, and the per-user turn counters.
- **Not retained by Crucible:** Discord message ids, attachments, and the
  channel history the bot did not answer. The plugin reads no history.

## Demonstrating the full loop

This is the end-to-end check that a Discord conversation becomes durable
knowledge. State the question you will ask afterwards **before** you start, so
the demonstration cannot be fitted to the outcome.

**Prerequisites.** [[Help/Concepts/Reflection Pass]] has no default model and
bails without one, and its `min_turns` default of 3 will skip the short
exchange Discord's own prompt asks for. Both are required:

```toml
[plugins.reflection]
model = "claude-haiku-4-5-20251001"   # required — no default; without it reflection skips
min_turns = 1                          # required — the default of 3 skips a Discord-length chat
```

`min_turns = 1` needs no recursion guard from you. The reflection subagent's
auxiliary session is ended through `cru.sessions.end_session`, which goes
through the session bridge and fires no session hooks — and the plugin
independently tags its own aux sessions with a marker in their system prompt
and skips them regardless of turn count.

**The procedure.**

1. Decide and write down the question you will ask at step 6.
2. Hold the conversation in Discord — enough that the answer to that question
   is established in it.
3. End the session explicitly:

   ```
   cru session list
   cru session end <id>
   ```

   This is the path that fires `on_session_end`. Letting the session time out
   does not run the reflection pass.
4. Review what was proposed:

   ```
   cru proposals list
   cru proposals show <id>
   ```

   Run these with the Discord `kiln` as the active kiln — proposals are read
   from `<kiln>/.crucible/proposals/`.
5. Accept it. This writes the note into the kiln and deletes the proposal;
   indexing happens on the daemon's next scan, so force it rather than wait:

   ```
   cru proposals accept <id>
   cru process <kiln>
   ```
6. In a **new** Discord session — a new channel, or after the reuse window has
   lapsed, so precognition runs — ask the question from step 1. The answer
   should draw on the accepted note, and name its title.

If step 6 produces no citation, check the precognition prerequisites above
before suspecting the prompt: an unindexed kiln retrieves nothing, and a
reused session never invokes precognition at all.

## See Also

- [[Help/Concepts/Precognition]] — what puts the notes in context
- [[Help/Concepts/Reflection Pass]] — what turns a conversation into a proposal
- [[Help/Extending/Creating Plugins]] — plugin structure and `setup()`
- [[Help/Plugins/Lua Runtime API]] — `cru.sessions`, `cru.ws`, `cru.http`
- [[Help/Concepts/Permission Precedence]] — why an `ask` rule denies here
- [[Container Isolation]] — the other bundled plugin built entirely in Lua
