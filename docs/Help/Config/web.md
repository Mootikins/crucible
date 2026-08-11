---
title: "Web UI Configuration"
description: Every field on the [web] config section for the browser UI served by cru web
tags:
  - help
  - config
  - web
---

# Web UI Configuration

`[web]` configures the browser UI that `cru web` serves. Every field has a default, so the
section is optional — `cru web` works with no configuration at all, serving the embedded
frontend on `http://localhost:3000`.

Add it to `~/.config/crucible/config.toml`.

## `[web]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `port` | integer | `3000` | Port to listen on |
| `host` | string | `"127.0.0.1"` | Bind address. `0.0.0.0` exposes the UI to your network |
| `static_dir` | string | *(unset)* | Serve assets from this directory instead of the ones embedded in the binary |
| `api_key` | string | *(unset)* | Bearer token for non-localhost clients. Unset generates and persists one; `""` disables auth entirely |
| `remote_shell` | bool | `false` | Let authenticated non-localhost clients use the terminal routes |
| `registration_roots` | array of strings | `[]` | Optional confinement for the web UI's "add project" button. Empty allows any ordinary directory (the floor is the only gate); a non-empty list confines registration to it — see [Project registration from the web UI](#project-registration-from-the-web-ui) |
| `allowed_hosts` | array of strings | `[]` | Extra `Host` authorities the server answers to. Empty derives them from the bind address and this machine's own hostname — see [Host validation](#host-validation) |
| `enabled` | bool | `false` | **Currently unread.** `cru web` starts the server unconditionally; nothing consults this field |

```toml
[web]
port = 3000
host = "127.0.0.1"
```

`allowed_hosts` is empty by default and rarely needs filling: a client on another machine may
use any name once it holds the API key, and a loopback caller already gets the loopback
spellings and this machine's own hostname. What is left for it is a name a *loopback* caller
uses — a reverse proxy on this same box. `registration_roots` is the opposite — empty allows
any ordinary directory, and you set it only to *restrict*.

`cru web` overrides `port`, `host`, `static_dir`, and `remote_shell` from the command line:

```bash
cru web --port 8080
cru web --host 0.0.0.0
cru web --static-dir ./web/dist
cru web --remote-shell
```

`--remote-shell` is additive with the config value — passing the flag turns it on, but it
cannot turn a configured `remote_shell = true` off.

## Authentication

Localhost requests never need a key. Non-localhost requests must present one as a bearer
token, or sign in once through the UI to get an HttpOnly session cookie.

The key resolves in this order:

1. `api_key` in `[web]`, if set to a non-empty string — used as-is.
2. `api_key = ""` — **auth is disabled**; every client is trusted.
3. Otherwise `~/.config/crucible/api_key`, read if it exists and is non-empty.
4. Otherwise a random key is generated and written there (mode `0600` on Unix) on first
   start.

```bash
cru web key              # print the current key
cru web key --rotate     # generate a new one (fails if api_key is set in config)
```

`--rotate` only manages the generated key file. When `api_key` is set explicitly in the
config, change it there instead.

The key is deliberately never embedded in the URLs `cru web` prints — query-string tokens
leak through browser history, server logs, and referrer headers.

## Remote shell access

The terminal routes hand out a PTY, which is unrestricted shell access on the host. They are
therefore **loopback-only by default**. Setting `remote_shell = true` lifts that restriction
for authenticated clients only, and it is fail-closed: with no API key configured (including
`api_key = ""`), `remote_shell` is ignored and the loopback restriction stays, with a warning
logged at startup.

```toml
[web]
host = "0.0.0.0"
remote_shell = true       # only takes effect because a key is in use
```

## Host validation

Every request is checked against the set of authorities this server answers to, and the check
is the **outermost** layer: it runs before CORS, before the body limit, and before everything
in auth — the localhost bypass, the API key, the same-origin WebSocket shortcut. It covers
every route, including the health endpoint and the static frontend bundle. A request whose
`Host` is not one of them gets:

```json
{ "error": { "code": 403, "message": "Request Host is not an address this server answers to" } }
```

This is the DNS-rebinding defence. A browser puts the authority it navigated to in `Host`
and page script cannot override it, so an attacker page on `evil.test` — even one whose DNS
record has been rebound to `127.0.0.1` — is stuck sending `Host: evil.test`. Refusing every
authority that is not ours is what makes the loopback bypass safe.

What rebinding is *for*, though, is the loopback bypass — the rebound page runs in a browser
on this machine, so its requests arrive from 127.0.0.1 and skip auth entirely, and the Host
check is all that stands in the way. **A request from another machine has no such shortcut**:
it presents the API key or it gets a 401. So the check is strict for loopback callers and
relaxed for the rest, and the practical effect is the one you want:

| Request arrives from | `Host` it may use |
|---|---|
| this machine (loopback) | the loopback spellings, this machine's own name, `allowed_hosts` — and nothing else |
| another machine, key configured | **any name**; the API key is the gate, not the name |
| another machine, `api_key = ""` | the same strict list as loopback — with no key behind it, the list is the whole defence |

That is what makes a LAN bind work by **any** FQDN that resolves to this box — `impulse`,
`impulse.lan`, a tailnet name, a CNAME, a name only the phone's resolver knows — with nothing
to enumerate and nothing to configure. What a remote client with no key can still reach is
what was never behind auth anyway: `/health` and the static bundle.

`allowed_hosts` remains for the cases that *are* loopback callers — most often a reverse proxy
on this same machine forwarding a public name:

```toml
[web]
host = "0.0.0.0"
allowed_hosts = ["crucible.example.com"]
```

### What is accepted

- **Always**, whatever else is configured: `localhost:<port>`, `127.0.0.1:<port>` and
  `[::1]:<port>` — the three spellings of the loopback you actually bound.
- **Whatever `host` says**, on `<port>`:
  - `host` is a specific IP (`192.168.1.10`) → that IP on `<port>`.
  - `host` is a name → that name on `<port>`.
  - `host` is a wildcard (`0.0.0.0` or `::`) → **any IP-literal `Host` on `<port>`** is
    accepted. That is the LAN case: a machine's reachable addresses cannot be enumerated up
    front, and an IP literal in `Host` cannot come from rebinding, which needs a *name*.
- **This machine's own names**, on `<port>` and bare, for any bind that is not
  loopback-only: the system hostname, plus `<hostname>.local` when the hostname is a bare
  label. This is what lets the operator's own browser use `http://impulse:3000` even when
  that name resolves to loopback, and it is what `cru web` prints on startup.
- **Every entry in `allowed_hosts`.**
- **Any name at all**, when the request came from another machine and an API key is
  configured — see the table above. This is the rule that makes arbitrary FQDNs work; the
  three above are what a *loopback* caller is held to.

### How an entry is matched

- An entry **without** a port matches both bare and with `<port>` appended.
  `"crucible.example.com"` accepts `Host: crucible.example.com` (a proxy terminating on
  80/443 forwards the public name with no port) *and* `Host: crucible.example.com:3000`.
- An entry **with** a port matches that port only. `"old.example:8443"` accepts
  `Host: old.example:8443` and nothing else — not `old.example`, not `old.example:3000`.
- Ports are always compared numerically, so `:03000` and `:3000` are the same port.
- Matching is case-insensitive, and one trailing dot is stripped from the host
  (`evil.test.` and `evil.test` are the same name to a resolver, so they are the same
  string here).
- IPv6 must be bracketed, and is canonicalised: `[0:0:0:0:0:0:0:1]` and `[::1]` are the same
  entry. An unbracketed IPv6 address is not a legal HTTP authority and is rejected.
- There is **no wildcard or glob syntax**. `"*"` and `"*.example.com"` are not patterns —
  `*` is not a character a hostname may contain, so both are dropped as unparseable at
  startup (with a warning) and match nothing. List each name.
- An empty list means "derive from the bind address" — i.e. exactly the two bullets above
  it. It does not mean "allow anything".

### What is refused

Anything the server cannot resolve to exactly one unambiguous authority is a 403, not a
guess:

- no `Host` header at all;
- two `Host` headers;
- a `Host` that disagrees with an absolute-form request target or an HTTP/2 `:authority`;
- an authority carrying userinfo, a path, a scheme, whitespace, non-ASCII, percent-escapes,
  port `0`, or an out-of-range port.

An `allowed_hosts` entry that does not parse is dropped with a warning at startup and never
matches; it does not fail the whole list.

## Project registration from the web UI

`POST /api/project/register` — the web UI's "add project" button — registers any ordinary
directory you point it at, including the repository you are working in. A registered project
root is also a read scope for `/api/file/raw`, so a small floor is always refused (see below);
everything else is allowed by default.

`registration_roots` is **empty by default, which means the floor is the only gate**. Set it
only if you want to *confine* registration further — with a non-empty list, a new root must
also be inside one of its entries:

```toml
[web]
registration_roots = ["~/work/repos"]
```

A leading `~/` expands to your home directory. Entries are canonicalised before use, so a
symlink cannot present a name inside a root for a target outside it. An entry that does not
resolve, or that is itself a floor-refused root, is dropped with a warning; if a non-empty
list has no valid entries left, registration is refused entirely (a misconfigured allowlist
fails closed rather than falling back to the floor).

The floor holds for every caller — CLI, TUI, RPC and web alike — and is:

- the filesystem root `/`;
- your home directory itself, or any ancestor of it (every credential you own lives under
  it), so `registration_roots = ["~"]` is refused while `["~/work"]` is fine;
- `/home` and `/Users`, which hold every user's home;
- a credential store (`.ssh`, `.gnupg`, `.aws`, …) or the user's config/state tree
  (`.config`, `.local`) — the web caller is untrusted, so these are refused even though a
  local `cru` may register them;
- anything under `/etc`, `/proc`, `/sys`, `/dev`, `/boot`, `/root` or `/run`.

So `registration_roots = ["/"]` does not re-open the door; it is dropped.

The daemon resolves a registration inside a git repo up to the repo root, which can land
*above* the directory you asked for. If that escapes the floor (or an active
`registration_roots` restriction) the registration is rolled back.

## Endpoint validation

A session created through the web API may name a custom provider `endpoint` (a self-hosted
model). The web layer validates it first, because the server dialing a URL the browser chose
is an SSRF primitive: the browser is a confused deputy for everything the *server* can
reach.

The rule is an allow-list, not a deny-list. An endpoint is accepted only if:

- the scheme is `http` or `https`; and
- **every** address its host maps to is a globally routable unicast address.

For IPv4 that refuses loopback, the RFC 1918 private ranges, link-local `169.254.0.0/16`
(which is where the cloud metadata address `169.254.169.254` lives), CGNAT
`100.64.0.0/10`, `0.0.0.0/8`, `192.0.0.0/24`, benchmarking `198.18.0.0/15`, reserved
`240.0.0.0/4`, multicast, broadcast and the unspecified address. For IPv6 only global
unicast `2000::/3` is accepted at all, minus the documentation prefix `2001:db8::/32` — so
`::1`, unique-local `fc00::/7`, link-local `fe80::/10`, multicast and every other reserved
prefix are refused without having to be enumerated.

Hostnames are resolved and judged on **all** their answers, so one internal record in an
otherwise public answer set refuses the whole endpoint. An unresolvable host is refused
too — an unknown host is not a safe host. IPv6 forms that encode an IPv4 destination
(v4-mapped, v4-compatible, v4-translated, 6to4, NAT64) are judged as that IPv4 address, and
so are alternative spellings the URL parser normalises (`http://2130706433`, `http://0x7f.1`
are both `127.0.0.1`).

### Loopback endpoints

A local Ollama on `http://localhost:11434` is loopback, and so would be refused by the rule
above. It is the product's headline local-LLM path, so it gets an exception — decided by the
**bind address**, with no configuration (the escape hatch below can only widen this):

| Effective bind (`[web] host`, or `cru web --host`) | Loopback endpoints |
|---|---|
| `127.0.0.1` (the default), any `127.x.x.x`, `::1`, `localhost` | **allowed** |
| `0.0.0.0`, `::` | refused |
| a LAN address, or any other name | refused |

The reasoning is who the browser is. On a loopback bind the only browser that can reach this
server is already on this machine, so pointing it at this machine's loopback grants it
nothing it did not already have. On a LAN or public bind the browser is a confused deputy:
the server's own loopback services are exactly what that browser cannot reach on its own.

`0.0.0.0` and `::` are the *unspecified* address, not loopback, so a wildcard bind refuses.
A bind host that is neither `localhost` nor a parseable IP is treated as reachable from
elsewhere and refuses too. The `localhost` match is case-insensitive, and brackets around an
IPv6 bind are ignored (`[::1]` is `::1`).

A refusal reads:

```text
Endpoint must not target a private/internal address: localhost → 127.0.0.1 (loopback
endpoints are allowed only on a loopback bind, or with
CRUCIBLE_WEB_ALLOW_LOOPBACK_ENDPOINTS=1)
```

The parenthetical appears only when the target actually is loopback — including the IPv4
mapped and embedded spellings, so `http://[::ffff:127.0.0.1]` gets it too. The other internal
ranges have no opt-in and get the bare message.

#### The escape hatch

`CRUCIBLE_WEB_ALLOW_LOOPBACK_ENDPOINTS` is for the one case the bind rule gets wrong on
purpose: an operator who deliberately exposes `cru web` on a LAN and still wants sessions
pointed at the server's own Ollama.

```bash
CRUCIBLE_WEB_ALLOW_LOOPBACK_ENDPOINTS=1 cru web --host 0.0.0.0
```

- It only ever **adds** permission. On a loopback bind it is redundant.
- The value must be exactly `1` or `true` (case-insensitive, surrounding whitespace
  ignored). Anything else — `yes`, `on`, `0`, empty, unset — is off.
- It is an environment variable on the `cru web` process. There is no config-file
  equivalent, deliberately: it should be a decision someone makes at launch, not one that
  outlives the reason for it.
- It unlocks **loopback only**. Private, CGNAT, link-local and the metadata address stay
  refused with it set; it is not a general "allow internal addresses" switch.

Setting it alongside a LAN bind means anyone on that network can steer the server at its own
loopback services. That is the trade you are making.

Two further limits worth knowing:

- This check happens at validation time, not at connect time. A short-TTL DNS record can
  answer with a public address here and an internal one when the provider actually connects.
  It raises the cost of the attack; it is not a boundary.
- It lives in the web layer only. The same endpoint reaches the daemon unvalidated from the
  TUI or a direct RPC client, which are already local-trust paths.

## Webhooks

`POST /api/webhook/{name}` turns a request into a `webhook:received` event on every plugin's
stream. It sits inside the bearer-auth layer, but that layer waves loopback callers
through — so on the machine running `cru web`, any page you visit could otherwise reach it
cross-origin with no credential. Every delivery therefore carries its own signature, and a
webhook with no configured secret is refused.

**The ingress is closed until you write a secrets file.** Out of the box every delivery gets
a `401 Missing or invalid webhook signature`. That one message covers every reason —
unknown name, missing secret, bad signature, stale timestamp — so the endpoint is not an
oracle for which webhooks exist. The real reason is in the server log.

### Secrets file

`~/.config/crucible/webhooks.toml`, one entry per webhook. It is not part of `config.toml`,
and it is read **once, when the server starts** — restart `cru web` after editing it.

<!-- crucible:not-config — this block is webhooks.toml, not config.toml -->
```toml
# ~/.config/crucible/webhooks.toml
[webhooks.ci]
secret = "at-least-16-bytes-of-secret"

[webhooks.deploy]
secret = "a-different-at-least-16-byte-secret"
```

Mint one with 32 random bytes and keep the file to yourself:

```bash
mkdir -p ~/.config/crucible
printf '[webhooks.ci]\nsecret = "%s"\n' "$(openssl rand -hex 32)" >> ~/.config/crucible/webhooks.toml
chmod 600 ~/.config/crucible/webhooks.toml
```

A webhook name is a URL path segment and a TOML key at the same time, so keep it to ASCII
letters, digits, `-` and `_`.

Rules, all fail-closed:

- A secret shorter than **16 bytes** is dropped with a warning; that webhook then refuses
  everything. A captured delivery lets an attacker brute-force a short secret offline.
- Two webhooks sharing one secret are **both** dropped. Nothing in the signature names the
  webhook, so a delivery aimed at one would authenticate the other. Give each its own.
- A missing, unreadable or malformed secrets file leaves the ingress closed. Failing to read
  the secrets never means "let it through".

### Signature scheme

Deliveries must be `Content-Type: application/json`. That is not cosmetic: `text/plain`,
`application/x-www-form-urlencoded` and `multipart/form-data` are CORS-safelisted, so a
cross-origin `fetch` using one of them is a *simple* request the browser sends with no
preflight and no chance to refuse. Requiring JSON forces the preflight, which the CORS layer
answers only for the app's own origins. Defence in depth *behind* the signature, never
instead of it.

Three signature headers are accepted, so an off-the-shelf sender works unmodified. They are
tried strongest-first, and a delivery that carries a timestamped header is never downgraded
to the body-only check — otherwise a caller who could send either would get to pick the
weaker one.

| Header | Value | Signed material | Replay |
|---|---|---|---|
| `X-Crucible-Signature` | `t=<unix>,v1=<hex>` | `<t>` `.` raw body | timestamp + memory |
| `Stripe-Signature` | `t=<unix>,v1=<hex>` | `<t>` `.` raw body | timestamp + memory |
| `X-Hub-Signature-256` | `sha256=<hex>` | raw body alone | memory only |

Common to all three: HMAC-SHA256 under that webhook's secret, over **the raw body bytes**.
Sign the bytes you send — a re-encoded body (pretty-printed JSON, a reserialised value)
produces a different signature. Hex is case-insensitive, and the tag must be exactly 32
bytes (64 hex characters).

**Timestamped** (Crucible's and Stripe's — the shape to prefer, and the same wire format, so
a real Stripe endpoint verifies against its own `whsec_…` with nothing bespoke in between):

- `t` is signed as the literal text you send — reformatting it (`007` → `7`) hashes
  different bytes than the sender hashed.
- `t` is also checked against the server clock and must be within **300 seconds** either
  way. The tolerance is symmetric, so a sender whose clock runs fast is not permanently
  rejected.
- Fields other than `t` and `v1` are **ignored**, not refused — Stripe sends `v0=` alongside
  `v1=`, and a verifier that rejects every field it has not seen cannot survive its own
  senders adding one. A *repeated* `t` or `v1` is still refused: a verifier that picks one of
  two candidate signatures is a verifier an attacker gets to aim.

**Body-only** (GitHub's) is accepted so a GitHub webhook works as shipped, but it is the
weaker scheme and worth understanding before you rely on it. Nothing in the signature says
*when* the delivery was sent, so there is no freshness check at all — only the replay memory
below stands between a captured delivery and a replay, and once its tag ages out the
identical delivery verifies again. `sha1=` and unprefixed values are refused; the algorithm
prefix is required.

**Replay memory**, common to all three: a signature that has already verified is remembered
and refused if it comes back. Remembered entries are dropped after the 300-second window, and
the store holds at most 4096 of them, evicting oldest-first. For a timestamped delivery that
does not matter — the clock check refuses it once it leaves the window anyway. For a
body-only delivery the memory is the *entire* replay defence, so its real window is whichever
comes first: 300 seconds, or 4096 further deliveries pushing the tag out. Under a chatty
sender that can be well under 300 seconds.

Signing a delivery from a shell:

```bash
body='{"event":"push"}'
t=$(date +%s)
sig=$(printf '%s.%s' "$t" "$body" | openssl dgst -sha256 -hmac "$SECRET" -hex | awk '{print $NF}')
curl -X POST http://localhost:3000/api/webhook/ci \
  -H 'Content-Type: application/json' \
  -H "X-Crucible-Signature: t=$t,v1=$sig" \
  --data-raw "$body"
```

Pointing GitHub at it needs no code: set the webhook's **Payload URL** to
`https://…/api/webhook/<name>`, paste the same secret into GitHub's **Secret** field, and set
**Content type** to `application/json`. GitHub's default there is
`application/x-www-form-urlencoded`, which this endpoint refuses with a
`415 Unsupported Media Type` — if deliveries fail with a 415 rather than a 401, that setting
is why.

The delivery's headers go onto the plugin event stream verbatim, minus the credentials:
`Authorization`, `Cookie`, `Proxy-Authorization` and all three signature headers are
stripped first.

## CORS

The server allows its own origins automatically: `http://<host>:<port>`,
`http://127.0.0.1:<port>`, and `http://localhost:<port>` (plus the Vite dev server on
`http://localhost:5173` in debug builds). Add more with a comma-separated
`CRUCIBLE_CORS_ORIGINS`:

```bash
CRUCIBLE_CORS_ORIGINS="https://notes.example.com,http://192.168.1.10:3000" cru web
```

CORS is a different question from [host validation](#host-validation), and the two lists are
deliberately not mirrored. `allowed_hosts` names authorities the server *answers to*; CORS
names page origins allowed to talk to it *cross-origin*. A deployment behind a proxy is
same-origin from the browser's point of view, so CORS is never consulted for it.

## Security headers

Every response carries these, set only if the route did not already set its own — so
`/api/file/raw`, which sandboxes the documents it refuses to serve inline, keeps its stricter
policy.

| Header | Value |
|---|---|
| `X-Content-Type-Options` | `nosniff` |
| `Referrer-Policy` | `no-referrer` — the app's URLs name your machine and ports; nothing it links out to needs them |
| `Content-Security-Policy` | see below |

The CSP is written from what the frontend actually does. The parts worth knowing:

- `script-src 'self' 'wasm-unsafe-eval'` — only the app's own bundle executes. No
  `'unsafe-inline'`, no `'unsafe-eval'`. WASM is allowed because shiki's regex engine and the
  opt-in local Whisper model both compile it.
- `frame-ancestors 'none'` — a loopback instance is authenticated without a cookie, and so
  would be clickjackable if framable.
- `style-src 'self' 'unsafe-inline'` — unavoidable today. CodeMirror, xterm, mermaid and
  katex all inject styles at runtime, and a nonce cannot reach them.
- `connect-src 'self' https:` — `'self'` is what lets the terminal dial its own WebSocket.
  The policy names **no authority**, deliberately: a proxied deployment on
  `https://crucible.example.com` gets its own `wss://` socket from the same `'self'`, so
  there is nothing here that can fall out of step with `allowed_hosts`. The `https:` is for
  the optional transcription model download.
- `img-src`/`media-src`/`frame-src` stay open to remote schemes because markdown renders web
  images and a canvas link node embeds pages in a sandboxed iframe.

None of this is configurable.

### Serving kiln files

`/api/file/raw` is the one route that hands back bytes an agent may have written, and it is
same-origin with the API — so a file the browser parses as a *document* there could
`fetch('/api/shell/exec')` with your credentials already applied. It is therefore an
allowlist of what may be rendered inline:

| Content type | Served as |
|---|---|
| `image/*`, `audio/*`, `video/*` | itself — a media decoder, no scripting surface |
| `image/svg+xml` | itself, but under `Content-Security-Policy: sandbox` |
| `application/pdf` | itself — the viewer's scripting has no access to the embedding page |
| `text/plain` | `text/plain; charset=utf-8`, charset pinned so the browser cannot pick one out of the bytes |
| **everything else**, including files with no extension | `application/octet-stream` + `Content-Disposition: attachment` + sandbox CSP |

The practical consequence: **clicking an `.html` file in your kiln downloads it rather than
opening it.** That is deliberate, not a bug — rendering it would put agent-written script on
the app's own origin. `nosniff` is set by the route itself rather than left to the global
layer, because a declared content type is only binding with it.

The sandbox CSP (`sandbox; frame-ancestors 'none'`, no `allow-*` tokens) puts the document in
a unique opaque origin, so even if something renders those bytes anyway — a plugin, an
external viewer, a browser mishandling the disposition — its script cannot reach the API, the
session cookie, or the app's DOM. Download filenames are reduced to `[A-Za-z0-9._-]`, since
the name is attacker-chosen and could otherwise inject a second header.

Note which rows *don't* get that sandbox: it covers the download path and SVG only. PDFs and
decoded media are served under the app's own policy, deliberately — `sandbox` breaks Chrome's
PDF viewer, and a canvas file card embeds one. The accepted risk is that a PDF from your kiln
is rendered by the browser's viewer with the app's policy applied; the judgement is that the
viewer gives a PDF's own scripting no DOM, cookie or same-origin fetch access to the
embedding page. If that assumption ever fails, this is the row to revisit.

## `[web]` vs `[server]`

These are different sections. `[web]` is the browser UI above. `[server]` holds daemon-side
settings (`auto_archive_hours`) plus several TLS and request-limit fields reserved for
future use and not yet wired to any behaviour. Configuring the web UI under `[server]` has
no effect.

## See Also

- [[Help/Config/acp]] — ACP agent configuration
- [[Help/Config/permissions]] — tool permission rules, which apply to web sessions too
- [[Help/Extending/Workflow Authoring]] — the `webhook` workflow trigger these deliveries feed
- [[Help/CLI/Index]] — full CLI reference
