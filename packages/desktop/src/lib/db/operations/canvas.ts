import { getDatabase } from '../index';
import { DatabaseCanvasNode, DatabaseCanvasEdge, ViewportState } from '../../../shared/src/types';

export class CanvasRepository {
  // Canvas Nodes
  async createCanvasNode(documentId: string, node: Omit<DatabaseCanvasNode, 'id' | 'document_id'>): Promise<DatabaseCanvasNode> {
    const db = getDatabase();

    const result = await db.query(`
      INSERT INTO canvas_nodes (document_id, node_id, x, y, width, height, automerge_state, color, shape, z_index)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
      RETURNING id, document_id, node_id, x, y, width, height, automerge_state, color, shape, z_index;
    `, [
      documentId,
      node.node_id || null,
      node.x,
      node.y,
      node.width,
      node.height,
      node.automerge_state || null,
      node.color,
      node.shape,
      node.z_index
    ]);

    return result.rows[0];
  }

  async getCanvasNode(id: string): Promise<DatabaseCanvasNode | null> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, document_id, node_id, x, y, width, height, automerge_state, color, shape, z_index
      FROM canvas_nodes
      WHERE id = $1;
    `, [id]);

    return result.rows[0] || null;
  }

  async updateCanvasNode(id: string, updates: Partial<DatabaseCanvasNode>): Promise<DatabaseCanvasNode | null> {
    const db = getDatabase();

    const setClause: string[] = [];
    const values: any[] = [];
    let paramIndex = 1;

    if (updates.x !== undefined) {
      setClause.push(`x = $${paramIndex++}`);
      values.push(updates.x);
    }

    if (updates.y !== undefined) {
      setClause.push(`y = $${paramIndex++}`);
      values.push(updates.y);
    }

    if (updates.width !== undefined) {
      setClause.push(`width = $${paramIndex++}`);
      values.push(updates.width);
    }

    if (updates.height !== undefined) {
      setClause.push(`height = $${paramIndex++}`);
      values.push(updates.height);
    }

    if (updates.automerge_state !== undefined) {
      setClause.push(`automerge_state = $${paramIndex++}`);
      values.push(updates.automerge_state);
    }

    if (updates.color !== undefined) {
      setClause.push(`color = $${paramIndex++}`);
      values.push(updates.color);
    }

    if (updates.shape !== undefined) {
      setClause.push(`shape = $${paramIndex++}`);
      values.push(updates.shape);
    }

    if (updates.z_index !== undefined) {
      setClause.push(`z_index = $${paramIndex++}`);
      values.push(updates.z_index);
    }

    if (setClause.length === 0) {
      return this.getCanvasNode(id);
    }

    values.push(id);

    const result = await db.query(`
      UPDATE canvas_nodes
      SET ${setClause.join(', ')}
      WHERE id = $${paramIndex}
      RETURNING id, document_id, node_id, x, y, width, height, automerge_state, color, shape, z_index;
    `, values);

    return result.rows[0] || null;
  }

  async deleteCanvasNode(id: string): Promise<boolean> {
    const db = getDatabase();

    const result = await db.query(`
      DELETE FROM canvas_nodes
      WHERE id = $1
      RETURNING id;
    `, [id]);

    return result.rows.length > 0;
  }

  async getCanvasNodes(documentId: string): Promise<DatabaseCanvasNode[]> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, document_id, node_id, x, y, width, height, automerge_state, color, shape, z_index
      FROM canvas_nodes
      WHERE document_id = $1
      ORDER BY z_index, y, x;
    `, [documentId]);

    return result.rows;
  }

  // Canvas Edges
  async createCanvasEdge(documentId: string, edge: Omit<DatabaseCanvasEdge, 'id' | 'document_id'>): Promise<DatabaseCanvasEdge> {
    const db = getDatabase();

    const result = await db.query(`
      INSERT INTO canvas_edges (document_id, from_node_id, to_node_id, edge_type, label, color, width, style, properties)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
      RETURNING id, document_id, from_node_id, to_node_id, edge_type, label, color, width, style, properties;
    `, [
      documentId,
      edge.from_node_id,
      edge.to_node_id,
      edge.edge_type,
      edge.label || null,
      edge.color,
      edge.width,
      edge.style,
      JSON.stringify(edge.properties)
    ]);

    return {
      ...result.rows[0],
      properties: JSON.parse(result.rows[0].properties)
    };
  }

  async getCanvasEdge(id: string): Promise<DatabaseCanvasEdge | null> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, document_id, from_node_id, to_node_id, edge_type, label, color, width, style, properties
      FROM canvas_edges
      WHERE id = $1;
    `, [id]);

    if (!result.rows[0]) return null;

    return {
      ...result.rows[0],
      properties: JSON.parse(result.rows[0].properties)
    };
  }

  async updateCanvasEdge(id: string, updates: Partial<DatabaseCanvasEdge>): Promise<DatabaseCanvasEdge | null> {
    const db = getDatabase();

    const setClause: string[] = [];
    const values: any[] = [];
    let paramIndex = 1;

    if (updates.edge_type !== undefined) {
      setClause.push(`edge_type = $${paramIndex++}`);
      values.push(updates.edge_type);
    }

    if (updates.label !== undefined) {
      setClause.push(`label = $${paramIndex++}`);
      values.push(updates.label);
    }

    if (updates.color !== undefined) {
      setClause.push(`color = $${paramIndex++}`);
      values.push(updates.color);
    }

    if (updates.width !== undefined) {
      setClause.push(`width = $${paramIndex++}`);
      values.push(updates.width);
    }

    if (updates.style !== undefined) {
      setClause.push(`style = $${paramIndex++}`);
      values.push(updates.style);
    }

    if (updates.properties !== undefined) {
      setClause.push(`properties = $${paramIndex++}`);
      values.push(JSON.stringify(updates.properties));
    }

    if (setClause.length === 0) {
      return this.getCanvasEdge(id);
    }

    values.push(id);

    const result = await db.query(`
      UPDATE canvas_edges
      SET ${setClause.join(', ')}
      WHERE id = $${paramIndex}
      RETURNING id, document_id, from_node_id, to_node_id, edge_type, label, color, width, style, properties;
    `, values);

    if (!result.rows[0]) return null;

    return {
      ...result.rows[0],
      properties: JSON.parse(result.rows[0].properties)
    };
  }

  async deleteCanvasEdge(id: string): Promise<boolean> {
    const db = getDatabase();

    const result = await db.query(`
      DELETE FROM canvas_edges
      WHERE id = $1
      RETURNING id;
    `, [id]);

    return result.rows.length > 0;
  }

  async getCanvasEdges(documentId: string): Promise<DatabaseCanvasEdge[]> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, document_id, from_node_id, to_node_id, edge_type, label, color, width, style, properties
      FROM canvas_edges
      WHERE document_id = $1
      ORDER BY z_index;
    `, [documentId]);

    return result.rows.map(row => ({
      ...row,
      properties: JSON.parse(row.properties)
    }));
  }

  // Canvas Layout
  async getCanvasLayout(documentId: string) {
    const db = getDatabase();

    const nodes = await this.getCanvasNodes(documentId);
    const edges = await this.getCanvasEdges(documentId);

    return {
      nodes,
      edges
    };
  }

  // Viewport State
  async saveViewportState(documentId: string, sessionId: string, viewport: ViewportState): Promise<void> {
    const db = getDatabase();

    await db.query(`
      INSERT INTO canvas_viewports (document_id, session_id, zoom, x, y, width, height)
      VALUES ($1, $2, $3, $4, $5, $6, $7)
      ON CONFLICT (document_id, session_id) DO UPDATE SET
        zoom = EXCLUDED.zoom,
        x = EXCLUDED.x,
        y = EXCLUDED.y,
        width = EXCLUDED.width,
        height = EXCLUDED.height,
        updated_at = CURRENT_TIMESTAMP;
    `, [
      documentId,
      sessionId,
      viewport.zoom,
      viewport.x,
      viewport.y,
      viewport.width,
      viewport.height
    ]);
  }

  async getViewportState(documentId: string, sessionId: string): Promise<ViewportState | null> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT zoom, x, y, width, height
      FROM canvas_viewports
      WHERE document_id = $1 AND session_id = $2;
    `, [documentId, sessionId]);

    return result.rows[0] || null;
  }

  // Batch operations
  async saveCanvasLayout(documentId: string, nodes: DatabaseCanvasNode[], edges: DatabaseCanvasEdge[]): Promise<void> {
    const db = getDatabase();

    await db.transaction(async () => {
      // Clear existing layout
      await db.query('DELETE FROM canvas_edges WHERE document_id = $1;', [documentId]);
      await db.query('DELETE FROM canvas_nodes WHERE document_id = $1;', [documentId]);

      // Insert nodes
      for (const node of nodes) {
        await db.query(`
          INSERT INTO canvas_nodes (id, document_id, node_id, x, y, width, height, automerge_state, color, shape, z_index)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10);
        `, [
          node.id,
          documentId,
          node.node_id || null,
          node.x,
          node.y,
          node.width,
          node.height,
          node.automerge_state || null,
          node.color,
          node.shape,
          node.z_index
        ]);
      }

      // Insert edges
      for (const edge of edges) {
        await db.query(`
          INSERT INTO canvas_edges (id, document_id, from_node_id, to_node_id, edge_type, label, color, width, style, properties)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10);
        `, [
          edge.id,
          documentId,
          edge.from_node_id,
          edge.to_node_id,
          edge.edge_type,
          edge.label || null,
          edge.color,
          edge.width,
          edge.style,
          JSON.stringify(edge.properties)
        ]);
      }
    });
  }
}

export const canvasRepository = new CanvasRepository();