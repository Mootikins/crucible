import { getDatabase } from '../index';
import { DocumentNode, DatabaseDocument, DatabaseDocumentNode } from '../../../shared/src/types';

export class DocumentRepository {
  async createDocument(title: string, metadata: Record<string, any> = {}): Promise<DatabaseDocument> {
    const db = getDatabase();

    const result = await db.query(`
      INSERT INTO documents (title, metadata)
      VALUES ($1, $2)
      RETURNING id, title, created_at, updated_at, metadata, version;
    `, [title, JSON.stringify(metadata)]);

    return result.rows[0];
  }

  async getDocument(id: string): Promise<DatabaseDocument | null> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, title, created_at, updated_at, metadata, crdt_state, version
      FROM documents
      WHERE id = $1;
    `, [id]);

    return result.rows[0] || null;
  }

  async updateDocument(id: string, updates: {
    title?: string;
    metadata?: Record<string, any>;
    crdt_state?: Uint8Array;
  }): Promise<DatabaseDocument | null> {
    const db = getDatabase();

    const setClause: string[] = [];
    const values: any[] = [];
    let paramIndex = 1;

    if (updates.title !== undefined) {
      setClause.push(`title = $${paramIndex++}`);
      values.push(updates.title);
    }

    if (updates.metadata !== undefined) {
      setClause.push(`metadata = $${paramIndex++}`);
      values.push(JSON.stringify(updates.metadata));
    }

    if (updates.crdt_state !== undefined) {
      setClause.push(`crdt_state = $${paramIndex++}`);
      values.push(updates.crdt_state);
    }

    if (setClause.length === 0) {
      return this.getDocument(id);
    }

    values.push(id);

    const result = await db.query(`
      UPDATE documents
      SET ${setClause.join(', ')}, version = version + 1
      WHERE id = $${paramIndex}
      RETURNING id, title, created_at, updated_at, metadata, crdt_state, version;
    `, values);

    return result.rows[0] || null;
  }

  async deleteDocument(id: string): Promise<boolean> {
    const db = getDatabase();

    const result = await db.query(`
      DELETE FROM documents
      WHERE id = $1
      RETURNING id;
    `, [id]);

    return result.rows.length > 0;
  }

  async listDocuments(limit: number = 50, offset: number = 0): Promise<DatabaseDocument[]> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, title, created_at, updated_at, metadata, version
      FROM documents
      ORDER BY updated_at DESC
      LIMIT $1 OFFSET $2;
    `, [limit, offset]);

    return result.rows;
  }

  // Document Nodes
  async createNode(documentId: string, node: Omit<DatabaseDocumentNode, 'id' | 'document_id' | 'created_at' | 'updated_at'>): Promise<DatabaseDocumentNode> {
    const db = getDatabase();

    const result = await db.query(`
      INSERT INTO document_nodes (document_id, parent_id, content, title, position, collapsed, properties)
      VALUES ($1, $2, $3, $4, $5, $6, $7)
      RETURNING id, document_id, parent_id, content, title, position, collapsed, properties, created_at, updated_at;
    `, [
      documentId,
      node.parent_id || null,
      node.content,
      node.title || null,
      node.position,
      node.collapsed,
      JSON.stringify(node.properties)
    ]);

    return {
      ...result.rows[0],
      properties: JSON.parse(result.rows[0].properties)
    };
  }

  async getNode(id: string): Promise<DatabaseDocumentNode | null> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, document_id, parent_id, content, title, position, collapsed, properties, created_at, updated_at
      FROM document_nodes
      WHERE id = $1;
    `, [id]);

    if (!result.rows[0]) return null;

    return {
      ...result.rows[0],
      properties: JSON.parse(result.rows[0].properties)
    };
  }

  async updateNode(id: string, updates: Partial<DatabaseDocumentNode>): Promise<DatabaseDocumentNode | null> {
    const db = getDatabase();

    const setClause: string[] = [];
    const values: any[] = [];
    let paramIndex = 1;

    if (updates.content !== undefined) {
      setClause.push(`content = $${paramIndex++}`);
      values.push(updates.content);
    }

    if (updates.title !== undefined) {
      setClause.push(`title = $${paramIndex++}`);
      values.push(updates.title);
    }

    if (updates.parent_id !== undefined) {
      setClause.push(`parent_id = $${paramIndex++}`);
      values.push(updates.parent_id || null);
    }

    if (updates.position !== undefined) {
      setClause.push(`position = $${paramIndex++}`);
      values.push(updates.position);
    }

    if (updates.collapsed !== undefined) {
      setClause.push(`collapsed = $${paramIndex++}`);
      values.push(updates.collapsed);
    }

    if (updates.properties !== undefined) {
      setClause.push(`properties = $${paramIndex++}`);
      values.push(JSON.stringify(updates.properties));
    }

    if (setClause.length === 0) {
      return this.getNode(id);
    }

    values.push(id);

    const result = await db.query(`
      UPDATE document_nodes
      SET ${setClause.join(', ')}
      WHERE id = $${paramIndex}
      RETURNING id, document_id, parent_id, content, title, position, collapsed, properties, created_at, updated_at;
    `, values);

    if (!result.rows[0]) return null;

    return {
      ...result.rows[0],
      properties: JSON.parse(result.rows[0].properties)
    };
  }

  async deleteNode(id: string): Promise<boolean> {
    const db = getDatabase();

    const result = await db.query(`
      DELETE FROM document_nodes
      WHERE id = $1
      RETURNING id;
    `, [id]);

    return result.rows.length > 0;
  }

  async getDocumentNodes(documentId: string): Promise<DatabaseDocumentNode[]> {
    const db = getDatabase();

    const result = await db.query(`
      SELECT id, document_id, parent_id, content, title, position, collapsed, properties, created_at, updated_at
      FROM document_nodes
      WHERE document_id = $1
      ORDER BY position, created_at;
    `, [documentId]);

    return result.rows.map(row => ({
      ...row,
      properties: JSON.parse(row.properties)
    }));
  }

  async getNodeHierarchy(documentId: string): Promise<DocumentNode[]> {
    const nodes = await this.getDocumentNodes(documentId);

    // Build hierarchy
    const nodeMap = new Map<string, DocumentNode & { children: string[] }>();

    // Create flat map first
    nodes.forEach(node => {
      nodeMap.set(node.id, {
        id: node.id,
        title: node.title || '',
        content: node.content,
        created_at: node.created_at,
        updated_at: node.updated_at,
        parent_id: node.parent_id,
        children: [],
        properties: node.properties,
        collapsed: node.collapsed,
        position: node.position
      });
    });

    // Build hierarchy
    const rootNodes: DocumentNode[] = [];

    nodeMap.forEach(node => {
      if (node.parent_id && nodeMap.has(node.parent_id)) {
        const parent = nodeMap.get(node.parent_id)!;
        parent.children.push(node.id);
      } else {
        rootNodes.push(node);
      }
    });

    return rootNodes;
  }
}

export const documentRepository = new DocumentRepository();