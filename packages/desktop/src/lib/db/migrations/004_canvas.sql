-- Canvas and spatial data tables
-- This migration adds support for canvas node positioning and connections

-- Canvas nodes table for spatial positioning
CREATE TABLE canvas_nodes (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  node_id UUID REFERENCES document_nodes(id) ON DELETE CASCADE,
  x FLOAT NOT NULL DEFAULT 0,
  y FLOAT NOT NULL DEFAULT 0,
  width FLOAT NOT NULL DEFAULT 200,
  height FLOAT NOT NULL DEFAULT 100,
  automerge_state TEXT, -- Base64 encoded Automerge state
  color VARCHAR(7) DEFAULT '#007ACC',
  shape VARCHAR(20) DEFAULT 'rectangle',
  z_index INTEGER DEFAULT 0,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Canvas edges table for connections between nodes
CREATE TABLE canvas_edges (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  from_node_id UUID NOT NULL REFERENCES canvas_nodes(id) ON DELETE CASCADE,
  to_node_id UUID NOT NULL REFERENCES canvas_nodes(id) ON DELETE CASCADE,
  edge_type VARCHAR(20) DEFAULT 'related', -- 'parent', 'child', 'related', 'custom'
  label VARCHAR(255),
  color VARCHAR(7) DEFAULT '#666666',
  width FLOAT DEFAULT 2,
  style VARCHAR(20) DEFAULT 'solid', -- 'solid', 'dashed', 'dotted'
  properties JSONB DEFAULT '{}',
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT no_self_edges CHECK (from_node_id != to_node_id)
);

-- Canvas viewport state for each user/session
CREATE TABLE canvas_viewports (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  zoom FLOAT NOT NULL DEFAULT 1.0,
  x FLOAT NOT NULL DEFAULT 0,
  y FLOAT NOT NULL DEFAULT 0,
  width FLOAT NOT NULL DEFAULT 800,
  height FLOAT NOT NULL DEFAULT 600,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(document_id, session_id)
);

-- Canvas layout templates
CREATE TABLE canvas_templates (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(100) NOT NULL,
  description TEXT,
  layout_data JSONB NOT NULL,
  is_public BOOLEAN DEFAULT FALSE,
  created_by UUID, -- User ID if available
  created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for performance
CREATE INDEX idx_canvas_nodes_document_id ON canvas_nodes(document_id);
CREATE INDEX idx_canvas_nodes_node_id ON canvas_nodes(node_id);
CREATE INDEX idx_canvas_nodes_position ON canvas_nodes(document_id, x, y);
CREATE INDEX idx_canvas_edges_document_id ON canvas_edges(document_id);
CREATE INDEX idx_canvas_edges_from_node ON canvas_edges(from_node_id);
CREATE INDEX idx_canvas_edges_to_node ON canvas_edges(to_node_id);
CREATE INDEX idx_canvas_viewports_document_id ON canvas_viewports(document_id);
CREATE INDEX idx_canvas_viewports_session_id ON canvas_viewports(session_id);

-- Update trigger for canvas tables
CREATE TRIGGER update_canvas_nodes_updated_at
    BEFORE UPDATE ON canvas_nodes
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_canvas_edges_updated_at
    BEFORE UPDATE ON canvas_edges
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_canvas_viewports_updated_at
    BEFORE UPDATE ON canvas_viewports
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Function to get canvas layout
CREATE OR REPLACE FUNCTION get_canvas_layout(
    doc_id UUID
)
RETURNS TABLE(
    id UUID,
    node_id UUID,
    x FLOAT,
    y FLOAT,
    width FLOAT,
    height FLOAT,
    title VARCHAR,
    content TEXT,
    color VARCHAR,
    shape VARCHAR,
    z_index INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        cn.id,
        cn.node_id,
        cn.x,
        cn.y,
        cn.width,
        cn.height,
        dn.title,
        dn.content,
        cn.color,
        cn.shape,
        cn.z_index
    FROM canvas_nodes cn
    LEFT JOIN document_nodes dn ON cn.node_id = dn.id
    WHERE cn.document_id = doc_id
    ORDER BY cn.z_index, cn.y, cn.x;
END;
$$ LANGUAGE plpgsql;

-- Function to get canvas connections
CREATE OR REPLACE FUNCTION get_canvas_connections(
    doc_id UUID
)
RETURNS TABLE(
    id UUID,
    from_node_id UUID,
    to_node_id UUID,
    edge_type VARCHAR,
    label VARCHAR,
    color VARCHAR,
    width FLOAT,
    style VARCHAR
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        ce.id,
        ce.from_node_id,
        ce.to_node_id,
        ce.edge_type,
        ce.label,
        ce.color,
        ce.width,
        ce.style
    FROM canvas_edges ce
    WHERE ce.document_id = doc_id
    ORDER BY ce.z_index;
END;
$$ LANGUAGE plpgsql;