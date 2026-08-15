# Contributing to Crucible

Thanks for your interest in Crucible. This document covers setup, the test system, and
the conventions a pull request is reviewed against.

[AGENTS.md](./AGENTS.md) is the binding architecture and style guide — this file does not
repeat it. Read it before a non-trivial change.

## Getting Started

1. **Fork the repository** and clone your fork.
2. **Install Rust** (stable toolchain): <https://rustup.rs/>
3. **Install [`just`](https://github.com/casey/just)**: `cargo install just`. Every check
   in this document is a `just` recipe; the recipes encode build-parallelism caps that
   keep a full workspace build from exhausting the machine.
4. **Install the rest of the prerequisites**: `just setup`.
5. **Build**: `just build`
6. **Test**: `just ci`

### Prerequisites

`just setup` is the bootstrap. It installs the cargo tools, the web dependencies, and the
Playwright browser, and it tells you the exact command for the system packages it cannot
install for you:

| Tool | Needed for | `just setup` |
|------|-----------|--------------|
| [`bun`](https://bun.sh) | the SolidJS web frontend — **not** npm or yarn | reports it, prints the install command |
| `jq` | the justfile resolves the cargo target directory with it | not checked — install it yourself |
| `cargo-nextest` | the Rust test runner; `cargo test` is not the supported path | installs |
| `cargo-deny` | dependency licence gate (`just lint license`) | installs |
| Playwright chromium | web E2E tests (`just web-test`) | installs |

Without `jq`, any recipe that runs the built
`cru` binary — including `just ci`, via `test plugins` — fails.

If Playwright reports missing system libraries, run `bunx playwright install-deps
chromium` yourself; `just setup` deliberately does not, because it shells out to `sudo`.

## Development Workflow

### Before You Start

- Check existing [issues](https://github.com/Mootikins/crucible/issues) to avoid
  duplicate work.
- For large changes, open an issue first to discuss the approach.
- Read [AGENTS.md](./AGENTS.md) for the architecture, crate layout, and terminology
  (`project` / `kiln` / `workspace` are distinct terms and are not interchangeable).

### Making Changes

1. Create a feature branch from `master`.
2. Make your changes with clear, focused commits.
3. Write tests for new functionality; bugfixes start with a failing test.
4. Run `just ci` before pushing — the same gates GitHub runs; `just --show ci` lists
   them.

Do not run bare `cargo build --workspace` / `cargo test --workspace`. The justfile caps
`CARGO_BUILD_JOBS`, because `rust-lld` peaks around 7 GB per link job and an uncapped
workspace build runs roughly 30 of them at once. Use the recipes.

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:** `feat`, `fix`, `docs`, `refactor`, `test`, `chore`.

**Examples:**

```
feat(parser): add support for nested wikilinks
fix(cli): correct path handling on Windows
docs: update installation instructions
```

## Testing

Tests run under **cargo-nextest**.

### Running tests

```bash
just ci                                   # everything CI runs — do this before pushing
just test                                 # all non-#[ignore]d tests
just test ignored                         # only the #[ignore]d tests
just test full                            # both
just test -p crucible-core                # one crate (anything after the tier goes to nextest)
just test -p crucible-core -E 'test(wikilink)'  # a filtered subset of one crate
just test doc                             # doctests (nextest cannot run these)
just test plugins                         # every shipped plugin's Lua suite
just web-test unit                        # web unit tests (Vitest)
just web-test                             # web E2E tests (Playwright)
just lint types                           # web typecheck, no emit
just lint docs                            # validate the docs/ kiln (also run by `just ci`)
```

`just test` is the tier a contributor needs to pass with no external services running.
Tests that want a live LLM endpoint read `.env.local` at the repo root — copy
`.env.local.example` and edit it. Without that file they print `SKIPPED` and pass, so the
tier stays green on a machine with nothing configured.

How tests are gated, mocked, kept hermetic, and reviewed is the Testing section of
[AGENTS.md](./AGENTS.md).

## Code Style

Format with `just fmt`; the rules a review applies — error handling, naming, type
ownership, when a trait is justified — are the Code Principles section of
[AGENTS.md](./AGENTS.md).

## Pull Request Process

1. **Title**: conventional commit format.
2. **Description**: what changed and why.
3. **CI**: all checks must pass.
4. **Review**: address feedback.

The pull request template carries the pre-merge checklist.

## Reporting Issues

Use the [issue templates](https://github.com/Mootikins/crucible/issues/new/choose). Bug
reports need the `cru` version, your OS, steps to reproduce, and what you expected
instead.

**Security vulnerabilities do not go in the issue tracker.** See
[SECURITY.md](./SECURITY.md) for the private reporting channel.

## Questions?

- [Discussions](https://github.com/Mootikins/crucible/discussions) for general questions
- [AGENTS.md](./AGENTS.md) for architecture questions

## License

By contributing to Crucible, you agree that your contributions will be licensed under the
MIT License or Apache License 2.0, at your option.
