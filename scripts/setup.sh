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

# Setup git hooks
echo "🔗 Setting up git hooks..."
bunx husky install || npm i -g husky && husky install

# Build core crates
echo "🔨 Building core crates..."
cargo build --workspace

echo "✅ Setup complete! Run 'bun dev' to start developing."

