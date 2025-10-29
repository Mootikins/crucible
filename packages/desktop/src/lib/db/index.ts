import { invoke } from '@tauri-apps/api/core';

export async function initializeDatabase(): Promise<void> {
  await invoke('initialize_database');
}

export async function searchDocuments(query: string) {
  return await invoke('search_documents', { query });
}

export async function getDocument(path: string) {
  return await invoke('get_document', { path });
}

export async function createDocument(title: string, content: string) {
  return await invoke('create_document', { title, content });
}

export async function updateDocument(path: string, content: string) {
  return await invoke('update_document', { path, content });
}

export async function deleteDocument(path: string) {
  return await invoke('delete_document', { path });
}

export async function listDocuments() {
  return await invoke('list_documents');
}

export async function searchByTags(tags: string[]) {
  return await invoke('search_by_tags', { tags });
}

export async function searchByProperties(properties: Record<string, any>) {
  return await invoke('search_by_properties', { properties });
}

export async function semanticSearch(query: string, topK: number = 10) {
  return await invoke('semantic_search', { query, topK });
}

export async function indexKiln(force: boolean = false) {
  return await invoke('index_kiln', { force });
}

export async function getNoteMetadata(path: string) {
  return await invoke('get_note_metadata', { path });
}

export async function updateNoteProperties(path: string, properties: Record<string, any>) {
  return await invoke('update_note_properties', { path, properties });
}