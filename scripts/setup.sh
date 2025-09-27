#!/usr/bin/env bash
set -e

echo "🔥 Setting up Crucible development environment..."

# Check requirements
command -v cargo >/dev/null 2>&1 || { echo "Rust is required. Install from https://rustup.rs"; exit 1; }
command -v pnpm >/dev/null 2>&1 || { echo "pnpm is required. Install with: npm i -g pnpm"; exit 1; }
command -v node >/dev/null 2>&1 || { echo "Node.js 20+ is required."; exit 1; }

# Install Rust dependencies
echo "📦 Installing Rust dependencies..."
cargo fetch

# Install JS dependencies
echo "📦 Installing JavaScript dependencies..."
pnpm install

# Setup git hooks
echo "🔗 Setting up git hooks..."
pnpm dlx husky install

# Build core crates
echo "🔨 Building core crates..."
cargo build --workspace

echo "✅ Setup complete! Run 'pnpm dev' to start developing."

