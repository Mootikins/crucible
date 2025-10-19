-- ============================================================================
-- Crucible Knowledge Vault - Example Queries
-- ============================================================================
-- Demonstrates common query patterns for:
-- 1. Tag filtering
-- 2. Graph traversal (wikilinks)
-- 3. Full-text search
-- 4. Semantic search (embeddings)
-- 5. Hybrid queries (combining multiple patterns)
-- ============================================================================

-- ============================================================================
-- 1. TAG FILTERING QUERIES
-- ============================================================================

-- Basic: Find all notes with a specific tag
SELECT path, title, tags
FROM notes
WHERE tags CONTAINS "#rust"
ORDER BY modified_at DESC;

-- Multiple tags (AND logic - must have ALL tags)
SELECT path, title, tags
FROM notes
WHERE tags CONTAINSALL ["#rust", "#database"]
ORDER BY modified_at DESC;

-- Multiple tags (OR logic - must have ANY tag)
SELECT path, title, tags
FROM notes
WHERE tags CONTAINSANY ["#project", "#task", "#idea"]
ORDER BY modified_at DESC;

-- Tag with metadata filter
SELECT path, title, tags, metadata.status
FROM notes
WHERE tags CONTAINS "#project"
  AND metadata.status = "active"
ORDER BY modified_at DESC
LIMIT 20;

-- Hierarchical tags (find all notes with parent tag or child tags)
SELECT path, title, tags
FROM notes
WHERE tags CONTAINS SOME (
    SELECT name FROM tags
    WHERE name = "#project"
       OR parent_tag = tags:project
);

-- Tag usage statistics
SELECT
    name,
    usage_count,
    last_used,
    parent_tag
FROM tags
ORDER BY usage_count DESC
LIMIT 20;

-- Find notes without tags (untagged)
SELECT path, title
FROM notes
WHERE array::len(tags) = 0
ORDER BY modified_at DESC;

-- Tag co-occurrence (find tags often used together)
SELECT
    tags[0] AS tag1,
    tags[1] AS tag2,
    count() AS co_occurrence
FROM notes
WHERE array::len(tags) >= 2
GROUP BY tag1, tag2
ORDER BY co_occurrence DESC
LIMIT 20;

-- ============================================================================
-- 2. GRAPH TRAVERSAL QUERIES (Wikilinks)
-- ============================================================================

-- Direct outgoing links (notes linked FROM this note)
SELECT
    path,
    title,
    ->wikilink->notes.(path, title, tags) AS linked_notes
FROM notes:projects/crucible.md;

-- Backlinks (notes linking TO this note)
SELECT
    path,
    title,
    <-wikilink<-notes.(path, title, tags) AS referring_notes
FROM notes:projects/crucible.md;

-- Two-hop traversal (notes linked by linked notes)
SELECT
    path,
    title,
    ->wikilink->notes->wikilink->notes.(path, title) AS second_degree
FROM notes:projects/crucible.md;

-- Find orphan notes (no incoming or outgoing links)
SELECT path, title, created_at
FROM notes
WHERE array::len(<-wikilink) = 0
  AND array::len(->wikilink) = 0
ORDER BY created_at DESC;

-- Hub analysis (most linked notes)
SELECT
    path,
    title,
    count(<-wikilink) AS backlink_count,
    count(->wikilink) AS outlink_count,
    (count(<-wikilink) + count(->wikilink)) AS total_connections
FROM notes
GROUP BY path, title
ORDER BY backlink_count DESC
LIMIT 20;

-- Find broken links (wikilinks pointing to non-existent notes)
SELECT
    from.path AS source,
    link_text,
    to AS target
FROM wikilink
WHERE to NOT IN (SELECT id FROM notes);

-- Shortest path between two notes
SELECT path
FROM notes:start.md TO notes:end.md
VIA wikilink
LIMIT 1;

-- All paths up to depth 3 (recursive traversal)
SELECT
    path,
    title,
    ->wikilink->notes AS depth1,
    ->wikilink->notes->wikilink->notes AS depth2,
    ->wikilink->notes->wikilink->notes->wikilink->notes AS depth3
FROM notes:seed.md;

-- Bidirectional graph (notes that both link to AND are linked from a note)
SELECT
    path,
    title,
    (
        SELECT path FROM ->wikilink->notes
        WHERE id IN (SELECT <-wikilink<-notes.id FROM $parent)
    ) AS mutual_links
FROM notes:center.md;

-- Cluster detection (find highly interconnected notes)
SELECT
    a.path,
    count(DISTINCT b.id) AS connected_notes
FROM notes AS a
JOIN notes AS b ON (
    b.id IN a->wikilink->notes
    OR b.id IN a<-wikilink<-notes
)
GROUP BY a.path
HAVING connected_notes > 5
ORDER BY connected_notes DESC;

-- ============================================================================
-- 3. FULL-TEXT SEARCH QUERIES
-- ============================================================================

-- Basic full-text search (content only)
SELECT
    path,
    title,
    search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "knowledge management"
ORDER BY relevance DESC
LIMIT 20;

-- Full-text search with tag filter
SELECT
    path,
    title,
    tags,
    search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "SurrealDB schema design"
  AND tags CONTAINS "#database"
ORDER BY relevance DESC
LIMIT 20;

-- Multi-field search (title weighted 2x, content 1x)
SELECT
    path,
    title,
    (search::score(1) * 2 + search::score(2)) AS combined_relevance
FROM notes
WHERE title_text @1@ "architecture"
   OR content_text @2@ "architecture"
ORDER BY combined_relevance DESC
LIMIT 20;

-- Search with highlighted snippets
SELECT
    path,
    title,
    search::highlight('<mark>', '</mark>', 1) AS snippet,
    search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "graph database"
ORDER BY relevance DESC
LIMIT 10;

-- Phrase search (exact match)
SELECT path, title, tags
FROM notes
WHERE content_text @@ '"distributed systems"'
ORDER BY modified_at DESC;

-- Boolean search (AND, OR, NOT operators)
SELECT
    path,
    title,
    search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "rust AND database NOT sqlite"
ORDER BY relevance DESC;

-- Search within a specific folder
SELECT
    path,
    title,
    search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "agent orchestration"
  AND string::starts_with(path, "Projects/")
ORDER BY relevance DESC;

-- Recent documents matching query
SELECT
    path,
    title,
    search::score(1) AS relevance,
    modified_at
FROM notes
WHERE content_text @1@ "machine learning"
  AND modified_at > time::now() - 30d
ORDER BY relevance DESC, modified_at DESC;

-- ============================================================================
-- 4. SEMANTIC SEARCH QUERIES (Embeddings)
-- ============================================================================

-- Vector similarity search (cosine distance)
-- Note: $query_embedding would be provided by the application
LET $query_embedding = [0.123, -0.456, 0.789, /* ... 384 dimensions ... */];

SELECT
    path,
    title,
    tags,
    vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
ORDER BY similarity ASC  -- Lower distance = higher similarity
LIMIT 10;

-- Semantic search with minimum similarity threshold
LET $query_embedding = [/* embedding vector */];
LET $min_similarity = 0.3;  -- Adjust based on your model

SELECT
    path,
    title,
    vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
  AND vector::distance::cosine(embedding, $query_embedding) < $min_similarity
ORDER BY similarity ASC
LIMIT 10;

-- Hybrid: Semantic search + tag filter
LET $query_embedding = [/* embedding vector */];

SELECT
    path,
    title,
    tags,
    vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
  AND tags CONTAINS "#rust"
ORDER BY similarity ASC
LIMIT 10;

-- Hybrid: Semantic + full-text (weighted combination)
LET $query_embedding = [/* embedding vector */];
LET $semantic_weight = 0.7;
LET $text_weight = 0.3;

SELECT
    path,
    title,
    vector::distance::cosine(embedding, $query_embedding) AS semantic_score,
    search::score(1) AS text_score,
    (semantic_score * $semantic_weight + text_score * $text_weight) AS combined_score
FROM notes
WHERE embedding IS NOT NONE
  AND content_text @1@ "agent orchestration"
ORDER BY combined_score ASC
LIMIT 10;

-- Find similar notes using precomputed relations
SELECT
    path,
    title,
    ->relates_to[WHERE relation_type = "similar"]->(notes AS similar_note).(path, title, score)
FROM notes:foo.md
ORDER BY similar_note.score DESC
LIMIT 10;

-- Semantic search within a date range
LET $query_embedding = [/* embedding vector */];

SELECT
    path,
    title,
    modified_at,
    vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
  AND modified_at >= <datetime>"2025-01-01T00:00:00Z"
  AND modified_at <= <datetime>"2025-12-31T23:59:59Z"
ORDER BY similarity ASC
LIMIT 10;

-- Diversity search (semantic search with dissimilarity constraint)
-- Find top-K results where each result is sufficiently different from previous ones
LET $query_embedding = [/* embedding vector */];
LET $diversity_threshold = 0.5;

-- First result (most similar)
LET $result1 = (
    SELECT path, title, embedding
    FROM notes
    WHERE embedding IS NOT NONE
    ORDER BY vector::distance::cosine(embedding, $query_embedding) ASC
    LIMIT 1
);

-- Second result (similar to query, dissimilar to result1)
LET $result2 = (
    SELECT path, title, embedding
    FROM notes
    WHERE embedding IS NOT NONE
      AND path != $result1.path
      AND vector::distance::cosine(embedding, $result1.embedding) > $diversity_threshold
    ORDER BY vector::distance::cosine(embedding, $query_embedding) ASC
    LIMIT 1
);

-- Return diverse results
SELECT * FROM $result1 UNION SELECT * FROM $result2;

-- ============================================================================
-- 5. HYBRID QUERIES (Combining Multiple Patterns)
-- ============================================================================

-- Active projects with recent updates and backlinks
SELECT
    path,
    title,
    modified_at,
    tags,
    count(<-wikilink) AS backlink_count,
    metadata.status
FROM notes
WHERE tags CONTAINS "#project"
  AND metadata.status = "active"
  AND modified_at > time::now() - 7d
ORDER BY backlink_count DESC, modified_at DESC;

-- Discover related notes (tags + links + semantic)
LET $query_embedding = [/* embedding vector */];

SELECT
    path,
    title,
    tags,
    -- Similar tags
    (
        SELECT path, title
        FROM notes
        WHERE tags CONTAINSANY $parent.tags
          AND id != $parent.id
        LIMIT 5
    ) AS similar_by_tags,
    -- Outgoing links
    ->wikilink->notes.(path, title) AS linked_notes,
    -- Backlinks
    <-wikilink<-notes.(path, title) AS referring_notes,
    -- Semantic similarity
    vector::distance::cosine(embedding, $query_embedding) AS semantic_similarity
FROM notes:foo.md;

-- Build topic graph (notes + connections for visualization)
SELECT
    path,
    title,
    tags,
    array::len(->wikilink) AS outgoing_links,
    array::len(<-wikilink) AS incoming_links,
    ->wikilink->notes.(path, title, tags) AS connections
FROM notes
WHERE tags CONTAINS "#knowledge-graph"
ORDER BY (incoming_links + outgoing_links) DESC;

-- Find entry points (highly linked notes with broad topics)
SELECT
    path,
    title,
    tags,
    count(<-wikilink) AS popularity,
    count(DISTINCT ->wikilink->notes.folder) AS folder_diversity
FROM notes
WHERE count(<-wikilink) > 10
GROUP BY path, title, tags
ORDER BY popularity DESC, folder_diversity DESC
LIMIT 20;

-- Context-aware search (search + graph + tags)
LET $context_note = notes:current-context.md;

SELECT
    n.path,
    n.title,
    n.tags,
    search::score(1) AS text_relevance,
    -- Boost if linked from context note
    IF(n.id IN $context_note->wikilink->notes, 1.5, 1.0) AS link_boost,
    -- Boost if shares tags with context note
    array::len(array::intersect(n.tags, $context_note.tags)) AS tag_overlap
FROM notes AS n
WHERE content_text @1@ "your search query"
ORDER BY (text_relevance * link_boost * (1 + tag_overlap * 0.5)) DESC
LIMIT 20;

-- Recommendation engine (based on user's recent notes)
LET $recent_notes = (
    SELECT id, tags, embedding
    FROM notes
    WHERE metadata.read_by = "current_user"
    ORDER BY metadata.last_read DESC
    LIMIT 10
);

-- Average embedding of recent notes
LET $user_interest_embedding = vector::mean(
    SELECT embedding FROM $recent_notes WHERE embedding IS NOT NONE
);

-- Find similar notes not yet read
SELECT
    path,
    title,
    tags,
    vector::distance::cosine(embedding, $user_interest_embedding) AS relevance
FROM notes
WHERE embedding IS NOT NONE
  AND metadata.read_by != "current_user"
  AND tags CONTAINSANY (SELECT tags FROM $recent_notes)
ORDER BY relevance ASC
LIMIT 20;

-- Temporal analysis (how a topic evolved over time)
SELECT
    date::format(created_at, "%Y-%m") AS month,
    count() AS note_count,
    array::flatten(tags) AS all_tags
FROM notes
WHERE tags CONTAINS "#rust"
  AND created_at >= time::now() - 1y
GROUP BY month
ORDER BY month ASC;

-- ============================================================================
-- 6. ANALYTICAL QUERIES (Statistics & Insights)
-- ============================================================================

-- Vault statistics
SELECT
    count() AS total_notes,
    count(CASE WHEN embedding IS NOT NONE THEN 1 END) AS notes_with_embeddings,
    avg(array::len(tags)) AS avg_tags_per_note,
    avg(count(<-wikilink)) AS avg_backlinks,
    avg(string::len(content)) AS avg_content_length
FROM notes;

-- Tag distribution
SELECT
    tag,
    count() AS usage_count,
    avg(string::len(content)) AS avg_note_length
FROM notes,
     UNNEST tags AS tag
GROUP BY tag
ORDER BY usage_count DESC
LIMIT 20;

-- Folder statistics
SELECT
    folder,
    count() AS note_count,
    avg(count(<-wikilink)) AS avg_backlinks,
    max(modified_at) AS last_modified
FROM notes
GROUP BY folder
ORDER BY note_count DESC;

-- Link density over time (notes created each month + avg links)
SELECT
    date::format(created_at, "%Y-%m") AS month,
    count() AS notes_created,
    avg(array::len(->wikilink)) AS avg_outgoing_links
FROM notes
GROUP BY month
ORDER BY month DESC;

-- Most influential notes (PageRank-style)
-- Simplified version: count of backlinks weighted by the popularity of linking notes
SELECT
    n.path,
    n.title,
    count(<-wikilink) AS direct_backlinks,
    sum(count(backlink_source<-wikilink)) AS indirect_influence
FROM notes AS n,
     n<-wikilink<-notes AS backlink_source
GROUP BY n.path, n.title
ORDER BY indirect_influence DESC
LIMIT 20;

-- Tag correlation matrix (which tags appear together)
SELECT
    t1.name AS tag1,
    t2.name AS tag2,
    count() AS co_occurrence
FROM notes AS n,
     UNNEST n.tags AS t1,
     UNNEST n.tags AS t2
WHERE t1 != t2
GROUP BY tag1, tag2
HAVING co_occurrence > 3
ORDER BY co_occurrence DESC;

-- Orphan clusters (groups of notes with no external links)
SELECT
    a.path,
    a.title,
    array::len(a->wikilink->notes) AS internal_links
FROM notes AS a
WHERE array::len(a<-wikilink) = 0
  AND array::len(a->wikilink->notes) > 0
ORDER BY internal_links DESC;

-- ============================================================================
-- 7. MAINTENANCE QUERIES
-- ============================================================================

-- Find notes missing embeddings
SELECT path, title, modified_at
FROM notes
WHERE embedding IS NONE
ORDER BY modified_at DESC;

-- Find stale embeddings (content modified after embedding)
SELECT path, title, modified_at, embedding_updated_at
FROM notes
WHERE embedding IS NOT NONE
  AND modified_at > embedding_updated_at
ORDER BY modified_at DESC;

-- Duplicate detection (similar titles)
SELECT
    a.path AS path1,
    b.path AS path2,
    a.title
FROM notes AS a
JOIN notes AS b ON (
    a.title = b.title
    AND a.id < b.id
)
ORDER BY a.title;

-- Empty notes (no content or very short)
SELECT path, title, string::len(content) AS content_length
FROM notes
WHERE string::len(content) < 100
ORDER BY content_length ASC;

-- Recently modified notes (last 7 days)
SELECT path, title, modified_at, tags
FROM notes
WHERE modified_at > time::now() - 7d
ORDER BY modified_at DESC;

-- ============================================================================
-- 8. BATCH OPERATIONS (Data Management)
-- ============================================================================

-- Add tag to multiple notes
UPDATE notes
SET tags += "#archived"
WHERE metadata.status = "archived"
  AND tags NOT CONTAINS "#archived";

-- Recalculate tag usage counts
UPDATE tags
SET usage_count = (
    SELECT count()
    FROM notes
    WHERE tags CONTAINS ("#" + $parent.name)
);

-- Create similarity edges for all notes (batch precomputation)
-- Warning: Expensive operation for large vaults
FOR $note IN (SELECT id, embedding FROM notes WHERE embedding IS NOT NONE) {
    LET $similar = (
        SELECT id, vector::distance::cosine(embedding, $note.embedding) AS score
        FROM notes
        WHERE embedding IS NOT NONE
          AND id != $note.id
        ORDER BY score ASC
        LIMIT 10
    );

    FOR $similar_note IN $similar {
        RELATE $note.id->relates_to->$similar_note.id SET
            relation_type = "similar",
            score = $similar_note.score,
            computed_at = time::now();
    };
};

-- Cleanup broken wikilinks
DELETE FROM wikilink
WHERE out NOT IN (SELECT id FROM notes);

-- Archive old notes to separate table
-- Step 1: Copy to archive
INSERT INTO notes_archive (
    SELECT * FROM notes
    WHERE modified_at < time::now() - 1y
      AND metadata.archived != false
);

-- Step 2: Delete from main table
DELETE FROM notes
WHERE modified_at < time::now() - 1y
  AND metadata.archived != false;

-- ============================================================================
-- END OF EXAMPLES
-- ============================================================================

-- To run a specific query:
-- 1. Copy the query
-- 2. Replace placeholders (e.g., $query_embedding, :note-id)
-- 3. Execute via SurrealDB CLI or client library

-- For programmatic access from Rust:
-- See crates/crucible-surrealdb/src/queries.rs for type-safe query builders
