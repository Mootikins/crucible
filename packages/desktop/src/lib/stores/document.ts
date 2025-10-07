import { writable } from 'svelte/store';

export const documentStore = writable({
  documents: [],
  currentDocument: null,
  loading: false
});
