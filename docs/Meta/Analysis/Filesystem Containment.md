---
title: Filesystem Containment
description: How the daemon confines agent file tools to a default-deny set of roots, and the CVE survey that shaped the design
tags:
  - meta
  - analysis
  - security
---

# Filesystem Containment

This document describes the filesystem containment the daemon enforces today. It was built in
three commits: `d5ea72fbd` (capability-scoped access, protected paths, write-denied
transcripts), `7c52d2885` (per-tool surface, execution roots) and `335a7448d` (remaining
write sinks). The open follow-ups (cap-std, Landlock) live in
`thoughts/Agent Filesystem Containment.md`. Checked against the code at commit 7053bcfe7 (2026-08-22).

## Summary

- A session holds an allowlist of roots. A path outside every root is refused.
  (`crates/crucible-daemon/src/tools/containment.rs:1`)
- Tools do not hold a root path. They hold an `FsScope`. Only `FsScope::resolve` and
  `FsScope::resolve_for_write` produce the path types the tools accept.
  (`crates/crucible-daemon/src/tools/fs_scope.rs:31`)
- A path is resolved once into a lexical form and a canonical form. The judge asks both.
  A path inside by name but outside when resolved is a `SymlinkEscape`, not a silent refusal.
  (`crates/crucible-daemon/src/tools/path_resolution.rs:182`;
  `containment.rs:73-80,256,320`)
- Tool trust is classified per tool, in one exhaustive table. A name with no entry is
  `Unknown`. The isolation gate refuses `Unknown` as it refuses `Host`.
  (`crates/crucible-daemon/src/tools/surface.rs:1`;
  `crates/crucible-daemon/src/agent_manager/messaging/isolation_gate.rs:16`)
- A hardcoded protected set denies writes to paths the daemon or a login session later
  executes. No allow rule re-opens it.
  (`crates/crucible-daemon/src/tools/protected.rs:104`)
- The trees the daemon loads code from come from one function. The loaders and the
  protected set both call it. (`crates/crucible-daemon/src/execution_roots.rs:155`;
  `protected.rs:288`)
- `bash` is the honest hole. A shell process is not mediated by this layer. The claim is
  therefore about the file tools, not the session. (`fs_scope.rs:64-70`)

## The roots: default-deny allowlist

`containment.rs` keeps an allowlist of roots. A path is refused unless a root admits it.
Denied roots survive only as carve-outs inside an allowed root. Example: a kiln at the data
root encloses the sessions root. The session may reach the kiln. It may not reach
`sessions/`, except its own session directory (`agent_manager/scope.rs:76-112`).

The predecessor was default-allow minus a denylist. Every escape found in two review passes
took one of two forms against that model. The first form out-ranks a denial: attach a kiln
deeper than the denied root, or inflate a path's depth through a directory that does not
exist yet. The second form side-steps a denial: an empty root matches every path. Neither
form exists against an allowlist. There is nothing to out-rank.

`Path::starts_with` in Rust is component-wise. `/project-evil` does not match `/project`.
The defect was never the comparison. It was un-normalized input.

## The door: `FsScope`

A tool family holds an `FsScope`. `FsScope::resolve` returns a `ContainedPath`. The field is
private. There is no public constructor and no `From<PathBuf>`. A signature that takes
`&ContainedPath` therefore carries a compiler-checked proof that containment was consulted.

A write needs a stronger proof. `FsScope::resolve_for_write` returns a `WritablePath`. It
clears everything a read clears, plus the protected set and the write-denied roots. There is
no `From<ContainedPath>`. As a result, `grep resolve_for_write` enumerates the write surface.

`FsScope::walk_files` and `FsScope::read_dir` apply the rule to what the walk yields
(`fs_scope.rs:421,434`). A walker that starts inside an allowed root cannot collect a denied
subtree beneath it.

A shared check function was not enough. The predecessor, `validate_path_within_kiln`, leaked
because the tools that produce paths by a filesystem walk never called it. Hermes ran the same
experiment with `tools/path_security.py::validate_within_dir`. Their docstring concedes "this
is NOT a security boundary". A shared check depends on every call site. A capability handle
makes the safe route and the ergonomic route the same route.

## Resolve once, ask twice

`ResolvedPath::resolve` (`path_resolution.rs:182`) produces two forms. The lexical form
clamps `..` at the root and is defined on paths that do not exist. The canonical form resolves
through the deepest existing ancestor. `RootSet::judge_resolved` (`containment.rs:256`) takes
the resolved pair. Inside by both forms is `Permitted`. Inside by name but outside when
resolved is `SymlinkEscape { canonical_target }`. Anything else is `Outside`.

Two properties follow. First, a path through a directory that does not exist yet cannot dodge
a denial, because the lexical form judges it. Second, the judge reads the filesystem once for
both root sets, so a link that moves between two checks cannot make the two verdicts disagree.

The old `canonicalize_lenient` helper no longer exists. It re-appended the tail of a path
unnormalized after it canonicalized the existing ancestor. That shape is structural. It was
replaced, not patched.

## Trust per tool, not per executor

`ToolSurface` answers one question: if nothing intercepts this call, does it touch the host?
`BuiltinTool::surface` (`surface.rs:189`) is an exhaustive `match` with no wildcard arm. A new
variant does not compile until someone classifies it. Two module-level clippy denies catch a
`_ => Daemon` escape. `classify(name)` (`surface.rs:242`) returns `Unknown` for a name with no
variant. `ToolSurface` has no `Default`.

The previous design assigned the surface per executor. `McpToolExecutor` answered `Daemon` for
all its tools. That was false for `create_note`, `update_note` and `delete_note`. They reach
`std::fs` on the host, in the daemon process. An isolated session refused `bash` and
`write_file` but let `create_note` write a `.lua` file under a runtimepath tree. The daemon
would execute it on the next start. `McpToolExecutor::surface` now delegates to `classify`
(`crates/crucible-daemon/src/tool_dispatch.rs:606`).

`isolation_refusal` (`isolation_gate.rs:16`) asks the plugin registry whether host execution
is allowed for this surface. The gate ordering in `messaging/tool_call.rs` keeps the refusal
before a Lua `handled` result.

`reserved_tool_names()` (`surface.rs:294`) is the one set that rejects a plugin tool name that
collides with a built-in. `builtin_tool_names()` in `plugin_tools.rs:63-71` and the gateway
collision check both call it. The set does not depend on whether a kiln is attached, so the
ten kiln tools cannot be shadowed on a kiln-less session.

## Protected paths

`protected.rs` keeps two hardcoded lists. `PROTECTED_DIRS` (line 104) names `.crucible`,
`.git`, `.claude`, `.codex`, `.opencode` and `.pi`. `SHELL_STARTUP_FILES` (line 116) names
the files a login session executes, with `.gitconfig` (line 131) because `core.fsmonitor` and
`core.pager` run code. `write_protection` (line 184) applies them to a `ResolvedPath`. A
symlink at a protected path protects its target too.

`daemon_roots()` (line 288) delegates to `execution_roots::all()`. That module is the one
answer to "where does the daemon load code from". Before it existed, four resolvers
disagreed, and the protected set named none of the env-var roots.

`write_protection` has two callers: `FsScope::resolve_for_write` (`fs_scope.rs:323`) and the
`session.export_to_file` handler (`server/observe.rs:279`). The export handler resolves the
caller's path before it writes. Its default path asserts canonical equality.

## Destructive sinks assert equality

`remove_session_dir` (`crates/crucible-daemon/src/session_manager.rs:120`) canonicalizes
both the root and the target. It refuses unless the target equals the expected session
directory under the canonical root. "Beneath a root" is not enough for a `remove_dir_all`.

`SessionId::parse` (`crates/crucible-core/src/session/types/id.rs:76`) is a validated
newtype. A raw string never reaches `Path::join`, whether it arrives by RPC or from
`meta.json`.

## Other closed-set gates

- `diff_synth::normalize_tool_name` maps `create_note` to `ToolKind::Write` and
  `delete_note` to `ToolKind::Delete` (`tools/diff_synth.rs:76,83`). The approver sees a diff.
- `plugin_tool_barred` asks `BuiltinMode::is_read_only()` (`tools/tool_modes.rs:79`), not
  the literal `"plan"`.
- `DiscoveryConfig.additional_paths` was deleted.

## Why the threat model is tampering, not reading

Claude Code blocks writes to `~/.claude/projects/**.jsonl` and states that reading a
transcript is not blocked. Gemini CLI keeps `chats/` inside an allowed root. Codex has no
carve-out. The industry view: an agent that reads a transcript is not the interesting attack.
An agent that edits one is, because a transcript replays into a future context.

Crucible keeps the read denial. A shared kiln stores other users' material in a way the
others do not. But the write denial matters more, and `d5ea72fbd` added it.

## Rationale: what comparable agents do

| Project | Real boundary | Transcript reachable by its own tools? |
|---|---|---|
| **Zed** | In-process, default-deny allowlist of worktrees. Canonical component-wise `starts_with`, ancestor walk for paths that do not exist, symlink escape as a distinct outcome | No. Threads are SQLite plus zstd outside any worktree |
| **Codex** (OpenAI) | OS sandbox: Seatbelt, bubblewrap, restricted tokens. In-process policy compiles down to these | Yes. `~/.codex/sessions/**.jsonl` under a global read grant |
| **Claude Code** | OS sandbox for Bash only. Read/Edit/Write use permission rules | Yes, by design. Transcript writes are blocked as defense in depth |
| **Gemini CLI** | In-process `path.relative` form, `realpath` plus ancestor walk | Yes. The project temp dir that holds `chats/` is an allowed root |
| **Cursor** | OS sandbox plus allowlist | Unknown |
| **OpenCode** (sst) | In-process only, permission prompts | Outside the project root a prompt gates it, not a deny |
| **Hermes** (Nous) | Container, SSH and Modal backends isolate. The local backend has only a deny-list | Blocked for the file-read tool, not for the exec tool |
| **Aider** | None. `abs_root_path()` resolves `..` and never compares against the root | History lives inside the repo |
| **Goose** | None for file tools. `resolve_path()` uses the model path verbatim. Safety is per-call confirmation | Not path-blocked. SQLite, not JSONL |

Zed is the reference, not Codex. It is the only one of the nine where transcripts are
unreachable. It achieves that in process for two reasons: a default-deny allowlist, and
storage that sits outside every allowed root. Crucible copies both.

Every project that relies on default-allow minus a denylist has a CVE or an acknowledged
bypass. The two with no CVE in this class, Zed and Goose, are default-deny and no containment
at all.

## Rationale: the three shapes, with CVE numbers

The two review passes found three shapes, not three bugs.

1. **An unvalidated identifier reaches `Path::join`.** `join` normalizes nothing. An absolute
   component replaces the base.
2. **Containment enforced per tool family.** `read_note` returned a transcript that
   `read_file` refused.
3. **Lenient canonicalize re-appends `..` literally.** A path through a directory that does
   not exist yet dodged `starts_with(denied_root)`.

Precedent:

- **Codex, CVE-2025-59532** (CVSS 8.6). A model-generated `cwd` became the writable root. Fix:
  anchor the boundary to where the user started the session.
- **OpenClaw, GHSA-575v-8hfq-m3mc** (CVSS 8.4, CWE-22 plus CWE-59). Bind mounts bypassed the
  parent-directory checks. Fix: validate twice, on the normalized source and after the
  deepest existing ancestor. Shape 3.
- **Cursor, CVE-2026-50549.** "A canonicalization failure that fails open." Shape 3.
- **Anthropic Git MCP server, CVE-2025-68143/68144/68145.** Path traversal, Jan 2026.
- **AutoGPT, CVE-2023-37274.** Unsanitized filename from an LLM tool call. Shape 1.
- **tower-http `ServeDir`, RUSTSEC-2022-0043.** A Windows absolute component mid-path
  replaced the base. Shape 1.
- **Anthropic `server-filesystem` MCP, CVE-2025-53109/53110.** A prefix check let any path
  that begins with the approved directory through. A symlink pointed anywhere. Shapes 1 and 3.
- **Gemini CLI PR #27767.** All three shapes in one patch: unvalidated `name` from `SKILL.md`,
  a clone path checked apart from the install path, and `startsWith(targetDir)` matching
  `skills-attacker` against `skills`.
- **Claude Code, CVE-2026-39861.** Sandboxed Bash created a symlink out of the workspace. The
  unsandboxed write path followed it. Shape 2.
- Ecosystem scale: about 82% of 2,614 scanned MCP servers use filesystem operations prone to
  traversal. More than 30 CVEs hit MCP servers in one 60-day window in early 2026.

## Rationale: the write-then-execute class

Researchers escaped Cursor, Codex, Gemini CLI and Antigravity at once. The agent wrote a file
that a trusted host process later executed outside the sandbox. Instances: Claude Code
**CVE-2026-25725** (bubblewrap did not protect `.claude/settings.json` when it did not exist
yet; sandboxed code created it and injected `SessionStart` hooks), Claude Code
**CVE-2026-55607** (a worktree named `.git` plus git fsmonitor), and Cursor
**CVE-2026-26268** (agent-written git hooks).

Crucible had the same exposure. An agent with write access could write
`.crucible/project.toml`, `.crucible/kiln.toml` or a Lua plugin under `runtime/plugins/`. The
protected set and `execution_roots::all()` close it for the file tools.
