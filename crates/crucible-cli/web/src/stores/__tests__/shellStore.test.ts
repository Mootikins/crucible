import { describe, it, expect } from 'vitest';
import {
  shellStore,
  shellActions,
  surfaceForContentType,
  syncShellSurface,
} from '../shellStore';
import type { Tab } from '@/types/windowTypes';

function tab(contentType: Tab['contentType']): Tab {
  return { id: `tab-${contentType}`, title: contentType, contentType };
}

describe('shellStore — surface mapping', () => {
  it('maps chat tabs to the session surface', () => {
    expect(surfaceForContentType('chat')).toBe('session');
  });

  it('maps file tabs to the edit surface', () => {
    expect(surfaceForContentType('file')).toBe('edit');
  });

  it('maps home and inbox tabs to their surfaces', () => {
    expect(surfaceForContentType('home')).toBe('home');
    expect(surfaceForContentType('inbox')).toBe('inbox');
  });

  it('returns null for tabs that do not change the surface', () => {
    expect(surfaceForContentType('settings')).toBeNull();
    expect(surfaceForContentType('terminal')).toBeNull();
    expect(surfaceForContentType('sessions')).toBeNull();
    expect(surfaceForContentType('activity')).toBeNull();
  });
});

describe('shellStore — syncShellSurface', () => {
  it('updates the active surface when focusing a surface-bearing tab', () => {
    shellActions.setActiveSurface('home');
    syncShellSurface(tab('chat'));
    expect(shellStore.activeSurface()).toBe('session');

    syncShellSurface(tab('file'));
    expect(shellStore.activeSurface()).toBe('edit');

    syncShellSurface(tab('inbox'));
    expect(shellStore.activeSurface()).toBe('inbox');
  });

  it('keeps the current surface when focusing a neutral tab', () => {
    shellActions.setActiveSurface('session');
    syncShellSurface(tab('settings'));
    expect(shellStore.activeSurface()).toBe('session');
  });

  it('ignores null/undefined tabs', () => {
    shellActions.setActiveSurface('edit');
    syncShellSurface(null);
    syncShellSurface(undefined);
    expect(shellStore.activeSurface()).toBe('edit');
  });
});
