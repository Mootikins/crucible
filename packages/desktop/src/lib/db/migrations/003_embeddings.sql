-- Vector embeddings and semantic search tables
-- This migration adds support for vector search and embeddings

-- Enable vector extension if not already enabled
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') THEN
        CREATE EXTENSION vector;
    END IF;
END
$$;

-- Document embeddings table for semantic search
CREATE TABLE document_embeddings (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  node_id UUID REFERENCES document_nodes(id) ON DELETE CASCADE,
  chunk_text TEXT NOT NULL,
  embedding VECTOR(384) NOT NULL,
  position INTEGER DEFAULT 0,
  model VARCHAR(100) DEFAULT 'Xenova/all-MiniLM-L6-v2',
  similarity_threshold FLOAT DEFAULT 0.5,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Search index for fast text search
CREATE TABLE search_index (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  node_id UUID REFERENCES document_nodes(id) ON DELETE CASCADE,
  content_tsvector TSVECTOR NOT NULL,
  title_tsvector TSVECTOR,
  metadata JSONB DEFAULT '{}',
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Tag management
CREATE TABLE tags (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(100) UNIQUE NOT NULL,
  color VARCHAR(7) DEFAULT '#007ACC',
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Document-tag relationships
CREATE TABLE document_tags (
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (document_id, tag_id)
);

-- Create vector index for similarity search
CREATE INDEX idx_document_embeddings_embedding ON document_embeddings USING hnsw (embedding vector_cosine_ops);
CREATE INDEX idx_document_embeddings_document_id ON document_embeddings(document_id);
CREATE INDEX idx_document_embeddings_node_id ON document_embeddings(node_id);

-- Create GIN indexes for full-text search
CREATE INDEX idx_search_index_content ON search_index USING GIN (content_tsvector);
CREATE INDEX idx_search_index_title ON search_index USING GIN (title_tsvector);
CREATE INDEX idx_search_index_document_id ON search_index(document_id);

-- Create indexes for tags
CREATE INDEX idx_document_tags_document_id ON document_tags(document_id);
CREATE INDEX idx_document_tags_tag_id ON document_tags(tag_id);

-- Function to update search index triggers
CREATE OR REPLACE FUNCTION update_search_index()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' OR TG_OP = 'UPDATE' THEN
        INSERT INTO search_index (document_id, node_id, content_tsvector, title_tsvector, metadata)
        VALUES (
            NEW.document_id,
            NEW.id,
            to_tsvector('english', COALESCE(NEW.content, '')),
            to_tsvector('english', COALESCE(NEW.title, '')),
            jsonb_build_object(
                'position', NEW.position,
                'collapsed', NEW.collapsed
            )
        )
        ON CONFLICT (document_id, node_id) DO UPDATE SET
            content_tsvector = to_tsvector('english', COALESCE(NEW.content, '')),
            title_tsvector = to_tsvector('english', COALESCE(NEW.title, '')),
            metadata = jsonb_build_object(
                'position', NEW.position,
                'collapsed', NEW.collapsed
            );
        RETURN NEW;
    ELSIF TG_OP = 'DELETE' THEN
        DELETE FROM search_index WHERE node_id = OLD.id;
        RETURN OLD;
    END IF;
    RETURN NULL;
END;
$$ language 'plpgsql';

-- Triggers for automatic search index updates
CREATE TRIGGER trigger_document_nodes_search
    AFTER INSERT OR UPDATE OR DELETE ON document_nodes
    FOR EACH ROW EXECUTE FUNCTION update_search_index();

-- Function for similarity search
CREATE OR REPLACE FUNCTION search_documents(
    query_vector VECTOR(384),
    limit INTEGER DEFAULT 10,
    threshold FLOAT DEFAULT 0.5
)
RETURNS TABLE(
    id UUID,
    document_id UUID,
    node_id UUID,
    chunk_text TEXT,
    similarity FLOAT,
    title VARCHAR,
    content TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        de.id,
        de.document_id,
        de.node_id,
        de.chunk_text,
        1 - (de.embedding <=> query_vector) as similarity,
        dn.title,
        dn.content
    FROM document_embeddings de
    LEFT JOIN document_nodes dn ON de.node_id = dn.id
    WHERE 1 - (de.embedding <=> query_vector) > threshold
    ORDER BY de.embedding <=> query_vector
    LIMIT limit;
END;
$$ LANGUAGE plpgsql;