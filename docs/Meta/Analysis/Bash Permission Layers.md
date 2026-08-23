---
title: Bash Permission Layers
description: The order in which the daemon checks a bash command, and why the four lists stay separate
tags:
  - meta
  - analysis
  - security
  - permissions
---

# Bash Permission Layers

This document records the order in which the daemon checks a `bash` tool call. Four lists take part. Each list has its own override semantics, so the lists stay separate. The [[Consolidation Plan]] entry C9 made that decision. Checked against the code at commit 7053bcfe7 (2026-08-22).

## Summary

- The permission gate runs first, in `agent_manager/messaging/permission.rs`. It decides allow, deny or prompt.
- The tool executor runs second, in `tools/workspace.rs`. It applies the project `[security.shell]` policy.
- A deny at any layer is final. An allow at a layer skips the layers after it, inside the gate only.
- The executor never reads the gate's lists. The gate never reads the executor's list.

## The four lists

| List | Type | Source | Semantics |
|---|---|---|---|
| Hardcoded deny | `is_hardcoded_denied` (`config/components/permissions/hardcoded.rs`) | compiled in | Deny only. No config overrides it. |
| Config rules | `PermissionConfig.{allow,deny,ask}` (`config/components/permissions/types.rs`) | `[permissions]` in the daemon config, the agent card, or a mode | Deny beats ask. Ask beats allow. `default` decides the rest. |
| Saved patterns | `BashPatterns.allowed_prefixes` (`config/patterns.rs`) | the user chose "always allow" at a prompt; a `Project` grant is saved per project under the whitelists directory, a `User` grant in `user.toml` there; the gate reads both | Allow only. A prefix match skips the prompt. |
| Shell policy | `ShellPolicy.{whitelist,blacklist}` (`config/security.rs`) | `[security.shell]` in `.crucible/project.toml` | Blacklist beats whitelist. An empty policy imposes nothing. A non-empty whitelist denies every command that it does not list. |

## Order of evaluation

The gate evaluates the steps below in order. The first step that returns a decision ends the gate.

1. **Session override.** A session with permission mode `Allow` or `Deny` returns that decision. `Ask` continues.
2. **Permission engine** (`PermissionEngine::evaluate`). The engine splits the command on `;`, `&&`, `||`, `|` and newlines. For each segment it checks, in order: the hardcoded deny list, the `deny` rules, the `ask` rules, then the `allow` rules. A command is allowed only when every segment matches an `allow` rule. A construct the splitter cannot read (for example `$(...)`) can only tighten the result. Deny and allow end the gate. Ask continues.
3. **Saved patterns** (`PatternStore::matches_bash`). A prefix match on the full command string allows the call. No match continues.
4. **Lua hooks** (`execute_permission_hooks_with_timeout`). A hook can allow, deny or prompt. Allow and deny end the gate. Prompt continues.
5. **Mode stance** (`ModePermissions`). When the mode has rules, the daemon evaluates them with the same `PermissionEngine` as step 2. Otherwise it uses the mode's `default` stance. Allow and deny end the gate. Ask continues.
6. **Prompt.** An interactive session asks the user. A non-interactive session denies.

The executor then runs one more check before it spawns the shell:

7. **Shell policy** (`WorkspaceTools::bash`). The executor splits the command with `split_chained_commands`. For each statement, a blacklist prefix match blocks the call. When the whitelist is not empty, a statement with no whitelist prefix match also blocks the call.

## Why the lists stay separate

- The hardcoded list is a floor. A user must not be able to remove `rm -rf /` from it. A config list can always be edited.
- The config rules carry three stances. The saved patterns carry one. A merge would give saved patterns a `deny` stance that the prompt flow never writes.
- The saved patterns are written by the prompt flow (`store_pattern`). The config rules are written by the user. One file with two writers would need a merge policy.
- The shell policy lives in the project, not in the daemon config. It runs in the executor, so it also applies to a call that the gate allowed. That is its purpose: defense in depth against a too-wide `allow` rule or a saved pattern.
- `is_hardcoded_denied` and `ShellPolicy::default_blacklist` overlap on `rm -rf /`, `rm -rf ~` and `mkfs`. The overlap is deliberate. The hardcoded list is always on. The default blacklist applies only when a project calls `ShellPolicy::with_defaults`, and a project can remove an entry from it.

## Known gaps

- Step 3 matches a prefix against the whole command string. A saved pattern `git ` therefore allows `git log; curl evil`. Steps 2 and 7 split the command; step 3 does not. [[Gaps]] row G3 tracks the wider question.
- Step 3 reads and writes the pattern file with blocking I/O inside the async gate. [[Gaps]] row G18 tracks it.
