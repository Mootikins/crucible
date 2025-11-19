# Add Query System for Context Enrichment

## Why

The ACP-MVP's core value proposition is intelligent context enrichment where agents receive relevant knowledge to enhance their responses. While Crucible has working semantic search capabilities, there's no formal specification for how agents should query the kiln, rank results, and format context for prompt injection.

Currently, context enrichment happens at the CLI level (`cru chat`), but there's no standardized query interface that agents can use directly. This creates several problems:

1. **Inconsistent Context**: Different agents may receive different context formats for the same queries
2. **Limited Agent Autonomy**: Agents can't proactively search for additional relevant information
3. **No Query Optimization**: Missing standardized approaches for query expansion, filtering, and ranking
4. **Performance Gaps**: No performance requirements or optimization strategies defined

The query system specification will define how agents discover, retrieve, and rank knowledge for context enrichment, ensuring consistent high-quality agent interactions across the ecosystem.

## What Changes

**NEW CAPABILITY:**

**Standardized Query Interface:**
- Define consistent query patterns for semantic search, metadata filtering, and hybrid approaches
- Specify result formats optimized for agent consumption and prompt injection
- Create query expansion and optimization strategies for better recall
- Establish performance requirements and caching strategies

**Context Enrichment Algorithms:**
- Define ranking algorithms that balance relevance, recency, and diversity
- Specify context window optimization strategies for large result sets
- Create query intent understanding for better result filtering
- Establish serendipity mechanisms for discovering unexpected connections

**Agent Query Patterns:**
- Define standard query types: exploratory, targeted, comparative, temporal
- Specify query refinement and iteration patterns for agent workflows
- Create query result summarization techniques for context optimization
- Establish query quality metrics and feedback mechanisms

**Integration with Tool System:**
- Connect query system to the semantic search tool from tool-system spec
- Define query permission boundaries and access controls
- Specify query result caching and performance optimization
- Create query analytics and monitoring capabilities

**Performance and Scalability:**
- Define performance requirements (<100ms for typical queries)
- Specify caching strategies for frequent query patterns
- Create query optimization and indexing requirements
- Establish monitoring and alerting for query performance

## Impact

### Affected Specs
- **query-system** (NEW) - Define standardized query interface for agent context enrichment
- **tool-system** (reference) - Semantic search tool will be primary query interface
- **acp-integration** (future) - Query system will be core of agent knowledge access

### Affected Code
**New Components:**
- `crates/crucible-query/` - NEW - Dedicated query system crate
- `crates/crucible-query/src/context_enrichment.rs` - Context enrichment algorithms
- `crates/crucible-query/src/query_optimizer.rs` - Query optimization and expansion
- `crates/crucible-query/src/result_ranking.rs` - Ranking and relevance algorithms

**Integration Points:**
- `crates/crucible-cli/src/acp/context_enrichment.rs` - NEW - ACP context injection
- `crates/crucible-tools/src/search_tools.rs` - Enhanced with query system patterns
- `crates/crucible-surrealdb/src/kiln_integration.rs` - Backend query optimizations

**Existing Enhancements:**
- `crates/crucible-cli/src/commands/semantic.rs` - Refactored to use query system
- `crates/crucible-cli/src/core_facade.rs` - Enhanced with query system integration
- ACP client context enrichment workflows

### Implementation Strategy

**Phase 1: Core Query Interface (Week 1)**
- Implement standardized query result formats for agent consumption
- Create basic ranking algorithms (relevance, recency, diversity)
- Define query types and patterns (exploratory, targeted, etc.)
- Integration with existing semantic search implementation

**Phase 2: Context Enrichment (Week 1-2)**
- Implement context window optimization strategies
- Create query expansion and optimization algorithms
- Add query result summarization techniques
- Performance optimization and caching

**Phase 3: Advanced Features (Week 2)**
- Implement serendipity and discovery mechanisms
- Add query quality metrics and feedback systems
- Create monitoring and analytics capabilities
- Comprehensive testing with agent workflows

### User-Facing Impact
- **Better Agent Responses**: Higher quality context leads to more relevant and insightful agent responses
- **Consistent Experience**: Standardized queries ensure consistent agent behavior across different tools
- **Faster Responses**: Performance optimizations and caching reduce query latency
- **Intelligent Discovery**: Query expansion and serendipity help agents discover unexpected relevant information

### Timeline
- **Week 1**: Core query interface and basic ranking
- **Week 2**: Context enrichment algorithms and optimization
- **Estimated effort**: 2 weeks for complete implementation

### Dependencies
- Tool system specification (semantic search tool)
- Existing semantic search implementation (already complete)
- SurrealDB kiln integration and indexing
- ACP client integration (parallel development)