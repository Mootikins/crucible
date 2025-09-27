#!/usr/bin/env bash
set -e

echo "🔨 Building Crucible..."

# Build Rust crates
echo "Building Rust crates..."
cargo build --release

# Build JavaScript packages
echo "Building JavaScript packages..."
pnpm build

echo "✅ Build complete!"

