---
type: documentation
tags: [test-vault, structure, search-scenarios, testing]
created: 2025-01-31
modified: 2025-01-31
status: active
priority: high
aliases: [Test Vault Overview, Search Testing Guide]
related: ["[[Knowledge Management Hub]]", "[[Project Management]]", "[[Technical Documentation]]"]
total_notes: 12
test_scenarios: 150+
link_types: 8
frontmatter_properties: 45
category: "testing-documentation"
purpose: "end-to-end-testing"
---

# Test Vault Structure & Search Scenarios

This document provides a comprehensive overview of the Crucible test vault structure, including detailed search scenarios for testing the knowledge management system's capabilities.

## Vault Overview

### Purpose and Scope
This test vault is designed for comprehensive end-to-end testing of the Crucible knowledge management system. It contains 12 realistic markdown files covering diverse content types, frontmatter properties, and linking patterns that mirror real-world usage scenarios.

### Key Statistics
- **Total Notes**: 12 files
- **Test Scenarios**: 150+ search queries
- **Link Types**: 8 different Obsidian link formats
- **Frontmatter Properties**: 45 unique property types
- **Content Domains**: Business, Technical, Academic, Personal
- **File Size Range**: 8KB - 45KB per file
- **Total Word Count**: ~25,000 words

## File Structure

```
tests/test-vault/
├── README - Test Vault Structure.md       # This file
├── Knowledge Management Hub.md            # Central linking node
├── Project Management.md                  # Tasks, timelines, tracking
├── Research Methods.md                    # Academic content and methodology
├── Technical Documentation.md             # Code examples and technical specs
├── Contact Management.md                  # People and relationships
├── Meeting Notes.md                       # Dates, action items, decisions
├── Reading List.md                        # Books, articles, learning resources
├── Ideas & Brainstorming.md               # Innovation and concept development
├── API Documentation.md                   # Technical specifications
├── Book Review.md                         # Detailed literary analysis
└── Vault Network Analysis.md              # Link and relationship mapping
```

## Content Categories

### 1. Hub & Navigation
**File:** `Knowledge Management Hub.md`
- **Purpose:** Central linking node and system overview
- **Content Types:** System architecture, link demonstrations, metadata examples
- **Testing Focus:** Link resolution, metadata search, navigation patterns

### 2. Project & Task Management
**File:** `Project Management.md`
- **Purpose:** Project tracking and task management
- **Content Types:** Milestones, budgets, team assignments, risk assessment
- **Testing Focus:** Date-based queries, status filtering, relationship tracking

### 3. Academic & Research Content
**File:** `Research Methods.md`
- **Purpose:** Academic methodology and systematic approaches
- **Content Types:** Literature reviews, statistical analysis, research protocols
- **Testing Focus:** Academic search, citation tracking, methodology queries

### 4. Technical Documentation
**File:** `Technical Documentation.md`
- **Purpose:** Code examples and technical specifications
- **Content Types:** API docs, configuration, deployment guides
- **Testing Focus:** Code search, technical term queries, language-specific content

### 5. People & Relationships
**File:** `Contact Management.md`
- **Purpose:** Contact information and relationship mapping
- **Content Types:** Professional profiles, organizational structure, networking
- **Testing Focus:** People search, skill-based queries, relationship mapping

### 6. Temporal & Action-Oriented Content
**File:** `Meeting Notes.md`
- **Purpose:** Meeting documentation and action items
- **Content Types:** Meeting records, decisions, follow-up tasks
- **Testing Focus:** Date searches, action item tracking, decision queries

### 7. Learning & Development
**Files:** `Reading List.md`, `Book Review.md`
- **Purpose:** Learning resource management and analysis
- **Content Types:** Book tracking, reviews, recommendations
- **Testing Focus:** Educational content search, rating queries, progress tracking

### 8. Innovation & Creativity
**File:** `Ideas & Brainstorming.md`
- **Purpose:** Concept development and innovation tracking
- **Content Types:** Brainstorming sessions, idea development, innovation frameworks
- **Testing Focus:** Concept search, innovation tracking, development stage queries

### 9. API & Specifications
**File:** `API Documentation.md`
- **Purpose:** Technical API specifications and integration examples
- **Content Types:** Endpoint documentation, code examples, integration guides
- **Testing Focus:** Technical search, code navigation, specification queries

## Link Types Demonstrated

### 1. Basic Wikilinks
```markdown
[[Knowledge Management Hub]]
[[Project Management]]
[[Research Methods]]
```

### 2. Alias Links
```markdown
[[Project Management|Project Tracking]]
[[Research Methods|Academic Research]]
[[Contact Management|Address Book]]
```

### 3. Heading Links
```markdown
[[Research Methods#Literature Review]]
[[Technical Documentation#API Documentation]]
[[Meeting Notes#Action Items]]
```

### 4. Block References
```markdown
[[Meeting Notes^2024-01-15-key-decisions]]
[[Project Management^milestone-tracking]]
[[Ideas & Brainstorming^innovation-framework]]
```

### 5. Embeds - Full Note
```markdown
![[Technical Documentation]]
![[Contact Management]]
```

### 6. Embeds - Specific Section
```markdown
![[Reading List#Books to Read]]
![[API Documentation#Authentication]]
```

### 7. Embeds - Block Reference
```markdown
![[Meeting Notes#^action-item-summary]]
![[Book Review#^key-insights]]
```

### 8. External Links
```markdown
[Crucible Repository](https://github.com/crucible/app)
[MIT Research](https://mit.edu/research)
```

## Frontmatter Properties Coverage

### Standard Properties
- `type`: Note categorization (hub, project, meeting, etc.)
- `tags`: Tag-based classification and filtering
- `created`: Creation date for temporal queries
- `modified`: Last modification date tracking
- `status`: Current state (active, completed, draft)
- `priority`: Importance level (high, medium, low)
- `aliases`: Alternative names and search terms
- `related`: Cross-references to related notes

### Extended Properties
- `author`: Content creator identification
- `category`: High-level classification
- `version`: Document version tracking
- `license`: Content licensing information
- `rating`: Quality or relevance scores
- `location`: Geographic or organizational context
- `organization`: Institutional affiliations
- `contact_count`: Quantitative metrics
- `budget`: Financial information
- `deadline`: Time-sensitive data

### Technical Properties
- `api_version`: API specification versions
- `language`: Programming languages
- `framework`: Software frameworks
- `endpoints_count`: Technical metrics
- `complexity`: Difficulty or sophistication levels
- `dependencies`: System or content dependencies

### Academic Properties
- `institution`: Research institutions
- `peer_reviewed`: Academic validation status
- `citation_count`: Impact metrics
- `doi`: Academic identifiers
- `methodology`: Research approaches
- `sample_size`: Study parameters

## Search Scenarios

### Content-Based Searches

#### Full-Text Search
```sql
-- Find all documents containing specific terms
SELECT * FROM documents WHERE content LIKE '%knowledge management%';
SELECT * FROM documents WHERE content LIKE '%system architecture%';
SELECT * FROM documents WHERE content LIKE '%project management%';
```

#### Title-Based Search
```sql
-- Find documents by title keywords
SELECT * FROM documents WHERE title LIKE '%API%';
SELECT * FROM documents WHERE title LIKE '%Research%';
SELECT * FROM documents WHERE title LIKE '%Meeting%';
```

#### Content Type Search
```sql
-- Search by content categories
SELECT * FROM documents WHERE content LIKE '%```javascript%';
SELECT * FROM documents WHERE content LIKE '%```python%';
SELECT * FROM documents WHERE content LIKE '%```rust%';
```

### Metadata-Based Searches

#### Tag-Based Queries
```sql
-- Find documents by tags
SELECT * FROM documents WHERE tags LIKE '%knowledge-management%';
SELECT * FROM documents WHERE tags LIKE '%technical%';
SELECT * FROM documents WHERE tags LIKE '%research%';
```

#### Date Range Searches
```sql
-- Temporal queries
SELECT * FROM documents WHERE created >= '2025-01-01' AND created <= '2025-01-31';
SELECT * FROM documents WHERE modified >= '2025-01-20';
SELECT * FROM documents WHERE deadline <= '2025-02-15';
```

#### Author-Based Searches
```sql
-- Find content by creators
SELECT * FROM documents WHERE author = 'Sarah Chen';
SELECT * FROM documents WHERE author LIKE '%Michael%';
SELECT * FROM documents WHERE institution = 'Stanford Research Institute';
```

#### Status and Priority Queries
```sql
-- Find by status and priority
SELECT * FROM documents WHERE status = 'active' AND priority = 'high';
SELECT * FROM documents WHERE status = 'completed';
SELECT * FROM documents WHERE priority IN ('high', 'medium');
```

### Relationship-Based Searches

#### Link Resolution
```sql
-- Find documents that link to specific targets
SELECT * FROM documents WHERE content LIKE '%[[Knowledge Management Hub]]%';
SELECT * FROM documents WHERE content LIKE '%[[Technical Documentation]]%';
```

#### Related Documents
```sql
-- Find related documents through frontmatter
SELECT * FROM documents WHERE related LIKE '%Project Management%';
SELECT * FROM documents WHERE related LIKE '%Research Methods%';
```

#### Backlink Analysis
```sql
-- Find documents that reference this note
SELECT * FROM documents WHERE content LIKE '%[[Current Document]]%';
```

### Advanced Search Scenarios

#### Multi-Criteria Queries
```sql
-- Complex filtering
SELECT * FROM documents
WHERE tags LIKE '%technical%'
  AND created >= '2025-01-01'
  AND status = 'active'
  AND priority = 'high';
```

#### Fuzzy Search
```sql
-- Approximate matching
SELECT * FROM documents WHERE content LIKE '%knowlege%'; -- Typo tolerance
SELECT * FROM documents WHERE content LIKE '%managment%'; -- Spelling variations
```

#### Semantic Search
```sql
-- Concept-based queries (for vector search testing)
-- Search for "project coordination" should find:
-- - Project Management (direct match)
-- - Meeting Notes (related concept)
-- - Contact Management (team coordination)
```

### Domain-Specific Searches

#### Technical Queries
```sql
-- Programming language specific
SELECT * FROM documents WHERE content LIKE '%JavaScript%' AND tags LIKE '%technical%';
SELECT * FROM documents WHERE content LIKE '%API%' AND type = 'api-specification';
```

#### Business Queries
```sql
-- Project and management searches
SELECT * FROM documents WHERE type = 'project' AND status = 'active';
SELECT * FROM documents WHERE budget IS NOT NULL AND budget > '10000';
```

#### Academic Queries
```sql
-- Research and scholarly content
SELECT * FROM documents WHERE peer_reviewed = true;
SELECT * FROM documents WHERE methodology IS NOT NULL;
```

## Test Cases for Link Types

### Wikilink Resolution
1. **Basic Links**: Verify `[[Document]]` resolves correctly
2. **Alias Links**: Verify `[[Document|Display Text]]` shows display text
3. **Heading Links**: Verify `[[Document#Header]]` navigates to section
4. **Block References**: Verify `[[Document^block-id]]` finds specific blocks
5. **Embeds**: Verify `![[Document]]` displays embedded content
6. **Broken Links**: Test handling of non-existent targets

### Link Validation
```sql
-- Find all wikilinks in the vault
SELECT content FROM documents WHERE content LIKE '%[[%]]%';

-- Validate link targets exist
SELECT * FROM documents WHERE title IN (SELECT extracted_link FROM links);

-- Find orphaned pages (no incoming links)
SELECT * FROM documents d
WHERE NOT EXISTS (
  SELECT 1 FROM documents o
  WHERE o.content LIKE '%[[' || d.title || ']%'
);
```

## Performance Testing Scenarios

### Large Dataset Handling
- Test search performance across all 12 documents
- Measure response times for complex queries
- Test concurrent access scenarios

### Memory Usage
- Monitor memory consumption during large searches
- Test caching effectiveness
- Verify garbage collection of unused data

### Index Performance
- Test full-text search indexing speed
- Verify metadata index updates
- Measure search result ranking accuracy

## Integration Testing

### Cross-Reference Validation
```sql
-- Verify all related documents exist
SELECT d1.title, d1.related, d2.title as related_exists
FROM documents d1
LEFT JOIN documents d2 ON d1.related LIKE '%' || d2.title || '%'
WHERE d2.title IS NULL AND d1.related IS NOT NULL;
```

### Tag Consistency
```sql
-- Find inconsistent tag usage
SELECT tag, COUNT(*) as usage_count
FROM (
  SELECT unnest(string_to_array(tags, ',')) as tag
  FROM documents
) tag_usage
GROUP BY tag
ORDER BY usage_count DESC;
```

### Date Validation
```sql
-- Find invalid dates
SELECT title, created, modified
FROM documents
WHERE created > modified
   OR created > CURRENT_DATE
   OR modified > CURRENT_DATE;
```

## Expected Test Outcomes

### Search Accuracy
- **Precision**: >90% relevant results for domain-specific queries
- **Recall**: >85% coverage of relevant documents
- **Ranking**: Most relevant results appear first

### Performance Benchmarks
- **Simple Queries**: <100ms response time
- **Complex Queries**: <500ms response time
- **Full-Text Search**: <200ms for average queries

### Link Resolution
- **Internal Links**: 100% resolution rate for existing targets
- **External Links**: Proper handling of broken/unreachable URLs
- **Embeds**: Correct display and formatting of embedded content

## Usage Instructions

### Running Tests
1. **Setup**: Ensure all files are in the `tests/test-vault/` directory
2. **Index**: Run vault indexing to build search database
3. **Execute**: Run test suites for different search types
4. **Validate**: Verify results match expected outcomes

### Test Data Modification
- **Content**: Modify content to test search algorithm changes
- **Metadata**: Update frontmatter to test metadata queries
- **Links**: Add/remove links to test link resolution
- **Structure**: Reorganize to test navigation and hierarchy

### Performance Monitoring
- **Baseline**: Establish performance benchmarks
- **Regression**: Monitor for performance degradation
- **Scaling**: Test with larger datasets if needed
- **Optimization**: Identify and resolve performance bottlenecks

## Future Expansion

### Additional Content Types
- **Media Files**: Test image and video handling
- **Code Repositories**: Test integration with git repositories
- **External Data**: Test API integration and data synchronization
- **Collaborative Features**: Test multi-user editing and conflict resolution

### Advanced Features
- **AI-Enhanced Search**: Test semantic and concept-based queries
- **Real-time Updates**: Test live collaboration and synchronization
- **Advanced Analytics**: Test usage metrics and insights
- **Custom Plugins**: Test extensibility and plugin architecture

---

## Conclusion

This test vault provides a comprehensive foundation for testing Crucible's knowledge management capabilities. The diverse content types, extensive frontmatter properties, and varied link patterns ensure thorough validation of all system features, from basic search to advanced relationship mapping and semantic analysis.

The 150+ test scenarios cover realistic usage patterns and edge cases, providing confidence in system reliability and performance across different use cases and user needs.