import type { SimulationNodeDatum } from 'd3-force';

/** Wire shape of `GET /api/kiln/graph` (daemon `kiln.graph`). */
export interface GraphNoteDto {
  path: string;
  title: string;
  tags: string[];
}

export interface GraphLinkDto {
  source: string;
  /** Resolved: a note path joinable against `notes[].path`. Unresolved: the
   * normalized target key of a link pointing at no existing note. */
  target: string;
  resolved: boolean;
}

export interface GraphDto {
  notes: GraphNoteDto[];
  links: GraphLinkDto[];
}

export type GraphNodeKind = 'note' | 'phantom' | 'tag';

/** Simulation node — d3-force mutates x/y/vx/vy in place. */
export interface GraphNode extends SimulationNodeDatum {
  id: string;
  label: string;
  kind: GraphNodeKind;
  /** Kiln note path (note nodes only). */
  path?: string;
  degree: number;
}

export type GraphEdgeKind = 'link' | 'unresolved' | 'tag';

/** d3-force rewrites string endpoints to node references on init. */
export interface GraphEdge {
  source: string | GraphNode;
  target: string | GraphNode;
  kind: GraphEdgeKind;
}

export interface BuiltGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface GraphFilters {
  /** Case-insensitive substring over note label + path. */
  query: string;
  showTags: boolean;
  /** Unresolved link targets rendered as ghost nodes (Obsidian's
   * "existing files only" toggle, inverted). */
  showPhantoms: boolean;
  showOrphans: boolean;
}

export interface GraphDisplay {
  /** Multiplier on node radius (0.4..2.5). */
  nodeSize: number;
  /** Link stroke width multiplier (0.3..3). */
  linkThickness: number;
}

export interface GraphForces {
  /** Pull toward viewport center (0..1). */
  centerForce: number;
  /** Node-node repulsion multiplier (0..2). */
  repelForce: number;
  /** Link spring strength (0..1). */
  linkForce: number;
  /** Link rest length in world units (30..500). */
  linkDistance: number;
}

export interface GraphSettings {
  filters: GraphFilters;
  display: GraphDisplay;
  forces: GraphForces;
}

export const DEFAULT_GRAPH_SETTINGS: GraphSettings = {
  filters: { query: '', showTags: false, showPhantoms: true, showOrphans: true },
  display: { nodeSize: 1, linkThickness: 1 },
  forces: { centerForce: 0.3, repelForce: 1, linkForce: 0.7, linkDistance: 140 },
};
