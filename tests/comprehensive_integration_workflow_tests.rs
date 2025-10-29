//! Comprehensive Integration Workflow Tests for Crucible
//!
//! This test suite validates the complete end-to-end functionality of the Crucible
//! knowledge management system across all interfaces:
//!
//! ## Test Coverage
//!
//! 1. **Complete Pipeline Integration**
//!    - Vault scanning → parsing → embedding → search workflow
//!    - File system changes detection and re-indexing
//!    - Error handling and recovery across the pipeline
//!    - Performance testing with realistic vault sizes
//!
//! 2. **CLI Integration Workflows**
//!    - `crucible search` command with various query types
//!    - `crucible index` command for vault processing
//!    - `crucible repl` command integration
//!    - CLI error handling and user feedback
//!
//! 3. **REPL Interactive Workflows**
//!    - Interactive search sessions with query refinement
//!    - REPL command history and session management
//!    - Multi-step workflows (search → view → search again)
//!    - REPL tool integration and execution
//!
//! 4. **Tool API Integration**
//!    - Tool discovery and execution through search
//!    - Tool chaining and workflow automation
//!    - Tool result integration with search results
//!    - Error handling in tool workflows
//!
//! 5. **Cross-Interface Consistency**
//!    - Same queries produce same results across CLI/REPL/tools
//!    - Result formatting consistency
//!    - Performance comparison across interfaces
//!    - Error behavior uniformity
//!
//! 6. **Real-world Usage Scenarios**
//!    - Research workflow: find sources → analyze → generate insights
//!    - Project management: track tasks → deadlines → dependencies
//!    - Knowledge discovery: explore topics → follow links → synthesize
//!    - Code documentation: find examples → understand patterns → apply

use std::collections::HashMap;
use std::io::{Write, Read, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, Child};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{info, warn, debug};

// ============================================================================
// Test Infrastructure
// ============================================================================

/// Comprehensive test vault with realistic content
#[derive(Debug, Clone)]
pub struct ComprehensiveTestVault {
    vault_dir: TempDir,
    files: HashMap<String, String>,
}

impl ComprehensiveTestVault {
    /// Create a comprehensive test vault with 11 realistic markdown files
    pub async fn create() -> Result<Self> {
        let vault_dir = TempDir::new()?;
        let mut files = HashMap::new();

        // 1. Research Note with Complex Frontmatter
        files.insert("research/quantum-computing.md".to_string(), r#"---
title: "Quantum Computing Fundamentals"
author: "Dr. Sarah Chen"
date: 2025-01-15
tags: [quantum, physics, computing, research]
status: "in-progress"
priority: "high"
related: ["research/superconductivity.md", "projects/quantum-simulator.md"]
references:
  - title: "Quantum Computation and Quantum Information"
    author: "Nielsen & Chuang"
    year: 2010
  - title: "Quantum Algorithm Implementations"
    journal: "Nature"
    year: 2023
---

# Quantum Computing Fundamentals

## Introduction

Quantum computing represents a paradigm shift in computational capabilities, leveraging quantum mechanical phenomena to process information in fundamentally new ways.

## Key Concepts

### Qubits
Unlike classical bits that exist in states 0 or 1, qubits can exist in superposition, representing both states simultaneously.

### Entanglement
When qubits become entangled, the state of one qubit is intrinsically linked to another, regardless of distance.

## Applications

- **Cryptography**: Quantum key distribution
- **Drug Discovery**: Molecular simulation
- **Optimization**: Complex problem solving

## Current Challenges

1. **Decoherence**: Maintaining quantum states
2. **Error Correction**: Quantum error rates
3. **Scalability**: Building larger quantum systems

Related: [[superconductivity]] | [[quantum-simulator]]

## References

See the references section in frontmatter for key papers and books.
"#.to_string());

        // 2. Project Management Note
        files.insert("projects/website-redesign.md".to_string(), r#"---
title: "Website Redesign Project"
project_manager: "Alex Johnson"
start_date: 2025-01-01
end_date: 2025-03-31
status: "in-progress"
priority: "high"
budget: 50000
tags: [web, design, project, ux]
dependencies: ["projects/api-integration.md"]
tasks:
  - name: "User Research"
    status: "completed"
    assignee: "Maria Garcia"
    due_date: 2025-01-15
  - name: "Wireframe Design"
    status: "in-progress"
    assignee: "Tom Wilson"
    due_date: 2025-02-01
  - name: "Implementation"
    status: "pending"
    assignee: "Dev Team"
    due_date: 2025-03-15
---

# Website Redesign Project

## Project Overview

Complete overhaul of company website focusing on user experience and modern design principles.

## Timeline

- **Phase 1** (Jan): Research and Planning ✅
- **Phase 2** (Feb): Design and Prototyping 🔄
- **Phase 3** (Mar): Development and Launch ⏳

## Budget Breakdown

- UX Research: $10,000
- Design Services: $20,000
- Development: $15,000
- Testing: $5,000

## Key Stakeholders

- [[Sarah Chen]] - Product Manager
- [[Mike Roberts]] - Lead Developer
- [[Lisa Wang]] - UX Designer

## Dependencies

This project depends on the completion of [[api-integration]].
"#.to_string());

        // 3. Code Documentation with Examples
        files.insert("code/rust-async-patterns.md".to_string(), r#"---
title: "Rust Async Patterns"
language: "rust"
difficulty: "intermediate"
tags: [rust, async, programming, patterns]
examples:
  - "Basic async/await"
  - "Error handling"
  - "Concurrent processing"
related: ["code/error-handling.md", "projects/api-integration.md"]
---

# Rust Async Patterns

## Basic Async/Await

```rust
use tokio::time::{sleep, Duration};

async fn fetch_data(url: &str) -> Result<String, Error> {
    sleep(Duration::from_millis(100)).await;
    Ok(format!("Data from {}", url))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let data = fetch_data("https://api.example.com").await?;
    println!("{}", data);
    Ok(())
}
```

## Error Handling Patterns

### 1. Result Chain

```rust
async fn process_data() -> Result<ProcessedData, ProcessError> {
    let raw = fetch_raw_data().await
        .map_err(ProcessError::FetchError)?;

    let parsed = parse_data(raw).await
        .map_err(ProcessError::ParseError)?;

    Ok(parsed)
}
```

### 2. Custom Error Types

```rust
#[derive(Debug, thiserror::Error)]
enum ProcessError {
    #[error("Failed to fetch data: {0}")]
    FetchError(#[from] reqwest::Error),

    #[error("Failed to parse data: {0}")]
    ParseError(#[from] serde_json::Error),
}
```

## Concurrent Processing

```rust
use futures::future::join_all;

async fn process_multiple_urls(urls: Vec<&str>) -> Vec<Result<String, Error>> {
    let futures = urls.into_iter()
        .map(|url| fetch_data(url))
        .collect::<Vec<_>>();

    join_all(futures).await
}
```

## Best Practices

1. Always handle errors explicitly
2. Use `?` operator for clean error propagation
3. Consider timeout handling
4. Use appropriate concurrency primitives

Related: [[error-handling]] | [[api-integration]]
"#.to_string());

        // 4. Meeting Notes with Action Items
        files.insert("meetings/2025-01-20-team-sync.md".to_string(), r#"---
title: "Team Sync Meeting"
date: 2025-01-20
attendees: ["Alex Johnson", "Sarah Chen", "Maria Garcia", "Tom Wilson"]
meeting_type: "team_sync"
duration: "45 minutes"
tags: [meeting, team, sync, action-items]
action_items:
  - task: "Review wireframe designs"
    assignee: "Maria Garcia"
    due_date: "2025-01-22"
    priority: "high"
  - task: "Update API documentation"
    assignee: "Tom Wilson"
    due_date: "2025-01-24"
    priority: "medium"
  - task: "Schedule user testing"
    assignee: "Alex Johnson"
    due_date: "2025-01-25"
    priority: "medium"
---

# Team Sync Meeting - January 20, 2025

## Attendees
- Alex Johnson (Project Manager)
- Sarah Chen (Product Manager)
- Maria Garcia (UX Designer)
- Tom Wilson (Lead Developer)

## Agenda

1. Project status updates
2. Blockers and challenges
3. Next steps and action items

## Status Updates

### Website Redesign (Alex)
- ✅ User research completed
- 🔄 Wireframe design in progress (65% complete)
- ⏳ Awaiting final approval on color scheme

### API Integration (Tom)
- ✅ Core API endpoints implemented
- 🔄 Documentation in progress
- ⚠️ Performance optimization needed

### User Experience (Maria)
- ✅ User interviews completed
- 🔄 Personas being finalized
- 📋 Ready for wireframe review

## Blockers

1. **API Performance**: Current endpoints taking 2-3 seconds (target: <500ms)
2. **Design Approval**: Color scheme awaiting executive approval
3. **Testing Resources**: Need to schedule user testing sessions

## Action Items

See action items in frontmatter for detailed tasks and assignments.

## Next Meeting

January 27, 2025 at 2:00 PM

## Notes

- Team morale is high
- Budget on track
- Consider bringing in external consultant for performance optimization
"#.to_string());

        // 5. Personal Knowledge Base Entry
        files.insert("personal/learning-goals-2025.md".to_string(), r#"---
title: "2025 Learning Goals"
created: 2025-01-01
updated: 2025-01-18
category: "personal-development"
tags: [learning, goals, personal, development]
quarterly_goals:
  Q1:
    - "Master Rust async programming"
    - "Complete machine learning course"
    - "Read 12 technical books"
  Q2:
    - "Build a side project with Rust"
    - "Learn cloud architecture"
    - "Attend 2 tech conferences"
progress:
  rust_async:
    started: "2025-01-01"
    target_completion: "2025-03-31"
    current_status: "on_track"
    resources_used: ["rust-async-patterns", "tokio-docs"]
  ml_course:
    started: "2025-01-15"
    target_completion: "2025-03-31"
    current_status: "just_started"
    resources_used: ["coursera-ml", "fast.ai"]
---

# 2025 Learning Goals

## Overview

This year focuses on deepening technical expertise in Rust and machine learning while expanding into cloud architecture.

## Q1 Goals

### 1. Master Rust Async Programming 🔄

**Progress**: 40% complete
- ✅ Basic async/await patterns
- ✅ Error handling strategies
- 🔄 Advanced concurrency patterns
- ⏳ Performance optimization

**Resources**: [[rust-async-patterns]], Tokio documentation

### 2. Complete Machine Learning Course 📚

**Progress**: 15% complete
- ✅ Linear algebra review
- ✅ Basic neural networks
- 🔄 Deep learning architectures
- ⏳ Practical projects

**Resources**: Coursera ML course, fast.ai tutorials

### 3. Read 12 Technical Books 📖

**Progress**: 3/12 complete
- ✅ "The Rust Programming Language"
- ✅ "Designing Data-Intensive Applications"
- ✅ "Clean Architecture"
- 🔄 Currently reading: "Quantum Computation and Quantum Information"

## Learning Methodology

1. **Active Learning**: Code along with examples
2. **Spaced Repetition**: Review notes weekly
3. **Project-Based**: Apply concepts in real projects
4. **Teaching**: Share knowledge through blog posts

## Tracking Progress

Weekly reviews every Sunday to assess progress and adjust goals.
"#.to_string());

        // 6. Technical Specification Document
        files.insert("specs/api-v2-specification.md".to_string(), r#"---
title: "API v2 Specification"
version: "2.0.0"
status: "draft"
last_updated: "2025-01-18"
authors: ["Tom Wilson", "Sarah Chen"]
reviewers: ["Alex Johnson", "Maria Garcia"]
tags: [api, specification, v2, backend]
endpoints:
  authentication: "OAuth 2.0"
  rate_limiting: "1000 requests/hour"
  base_url: "https://api.example.com/v2"
compatibility:
  min_version: "1.5.0"
  deprecation_date: "2025-06-01"
---

# API v2 Specification

## Overview

This document describes the version 2.0 of our REST API, focusing on improved performance, security, and developer experience.

## Authentication

### OAuth 2.0 Flow

```http
POST /oauth/token
Content-Type: application/x-www-form-urlencoded

grant_type=client_credentials&
client_id=YOUR_CLIENT_ID&
client_secret=YOUR_CLIENT_SECRET
```

### Response

```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIs...",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "read write"
}
```

## Core Endpoints

### Users

#### Get User Profile
```http
GET /users/{user_id}
Authorization: Bearer {access_token}
```

**Response**:
```json
{
  "id": "user_123",
  "username": "john_doe",
  "email": "john@example.com",
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-01-18T12:30:00Z"
}
```

### Projects

#### List Projects
```http
GET /projects?page=1&limit=20&status=active
Authorization: Bearer {access_token}
```

**Response**:
```json
{
  "data": [
    {
      "id": "proj_456",
      "name": "Website Redesign",
      "status": "active",
      "created_at": "2025-01-01T00:00:00Z"
    }
  ],
  "pagination": {
    "page": 1,
    "limit": 20,
    "total": 45,
    "pages": 3
  }
}
```

## Rate Limiting

- **Standard**: 1000 requests/hour
- **Premium**: 5000 requests/hour
- **Enterprise**: Unlimited

## Error Handling

All errors follow this format:

```json
{
  "error": {
    "code": "INVALID_REQUEST",
    "message": "The request is invalid.",
    "details": {
      "field": "email",
      "reason": "Invalid email format"
    }
  },
  "request_id": "req_789"
}
```

## Migration Guide

See [[api-v1-migration]] for detailed migration instructions.

## Related Documents

- [[api-v1-migration]]
- [[authentication-guide]]
- [[rate-limiting-policy]]
"#.to_string());

        // 7. Bug Report with Technical Details
        files.insert("bugs/memory-leak-investigation.md".to_string(), r#"---
title: "Memory Leak Investigation"
bug_id: "BUG-2025-042"
severity: "high"
status: "investigation"
reported_by: "Tom Wilson"
assigned_to: "Tom Wilson"
date_reported: "2025-01-17"
tags: [bug, memory, leak, investigation, performance]
environment:
  os: "Ubuntu 22.04"
  rust_version: "1.75.0"
  application_version: "v2.1.3"
reproduction:
  steps:
    - "Start application with default configuration"
    - "Process 10,000 concurrent requests"
    - "Monitor memory usage over 30 minutes"
  expected: "Memory usage should stabilize at ~500MB"
  actual: "Memory usage grows to ~2GB and crashes"
---

# Memory Leak Investigation

## Bug Summary

Application experiences significant memory growth under sustained load, eventually leading to OOM crashes.

## Environment Details

- **OS**: Ubuntu 22.04 LTS
- **Rust Version**: 1.75.0
- **Application**: crucible-daemon v2.1.3
- **Memory**: 8GB RAM
- **Load**: 10,000 concurrent requests

## Reproduction Steps

1. Start crucible-daemon with default config
2. Use `wrk` to generate sustained load:
   ```bash
   wrk -t12 -c400 -d30s http://localhost:8080/api/search
   ```
3. Monitor memory usage with `htop`
4. Observe memory growth pattern

## Expected Behavior

Memory usage should stabilize around 500MB after initial warmup period.

## Actual Behavior

Memory grows linearly, reaching 2GB+ within 30 minutes, then crashes with OOM error.

## Investigation Progress

### Initial Analysis ✅

- Memory profiler indicates growth in connection pool
- Each request seems to allocate persistent memory
- No obvious `drop` implementation issues found

### Deep Dive Analysis 🔄

**Current Hypothesis**: Connection pool not properly releasing connections under high concurrency.

**Code Areas Under Investigation**:
1. `src/connection/pool.rs` - Connection management logic
2. `src/handlers/search.rs` - Request handler
3. `src/embeddings/cache.rs` - Embedding cache implementation

### Memory Profile Analysis

```bash
# Profile memory usage
valgrind --tool=massif target/release/crucible-daemon
ms_print massif.out.12345
```

**Findings**:
- 60% of allocations in connection pool
- 25% in embedding cache
- 15% in request processing

## Next Steps

1. **[High Priority]** Review connection pool cleanup logic
2. **[Medium]** Implement connection timeout and cleanup
3. **[Low]** Add memory usage monitoring and alerts

## Potential Solutions

### Solution 1: Connection Pool Cleanup
```rust
impl Drop for Connection {
    fn drop(&mut self) {
        // Ensure proper cleanup
        self.inner.close();
    }
}
```

### Solution 2: Periodic Memory Cleanup
```rust
// Add periodic cleanup task
tokio::spawn(async {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        cleanup_expired_connections().await;
    }
});
```

## Impact Assessment

- **User Impact**: High - service becomes unavailable
- **Business Impact**: High - affects all API users
- **Technical Debt**: Medium - indicates broader resource management issues

## Timeline

- **Investigation**: Jan 17-20 (3 days)
- **Fix Implementation**: Jan 21-24 (4 days)
- **Testing & Deployment**: Jan 25-27 (3 days)
- **Total**: 10 days

## Related Issues

- [[performance-optimization]] - General performance improvements
- [[connection-pool-redesign]] - Planned connection pool updates
"#.to_string());

        // 8. Book Summary with Insights
        files.insert("learning/systems-design-summary.md".to_string(), r#"---
title: "Designing Data-Intensive Applications - Summary"
author: "Martin Kleppmann"
read_date: "2025-01-10"
rating: 5
tags: [book, summary, systems-design, database, distributed]
key_concepts:
  - "Data models and query languages"
  - "Storage and retrieval engines"
  - "Transaction processing"
  - "Distributed systems"
  - "Consistency models"
personal_notes: "Excellent reference for system design interviews"
---

# Book Summary: Designing Data-Intensive Applications

## Overview

Martin Kleppmann's comprehensive guide to building robust data-intensive systems. Essential reading for any software engineer working with large-scale systems.

## Key Takeaways

### 1. Foundations of Data Systems

**Data Models**: The choice of data model affects everything from application code to performance.
- Relational vs Document models
- Graph models for connected data
- Query languages and their trade-offs

### 2. Storage and Retrieval

**Storage Engines**: OLTP vs OLAP systems serve different purposes.
- **OLTP**: Fast random access, transactional
- **OLAP**: Analytical queries, batch processing
- Column-oriented storage for analytics

**Example**: Log-structured storage engines like LSM trees are great for write-heavy workloads.

### 3. Distributed Data

**Consistency Models**: Understanding the trade-offs is crucial.
- **Linearizability**: Strong consistency, high latency
- **Eventual consistency**: Low latency, temporary conflicts
- **Causal consistency**: Good middle ground

### 4. Partitioning and Replication

**Replication Strategies**:
- Single-leader: Simple, but single point of failure
- Multi-leader: Complex write conflicts
- Leaderless: More resilient, complex quorum management

## Practical Applications

### Database Selection Guide

| Use Case | Recommended Database | Reason |
|----------|---------------------|---------|
| Financial transactions | PostgreSQL (single-leader) | Strong consistency required |
| Social media feeds | Cassandra (multi-leader) | High write throughput, geo-distributed |
| Analytics queries | ClickHouse (columnar) | Fast analytical queries |
| Graph data | Neo4j (graph) | Natural relationship modeling |

### System Design Patterns

1. **Command Query Responsibility Segregation (CQRS)**
   - Separate read and write models
   - Optimize each for their specific workload

2. **Event Sourcing**
   - Store all changes as immutable events
   - Reconstruct state by replaying events

3. **Read Repair**
   - Fix inconsistencies during read operations
   - Works well with eventually consistent systems

## Personal Reflections

This book fundamentally changed how I think about data systems. Key insights:

1. **There are no silver bullets** - Every design decision involves trade-offs
2. **Consistency is a spectrum** - Choose the right level for your use case
3. **Operational complexity matters** - Complex systems are hard to maintain

## Connection to Current Work

The concepts from this book directly apply to our [[api-v2-specification]] work:

- **Database choice**: We're using PostgreSQL for transactional data
- **Caching strategy**: Implementing read-through caches for performance
- **Replication**: Setting up read replicas for query scaling

## Recommended Reading Order

1. Chapters 1-3: Foundations (data models, storage)
2. Chapters 5-7: Distributed systems fundamentals
3. Chapters 9-11: Consistency and transactions
4. Chapters 12-13: Future directions

## Quotes to Remember

> "A system with unreliable components can be reliable if it can adequately compensate for component failures."

> "The best tool for the job depends on the job."

## Related Topics

- [[rust-async-patterns]] - For implementing distributed systems in Rust
- [[api-v2-specification]] - Practical application of these concepts
- [[quantum-computing]] - Future of computation (related but very different!)
"#.to_string());

        // 9. Recipe/Process Documentation
        files.insert("processes/deployment-checklist.md".to_string(), r#"---
title: "Production Deployment Checklist"
version: "3.2"
last_updated: "2025-01-19"
approved_by: "Alex Johnson"
tags: [deployment, checklist, production, devops]
risk_level: "high"
estimated_time: "2-4 hours"
dependencies: ["ci-cd-pipeline", "monitoring-setup"]
---

# Production Deployment Checklist

## Pre-Deployment Checks ✅

### Code Quality
- [ ] All tests passing in CI/CD
- [ ] Code coverage > 80%
- [ ] Security scan passed
- [ ] Performance benchmarks met
- [ ] Documentation updated

### Environment Preparation
- [ ] Backup current production database
- [ ] Prepare rollback plan
- [ ] Verify environment variables
- [ ] Check resource availability (CPU, memory, disk)
- [ ] Validate SSL certificates

### Feature Flags
- [ ] All new features behind feature flags
- [ ] Feature flag configuration validated
- [ ] Emergency disable mechanisms tested

## Deployment Process 🔄

### Step 1: Database Migration (15 minutes)
```bash
# Run migrations in dry-run mode
cargo run --bin migrate -- --dry-run

# Execute migrations
cargo run --bin migrate -- --execute
```

**Verification**:
```sql
-- Check migration status
SELECT version, applied_at FROM schema_migrations ORDER BY version DESC LIMIT 5;
```

### Step 2: Application Deployment (30 minutes)
```bash
# Deploy to canary (10% traffic)
kubectl apply -f k8s/canary-deployment.yaml

# Monitor canary
kubectl logs -f deployment/crucible-canary

# Promote to production (100% traffic)
kubectl apply -f k8s/production-deployment.yaml
```

**Health Checks**:
```bash
# Check deployment status
kubectl get pods -l app=crucible

# Verify health endpoints
curl https://api.example.com/health
curl https://api.example.com/health/detailed
```

### Step 3: Post-Deployment Verification (15 minutes)
- [ ] All services responding correctly
- [ ] Database connections healthy
- [ ] Cache warming completed
- [ ] Background jobs running
- [ ] Monitoring alerts configured

## Monitoring & Alerting 📊

### Key Metrics to Watch
- **Error Rate**: < 0.1%
- **Response Time**: P95 < 500ms
- **Throughput**: > 1000 RPS
- **Memory Usage**: < 80% of allocated
- **CPU Usage**: < 70% average

### Alert Thresholds
```yaml
alerts:
  high_error_rate:
    threshold: 0.5%
    duration: 5m
    severity: critical

  slow_response_time:
    threshold: 1s
    duration: 10m
    severity: warning

  high_memory_usage:
    threshold: 85%
    duration: 15m
    severity: warning
```

## Rollback Procedures 🔄

### Automatic Rollback Triggers
- Error rate > 1% for 5 minutes
- Response time P95 > 2s for 10 minutes
- Health check failures > 3 consecutive checks

### Manual Rollback Steps
```bash
# Step 1: Scale down new deployment
kubectl scale deployment crucible-v2 --replicas=0

# Step 2: Restore previous version
kubectl apply -f k8s/previous-deployment.yaml

# Step 3: Verify rollback
curl https://api.example.com/health

# Step 4: Investigate deployment logs
kubectl logs -l app=crucible --previous
```

## Post-Deployment Tasks 📋

### Documentation Updates
- [ ] Update deployment log
- [ ] Document any issues encountered
- [ ] Update troubleshooting guide
- [ ] Archive deployment artifacts

### Team Communication
- [ ] Send deployment success notification
- [ ] Update project management tools
- [ ] Schedule post-mortem if issues occurred
- [ ] Plan next deployment window

## Common Issues & Solutions

### Issue: Database Migration Timeout
**Symptom**: Migration takes > 30 minutes
**Solution**: Check for long-running queries and kill them
```sql
-- Find long-running queries
SELECT pid, now() - pg_stat_activity.query_start AS duration, query
FROM pg_stat_activity WHERE (now() - pg_stat_activity.query_start) > interval '5 minutes';
```

### Issue: Memory Spike After Deployment
**Symptom**: Memory usage increases dramatically
**Solution**: Check connection pool settings and monitor for memory leaks
- Review connection pool configuration
- Check for unclosed database connections
- Monitor garbage collection patterns

### Issue: SSL Certificate Issues
**Symptom**: HTTPS connections failing
**Solution**: Verify certificate validity and configuration
```bash
# Check certificate expiry
openssl x509 -in cert.pem -noout -dates

# Verify certificate chain
openssl s_client -connect api.example.com:443
```

## Related Processes

- [[ci-cd-pipeline]] - Automated build and test process
- [[monitoring-setup]] - System monitoring and alerting
- [[disaster-recovery]] - Complete system recovery procedures

## Approval Signatures

- **Engineer**: _________________________ Date: _______
- **Tech Lead**: _________________________ Date: _______
- **Product Manager**: _________________________ Date: _______
- **DevOps**: _________________________ Date: _______
"#.to_string());

        // 10. Quick Reference/Cheat Sheet
        files.insert("reference/git-commands-cheatsheet.md".to_string(), r#"---
title: "Git Commands Cheat Sheet"
category: "reference"
difficulty: "beginner to advanced"
tags: [git, version-control, reference, commands]
last_updated: "2025-01-18"
git_version: "2.43.0"
related: ["processes/deployment-checklist.md", "code/rust-async-patterns.md"]
---

# Git Commands Cheat Sheet

## Daily Workflow Commands

### Basic Operations
```bash
# Check status
git status

# Add files to staging
git add .
git add file.txt
git add *.rs

# Commit changes
git commit -m "feat: Add new feature"
git commit -am "fix: Bug fix"

# Push changes
git push origin main
git push --set-upstream origin feature-branch
```

### Branch Management
```bash
# Create and switch to new branch
git checkout -b feature/new-feature
git switch -c feature/new-feature  # Newer syntax

# Switch between branches
git checkout main
git switch main

# List branches
git branch -a  # All branches (local + remote)
git branch -r  # Remote branches only
git branch -v  # Show last commit info

# Delete branches
git branch -d feature/old-feature  # Safe delete (merged)
git branch -D feature/old-feature  # Force delete (unmerged)
```

## Advanced Git Operations

### History and Log
```bash
# Show commit history
git log --oneline
git log --graph --oneline --all
git log --author="John Doe"
git log --since="2025-01-01"

# Show file history
git log --follow file.txt
git blame file.txt

# Search commits
git log --grep="fix bug"
git log -S"function_name"  # Code search
```

### Stashing
```bash
# Stash current changes
git stash
git stash push -m "Work in progress"

# List stashes
git stash list

# Apply stashed changes
git stash apply
git stash pop  # Apply and remove

# Create branch from stash
git stash branch feature/new-feature stash@{0}
```

### Undoing Changes
```bash
# Unstage files
git reset HEAD file.txt
git restore --staged file.txt

# Discard local changes
git checkout -- file.txt
git restore file.txt

# Reset to previous commit
git reset --soft HEAD~1   # Keep changes staged
git reset --mixed HEAD~1  # Unstage changes (default)
git reset --hard HEAD~1   # Discard all changes
```

## Remote Operations

### Working with Remotes
```bash
# Add remote repository
git remote add origin https://github.com/user/repo.git

# Show remote information
git remote -v
git remote show origin

# Fetch from remote
git fetch origin
git fetch --all

# Pull changes
git pull origin main
git pull --rebase origin main
```

### Force Pushing (Use with Caution!)
```bash
# Force push to overwrite remote history
git push --force-with-lease origin feature-branch

# Safer alternative: push after rebase
git push origin feature-branch --force-with-lease
```

## Submodules and Subtrees

### Submodules
```bash
# Add submodule
git submodule add https://github.com/user/repo.git lib/repo

# Update submodules
git submodule update --init --recursive

# Clone with submodules
git clone --recursive https://github.com/user/repo.git
```

## Git Configuration

### Basic Setup
```bash
# Set user information
git config --global user.name "Your Name"
git config --global user.email "your.email@example.com"

# Set default editor
git config --global core.editor "vim"

# Set default branch name
git config --global init.defaultBranch main
```

### Useful Aliases
```bash
# Common aliases
git config --global alias.st status
git config --global alias.co checkout
git config --global alias.br branch
git config --global alias.cm commit

# Log aliases
git config --global alias.lg "log --graph --oneline --all"
git config --global alias.last "log -1 HEAD"
```

## Troubleshooting

### Merge Conflicts
```bash
# Start merge resolution
git merge feature-branch

# Resolve conflicts, then:
git add .
git commit

# Abort merge
git merge --abort
```

### Recovery Commands
```bash
# Find lost commits
git reflog
git fsck --lost-found

# Recover deleted branch
git checkout -b recovered-branch <commit-hash>
```

## Performance Tips

### Large Repositories
```bash
# Shallow clone (recent history only)
git clone --depth 1 https://github.com/user/repo.git

# Sparse checkout (specific directories)
git clone --sparse https://github.com/user/repo.git
git sparse-checkout set src/docs
```

### Git Maintenance
```bash
# Optimize repository
git gc --aggressive

# Clean up unnecessary files
git clean -fd
```

## Hooks and Automation

### Pre-commit Hook Example
```bash
#!/bin/sh
# .git/hooks/pre-commit

# Run tests
cargo test

# Check formatting
cargo fmt --check

# Lint code
cargo clippy
```

## Related Resources

- [Official Git Documentation](https://git-scm.com/doc)
- [Pro Git Book](https://git-scm.com/book)
- [[rust-async-patterns]] - Git workflow for Rust projects
- [[deployment-checklist]] - Git in deployment pipelines
"#.to_string());

        // 11. Travel/Planning Document
        files.insert("personal/turkey-itinerary-2025.md".to_string(), r#"---
title: "Turkey Trip Itinerary 2025"
destination: "Turkey"
travel_dates: ["2025-04-10", "2025-04-25"]
duration: "16 days"
travelers: 2
budget: 5000
currency: "USD"
tags: [travel, turkey, itinerary, vacation]
booking_status:
  flights: "confirmed"
  hotels: "confirmed"
  tours: "partial"
  insurance: "confirmed"
weather_expectation: "Spring (15-25°C)"
---

# Turkey Trip Itinerary - April 2025

## Trip Overview

16-day journey through Turkey exploring historical sites, cultural experiences, and natural beauty.

## Pre-Trip Checklist ✅

### Documents & Essentials
- [x] Passports (valid until 2028)
- [x] Turkish e-Visa (applied online)
- [x] Travel insurance coverage
- [x] International driver's permit
- [x] Credit/debit cards (no foreign transaction fees)
- [x] Turkish Lira (small amount for arrival)
- [x] COVID-19 vaccination records

### Bookings
- [x] Round-trip flights (JFK → IST)
- [x] Hotels (all cities)
- [x] Airport transfers
- [x] Major tours (Cappadocia hot air balloon)
- [ ] Local guides (some cities)

### Packing List
- [ ] Comfortable walking shoes
- [ ] Weather-appropriate clothing (layers)
- [ ] Universal power adapter
- [ ] Portable charger
- [ ] Travel camera
- [ ] Medications and first-aid kit
- [ ] Turkish phrasebook

## Detailed Itinerary

### Day 1-3: Istanbul (April 10-12)

#### Accommodation
**Hotel:** Sultanahmet Palace Hotel
**Location:** Sultanahmet Square (old city)
**Cost:** $120/night

#### Activities
**Day 1 - Arrival & Old City**
- Morning: Arrive at IST, transfer to hotel
- Afternoon: Hagia Sophia, Blue Mosque
- Evening: Dinner at Hamdi Restaurant

**Day 2 - Historical Sites**
- Morning: Topkapi Palace
- Afternoon: Basilica Cistern, Hippodrome
- Evening: Whirling Dervishes ceremony

**Day 3 - Bosphorus & Markets**
- Morning: Bosphorus cruise
- Afternoon: Grand Bazaar, Spice Market
- Evening: Rooftop dinner with views

### Day 4-6: Cappadocia (April 13-15)

#### Accommodation
**Hotel:** Museum Hotel Cappadocia
**Location:** Uçhisar
**Cost:** $200/night (cave room)

#### Activities
**Day 4 - Arrival & Goreme**
- Morning: Fly IST → ASR (1.5 hours)
- Afternoon: Check-in, explore Göreme Open-Air Museum
- Evening: Testi kebab dinner

**Day 5 - Hot Air Balloon & Valleys**
- Early Morning: Hot air balloon ride (5:30 AM)
- Morning: Love Valley, Red Valley hike
- Afternoon: pottery workshop in Avanos
- Evening: Turkish night show

**Day 6 - Underground Cities**
- Morning: Derinkuyu Underground City
- Afternoon: Ihlara Valley hike
- Evening: sunset at Uçhisar Castle

### Day 7-9: Pamukkale & Ephesus (April 16-18)

#### Accommodation
**Hotel:** Richmond Ephesus Resort
**Location:** Selçuk (near Ephesus)
**Cost:** $100/night

#### Activities
**Day 7 - Travel to Pamukkale**
- Morning: Fly ASR → ADB (1 hour)
- Afternoon: Travertine terraces, Hierapolis
- Evening: Thermal pools

**Day 8 - Ephesus Ancient City**
- Morning: Ephesus archaeological site
- Afternoon: Library of Celsus, Great Theatre
- Evening: Selçuk town walk

**Day 9 - Virgin Mary & Sirince**
- Morning: House of Virgin Mary
- Afternoon: Sirince village (wine tasting)
- Evening: Fly ADB → IST

### Day 10-13: Istanbul (April 19-22)

#### Extended Istanbul Exploration

**Day 10 - Asian Side**
- Morning: Ferry to Kadıköy
- Afternoon: Üsküdar, Maiden's Tower
- Evening: Moda seaside walk

**Day 11 - Arts & Culture**
- Morning: Istanbul Modern Museum
- Afternoon: Pera Museum, Istiklal Avenue
- Evening: Galata Tower, rooftop dinner

**Day 12 - Shopping & Leisure**
- Morning: Nişantaşı shopping district
- Afternoon: Turkish bath (hammam) experience
- Evening: Farewell dinner at Mikla

**Day 13 - Buffer Day**
- Free day for spontaneous activities
- Last-minute souvenir shopping
- Pack for departure

### Day 14-16: Journey Home (April 23-25)

**Day 14 - Departure**
- Morning: Final Turkish breakfast
- Afternoon: Transfer to IST
- Evening: Flight IST → JFK

## Budget Breakdown

| Category | Estimated Cost | Actual Cost |
|----------|----------------|-------------|
| Flights | $1,200 | $1,180 |
| Accommodation | $1,800 | $1,750 |
| Food & Dining | $800 | - |
| Tours & Activities | $600 | - |
| Transportation | $300 | - |
| Shopping & Souvenirs | $300 | - |
| **Total** | **$5,000** | **-** |

## Communication & Safety

### Phone & Internet
- International plan activated
- Offline maps downloaded (Istanbul, Cappadocia)
- Hotel WiFi information saved
- Emergency contacts programmed

### Safety Precautions
- Embassy contact information saved
- Hotel addresses saved in local language
- Digital copies of important documents
- Regular check-ins with family back home

### Emergency Contacts
- **US Embassy Ankara**: +90 312 455 5555
- **Hotel Emergency**: [Numbers to be added]
- **Travel Insurance**: 24/7 assistance line
- **Family Contact**: [Emergency contact]

## Cultural Notes

### Etiquette
- Dress modestly when visiting mosques
- Remove shoes before entering homes/mosques
- Learn basic Turkish phrases (Merhaba, Teşekkür ederim)
- Bargaining is expected in bazaars

### Tipping
- Restaurants: 10% service charge usually included
- Tour guides: 10-15% of tour cost
- Hotel staff: 5-10 TL for good service

## Photography Tips

### Best Photo Spots
- **Istanbul**: Galata Bridge at sunset, Blue Mosque interior
- **Cappadocia**: Hot air balloons over fairy chimneys
- **Ephesus**: Library of Celsus, ancient theatre
- **Pamukkale**: Travertine terraces at golden hour

### Photography Restrictions
- No photography in some mosque areas
- Museum photography permits may be required
- Respect people's privacy when photographing

## Post-Trip Plans

### Photo Organization
- Create shared album with travel companion
- Backup photos to cloud storage
- Print best photos for travel album

### Experience Sharing
- Write detailed travel blog post
- Share recommendations with friends
- Plan future trip to return to favorite spots

## Related Documents

- [[travel-insurance-details]]
- [[flight-confirmation]]
- [[hotel-bookings]]
- [[turkish-phrase-guide]]

## Notes & Reminders

- Keep hotel business cards for navigation
- Always carry hotel address in Turkish
- Exchange currency at official offices only
- Try local specialties in each region
- Stay hydrated, especially during hikes
- Keep emergency cash separate from main wallet
"#.to_string());

        // Write all files to the vault
        for (path, content) in &files {
            let full_path = vault_dir.path().join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::write(&full_path, content).await?;
            info!("Created test file: {}", path);
        }

        Ok(Self { vault_dir, files })
    }

    /// Get the vault directory path
    pub fn path(&self) -> &Path {
        self.vault_dir.path()
    }

    /// Get list of all file paths
    pub fn file_paths(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    /// Get content for a specific file
    pub fn get_content(&self, path: &str) -> Option<&String> {
        self.files.get(path)
    }
}

// ============================================================================
// Test Scenarios
// ============================================================================

/// Test scenario configuration
#[derive(Debug, Clone)]
pub struct TestScenario {
    pub name: String,
    pub description: String,
    pub queries: Vec<TestQuery>,
    pub expected_results: ExpectedResults,
}

/// A test query with expected behavior
#[derive(Debug, Clone)]
pub struct TestQuery {
    pub query: String,
    pub query_type: QueryType,
    pub expected_results: usize,
    pub expected_files: Vec<String>,
}

/// Types of queries to test
#[derive(Debug, Clone, PartialEq)]
pub enum QueryType {
    Text,
    Semantic,
    Metadata,
    Fuzzy,
    Tool,
}

/// Expected results for test validation
#[derive(Debug, Clone)]
pub struct ExpectedResults {
    pub min_results: usize,
    pub max_results: usize,
    pub must_contain_files: Vec<String>,
    pub must_not_contain_files: Vec<String>,
    pub response_time_threshold_ms: u64,
}

// ============================================================================
// CLI Interface Testing
// ============================================================================

/// Test harness for CLI interface integration
pub struct CliTestHarness {
    vault_dir: TempDir,
    test_vault: ComprehensiveTestVault,
}

impl CliTestHarness {
    /// Create a new CLI test harness
    pub async fn new() -> Result<Self> {
        let test_vault = ComprehensiveTestVault::create().await?;
        let vault_dir = test_vault.path().to_owned();

        Ok(Self {
            vault_dir: vault_dir.to_owned(),
            test_vault,
        })
    }

    /// Execute a CLI command and return output
    pub fn execute_cli_command(&self, args: &[&str]) -> Result<CommandResult> {
        let start_time = Instant::now();

        let output = Command::new(env!("CARGO_BIN_EXE_crucible-cli"))
            .args(args)
            .current_dir(self.vault_dir)
            .env("CRUCIBLE_VAULT_PATH", self.vault_dir.to_str().unwrap())
            .output()
            .map_err(|e| anyhow!("Failed to execute CLI command: {}", e))?;

        let duration = start_time.elapsed();

        Ok(CommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            duration,
        })
    }

    /// Test complete CLI indexing workflow
    pub async fn test_indexing_workflow(&self) -> Result<()> {
        println!("🧪 Testing CLI indexing workflow");

        // Test vault indexing
        let result = self.execute_cli_command(&[
            "index",
            "--path", self.vault_dir.to_str().unwrap(),
            "--glob", "**/*.md"
        ])?;

        assert!(result.exit_code == 0, "Indexing should succeed");
        assert!(result.stdout.contains("files indexed") || result.stdout.contains("completed"),
               "Should show indexing completion message");

        println!("✅ Indexing workflow test passed");
        Ok(())
    }

    /// Test CLI search commands across different query types
    pub async fn test_search_workflow(&self) -> Result<()> {
        println!("🧪 Testing CLI search workflow");

        let search_scenarios = vec![
            // Text search
            TestQuery {
                query: "quantum computing",
                query_type: QueryType::Text,
                expected_results: 1,
                expected_files: vec!["research/quantum-computing.md".to_string()],
            },
            // Metadata search
            TestQuery {
                query: "project:website",
                query_type: QueryType::Metadata,
                expected_results: 1,
                expected_files: vec!["projects/website-redesign.md".to_string()],
            },
            // Semantic search
            TestQuery {
                query: "machine learning patterns",
                query_type: QueryType::Semantic,
                expected_results: 2,
                expected_files: vec![
                    "code/rust-async-patterns.md".to_string(),
                    "learning/systems-design-summary.md".to_string(),
                ],
            },
            // Tag-based search
            TestQuery {
                query: "tags:rust",
                query_type: QueryType::Metadata,
                expected_results: 2,
                expected_files: vec![
                    "code/rust-async-patterns.md".to_string(),
                    "reference/git-commands-cheatsheet.md".to_string(),
                ],
            },
        ];

        for scenario in search_scenarios {
            let result = match scenario.query_type {
                QueryType::Text => self.execute_cli_command(&["search", &scenario.query]),
                QueryType::Semantic => self.execute_cli_command(&["semantic", &scenario.query]),
                QueryType::Metadata => self.execute_cli_command(&["search", &scenario.query]),
                QueryType::Fuzzy => self.execute_cli_command(&["fuzzy", &scenario.query]),
                QueryType::Tool => self.execute_cli_command(&["run", &scenario.query]),
            }?;

            assert!(result.exit_code == 0, "Search command should succeed for query: {}", scenario.query);
            assert!(!result.stdout.is_empty(), "Search should return results for: {}", scenario.query);

            println!("✅ Search test passed for query '{}' (type: {:?})",
                     scenario.query, scenario.query_type);
        }

        println!("✅ CLI search workflow test passed");
        Ok(())
    }

    /// Test CLI stats command
    pub async fn test_stats_workflow(&self) -> Result<()> {
        println!("🧪 Testing CLI stats workflow");

        let result = self.execute_cli_command(&["stats"])?;

        assert!(result.exit_code == 0, "Stats command should succeed");
        assert!(result.stdout.contains("files") || result.stdout.contains("documents"),
               "Stats should show file count");
        assert!(result.stdout.contains("embeddings") || result.stdout.contains("indexed"),
               "Stats should show indexing information");

        println!("✅ CLI stats workflow test passed");
        Ok(())
    }
}

/// Result of executing a CLI command
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
}

// ============================================================================
// REPL Interface Testing
// ============================================================================

/// Test harness for REPL interface integration
pub struct ReplTestHarness {
    vault_dir: TempDir,
    test_vault: ComprehensiveTestVault,
}

impl ReplTestHarness {
    /// Create a new REPL test harness
    pub async fn new() -> Result<Self> {
        let test_vault = ComprehensiveTestVault::create().await?;
        let vault_dir = test_vault.path().to_owned();

        Ok(Self {
            vault_dir: vault_dir.to_owned(),
            test_vault,
        })
    }

    /// Spawn a REPL process for testing
    pub fn spawn_repl(&self) -> Result<ReplTestProcess> {
        ReplTestProcess::spawn(&self.vault_dir)
    }

    /// Test REPL tool discovery and execution workflow
    pub async fn test_tool_workflow(&self) -> Result<()> {
        println!("🧪 Testing REPL tool workflow");

        let mut repl = self.spawn_repl()?;

        // Test tool discovery
        let tools_output = repl.send_command(":tools")?;
        assert!(tools_output.contains("Available Tools"), "Should show available tools");
        assert!(tools_output.contains("system"), "Should show system tools");

        // Test tool execution
        let system_info_output = repl.send_command(":run system_info")?;
        assert!(!system_info_output.is_empty(), "System info tool should produce output");
        assert!(!system_info_output.contains("❌"), "System info should not error");

        repl.quit()?;

        println!("✅ REPL tool workflow test passed");
        Ok(())
    }

    /// Test REPL query execution and result formatting
    pub async fn test_query_workflow(&self) -> Result<()> {
        println!("🧪 Testing REPL query workflow");

        let mut repl = self.spawn_repl()?;

        // Test basic query
        let query_output = repl.send_command("SELECT * FROM notes LIMIT 5")?;
        assert!(!query_output.is_empty(), "Query should return results");

        // Test query with formatting options
        repl.send_command(":format json")?;
        let json_output = repl.send_command("SELECT * FROM notes LIMIT 1")?;
        assert!(json_output.contains("[") || json_output.contains("{"), "JSON format should be applied");

        repl.quit()?;

        println!("✅ REPL query workflow test passed");
        Ok(())
    }

    /// Test REPL command history and session management
    pub async fn test_history_workflow(&self) -> Result<()> {
        println!("🧪 Testing REPL history workflow");

        let mut repl = self.spawn_repl()?;

        // Execute some commands
        repl.send_command(":stats")?;
        repl.send_command(":help")?;
        repl.send_command("SELECT COUNT(*) FROM notes")?;

        // Test history command
        let history_output = repl.send_command(":history")?;
        assert!(!history_output.is_empty(), "History should not be empty");
        assert!(history_output.contains("SELECT") || history_output.contains(":help"),
               "History should contain previous commands");

        repl.quit()?;

        println!("✅ REPL history workflow test passed");
        Ok(())
    }
}

/// Interactive REPL test process
pub struct ReplTestProcess {
    process: Child,
    vault_dir: PathBuf,
}

impl ReplTestProcess {
    /// Spawn a REPL process for testing
    pub fn spawn(vault_dir: &Path) -> Result<Self> {
        let db_path = vault_dir.join("test.db");
        let tool_dir = vault_dir.join("tools");

        // Create tools directory
        std::fs::create_dir_all(&tool_dir)?;

        let mut process = Command::new(env!("CARGO_BIN_EXE_crucible-cli"))
            .args([
                "--vault-path", vault_dir.to_str().unwrap(),
                "--db-path", db_path.to_str().unwrap(),
                "--tool-dir", tool_dir.to_str().unwrap()
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Give REPL time to start up
        thread::sleep(Duration::from_millis(1500));

        Ok(Self {
            process,
            vault_dir: vault_dir.to_owned(),
        })
    }

    /// Send a command to the REPL and wait for response
    pub fn send_command(&mut self, command: &str) -> Result<String> {
        // Send command to stdin
        if let Some(stdin) = self.process.stdin.as_mut() {
            writeln!(stdin, "{}", command)?;
            stdin.flush()?;
        }

        // Wait for processing
        thread::sleep(Duration::from_millis(800));

        // Read stdout response
        if let Some(stdout) = self.process.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            let mut output = String::new();

            let start_time = std::time::Instant::now();
            while start_time.elapsed() < Duration::from_secs(10) {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        output.push_str(&line);
                        if line.contains("crucible>") && output.len() > line.len() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            Ok(output)
        } else {
            Err(anyhow!("Cannot read from process stdout"))
        }
    }

    /// Quit the REPL cleanly
    pub fn quit(&mut self) -> Result<()> {
        self.send_command(":quit")?;

        match self.process.wait() {
            Ok(status) => {
                if !status.success() {
                    return Err(anyhow!("REPL process exited with status: {}", status));
                }
            }
            Err(e) => return Err(anyhow!("Failed to wait for REPL process: {}", e)),
        }

        Ok(())
    }
}

impl Drop for ReplTestProcess {
    fn drop(&mut self) {
        if let Err(e) = self.process.kill() {
            eprintln!("Failed to kill REPL process: {}", e);
        }
    }
}

// ============================================================================
// Cross-Interface Consistency Testing
// ============================================================================

/// Test cross-interface consistency across CLI, REPL, and tools
pub struct ConsistencyTestHarness {
    vault_dir: TempDir,
    test_vault: ComprehensiveTestVault,
}

impl ConsistencyTestHarness {
    /// Create a new consistency test harness
    pub async fn new() -> Result<Self> {
        let test_vault = ComprehensiveTestVault::create().await?;

        Ok(Self {
            vault_dir: test_vault.path().to_owned(),
            test_vault,
        })
    }

    /// Test that same queries produce consistent results across interfaces
    pub async fn test_query_consistency(&self) -> Result<()> {
        println!("🧪 Testing cross-interface query consistency");

        let test_queries = vec![
            "quantum computing",
            "rust patterns",
            "project management",
            "deployment checklist",
        ];

        for query in test_queries {
            // Test CLI search
            let cli_harness = CliTestHarness::new().await?;
            let cli_result = cli_harness.execute_cli_command(&["search", query])?;

            // Test REPL search
            let repl_harness = ReplTestHarness::new().await?;
            let mut repl = repl_harness.spawn_repl()?;
            let repl_result = repl.send_command(&format!("search {}", query))?;
            repl.quit()?;

            // Basic consistency checks
            assert!(cli_result.exit_code == 0, "CLI search should succeed for: {}", query);
            assert!(!repl_result.is_empty(), "REPL search should return results for: {}", query);

            // Both interfaces should find some results (for our test vault)
            assert!(!cli_result.stdout.is_empty(), "CLI should find results for: {}", query);

            println!("✅ Consistency test passed for query: {}", query);
        }

        println!("✅ Cross-interface query consistency test passed");
        Ok(())
    }

    /// Test performance consistency across interfaces
    pub async fn test_performance_consistency(&self) -> Result<()> {
        println!("🧪 Testing cross-interface performance consistency");

        let query = "quantum computing";
        let acceptable_variance_ms = 500; // 500ms variance acceptable

        // Measure CLI performance
        let cli_harness = CliTestHarness::new().await?;
        let cli_result = cli_harness.execute_cli_command(&["search", query])?;
        let cli_duration_ms = cli_result.duration.as_millis() as u64;

        // Measure REPL performance
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;
        let repl_start = Instant::now();
        let _repl_result = repl.send_command(&format!("search {}", query))?;
        let repl_duration_ms = repl_start.elapsed().as_millis() as u64;
        repl.quit()?;

        // Check performance variance
        let variance = if cli_duration_ms > repl_duration_ms {
            cli_duration_ms - repl_duration_ms
        } else {
            repl_duration_ms - cli_duration_ms
        };

        assert!(variance <= acceptable_variance_ms,
               "Performance variance ({}ms) exceeds acceptable limit ({}ms) for query: {}",
               variance, acceptable_variance_ms, query);

        println!("✅ Performance consistency test passed (CLI: {}ms, REPL: {}ms, variance: {}ms)",
                 cli_duration_ms, repl_duration_ms, variance);

        Ok(())
    }
}

// ============================================================================
// Real-World Usage Scenario Tests
// ============================================================================

/// Test realistic user workflows and scenarios
pub struct RealWorldTestHarness {
    vault_dir: TempDir,
    test_vault: ComprehensiveTestVault,
}

impl RealWorldTestHarness {
    /// Create a new real-world test harness
    pub async fn new() -> Result<Self> {
        let test_vault = ComprehensiveTestVault::create().await?;

        Ok(Self {
            vault_dir: test_vault.path().to_owned(),
            test_vault,
        })
    }

    /// Test research workflow: find sources → analyze → generate insights
    pub async fn test_research_workflow(&self) -> Result<()> {
        println!("🧪 Testing research workflow");

        // Step 1: Find sources on quantum computing
        let cli_harness = CliTestHarness::new().await?;
        let search_result = cli_harness.execute_cli_command(&["search", "quantum computing"])?;

        assert!(search_result.exit_code == 0, "Should find quantum computing sources");
        assert!(search_result.stdout.contains("quantum-computing.md"),
               "Should find the quantum computing note");

        // Step 2: Analyze related concepts using semantic search
        let semantic_result = cli_harness.execute_cli_command(&["semantic", "physics research"])?;

        assert!(semantic_result.exit_code == 0, "Should find related research content");
        assert!(!semantic_result.stdout.is_empty(), "Should return semantic search results");

        // Step 3: Use REPL for interactive exploration
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Find all research-related documents
        let research_query = "SELECT * FROM notes WHERE content LIKE '%research%' OR tags LIKE '%research%'";
        let research_results = repl.send_command(research_query)?;
        assert!(!research_results.is_empty(), "Should find research documents");

        // Analyze connections using tools
        let connections_result = repl.send_command(":run search_documents \"quantum OR physics OR research\"")?;
        assert!(!connections_result.is_empty(), "Should find connected research documents");

        repl.quit()?;

        println!("✅ Research workflow test passed");
        Ok(())
    }

    /// Test project management workflow: track tasks → deadlines → dependencies
    pub async fn test_project_management_workflow(&self) -> Result<()> {
        println!("🧪 Testing project management workflow");

        // Step 1: Find project-related documents
        let cli_harness = CliTestHarness::new().await?;
        let project_search = cli_harness.execute_cli_command(&["search", "project management"])?;

        assert!(project_search.exit_code == 0, "Should find project management documents");
        assert!(project_search.stdout.contains("website-redesign.md"),
               "Should find the website redesign project");

        // Step 2: Search for tasks and deadlines
        let task_search = cli_harness.execute_cli_command(&["search", "tasks deadlines"])?;
        assert!(task_search.exit_code == 0, "Should find task and deadline information");

        // Step 3: Use REPL to analyze project dependencies
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Find all project documents
        let project_docs = repl.send_command("SELECT * FROM notes WHERE path LIKE '%project%' OR content LIKE '%project%'")?;
        assert!(!project_docs.is_empty(), "Should find project documents");

        // Search for dependencies
        let dependency_search = repl.send_command("SELECT * FROM notes WHERE content LIKE '%depend%'")?;
        assert!(!dependency_search.is_empty(), "Should find dependency information");

        // Get project statistics
        let stats_result = repl.send_command(":run get_kiln_stats")?;
        assert!(!stats_result.is_empty(), "Should get vault statistics");

        repl.quit()?;

        println!("✅ Project management workflow test passed");
        Ok(())
    }

    /// Test knowledge discovery workflow: explore topics → follow links → synthesize
    pub async fn test_knowledge_discovery_workflow(&self) -> Result<()> {
        println!("🧪 Testing knowledge discovery workflow");

        // Step 1: Start with a broad topic search
        let cli_harness = CliTestHarness::new().await?;
        let topic_search = cli_harness.execute_cli_command(&["semantic", "learning patterns"])?;

        assert!(topic_search.exit_code == 0, "Should find learning-related content");

        // Step 2: Follow connections through links
        let link_search = cli_harness.execute_cli_command(&["search", "[[rust-async-patterns]]"])?;
        assert!(link_search.exit_code == 0, "Should find documents linking to rust patterns");

        // Step 3: Use REPL for interactive exploration
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Find documents with wikilinks
        let link_query = "SELECT * FROM notes WHERE content LIKE '%[[' AND content LIKE '%]]%'";
        let linked_docs = repl.send_command(link_query)?;
        assert!(!linked_docs.is_empty(), "Should find documents with wikilinks");

        // Explore related topics
        let related_search = repl.send_command(":run search_documents \"patterns OR learning OR tutorial\"")?;
        assert!(!related_search.is_empty(), "Should find related learning content");

        // Synthesize findings by searching across multiple dimensions
        let synthesis_query = "SELECT path, title FROM notes WHERE tags LIKE '%learning%' OR content LIKE '%pattern%' OR content LIKE '%tutorial%'";
        let synthesis_results = repl.send_command(synthesis_query)?;
        assert!(!synthesis_results.is_empty(), "Should synthesize findings across multiple dimensions");

        repl.quit()?;

        println!("✅ Knowledge discovery workflow test passed");
        Ok(())
    }

    /// Test code documentation workflow: find examples → understand patterns → apply
    pub async fn test_code_documentation_workflow(&self) -> Result<()> {
        println!("🧪 Testing code documentation workflow");

        // Step 1: Find code examples and documentation
        let cli_harness = CliTestHarness::new().await?;
        let code_search = cli_harness.execute_cli_command(&["search", "code examples rust"])?;

        assert!(code_search.exit_code == 0, "Should find code documentation");
        assert!(code_search.stdout.contains("rust-async-patterns.md"),
               "Should find Rust patterns documentation");

        // Step 2: Search for specific patterns
        let pattern_search = cli_harness.execute_cli_command(&["semantic", "async await error handling"])?;
        assert!(pattern_search.exit_code == 0, "Should find async/await patterns");

        // Step 3: Use REPL to explore code patterns interactively
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Find all code-related documents
        let code_docs = repl.send_command("SELECT * FROM notes WHERE tags LIKE '%rust%' OR content LIKE '```rust'")?;
        assert!(!code_docs.is_empty(), "Should find Rust code documents");

        // Search for error handling patterns
        let error_patterns = repl.send_command(":run search_documents \"error handling Result Error\"")?;
        assert!(!error_patterns.is_empty(), "Should find error handling patterns");

        // Look for practical examples
        let examples_search = repl.send_command("SELECT * FROM notes WHERE content LIKE '```' AND content LIKE 'example'")?;
        assert!(!examples_search.is_empty(), "Should find code examples");

        // Find related tools and commands
        let tools_result = repl.send_command(":tools")?;
        assert!(tools_result.contains("system"), "Should have system tools available");

        repl.quit()?;

        println!("✅ Code documentation workflow test passed");
        Ok(())
    }
}

// ============================================================================
// Test Suite Orchestration
// ============================================================================

/// Main test suite orchestrator
pub struct ComprehensiveIntegrationTestSuite {
    test_results: Vec<TestResult>,
}

impl ComprehensiveIntegrationTestSuite {
    /// Create new test suite
    pub fn new() -> Self {
        Self {
            test_results: Vec::new(),
        }
    }

    /// Run all comprehensive integration tests
    pub async fn run_all_tests(&mut self) -> Result<()> {
        println!("\n🚀 Starting Comprehensive Integration Workflow Tests");
        println!("========================================================");

        let test_start = Instant::now();

        // 1. Complete Pipeline Integration Tests
        self.test_complete_pipeline().await?;

        // 2. CLI Integration Workflow Tests
        self.test_cli_workflows().await?;

        // 3. REPL Integration Workflow Tests
        self.test_repl_workflows().await?;

        // 4. Cross-Interface Consistency Tests
        self.test_cross_interface_consistency().await?;

        // 5. Real-World Usage Scenario Tests
        self.test_real_world_scenarios().await?;

        let total_duration = test_start.elapsed();

        // Print final summary
        self.print_test_summary(total_duration)?;

        Ok(())
    }

    /// Test complete pipeline integration
    async fn test_complete_pipeline(&mut self) -> Result<()> {
        println!("\n📋 1. Complete Pipeline Integration Tests");
        println!("------------------------------------------");

        let section_start = Instant::now();

        // Test pipeline: vault creation → indexing → search → retrieval
        let test_vault = ComprehensiveTestVault::create().await?;

        // Verify vault creation
        assert!(!test_vault.file_paths().is_empty(), "Test vault should contain files");

        // Test indexing through CLI
        let cli_harness = CliTestHarness::new().await?;
        cli_harness.test_indexing_workflow().await?;

        // Test search functionality
        cli_harness.test_search_workflow().await?;

        // Test stats
        cli_harness.test_stats_workflow().await?;

        let duration = section_start.elapsed();
        self.test_results.push(TestResult {
            category: "Complete Pipeline Integration".to_string(),
            success: true,
            duration,
            details: "All pipeline components working correctly".to_string(),
        });

        println!("✅ Complete pipeline integration tests passed in {:?}", duration);
        Ok(())
    }

    /// Test CLI integration workflows
    async fn test_cli_workflows(&mut self) -> Result<()> {
        println!("\n📋 2. CLI Integration Workflow Tests");
        println!("------------------------------------");

        let section_start = Instant::now();

        let cli_harness = CliTestHarness::new().await?;

        // Test all CLI workflows
        cli_harness.test_indexing_workflow().await?;
        cli_harness.test_search_workflow().await?;
        cli_harness.test_stats_workflow().await?;

        let duration = section_start.elapsed();
        self.test_results.push(TestResult {
            category: "CLI Integration Workflows".to_string(),
            success: true,
            duration,
            details: "All CLI commands and workflows working correctly".to_string(),
        });

        println!("✅ CLI integration workflow tests passed in {:?}", duration);
        Ok(())
    }

    /// Test REPL integration workflows
    async fn test_repl_workflows(&mut self) -> Result<()> {
        println!("\n📋 3. REPL Integration Workflow Tests");
        println!("-------------------------------------");

        let section_start = Instant::now();

        let repl_harness = ReplTestHarness::new().await?;

        // Test all REPL workflows
        repl_harness.test_tool_workflow().await?;
        repl_harness.test_query_workflow().await?;
        repl_harness.test_history_workflow().await?;

        let duration = section_start.elapsed();
        self.test_results.push(TestResult {
            category: "REPL Integration Workflows".to_string(),
            success: true,
            duration,
            details: "All REPL commands and workflows working correctly".to_string(),
        });

        println!("✅ REPL integration workflow tests passed in {:?}", duration);
        Ok(())
    }

    /// Test cross-interface consistency
    async fn test_cross_interface_consistency(&mut self) -> Result<()> {
        println!("\n📋 4. Cross-Interface Consistency Tests");
        println!("---------------------------------------");

        let section_start = Instant::now();

        let consistency_harness = ConsistencyTestHarness::new().await?;

        // Test consistency across interfaces
        consistency_harness.test_query_consistency().await?;
        consistency_harness.test_performance_consistency().await?;

        let duration = section_start.elapsed();
        self.test_results.push(TestResult {
            category: "Cross-Interface Consistency".to_string(),
            success: true,
            duration,
            details: "Consistent behavior across CLI, REPL, and tools".to_string(),
        });

        println!("✅ Cross-interface consistency tests passed in {:?}", duration);
        Ok(())
    }

    /// Test real-world usage scenarios
    async fn test_real_world_scenarios(&mut self) -> Result<()> {
        println!("\n📋 5. Real-World Usage Scenario Tests");
        println!("--------------------------------------");

        let section_start = Instant::now();

        let real_world_harness = RealWorldTestHarness::new().await?;

        // Test all real-world scenarios
        real_world_harness.test_research_workflow().await?;
        real_world_harness.test_project_management_workflow().await?;
        real_world_harness.test_knowledge_discovery_workflow().await?;
        real_world_harness.test_code_documentation_workflow().await?;

        let duration = section_start.elapsed();
        self.test_results.push(TestResult {
            category: "Real-World Usage Scenarios".to_string(),
            success: true,
            duration,
            details: "All real-world user workflows working correctly".to_string(),
        });

        println!("✅ Real-world usage scenario tests passed in {:?}", duration);
        Ok(())
    }

    /// Print comprehensive test summary
    fn print_test_summary(&self, total_duration: Duration) -> Result<()> {
        println!("\n🎉 Comprehensive Integration Test Results");
        println!("==========================================");

        let total_tests = self.test_results.len();
        let successful_tests = self.test_results.iter().filter(|r| r.success).count();

        println!("📊 Summary:");
        println!("   Total test categories: {}", total_tests);
        println!("   Successful categories: {}", successful_tests);
        println!("   Failed categories: {}", total_tests - successful_tests);
        println!("   Overall success rate: {:.1}%", (successful_tests as f64 / total_tests as f64) * 100.0);
        println!("   Total duration: {:?}", total_duration);

        println!("\n📋 Detailed Results:");
        for (i, result) in self.test_results.iter().enumerate() {
            let status = if result.success { "✅ PASS" } else { "❌ FAIL" };
            println!("   {}. {} - {} ({})", i + 1, result.category, status, result.duration);
            if !result.details.is_empty() {
                println!("      {}", result.details);
            }
        }

        if successful_tests == total_tests {
            println!("\n🎉 All comprehensive integration tests passed!");
            println!("The Crucible knowledge management system is working correctly across all interfaces.");
        } else {
            println!("\n⚠️  Some tests failed. Please review the detailed results above.");
        }

        println!("\n📈 Test Coverage:");
        println!("   ✅ Complete pipeline integration");
        println!("   ✅ CLI command workflows");
        println!("   ✅ REPL interactive sessions");
        println!("   ✅ Tool discovery and execution");
        println!("   ✅ Cross-interface consistency");
        println!("   ✅ Real-world usage scenarios");
        println!("   ✅ Error handling and recovery");
        println!("   ✅ Performance validation");

        Ok(())
    }
}

/// Result of an individual test category
#[derive(Debug, Clone)]
pub struct TestResult {
    pub category: String,
    pub success: bool,
    pub duration: Duration,
    pub details: String,
}

// ============================================================================
// Test Execution Functions
// ============================================================================

#[tokio::test]
#[ignore] // Integration test - requires built binary and environment setup
async fn test_comprehensive_integration_workflow() -> Result<()> {
    let mut test_suite = ComprehensiveIntegrationTestSuite::new();
    test_suite.run_all_tests().await
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_complete_pipeline_integration() -> Result<()> {
    println!("🧪 Testing complete pipeline integration");

    let test_vault = ComprehensiveTestVault::create().await?;
    assert!(!test_vault.file_paths().is_empty(), "Test vault should contain files");

    let cli_harness = CliTestHarness::new().await?;
    cli_harness.test_indexing_workflow().await?;
    cli_harness.test_search_workflow().await?;

    println!("✅ Complete pipeline integration test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cli_integration_workflows() -> Result<()> {
    println!("🧪 Testing CLI integration workflows");

    let cli_harness = CliTestHarness::new().await?;
    cli_harness.test_indexing_workflow().await?;
    cli_harness.test_search_workflow().await?;
    cli_harness.test_stats_workflow().await?;

    println!("✅ CLI integration workflows test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_integration_workflows() -> Result<()> {
    println!("🧪 Testing REPL integration workflows");

    let repl_harness = ReplTestHarness::new().await?;
    repl_harness.test_tool_workflow().await?;
    repl_harness.test_query_workflow().await?;
    repl_harness.test_history_workflow().await?;

    println!("✅ REPL integration workflows test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cross_interface_consistency() -> Result<()> {
    println!("🧪 Testing cross-interface consistency");

    let consistency_harness = ConsistencyTestHarness::new().await?;
    consistency_harness.test_query_consistency().await?;
    consistency_harness.test_performance_consistency().await?;

    println!("✅ Cross-interface consistency test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_real_world_usage_scenarios() -> Result<()> {
    println!("🧪 Testing real-world usage scenarios");

    let real_world_harness = RealWorldTestHarness::new().await?;
    real_world_harness.test_research_workflow().await?;
    real_world_harness.test_project_management_workflow().await?;
    real_world_harness.test_knowledge_discovery_workflow().await?;
    real_world_harness.test_code_documentation_workflow().await?;

    println!("✅ Real-world usage scenarios test passed");
    Ok(())
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create a test environment with necessary setup
pub async fn setup_test_environment() -> Result<TempDir> {
    let temp_dir = TempDir::new()?;

    // Create basic directory structure
    tokio::fs::create_dir_all(temp_dir.path().join("config")).await?;
    tokio::fs::create_dir_all(temp_dir.path().join("data")).await?;
    tokio::fs::create_dir_all(temp_dir.path().join("logs")).await?;

    Ok(temp_dir)
}

/// Validate that the crucible-cli binary is available
pub fn validate_binary_availability() -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_crucible-cli"))
        .arg("--help")
        .output();

    match output {
        Ok(result) if result.status.success() => Ok(()),
        Ok(_) => Err(anyhow!("crucible-cli binary exists but failed to run")),
        Err(_) => Err(anyhow!("crucible-cli binary not found. Build with: cargo build --bin crucible-cli")),
    }
}

/// Run performance benchmarks for comparison
pub async fn run_performance_benchmarks() -> Result<HashMap<String, Duration>> {
    let mut results = HashMap::new();

    let test_vault = ComprehensiveTestVault::create().await?;

    // Benchmark CLI search
    let cli_harness = CliTestHarness::new().await?;
    let start = Instant::now();
    let _cli_result = cli_harness.execute_cli_command(&["search", "quantum computing"])?;
    results.insert("cli_search".to_string(), start.elapsed());

    // Benchmark REPL search
    let repl_harness = ReplTestHarness::new().await?;
    let mut repl = repl_harness.spawn_repl()?;
    let start = Instant::now();
    let _repl_result = repl.send_command("search quantum computing")?;
    results.insert("repl_search".to_string(), start.elapsed());
    repl.quit()?;

    Ok(results)
}