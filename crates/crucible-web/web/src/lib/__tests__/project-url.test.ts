import { describe, it, expect } from 'vitest';
import { PROJECT_PARAM, projectFromUrl } from '@/lib/project-url';

describe('projectFromUrl', () => {
  it('reads the project a window was addressed to', () => {
    expect(projectFromUrl('?project=/home/me/atlas')).toBe('/home/me/atlas');
  });

  it('answers null when the window was addressed to nothing', () => {
    // Null, not '': the cold-start rule must still run for an ordinary window.
    expect(projectFromUrl('')).toBeNull();
    expect(projectFromUrl('?other=1')).toBeNull();
    expect(projectFromUrl(`?${PROJECT_PARAM}=`)).toBeNull();
    expect(projectFromUrl(`?${PROJECT_PARAM}=%20%20`)).toBeNull();
  });

  it('survives a path with characters a query string would eat', () => {
    const path = '/home/me/my repo & things';
    const url = new URL('http://x/');
    url.searchParams.set(PROJECT_PARAM, path);
    expect(projectFromUrl(url.search)).toBe(path);
  });
});
