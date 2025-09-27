-- CRDT-specific tables and extensions
-- This migration adds support for CRDT operations and sync state

-- CRDT operations log for tracking changes
CREATE TABLE crdt_operations (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  operation_type VARCHAR(50) NOT NULL, -- 'insert', 'delete', 'update'
  node_id UUID REFERENCES document_nodes(id) ON DELETE CASCADE,
  operation_data JSONB NOT NULL,
  timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  client_id UUID,
  applied BOOLEAN DEFAULT TRUE
);

-- Document sync state for collaboration
CREATE TABLE document_sync_state (
  document_id UUID PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  last_sync TIMESTAMP WITH TIME ZONE,
  sync_version INTEGER DEFAULT 0,
  conflict_state JSONB DEFAULT '{}',
  remote_crdt_state BYTEA
);

-- Sessions for tracking user sessions
CREATE TABLE sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  client_id UUID NOT NULL,
  cursor_position INTEGER DEFAULT 0,
  selected_node_id UUID REFERENCES document_nodes(id),
  view_state JSONB DEFAULT '{}',
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(document_id, client_id)
);

-- Undo/redo history
CREATE TABLE undo_redo_history (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  operation_type VARCHAR(20) NOT NULL, -- 'undo', 'redo'
  snapshot_data JSONB NOT NULL,
  timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for performance
CREATE INDEX idx_crdt_operations_document_id ON crdt_operations(document_id);
CREATE INDEX idx_crdt_operations_timestamp ON crdt_operations(timestamp);
CREATE INDEX idx_sessions_document_id ON sessions(document_id);
CREATE INDEX idx_sessions_client_id ON sessions(client_id);
CREATE INDEX idx_undo_redo_history_document_id ON undo_redo_history(document_id);
CREATE INDEX idx_undo_redo_history_session_id ON undo_redo_history(session_id);

-- Update trigger for sessions
CREATE TRIGGER update_sessions_updated_at
    BEFORE UPDATE ON sessions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();