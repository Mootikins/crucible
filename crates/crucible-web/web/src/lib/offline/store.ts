/**
 * Where offline data lives: a tiny key/value surface over IndexedDB.
 *
 * An interface rather than direct `indexedDB` calls, because everything that
 * matters here — what a kiln keeps, what the outbox owes, what a conflict does
 * — is logic, and logic tested through a real database is tested slowly and
 * flakily. `memoryStore()` is the same contract for a test.
 *
 * `localStorage` is not an option: it is synchronous, string-only and about
 * 5 MB, and a kept kiln is note bodies and sometimes images.
 */

export type OfflineTable = 'mirror' | 'outbox' | 'index' | 'blobs' | 'meta';

const TABLES: OfflineTable[] = ['mirror', 'outbox', 'index', 'blobs', 'meta'];

export interface OfflineStore {
  get<T>(table: OfflineTable, key: string): Promise<T | null>;
  put<T>(table: OfflineTable, key: string, value: T): Promise<void>;
  remove(table: OfflineTable, key: string): Promise<void>;
  /** Every entry, or every entry whose key starts with `prefix`. */
  list<T>(table: OfflineTable, prefix?: string): Promise<{ key: string; value: T }[]>;
  clear(table: OfflineTable): Promise<void>;
  /** Roughly how many bytes this table holds, for the settings group. */
  size(table: OfflineTable, prefix?: string): Promise<number>;
}

/** A store that lives in memory. Tests use it; nothing shipped does. */
export function memoryStore(): OfflineStore {
  const tables = new Map<OfflineTable, Map<string, unknown>>();
  const table = (name: OfflineTable) => {
    let found = tables.get(name);
    if (!found) {
      found = new Map();
      tables.set(name, found);
    }
    return found;
  };
  return {
    async get<T>(name: OfflineTable, key: string) {
      return (table(name).get(key) as T) ?? null;
    },
    async put<T>(name: OfflineTable, key: string, value: T) {
      table(name).set(key, value);
    },
    async remove(name, key) {
      table(name).delete(key);
    },
    async list<T>(name: OfflineTable, prefix?: string) {
      return [...table(name).entries()]
        .filter(([key]) => !prefix || key.startsWith(prefix))
        .map(([key, value]) => ({ key, value: value as T }));
    },
    async clear(name) {
      table(name).clear();
    },
    async size(name, prefix) {
      let total = 0;
      for (const { value } of await this.list(name, prefix)) total += approximateBytes(value);
      return total;
    },
  };
}

/** Bytes a value costs, near enough for a settings screen. */
export function approximateBytes(value: unknown): number {
  if (value instanceof Blob) return value.size;
  if (typeof value === 'string') return value.length * 2;
  if (value && typeof value === 'object') {
    let total = 0;
    for (const [key, inner] of Object.entries(value as Record<string, unknown>)) {
      total += key.length * 2 + approximateBytes(inner);
    }
    return total;
  }
  return 8;
}

const DB_NAME = 'crucible-offline';
const DB_VERSION = 1;

function openDatabase(name = DB_NAME): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(name, DB_VERSION);
    request.onupgradeneeded = () => {
      for (const table of TABLES) {
        if (!request.result.objectStoreNames.contains(table)) {
          request.result.createObjectStore(table);
        }
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

/** The shipped store. Opens lazily, so importing this costs nothing. */
export function idbStore(name = DB_NAME): OfflineStore {
  let db: Promise<IDBDatabase> | null = null;
  const database = () => (db ??= openDatabase(name));

  const run = <T>(
    table: OfflineTable,
    mode: IDBTransactionMode,
    work: (store: IDBObjectStore) => IDBRequest,
  ): Promise<T> =>
    database().then(
      (open) =>
        new Promise<T>((resolve, reject) => {
          const request = work(open.transaction(table, mode).objectStore(table));
          request.onsuccess = () => resolve(request.result as T);
          request.onerror = () => reject(request.error);
        }),
    );

  const entries = async <T>(table: OfflineTable, prefix?: string) => {
    const keys = await run<IDBValidKey[]>(table, 'readonly', (store) => store.getAllKeys());
    const values = await run<T[]>(table, 'readonly', (store) => store.getAll());
    return keys
      .map((key, i) => ({ key: String(key), value: values[i] }))
      .filter(({ key }) => !prefix || key.startsWith(prefix));
  };

  return {
    get<T>(table: OfflineTable, key: string) {
      return run<T | undefined>(table, 'readonly', (store) => store.get(key)).then(
        (value) => value ?? null,
      );
    },
    put: (table, key, value) =>
      run<void>(table, 'readwrite', (store) => store.put(value, key)).then(() => undefined),
    remove: (table, key) =>
      run<void>(table, 'readwrite', (store) => store.delete(key)).then(() => undefined),
    list: entries,
    clear: (table) => run<void>(table, 'readwrite', (store) => store.clear()).then(() => undefined),
    async size(table, prefix) {
      let total = 0;
      for (const { value } of await entries(table, prefix)) total += approximateBytes(value);
      return total;
    },
  };
}
