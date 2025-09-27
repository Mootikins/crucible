-- Initial database schema for Crucible
-- This migration sets up the core tables for document management

-- Documents table stores top-level documents
CREATE TABLE documents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  title VARCHAR(255) NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  metadata JSONB DEFAULT '{}',
  crdt_state BYTEA, -- Yrs document state
  version INTEGER DEFAULT 1
);

-- Document nodes table stores hierarchical document structure
CREATE TABLE document_nodes (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  parent_id UUID REFERENCES document_nodes(id) ON DELETE CASCADE,
  content TEXT NOT NULL,
  title VARCHAR(255),
  position INTEGER DEFAULT 0,
  collapsed BOOLEAN DEFAULT FALSE,
  properties JSONB DEFAULT '{}',
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for performance
CREATE INDEX idx_document_nodes_document_id ON document_nodes(document_id);
CREATE INDEX idx_document_nodes_parent_id ON document_nodes(parent_id);
CREATE INDEX idx_document_nodes_position ON document_nodes(document_id, position);
CREATE INDEX idx_documents_updated_at ON documents(updated_at);

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Triggers for automatic timestamp updates
CREATE TRIGGER update_documents_updated_at
    BEFORE UPDATE ON documents
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_document_nodes_updated_at
    BEFORE UPDATE ON document_nodes
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();