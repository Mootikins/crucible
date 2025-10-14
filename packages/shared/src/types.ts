export interface DocumentNode {
  id: string;
  title: string;
  content: string;
  created_at: string;
  updated_at: string;
  parent_id?: string;
  children: string[];
  properties: Record<string, any>;
  collapsed: boolean;
  position: number;
}

export interface CanvasNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  content: string;
  properties: Record<string, any>;
}

export interface CanvasEdge {
  id: string;
  from: string;
  to: string;
  label?: string;
  properties: Record<string, any>;
}

export interface ViewportState {
  zoom: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type PropertyValue = string | number | boolean | null | any[] | Record<string, any>;

// Database types
export interface DatabaseDocument {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  metadata: Record<string, any>;
  crdt_state?: Uint8Array;
  version: number;
}

export interface DatabaseDocumentNode {
  id: string;
  document_id: string;
  parent_id?: string;
  content: string;
  title?: string;
  position: number;
  collapsed: boolean;
  properties: Record<string, any>;
  created_at: string;
  updated_at: string;
}

export interface DatabaseCanvasNode {
  id: string;
  document_id: string;
  node_id?: string;
  x: number;
  y: number;
  width: number;
  height: number;
  automerge_state?: string;
  color: string;
  shape: string;
  z_index: number;
}

export interface DatabaseCanvasEdge {
  id: string;
  document_id: string;
  from_node_id: string;
  to_node_id: string;
  edge_type: string;
  label?: string;
  color: string;
  width: number;
  style: string;
  properties: Record<string, any>;
}

export interface DatabaseEmbedding {
  id: string;
  document_id: string;
  node_id?: string;
  chunk_text: string;
  embedding: number[];
  position: number;
  model: string;
  similarity_threshold: number;
}

