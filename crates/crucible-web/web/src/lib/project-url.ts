/**
 * The query parameter that addresses a window at one project.
 *
 * "Open in a new window" needs the new window to pin the project its row
 * named. Nothing else carries that across a window boundary: the pin is
 * in-memory client state, and a second window would otherwise run the same
 * cold-start rule as the first and land on whichever project sorts first.
 */
export const PROJECT_PARAM = 'project';

/** The project path this window was addressed to, or null. */
export function projectFromUrl(search: string = window.location.search): string | null {
  const value = new URLSearchParams(search).get(PROJECT_PARAM);
  return value && value.trim() !== '' ? value : null;
}
