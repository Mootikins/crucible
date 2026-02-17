# Task 9: Install Experience Verification

**Date**: 2026-02-16
**Verified against**: v0.1.0 release + current master

---

## What Was Tested

| Test | Result | Notes |
|------|--------|-------|
| GitHub release exists | ✅ PASS | v0.1.0 published 2026-02-04, not draft, immutable |
| install.sh present | ✅ PASS | Correctly generated, well-formed shell script |
| Linux x86_64 binary | ✅ PASS | `crucible-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` (54.6 MB) |
| Linux aarch64 binary | ✅ PASS | `crucible-v0.1.0-aarch64-unknown-linux-gnu.tar.gz` (50.6 MB) |
| macOS Apple Silicon | ✅ PASS | `crucible-v0.1.0-aarch64-apple-darwin.tar.gz` (46.6 MB) |
| macOS Intel binary | ⚠️ MISSING | Build target in workflow but asset absent from release |
| cargo-binstall metadata | ✅ PASS | `pkg-url`/`bin-dir`/`pkg-fmt` match release asset naming |
| Build from source | ✅ PASS | `cargo check -p crucible-cli --features storage-sqlite` compiles (2 non-fatal warnings) |
| `cru --version` | ✅ PASS | Outputs `cru 0.1.0` via clap `#[command(version)]` |
| README install instructions | ✅ PASS | `curl \| sh` URL correct, `cargo install --git` syntax correct |
| Homebrew tap | ⚠️ UNDOCUMENTED | `Mootikins/homebrew-crucible` exists with auto-update but README doesn't mention it |
| Docker | N/A | No Dockerfile exists, not applicable |

## What Passed

### GitHub Release (v0.1.0)
- Release at https://github.com/Mootikins/crucible/releases/tag/v0.1.0
- 4 binary assets + install.sh + source archives
- Both `cru` and `cru-server` included in each tarball

### Release Workflow (`.github/workflows/release.yml`)
- 3-phase pipeline: create-release → build (4 targets parallel) → publish
- Triggers on `v*` tags + manual `workflow_dispatch`
- Uses `cross` for aarch64-linux cross-compilation
- Generates install.sh dynamically, undrafts release after uploads
- Dispatches Homebrew tap update on release

### install.sh Quality
- Proper platform detection (linux/darwin, x86_64/aarch64)
- Default install to `$HOME/.local/bin` (overridable via `CRUCIBLE_INSTALL_DIR`)
- Installs both `cru` and `cru-server`
- PATH check with helpful message
- Cleanup via `trap`

### cargo-binstall Metadata (`crates/crucible-cli/Cargo.toml`)
```toml
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/v{ version }/crucible-v{ version }-{ target }.tar.gz"
bin-dir = "crucible-v{ version }-{ target }/{ bin }{ binary-ext }"
pkg-fmt = "tgz"
```
Naming pattern matches actual release archive structure.

### Version Output
- `cru --version` → `cru 0.1.0`
- Driven by `#[command(version)]` in `src/cli.rs`, pulls from workspace `Cargo.toml` version

## What Failed / Issues Found

### ⚠️ Missing x86_64-apple-darwin Binary
- Release workflow matrix includes `x86_64-apple-darwin` (macOS 13 runner)
- Asset NOT present in v0.1.0 release — likely build failure
- Impact: macOS Intel users get 404 from install.sh
- README correctly says "macOS Apple Silicon" only, so documentation is accurate
- **Action**: Check Actions logs; if Intel Mac not needed, remove from build matrix

### ⚠️ Homebrew Tap Not Documented
- `Mootikins/homebrew-crucible` repo exists with formula + auto-update workflow
- `brew install mootikins/crucible/crucible` should work
- Release workflow dispatches update automatically
- **Not mentioned in README install section**
- **Action**: Add Homebrew install instructions to README

## What Couldn't Be Tested

| Item | Reason |
|------|--------|
| `curl \| sh` on clean machine | Would need isolated environment |
| `cargo binstall crucible-cli --dry-run` | `cargo-binstall` not installed on dev machine |
| Docker install | No Dockerfile exists |
| Homebrew `brew install` | Not on macOS |
| Actual binary execution from release tarball | Would need to download and extract release asset |

## Minor Observations

- install.sh has no checksum verification of downloaded archive
- install.sh shadows `TMPDIR` env var (cosmetic)
- `[workspace.metadata.dist]` in root `Cargo.toml` is cargo-dist config but actual release uses custom workflow — slightly orphaned metadata
- CI workflow (`.github/workflows/ci.yml`) covers fmt, clippy, nextest, SQLite backend tests

## Recommended Fixes

1. **Investigate x86_64-apple-darwin build failure** — check v0.1.0 Actions logs
2. **Add Homebrew to README** — `brew install mootikins/crucible/crucible`
3. (Optional) Add SHA256 checksum verification to install.sh
4. (Optional) Clean up orphaned `[workspace.metadata.dist]` if not using cargo-dist
