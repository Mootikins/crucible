---
title: Project, Workspace, and Repository Taxonomy
description: Definitions and relationships between projects, workspaces, and repositories
tags:
  - meta
  - architecture
  - projects
---

# Project, Workspace, and Repository Taxonomy

This document defines the relationship between projects, workspaces, and repositories in Crucible.

## Definitions

| Term | Definition | Example |
|------|------------|---------|
| **Project** | A directory registered in the daemon for session grouping | `/home/user/my-app` |
| **Workspace** | The working directory for a session (may equal project path) | Session's `workspace` field |
| **Repository** | A git repo that may contain one or more projects/worktrees | `.git` or linked worktree |
| **Kiln** | A knowledge base (notes directory) attached to a project | `./notes`, `.crucible/` |

## Hierarchy

```
Repository (git)
├── Project (registered directory)
│   ├── Workspace (session working dir)
│   └── Kilns (attached knowledge bases)
└── Worktrees (linked checkouts)
    └── Project (auto-grouped by repo)
```

## Project Registration

Projects are auto-registered when:
1. A session is created with a workspace path
2. Explicitly via `project.register` RPC

Registration detects:
- **SCM info**: Git repository root, remote URL, worktree status
- **Metadata**: Name from `.crucible/workspace.toml` or directory name
- **Kilns**: Attached knowledge bases from workspace config

## Repository Grouping

When a project is inside a git repository:
- `repository.root` points to the repo root
- `repository.remote_url` captures the origin remote (if any)
- `repository.is_worktree` indicates if this is a linked worktree
- `repository.main_repo_git_dir` links worktrees to their main repo

Multiple projects can share the same repository ID, enabling UI grouping.

## Session-Project Relationship

```
Session
├── kiln: PathBuf        # Owning knowledge base
├── workspace: PathBuf   # Working directory (→ Project)
└── connected_kilns      # Additional kilns for queries
```

When a session is created:
1. The workspace path is canonicalized
2. If not already registered, a new Project is created
3. SCM detection runs (git repository, worktree status)
4. Project is persisted to `~/.crucible/projects.json`

## Configuration

```toml
[scm]
enabled = true           # Enable SCM detection (default: true)
detect_worktrees = false # Group worktrees under main repo (default: false)
```

## Storage

- **Projects**: `~/.crucible/projects.json`
- **Sessions**: `{kiln}/.crucible/sessions/{session_id}/`

## Related Concepts

- [[Help/Concepts/Kilns]] — Knowledge bases
- [[Meta/Systems]] — System boundaries
