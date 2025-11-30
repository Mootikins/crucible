# Multi-Kiln Architecture: Security Boundaries & Collaboration

## 🎯 Vision: Knowledge Bases as Security Boundaries

Crucible supports **multiple independent kilns** (knowledge bases), each with its own security tier, sharing policy, and backend preferences. This enables:

1. **Security isolation** - Separate sensitive/public knowledge
2. **Team collaboration** - Share kilns via git/Syncthing/etc.
3. **Monorepo integration** - Embed kilns in code repositories
4. **Flexible access control** - Per-kiln permissions and backends

---

## 🏗️ Architecture Overview

### What is a Kiln?

A **kiln** is a self-contained knowledge base:
```
my-kiln/
├── notes/              # Markdown notes
├── .crucible/          # Kiln configuration
│   ├── config.toml     # Kiln-specific settings
│   ├── agents/         # Kiln-specific agents
│   ├── sessions/       # Agent session history
│   └── security.toml   # Security policy for this kiln
└── README.md           # Kiln documentation
```

### Multi-Kiln System

Users can have **multiple active kilns**, each isolated:
```
~/.config/crucible/
├── config.toml         # Global Crucible config
├── kilns.toml          # Registry of all kilns
└── security.toml       # Global security policy

~/work/
├── company-internal/   # Private company kiln (High tier)
├── personal-notes/     # Personal kiln (Medium tier)
└── oss-project/        # Public OSS kiln (Public tier)

~/code/
└── my-app/
    └── .crucible/      # Embedded kiln in code repo
```

---

## 🔐 Security Isolation

### Per-Kiln Security Tiers

Each kiln has an **inherent security tier**:

```toml
# ~/work/company-internal/.crucible/security.toml
[kiln]
name = "company-internal"
security_tier = "high"              # Sensitive internal data
owner = "company"
allowed_backends = ["ollama", "copilot"]
data_classification = "proprietary"

[backends.default]
type = "ollama"
endpoint = "http://localhost:11434"
model = "deepseek-coder:33b"

[backends.fallback]
type = "copilot"
model = "claude-sonnet-4.5"
requires_approval = true            # Ask before using Copilot

[sharing]
enabled = false                     # Never share this kiln
git_sync = false                    # Don't sync to public repos
allowed_collaborators = ["team@company.com"]
```

```toml
# ~/work/oss-project/.crucible/security.toml
[kiln]
name = "oss-project"
security_tier = "public"            # Open source, public data
owner = "personal"
allowed_backends = ["anthropic", "openai", "copilot", "ollama"]
data_classification = "public"

[backends.default]
type = "anthropic"                  # Can use best quality
model = "claude-sonnet-4.5"

[sharing]
enabled = true
git_sync = true
public_repo = "https://github.com/user/oss-project"
license = "MIT"
```

### Security Boundaries Between Kilns

**Kilns are isolated:**
- ✅ Separate databases (if using SurrealDB)
- ✅ Separate embedding indexes
- ✅ Separate agent sessions
- ✅ **No cross-kiln context bleeding**

```rust
// In crucible-core
impl KilnManager {
    /// Load a kiln with security validation
    pub async fn load_kiln(&mut self, path: &Path) -> Result<KilnHandle> {
        // 1. Load kiln configuration
        let config = KilnConfig::load(path)?;

        // 2. Validate against global security policy
        let global_policy = self.global_policy();
        if !global_policy.allows_kiln(&config) {
            return Err(SecurityError::KilnBlocked {
                kiln: config.name.clone(),
                reason: "Violates global security policy"
            });
        }

        // 3. Create isolated context
        let context = KilnContext {
            path: path.to_path_buf(),
            security_tier: config.security_tier,
            allowed_backends: config.allowed_backends.clone(),
            storage: self.create_isolated_storage(path)?,
            embeddings: self.create_isolated_embeddings(path)?,
        };

        // 4. Return handle with enforced boundaries
        Ok(KilnHandle::new(context))
    }
}
```

---

## 🤝 Collaboration Patterns

### Pattern 1: Git-Synced Kiln (OSS/Team)

```bash
# Team member 1: Create and share
cd ~/work/team-docs
git init
cru kiln init --name "team-docs" --tier public

# Add some notes
cru note create onboarding.md
git add . && git commit -m "Add onboarding docs"
git push origin main

# Team member 2: Clone and use
git clone https://github.com/team/team-docs
cd team-docs
cru kiln register .
cru chat  # Works immediately!
```

**Benefits:**
- ✅ Version control for knowledge
- ✅ Team collaboration
- ✅ Conflict resolution via git
- ✅ Public/private repo support

### Pattern 2: Syncthing/Dropbox Shared Kiln

```bash
# User 1: Create kiln in synced folder
cd ~/Syncthing/shared-research
cru kiln init --name "research" --tier medium

# Auto-syncs via Syncthing

# User 2: Register synced kiln
cru kiln register ~/Syncthing/shared-research
cru kiln list
# ✓ research (medium tier, synced)
```

**Benefits:**
- ✅ Real-time sync (no manual commits)
- ✅ Works offline
- ✅ Cross-platform (Linux/Mac/Windows)
- ✅ E2E encryption (with Syncthing)

### Pattern 3: Monorepo Embedded Kiln

```bash
# In your codebase
my-app/
├── src/
├── tests/
└── .crucible/          # Embedded kiln
    ├── agents/
    │   └── code-reviewer.md
    ├── notes/
    │   ├── architecture.md
    │   └── decisions/
    └── config.toml

# Usage
cd ~/code/my-app
cru kiln detect           # Auto-detects .crucible/
cru agent run code-reviewer --pr 123
```

**Benefits:**
- ✅ Code + docs in one place
- ✅ Versioned with code
- ✅ Per-project agents
- ✅ Team knowledge sharing

### Pattern 4: Personal Multi-Kiln Setup

```toml
# ~/.config/crucible/kilns.toml
[[kilns]]
name = "work"
path = "~/work/company-internal"
security_tier = "high"
auto_load = true
default = true              # Use when in ~/work/**

[[kilns]]
name = "personal"
path = "~/notes/personal"
security_tier = "medium"
auto_load = true
default_for = ["~/projects/**", "~/home/**"]

[[kilns]]
name = "oss"
path = "~/notes/oss"
security_tier = "public"
auto_load = false           # Load manually
sync_url = "https://github.com/user/notes"

[[kilns]]
name = "learning"
path = "~/notes/learning"
security_tier = "low"
auto_load = true
default_for = ["~/courses/**"]
```

---

## 🔄 Kiln Management

### Registering Kilns

```bash
# Register a kiln
cru kiln register ~/work/company-internal

# Initialize new kiln
cru kiln init --name "my-notes" --path ~/notes --tier medium

# List all kilns
cru kiln list
# ✓ work (high tier, ~/work/company-internal)
# ✓ personal (medium tier, ~/notes/personal)
# ✓ oss (public tier, ~/notes/oss)

# Show kiln details
cru kiln info work
# Name: work
# Path: ~/work/company-internal
# Tier: high
# Backend: Ollama (deepseek-coder:33b)
# Notes: 1,234
# Last updated: 2025-01-15 14:30:00
```

### Switching Between Kilns

```bash
# Explicit kiln selection
cru chat --kiln work
cru search "authentication" --kiln oss

# Auto-detection based on working directory
cd ~/work/company-internal
cru chat  # Automatically uses "work" kiln

cd ~/notes/oss
cru chat  # Automatically uses "oss" kiln

# Override auto-detection
cd ~/work/company-internal
cru chat --kiln personal  # Use personal kiln instead
```

### Cross-Kiln Operations (Controlled)

```bash
# Search across multiple kilns (respects security boundaries)
cru search "docker" --kilns work,oss,personal

# Results grouped by kiln:
# work (high tier):
#   - deployment/docker.md (similarity: 0.95)
# oss (public tier):
#   - docker-setup.md (similarity: 0.89)
# personal (medium tier):
#   - learning/docker-notes.md (similarity: 0.82)

# Cross-kiln linking (with security warnings)
cru note link work:deployment/auth.md oss:public-api.md
# ⚠ Warning: Linking high-tier note to public-tier note
# This may leak sensitive context. Continue? [y/N]
```

---

## 🛡️ Security Model

### Tier Inheritance

```
Global Security Policy (strictest)
    ↓
Kiln Security Policy (can be more restrictive)
    ↓
Agent Security Tier (can be more restrictive)
    ↓
Task Security Tier (auto-detected, can be more restrictive)
```

**Example:**
```
Global: Allows Anthropic for public tier
  ↓
Kiln (work): Blocks Anthropic entirely
  ↓
Agent: Prefers Ollama
  ↓
Task: Requires Critical tier (auto-detected from file)
  ↓
Result: Uses Ollama (only allowed option)
```

### Access Control

```toml
# ~/.config/crucible/security.toml (Global)
[kilns.access]
# Who can load which kilns
work.allowed_users = ["moot@company.com"]
work.allowed_machines = ["work-laptop"]
work.vpn_required = true

personal.allowed_users = ["moot@personal.com"]
personal.allowed_machines = ["*"]  # Any machine

# Cross-kiln policies
[kilns.isolation]
allow_cross_kiln_search = false    # Strict isolation
allow_cross_kiln_links = "prompt"  # Prompt before linking
context_bleeding_prevention = true # Never mix contexts
```

---

## 📊 Use Cases

### Use Case 1: Enterprise Developer

**Setup:**
```bash
# Company-provided kiln (read-only base)
git clone https://github.com/company/engineering-kb
cru kiln register ~/company/engineering-kb --readonly

# Personal work kiln (extends company kiln)
cru kiln init --name work-personal --tier high
cru kiln link work-personal --extends ~/company/engineering-kb

# Personal learning (separate)
cru kiln init --name learning --tier low
```

**Daily usage:**
```bash
# Morning: Review company docs
cd ~/company/engineering-kb
cru search "new deploy process"

# Work: Use personal work kiln
cd ~/projects/internal-app
cru chat --kiln work-personal
# Has access to company KB + personal notes

# Evening: Learning
cd ~/courses
cru chat --kiln learning
# Isolated from work kilns
```

### Use Case 2: Open Source Maintainer

**Setup:**
```bash
# Personal knowledge base
cru kiln init --name personal --tier medium

# Per-project OSS kilns
cd ~/oss/project-a
cru kiln init --name project-a --tier public --git-sync

cd ~/oss/project-b
cru kiln init --name project-b --tier public --git-sync
```

**Collaboration:**
```bash
# Contributors can clone and use immediately
git clone https://github.com/user/project-a
cd project-a
cru kiln detect  # Auto-registers .crucible/

# Ask about architecture
cru chat
# > "How does the authentication system work?"
# Uses project-a kiln, finds architecture.md
```

### Use Case 3: Security Researcher

**Setup:**
```bash
# Public research (shareable)
cru kiln init --name public-research --tier public

# Private research (sensitive)
cru kiln init --name private-research --tier critical
# → Forces Ollama, no cloud backends

# Disclosed vulnerabilities (medium sensitivity)
cru kiln init --name disclosed-vulns --tier medium
```

**Workflow:**
```bash
# Analyzing proprietary code (critical)
cd ~/research/target-app
cru kiln switch private-research
cru agent run security-auditor src/
# → Uses Ollama only, never touches cloud

# Writing public disclosure
cd ~/research/writeups
cru kiln switch public-research
cru agent run docs-helper --file CVE-2025-1234.md
# → Can use any backend, optimized for quality

# Public blog post
cru agent run blog-writer --input CVE-2025-1234.md
# → Anthropic Claude (best writing quality)
```

---

## 🔧 Implementation Details

### Kiln Configuration Schema

```toml
# .crucible/config.toml (Per-kiln)
[kiln]
name = "my-kiln"
version = "1.0"
created = "2025-01-15T00:00:00Z"
security_tier = "medium"
owner = "user@example.com"

[storage]
# Storage backend for this kiln
type = "surrealdb"
path = ".crucible/db"
namespace = "my-kiln"

[embeddings]
# Embedding provider for this kiln
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
dimensions = 384
cache_path = ".crucible/embeddings"

[backends]
# Preferred LLM backends for this kiln
default = "ollama"
fallback = ["copilot", "openai"]

[backends.ollama]
endpoint = "http://localhost:11434"
model = "qwen2.5-coder:7b"

[backends.copilot]
model = "claude-sonnet-4.5"

[sharing]
enabled = true
sync_method = "git"
remote_url = "https://github.com/user/my-kiln"
branch = "main"

[agents]
# Default agents for this kiln
auto_load = true
path = ".crucible/agents"

[security]
# Security overrides for this kiln
max_tier = "medium"                # Can't be relaxed beyond this
min_tier = "low"                   # Can't be more restrictive
require_approval = ["delete"]      # Operations requiring approval
audit_all = true                   # Log all operations
```

### Kiln Registry

```toml
# ~/.config/crucible/kilns.toml (Global registry)
version = "1.0"

[[kilns]]
name = "work"
path = "/home/user/work/company-internal"
security_tier = "high"
last_accessed = "2025-01-15T14:30:00Z"
auto_load = true
default_for = ["/home/user/work/**"]

[[kilns]]
name = "personal"
path = "/home/user/notes/personal"
security_tier = "medium"
last_accessed = "2025-01-15T12:00:00Z"
auto_load = true
default_for = ["/home/user/projects/**"]

[[kilns]]
name = "oss"
path = "/home/user/notes/oss"
security_tier = "public"
sync_url = "https://github.com/user/notes"
last_synced = "2025-01-15T10:00:00Z"
auto_load = false

[defaults]
# Global defaults
default_kiln = "personal"
auto_switch = true              # Switch based on working directory
prompt_before_switch = true     # Ask before switching to different tier
```

### Cross-Kiln Context Prevention

```rust
impl ChatSession {
    /// Ensure context doesn't bleed between kilns
    pub async fn send_message(
        &mut self,
        message: &str,
        current_kiln: &KilnHandle,
    ) -> Result<String> {
        // 1. Check session kiln matches current kiln
        if self.kiln_id != current_kiln.id() {
            return Err(SecurityError::KilnMismatch {
                session_kiln: self.kiln_id.clone(),
                current_kiln: current_kiln.id().clone(),
                message: "Cannot use context from different kiln"
            });
        }

        // 2. Enrich context ONLY from current kiln
        let context = current_kiln
            .semantic_search(message, 5)
            .await?;

        // 3. Validate no cross-kiln references in context
        for result in &context {
            if result.kiln_id != current_kiln.id() {
                warn!(
                    "Filtering out cross-kiln result: {} from {}",
                    result.title, result.kiln_id
                );
                continue;
            }
        }

        // 4. Send to LLM with kiln-isolated context
        self.llm.chat_completion(/* ... */).await
    }
}
```

---

## 🚀 CLI Examples

### Complete Workflow

```bash
# 1. Setup multiple kilns
cru kiln init --name work --path ~/work/kb --tier high
cru kiln init --name personal --path ~/notes --tier medium
cru kiln init --name learning --path ~/courses --tier low

# 2. Configure auto-switching
cru config set kiln.auto_switch true
cru config set kiln.prompt_on_switch true

# 3. Work in different contexts
cd ~/work/company-app
cru chat
# 🔄 Switching to kiln: work (high tier)
# Backend: Ollama (deepseek-coder:33b)
# > "How does our auth system work?"

cd ~/notes
cru chat
# 🔄 Switching to kiln: personal (medium tier)
# Backend: GitHub Copilot (claude-sonnet-4.5)
# > "Summarize my notes on Docker"

cd ~/courses/rust
cru chat
# 🔄 Switching to kiln: learning (low tier)
# Backend: Ollama (qwen2.5-coder:7b)
# > "Explain Rust lifetimes"

# 4. Cross-kiln search (when allowed)
cru search "authentication" --all-kilns
# work (high tier):
#   - internal/auth-design.md
# personal (medium tier):
#   - learning/oauth2-notes.md
# learning (low tier):
#   - courses/security/auth-basics.md

# 5. Sync kilns
cru kiln sync learning
# ✓ Pulled 3 new notes from remote
# ✓ Pushed 1 local change

# 6. Audit kiln usage
cru kiln audit work --last-month
# 📊 Kiln: work (high tier)
# Period: 2025-01-01 to 2025-01-31
# Queries: 145
# Backend: 100% Ollama (compliant ✓)
# Cross-kiln references: 0 (isolated ✓)
```

---

## 📈 Benefits

### Security Benefits
- ✅ **Isolation**: Sensitive data never mixes with public
- ✅ **Granular control**: Per-kiln security policies
- ✅ **Audit trails**: Track access per kiln
- ✅ **Compliance**: Separate kilns for regulated data

### Collaboration Benefits
- ✅ **Team sharing**: Git/Syncthing for collaboration
- ✅ **Version control**: Changes tracked in git
- ✅ **Conflict resolution**: Standard git workflows
- ✅ **Access control**: Git repo permissions

### Developer Experience
- ✅ **Context switching**: Auto-detect based on directory
- ✅ **Embedded kilns**: Knowledge lives with code
- ✅ **Offline-first**: Works without network
- ✅ **Flexible backends**: Per-kiln LLM preferences

### Organizational Benefits
- ✅ **Centralized policies**: Global security config
- ✅ **Distributed knowledge**: Team members own kilns
- ✅ **Scalable**: Thousands of kilns, isolated storage
- ✅ **Composable**: Kilns can reference (but not blend) each other

---

## 🎯 Advanced Patterns

### Pattern: Tiered Access Hierarchy

```
Company Engineering KB (public tier)
    ↓ (extends)
Team KB (medium tier)
    ↓ (extends)
Personal Work KB (high tier)
    ↓ (extends)
Local Experiments (critical tier)
```

**Implementation:**
```toml
# ~/.config/crucible/kilns.toml
[[kilns]]
name = "company-kb"
path = "~/kb/company"
security_tier = "public"
readonly = true
sync_url = "https://github.com/company/kb"

[[kilns]]
name = "team-kb"
path = "~/kb/team"
security_tier = "medium"
extends = ["company-kb"]
sync_url = "https://github.com/company/team-backend"

[[kilns]]
name = "work-personal"
path = "~/kb/work"
security_tier = "high"
extends = ["team-kb"]
sync_url = "git@github.com:moot/work-kb"

[[kilns]]
name = "experiments"
path = "~/kb/experiments"
security_tier = "critical"
extends = ["work-personal"]
sync_enabled = false
```

**Usage:**
```bash
cru search "deployment" --kiln work-personal
# Searches: work-personal + team-kb + company-kb (respecting tiers)
# Results from company-kb can be shown
# Context from work-personal NEVER flows to company-kb
```

### Pattern: Monorepo Multi-Kiln

```
monorepo/
├── services/
│   ├── auth/
│   │   └── .crucible/          # Auth service kiln
│   ├── api/
│   │   └── .crucible/          # API service kiln
│   └── frontend/
│       └── .crucible/          # Frontend kiln
└── .crucible/                  # Monorepo root kiln
```

**Usage:**
```bash
cd monorepo/services/auth
cru agent run code-reviewer
# Uses auth-specific kiln + agents

cd monorepo
cru search "authentication" --all-service-kilns
# Searches across all service kilns
```

---

## 🔮 Future Enhancements

### v1.1: Federation
- **Cross-organization kilns**: Federated search/sharing
- **Public kiln registry**: Discover/share public kilns
- **Kiln templates**: Start from templates (OSS, docs, research)

### v1.2: Advanced Sync
- **Real-time collaboration**: Operational transforms for multi-user
- **Conflict resolution UI**: Visual merge for conflicting notes
- **Selective sync**: Sync only parts of kiln

### v1.3: Enterprise Features
- **Kiln governance**: Organization-wide kiln policies
- **Compliance packs**: Pre-configured kilns for SOC2/HIPAA
- **Data residency**: Geographic restrictions per kiln
- **Backup/restore**: Automated kiln backups

---

**Multi-kiln architecture completes the security story:**
- ✅ Security tiers (what backend to use)
- ✅ Multi-backend support (how to call LLMs)
- ✅ Multi-kiln isolation (where data lives)

This is **comprehensive, innovative, and solves real problems**. No other tool does this! 🔥
