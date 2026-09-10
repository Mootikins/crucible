# Plugin bridge spike

**Spike code. Nothing here is wired into the app, and nothing here should be.**

It answers one question for `docs/Meta/Analysis/Plugin Web Delivery.md`: what
does a `postMessage` bridge to a sandboxed opaque origin actually cost, and
does the containment hold?

It stands up a local origin that replicates `crucible-web`'s own
Content-Security-Policy (copied verbatim from
`crates/crucible-web/src/server.rs`), frames a "plugin" under
`Content-Security-Policy: sandbox allow-scripts`, and proxies `/api/*` to a
real running `cru web`.

## Run it

Start a daemon and the web server first, then:

```sh
cru web --port 3000                                  # in another terminal
python3 scripts/spikes/plugin-bridge/serve.py        # defaults to :8977 -> :3000
```

Open <http://127.0.0.1:8977/>. The page measures, then prints JSON.

Only the Python standard library is needed. `--port` is honoured by the server
but the frame documents name `127.0.0.1:8977` literally in their script tags,
because a sandboxed document cannot say `'self'` — see below. Change both if
you move the port.

## What each file is

| File | Role |
|---|---|
| `serve.py` | The origin. Owns both policies and the `/api` proxy. |
| `host.html`, `host.js` | The app half. Creates the frame, transfers the port, owns the method table, stamps the caller identity. |
| `frame.html`, `plugin.js` | The plugin half. Third-party-shaped: probes for escapes, then measures. |
| `quiet.html`, `quiet.js` | The smallest possible plugin, used only to time per-block mount cost. |

## What it reports

- Bridged versus direct call latency, small answer and 892 KB answer.
- Mount cost for 1, 4, 16 and 32 blocks on one page.
- An escape matrix: what the frame can reach for and what refuses it.
- `/frame-open.html` serves the same plugin with **no** `connect-src`
  restriction, to separate what the opaque origin refuses from what the policy
  refuses. Both refuse; the origin is the one that does not depend on anyone
  remembering to write a policy.

The numbers recorded in the analysis came from Chromium via Playwright, against
a debug daemon over loopback.

## One thing the spike taught that reading the policy does not

Inside a sandboxed document the origin is opaque, so **`'self'` matches no
URL**. The frame's policy must name the serving authority literally or the
plugin's script silently fails to load. `frame_csp()` in `serve.py` does this,
and the comment there is the one worth carrying into any real implementation.
