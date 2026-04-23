---
type: workflow
title: Basic Fixture Workflow
description: Covers goals, validation, gates, data flow, and nested steps.
---

## Goals

- [ ] Ship the feature
- [x] Document the feature

## Validation

- `cargo test --workspace` passes
- `cargo clippy --all-targets` clean
- Manual: happy path runs end-to-end

> [!gate]
> Leadership approval before kickoff

## Plan -> plan

Draft the approach from the goals.

## Build @developer

Implement per **plan**.

### Refactor Existing Code

### Add New Module

## Review and Deploy [type:: fan]

> [!gate]
> Staging sign-off

### Code Review @reviewer

### Smoke Test @qa
