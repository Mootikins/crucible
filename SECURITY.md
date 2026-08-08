# Security Policy

## Supported Versions

Crucible is pre-1.0 and ships from a single `master` line. Security fixes land on the
latest release only; there are no maintenance branches for older tags.

| Version | Supported |
|---------|-----------|
| 0.22.x (latest release) | yes |
| < 0.22 | no — upgrade |

Check what you are running with `cru --version`.

## Reporting a Vulnerability

**Report privately through GitHub Security Advisories:**

<https://github.com/Mootikins/crucible/security/advisories/new>

That form is private to you and the maintainer, and it is the only supported channel.
Do not open a public issue, discussion, or pull request for a suspected vulnerability —
a public report is a disclosure, and it starts the clock on every user before a fix
exists.

A useful report includes:

- The Crucible version (`cru --version`) and your OS.
- Which surface is affected — daemon RPC, MCP server, web server, Lua plugin runtime,
  ACP delegation, or the note/kiln parser.
- Steps to reproduce, ideally as a minimal kiln, plugin, or request.
- What an attacker gets: code execution, file read/write outside the kiln, credential
  disclosure, a bypass of a permission prompt or trust profile.

### What to expect

| Stage | Target |
|-------|--------|
| Acknowledgement of your report | 7 days |
| Initial assessment (accepted / not a vulnerability / need more detail) | 14 days |
| Fix released, or a written plan with a date if it will take longer | 90 days |

This is a single-maintainer hobby-scale project, not a vendor with an on-call rotation.
If a deadline slips you will hear about it rather than be left waiting.

## Disclosure and Embargo

- Please keep the report confidential until a fix is released or 90 days have passed
  from your initial report, whichever comes first.
- Fixes are published as a GitHub Security Advisory with a CVE where one is warranted,
  and credit to the reporter unless you ask to stay anonymous.
- If the vulnerability is already being exploited, or a third party discloses it first,
  the embargo is void and a fix ships as fast as it can be written.

## Scope

Crucible is an agent runtime. By design it does things that would be vulnerabilities in
other software, and the boundary matters when deciding whether a finding is a bug:

**In scope.** Anything that crosses a boundary Crucible claims to enforce:

- Escaping the permission gate — a tool call executing without the approval the
  configuration requires.
- A Lua or Fennel plugin reaching capabilities its declared permissions exclude, or
  reading state belonging to another session or kiln.
- The daemon's Unix socket, the MCP server, or the web server accepting a request from
  a principal that should not have been able to reach them, or leaking another user's
  sessions, notes, or provider credentials.
- Provider API keys or agent credentials written to logs, session transcripts, ACP wire
  recordings, or anywhere world-readable.
- Untrusted *content* achieving execution: a markdown note, a wikilink target, an ACP
  message, or an LLM tool-call argument that escapes parsing into code execution or a
  path traversal outside the kiln.
- Trust or delegation-depth limits configured under `[acp]` in
  `~/.config/crucible/config.toml` failing to hold.

**Out of scope.** These are the documented design, not defects:

- Plugins you install execute arbitrary Lua with your privileges. A plugin from an
  untrusted source is equivalent to running an untrusted script; install accordingly.
- Crucible spawns subprocesses — external ACP agents, MCP servers, editors — with your
  user's privileges, and delegates work to them. A vulnerability inside a third-party
  agent belongs to that project.
- The daemon's own socket is a Unix socket guarded by filesystem permissions — reports
  that a local user who can already open it is able to drive the daemon describe the
  design.
- The MCP server (`cru mcp`, SSE on port 3847) is unauthenticated and built for a
  trusted local host; binding it to a routable interface is your decision to make. The
  same goes for running `cru web` with `api_key = ""`, the one setting that turns its
  bearer auth off. `cru web` otherwise authenticates every non-localhost request by
  default, so defeating *that* is in scope — see above.
- An LLM being persuaded to request a harmful tool call is in scope only if it bypasses
  the permission gate. Approving a destructive call at the prompt is not a bypass.
- Findings against dependencies with no Crucible-specific exploit path; report those
  upstream.
