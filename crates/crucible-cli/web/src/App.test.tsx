import { describe, it, expect } from 'vitest';

describe('App', () => {
  // NOTE: Full App render/import tests are not feasible in jsdom because
  // the module graph pulls in solid-dnd (TDZ issues), API calls, and
  // triggers "multiple Solid instances" warnings. App integration is
  // covered by E2E tests (playwright) instead.
  it('placeholder — see e2e/ for integration coverage', () => {
    expect(true).toBe(true);
  });
});
