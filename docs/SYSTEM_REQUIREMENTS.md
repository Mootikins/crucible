# System Requirements

> **Status**: Active System Requirements
> **Version**: 1.0.0
> **Date**: 2025-10-23
> **Purpose**: Hardware and software requirements for running Crucible

## Overview

Crucible is designed to run efficiently on modern systems while providing powerful knowledge management capabilities. This document outlines the minimum and recommended requirements for different use cases.

## Table of Contents

- [Minimum Requirements](#minimum-requirements)
- [Recommended Requirements](#recommended-requirements)
- [Platform-Specific Requirements](#platform-specific-requirements)
- [Component Requirements](#component-requirements)
- [Performance Considerations](#performance-considerations)
- [Network Requirements](#network-requirements)
- [Storage Requirements](#storage-requirements)
- [Compatibility Notes](#compatibility-notes)

## Minimum Requirements

### Hardware

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| **CPU** | 2 cores, 64-bit | 4+ cores, 64-bit |
| **Memory (RAM)** | 4 GB | 8 GB+ |
| **Storage** | 1 GB free space | 5 GB+ free space |
| **Network** | Broadband connection | Broadband connection |

### Software

| Component | Minimum Version | Recommended Version |
|-----------|----------------|-------------------|
| **Operating System** | See platform-specific | Latest stable |
| **Rust** | 1.70.0 | 1.75.0+ |
| **Node.js** | 18.0.0 | 20.0.0+ |
| **Git** | 2.30.0 | 2.40.0+ |

## Recommended Requirements

### For Personal Use

- **CPU**: 4 cores, 2.5 GHz+
- **Memory**: 8 GB RAM
- **Storage**: 5 GB SSD storage
- **Network**: Stable broadband connection

### For Team Collaboration

- **CPU**: 6+ cores, 3.0 GHz+
- **Memory**: 16 GB+ RAM
- **Storage**: 20 GB+ SSD storage
- **Network**: High-speed broadband (100 Mbps+)

### For Development

- **CPU**: 8+ cores, 3.5 GHz+
- **Memory**: 32 GB+ RAM
- **Storage**: 50 GB+ NVMe SSD
- **Network**: High-speed broadband

## Platform-Specific Requirements

### Linux

#### Supported Distributions

| Distribution | Minimum Version | Recommended Version |
|--------------|----------------|-------------------|
| **Ubuntu** | 20.04 LTS | 22.04 LTS or later |
| **Debian** | 11 | 12 (Bookworm) or later |
| **Fedora** | 36 | 39 or later |
| **Arch Linux** | Rolling release | Current |
| **openSUSE** | Leap 15.4 | Tumbleweed |

#### System Dependencies

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential pkg-config libssl-dev sqlite3 libsqlite3-dev

# Fedora
sudo dnf install gcc gcc-c++ pkgconfig openssl-devel sqlite sqlite-devel

# Arch Linux
sudo pacman -S base-devel pkgconf sqlite
```

#### Kernel Requirements

- **Minimum**: Linux 5.10
- **Recommended**: Linux 6.1 or later

### macOS

#### Supported Versions

| Version | Minimum | Recommended |
|---------|---------|-------------|
| **macOS** | 11.0 (Big Sur) | 13.0 (Ventura) or later |
| **Xcode** | 12.0 | 15.0 or later |

#### System Dependencies

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Homebrew (recommended)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies via Homebrew
brew install openssl sqlite
```

#### Architecture Support

- **Intel**: x86_64 (Intel-based Macs)
- **Apple Silicon**: arm64 (M1/M2/M3 Macs)

### Windows

#### Supported Versions

| Version | Status |
|---------|--------|
| **Windows 10** | Limited support |
| **Windows 11** | Development in progress |
| **Windows Server** | Not currently supported |

#### Requirements (Development in Progress)

- **Windows 10** version 1903 or later
- **Windows Subsystem for Linux 2 (WSL2)** recommended
- **Visual Studio Build Tools** or **Visual Studio 2019+**

#### WSL2 Setup (Recommended)

```powershell
# Install WSL2
wsl --install

# Install Ubuntu distribution
wsl --install -d Ubuntu

# Setup Rust in WSL2
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Component Requirements

### Database Systems

#### SurrealDB

| Requirement | Minimum | Recommended |
|-------------|---------|-------------|
| **Memory** | 512 MB | 2 GB+ |
| **Storage** | 100 MB | 1 GB+ |
| **CPU** | 1 core | 2+ cores |

### AI/ML Components

#### Local Embeddings

| Model Size | Memory Required | Storage Required | Performance |
|------------|----------------|------------------|-------------|
| **Small** (e.g., MiniLM) | 1-2 GB | 500 MB | Fast |
| **Medium** (e.g., Sentence-BERT) | 2-4 GB | 1 GB | Balanced |
| **Large** (e.g., BERT-Large) | 8+ GB | 2 GB+ | High Quality |

#### External AI Services

| Service | Network | API Requirements |
|---------|---------|------------------|
| **OpenAI** | Internet | API key required |
| **Ollama** | Local/Network | Local installation |
| **Cohere** | Internet | API key required |

### Search and Indexing

| Kiln Size | Memory Required | Indexing Time | Search Performance |
|------------|----------------|---------------|-------------------|
| **Small** (< 1,000 files) | 512 MB | < 1 minute | Excellent |
| **Medium** (1,000-10,000 files) | 1-2 GB | 1-5 minutes | Good |
| **Large** (10,000+ files) | 2-4 GB+ | 5+ minutes | Good |

## Performance Considerations

### CPU Performance

- **Single-threaded**: Most operations are single-threaded
- **Multi-core**: Benefits from parallel indexing and search
- **Vector operations**: Benefit from SIMD instructions

### Memory Usage

#### Base Memory Usage

| Component | Typical Usage |
|-----------|---------------|
| **Core Application** | 100-200 MB |
| **Database** | 50-200 MB |
| **Search Index** | 100-500 MB |
| **CLI/REPL** | 50-100 MB |

#### Memory Scaling

- **Linear scaling** with kiln size for search indexing
- **Embedding models** add 1-8 GB depending on model size
- **Concurrent operations** increase memory usage proportionally

### Storage Performance

#### SSD vs HDD

| Operation | SSD | HDD |
|-----------|-----|-----|
| **Database Operations** | Excellent | Poor |
| **Search Indexing** | Excellent | Poor |
| **File Operations** | Good | Fair |
| **Recommended** | ✅ Required for good performance | ❌ Not recommended |

#### Storage Space Requirements

| Data Type | Space per Item | Example (1000 items) |
|-----------|----------------|----------------------|
| **Text Notes** | 10-50 KB | 10-50 MB |
| **Documents** | 100 KB - 5 MB | 100 MB - 5 GB |
| **Embeddings** | 1-4 KB | 1-4 MB |
| **Search Index** | 2-10 KB | 2-10 MB |

## Network Requirements

### Local Usage

- **No network required** for core functionality
- **Local search** works offline
- **Database operations** are local

### AI/ML Features

| Feature | Network Required | Bandwidth | Latency |
|---------|------------------|-----------|---------|
| **Local Models** | No | N/A | N/A |
| **Cloud APIs** | Yes | 1-10 MB/hour | < 500 ms |
| **Real-time Collaboration** | Yes | 1-5 MB/hour | < 100 ms |

### Update and Installation

- **Initial download**: 100-500 MB
- **Updates**: 10-100 MB
- **Dependency downloads**: 200-500 MB

## Storage Requirements

### Installation Storage

| Component | Space Required |
|-----------|----------------|
| **Rust Toolchain** | 1-2 GB |
| **Node.js/npm** | 500 MB - 1 GB |
| **Crucible Binaries** | 100-500 MB |
| **Dependencies** | 500 MB - 1 GB |
| **Total** | **2-4 GB** |

### Runtime Storage

| Kiln Size | Storage Required | Index Size |
|------------|------------------|------------|
| **Small** (1 GB) | 1.5-2 GB | 50-100 MB |
| **Medium** (10 GB) | 12-15 GB | 500 MB - 1 GB |
| **Large** (100 GB) | 120-150 GB | 2-5 GB |

### Temporary Storage

- **Build artifacts**: 1-5 GB during compilation
- **Temporary files**: 100-500 MB during operation
- **Logs**: 10-100 MB depending on usage
- **Cache**: 100 MB - 1 GB (configurable)

## Compatibility Notes

### Known Limitations

#### Platform Limitations

- **Windows**: Limited support, WSL2 recommended
- **ARM32**: Not supported (requires 64-bit)
- **32-bit systems**: Not supported

#### Resource Limitations

- **Memory**: May not work well on systems with < 4 GB RAM
- **Storage**: SSD strongly recommended for good performance
- **Network**: Some features require internet connectivity

### Browser Compatibility (Future)

For future web interface:

| Browser | Minimum Version | Recommended Version |
|---------|----------------|-------------------|
| **Chrome** | 90 | 120+ |
| **Firefox** | 88 | 115+ |
| **Safari** | 14 | 17+ |
| **Edge** | 90 | 120+ |

### Database Compatibility

#### Supported File Systems

| File System | Support | Notes |
|-------------|----------|-------|
| **ext4** | ✅ Excellent | Linux default |
| **APFS** | ✅ Excellent | macOS default |
| **NTFS** | ⚠️ Limited | Windows/WSL2 |
| **Btrfs** | ✅ Good | Linux |
| **ZFS** | ✅ Good | Available on Linux/macOS |

#### Character Encoding

- **UTF-8**: Required for all text files
- **Legacy encodings**: May cause issues with search and indexing

## Testing Your System

### System Check Script

```bash
#!/bin/bash
# System requirements check script

echo "=== Crucible System Requirements Check ==="

# Check OS
echo "Operating System: $(uname -s) $(uname -r)"

# Check CPU
echo "CPU cores: $(nproc)"
echo "CPU architecture: $(uname -m)"

# Check Memory
TOTAL_MEM=$(free -h | awk '/^Mem:/ {print $2}')
echo "Total memory: $TOTAL_MEM"

# Check Disk Space
DISK_SPACE=$(df -h . | awk 'NR==2 {print $4}')
echo "Available disk space: $DISK_SPACE"

# Check Rust
if command -v rustc &> /dev/null; then
    RUST_VERSION=$(rustc --version)
    echo "Rust version: $RUST_VERSION"
else
    echo "❌ Rust not found"
fi

# Check Node.js
if command -v node &> /dev/null; then
    NODE_VERSION=$(node --version)
    echo "Node.js version: $NODE_VERSION"
else
    echo "❌ Node.js not found"
fi

# Check Git
if command -v git &> /dev/null; then
    GIT_VERSION=$(git --version)
    echo "Git version: $GIT_VERSION"
else
    echo "❌ Git not found"
fi

echo "=== Check Complete ==="
```

### Performance Benchmark

```bash
# Quick performance test
time crucible-cli search "test" --limit 10

# Memory usage check
/usr/bin/time -v crucible-cli index --rebuild

# Disk speed test
dd if=/dev/zero of=tempfile bs=1M count=100 && rm tempfile
```

## Upgrade Recommendations

### From Minimum to Recommended

1. **Memory Upgrade**: 4 GB → 8 GB RAM
   - Improves search performance
   - Allows larger embedding models
   - Better multitasking

2. **Storage Upgrade**: HDD → SSD
   - Dramatically improves database operations
   - Faster search indexing
   - Better overall responsiveness

3. **CPU Upgrade**: 2 cores → 4+ cores
   - Parallel indexing operations
   - Better concurrent performance
   - Improved AI/ML processing

### Scaling for Teams

- **Server-class hardware** for team deployments
- **Network-attached storage** for shared kilns
- **Load balancing** for concurrent users
- **Backup systems** for data protection

---

*For the latest requirements and updates, check the [Crucible GitHub repository](https://github.com/matthewkrohn/crucible).*

*Last updated: 2025-10-23*