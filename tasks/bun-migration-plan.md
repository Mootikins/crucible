---
type: task
tags: [migration, bun, package-manager, infrastructure]
created: 2025-10-14
status: planning
title: Bun Migration Plan
---

# Bun Migration Plan

## Current State Analysis

### Current Package Manager: pnpm
- **Root workspace**: Uses pnpm workspaces with filters
- **Packages**: 3 main packages (root, packages/web, packages/shared, packages/web/vimflowy, packages/obsidian-plugin)
- **Lock file**: `pnpm-lock.yaml`
- **Scripts**: Heavily use `pnpm --filter` for workspace operations
- **Engines**: Node.js >=20.0.0, pnpm >=8.0.0

### Key Dependencies & Scripts

**Root package.json**:
- Scripts: `dev`, `build`, `test`, `lint` using pnpm filters
- Uses `pnpm.dl x husky install` for git hooks
- `onlyBuiltDependencies` config for esbuild

**packages/web** (SvelteKit):
- Vite-based build system
- Vitest for testing
- TypeScript, Svelte, Tailwind CSS
- Dependencies: file-saver, jquery, katex, localforage, lodash

**packages/shared** (TypeScript library):
- Simple TypeScript build
- Uses tsc with --watch for dev

**packages/web/vimflowy** (Legacy React):
- Older React Scripts setup
- Node-sass (deprecated)
- Many legacy dependencies

**packages/obsidian-plugin**:
- Obsidian plugin development
- TypeScript-based

## Migration Strategy

### Phase 1: Preparation & Backup
1. **Backup current state**
   - Commit all current changes
   - Tag current pnpm state: `git tag pnpm-state-$(date +%Y%m%d)`

2. **Install Bun**
   - Install Bun globally: `curl -fsSL https://bun.sh/install | bash`
   - Verify installation: `bun --version`

### Phase 2: Core Migration
1. **Update root package.json**
   - Replace `pnpm` with `bun` in all scripts
   - Update engines section
   - Remove pnpm-specific configuration

2. **Migrate workspace configuration**
   - Convert pnpm workspace syntax to bun workspaces
   - Update filter commands to equivalent bun syntax

3. **Convert lock file**
   - Remove `pnpm-lock.yaml`
   - Generate `bun.lockb`: `bun install`

### Phase 3: Package Updates

#### Root Package Changes:
```json
{
  "scripts": {
    "dev": "bun run dev:rust & bun run dev:js",
    "build": "bun run build:rust && bun run build:js",
    "build:rust": "cargo build --release",
    "build:js": "bun run build:packages",
    "build:packages": "bun run build:shared && bun run build:web",
    "test": "bun run test:rust && bun run test:js",
    "test:rust": "cargo test",
    "test:js": "bun run test:shared && bun run test:web",
    "setup": "./scripts/setup.sh"
  },
  "engines": {
    "bun": ">=1.0.0"
  },
  "workspaces": [
    "packages/*",
    "packages/web/*"
  ]
}
```

#### Script Updates:
- `pnpm --filter desktop dev` → `bun --cwd packages/crucible-tauri dev`
- `pnpm --filter '*' build` → `bun run build:packages`
- `pnpm --filter '*' test` → `bun run test:packages`

### Phase 4: Dependency Resolution
1. **Handle problematic dependencies**
   - **node-sass** → Replace with `sass` (Dart Sass)
   - **esbuild** → Should work fine with Bun
   - **Legacy packages** → Verify compatibility

2. **Update scripts/setup.sh**
   ```bash
   #!/usr/bin/env bash
   set -e

   echo "🔥 Setting up Crucible development environment..."

   # Check requirements
   command -v cargo >/dev/null 2>&1 || { echo "Rust is required. Install from https://rustup.rs"; exit 1; }
   command -v bun >/dev/null 2>&1 || { echo "Bun is required. Install from https://bun.sh"; exit 1; }

   # Install Rust dependencies
   echo "📦 Installing Rust dependencies..."
   cargo fetch

   # Install JS dependencies
   echo "📦 Installing JavaScript dependencies..."
   bun install

   # Setup git hooks (Bun doesn't have dlx, use npm or install globally)
   echo "🔗 Setting up git hooks..."
   bunx husky install || npm i -g husky && husky install

   # Build core crates
   echo "🔨 Building core crates..."
   cargo build --workspace

   echo "✅ Setup complete! Run 'bun dev' to start developing."
   ```

3. **Update other scripts**
   - `scripts/build.sh`: Replace `pnpm build` with `bun run build:packages`
   - `scripts/dev.sh`: No changes needed (uses cargo tauri)

### Phase 5: Testing & Validation
1. **Dependency installation test**
   ```bash
   rm -rf node_modules packages/*/node_modules
   bun install
   ```

2. **Build test**
   ```bash
   bun run build
   ```

3. **Development test**
   ```bash
   bun run dev
   ```

4. **Test suite**
   ```bash
   bun run test
   ```

### Phase 6: Performance Optimization
1. **Leverage Bun features**
   - Use `bun run` for faster script execution
   - Consider `bun test` for faster test runs (if compatible with Vitest)
   - Utilize Bun's built-in bundler for simpler builds

2. **Update CI/CD**
   - Replace pnpm with bun in GitHub Actions
   - Update Docker containers to include Bun

## Migration Risks & Mitigations

### High Risk Areas:
1. **Legacy vimflowy package**: Old React Scripts and node-sass
   - **Mitigation**: Plan separate modernization or containerize

2. **Husky git hooks**: Bun doesn't have `dlx` equivalent
   - **Mitigation**: Use `bunx` or install husky globally

3. **Workspace filtering**: pnpm's `--filter` is powerful
   - **Mitigation**: Create specific scripts for each workspace

### Medium Risk:
1. **Dependency compatibility**: Some packages may not support Bun
   - **Mitigation**: Test each package individually, use npm fallback if needed

2. **Build tools**: Vite, TypeScript, etc. compatibility
   - **Mitigation**: Most modern tools support Bun, but verify

## Expected Benefits
- **Faster installs**: Bun's installer is significantly faster
- **Reduced disk space**: Bun uses less space than pnpm
- **Built-in bundler**: Potential to replace some build tools
- **Better TypeScript support**: Native TypeScript execution

## Timeline Estimate
- **Phase 1-2**: 1-2 hours (setup and core migration)
- **Phase 3-4**: 2-4 hours (package updates and dependency resolution)
- **Phase 5-6**: 2-3 hours (testing and optimization)
- **Total**: 5-9 hours

## Rollback Plan
If migration fails:
1. Git checkout to `pnpm-state-` tag
2. Restore `pnpm-lock.yaml` if needed
3. Run `pnpm install` to restore dependencies
4. Document any issues found

## Next Steps
1. Review and approve this plan
2. Create a dedicated branch for migration
3. Begin Phase 1 execution
4. Update documentation after successful migration