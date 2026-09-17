/**
 * URL helpers for paths the browser turns into requests or links.
 *
 * Pure string work, deliberately outside `lib/api.ts`: `rawFileUrl` builds an
 * address the DOM itself fetches (a canvas media node, an inline image), and
 * `isGitRepoUrl` reads a paste before anything is sent. Keeping them here is
 * what lets components and the offline layer import none of the api module.
 */

/** URL serving a file's raw bytes, for canvas media nodes and inline images. */
export function rawFileUrl(absolutePath: string): string {
  return `/api/file/raw?path=${encodeURIComponent(absolutePath)}`;
}

/** True when the add-project input reads as a REMOTE git repo rather than a
 * local path: https/ssh URLs and `owner/repo` GitHub shorthand. */
export function isGitRepoUrl(input: string): boolean {
  const s = input.trim();
  if (/^(https?:\/\/|git@)\S+$/.test(s)) return true;
  return /^[\w.-]+\/[\w.-]+$/.test(s) && !s.startsWith('.');
}
