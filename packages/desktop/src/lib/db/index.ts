import { PGlite } from '@electric-sql/pglite';
import { vector } from '@electric-sql/pglite/vector';

let db: PGlite | null = null;

export async function initializePGlite(): Promise<PGlite> {
  if (db) {
    return db;
  }

  try {
    // Initialize PGlite with vector extension
    db = new PGlite({
      extensions: {
        vector
      }
    });

    // Run initial setup
    await setupDatabase(db);

    console.log('🔥 PGlite initialized successfully');
    return db;
  } catch (error) {
    console.error('Failed to initialize PGlite:', error);
    throw error;
  }
}

export function getDatabase(): PGlite {
  if (!db) {
    throw new Error('Database not initialized');
  }
  return db;
}

async function setupDatabase(db: PGlite): Promise<void> {
  try {
    // Enable vector extension
    await db.exec(`CREATE EXTENSION IF NOT EXISTS vector;`);

    // Run migrations
    await runMigrations(db);

    console.log('Database setup completed');
  } catch (error) {
    console.error('Database setup failed:', error);
    throw error;
  }
}

async function runMigrations(db: PGlite): Promise<void> {
  // Create migrations table if it doesn't exist
  await db.exec(`
    CREATE TABLE IF NOT EXISTS schema_migrations (
      version VARCHAR(255) PRIMARY KEY,
      applied_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
    );
  `);

  // Get applied migrations
  const result = await db.query('SELECT version FROM schema_migrations ORDER BY version;');
  const appliedMigrations = result.rows.map(row => row.version);

  // Run pending migrations
  const migrationFiles = [
    '001_initial.sql',
    '002_crdt.sql',
    '003_embeddings.sql',
    '004_canvas.sql'
  ];

  for (const migrationFile of migrationFiles) {
    if (!appliedMigrations.includes(migrationFile)) {
      await runMigration(db, migrationFile);
    }
  }
}

async function runMigration(db: PGlite, filename: string): Promise<void> {
  try {
    const response = await fetch(`/lib/db/migrations/${filename}`);
    if (!response.ok) {
      throw new Error(`Failed to load migration ${filename}`);
    }

    const sql = await response.text();
    await db.exec(sql);

    // Record migration
    await db.query(
      'INSERT INTO schema_migrations (version) VALUES ($1);',
      [filename]
    );

    console.log(`Applied migration: ${filename}`);
  } catch (error) {
    console.error(`Migration ${filename} failed:`, error);
    throw error;
  }
}

export async function closeDatabase(): Promise<void> {
  if (db) {
    await db.close();
    db = null;
  }
}