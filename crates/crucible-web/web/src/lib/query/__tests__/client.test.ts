import { describe, it, expect, afterEach } from 'vitest';
import { QueryClient } from '@tanstack/solid-query';
import {
  getQueryClient,
  setQueryClientForTests,
  queryClientOptions,
} from '../client';
import { keys } from '../keys';

afterEach(() => {
  // Each test gets the module singleton back, so an injected client cannot
  // leak into the next test.
  setQueryClientForTests(null);
  getQueryClient().clear();
});

describe('getQueryClient', () => {
  it('answers one QueryClient for the whole module', () => {
    const client = getQueryClient();
    expect(client).toBeInstanceOf(QueryClient);
    expect(getQueryClient()).toBe(client);
  });

  it('does not retry a read operation', () => {
    const defaults = getQueryClient().getDefaultOptions();
    expect(defaults.queries?.retry).toBe(false);
  });

  it('holds a read for five minutes before it is stale', () => {
    const defaults = getQueryClient().getDefaultOptions();
    expect(defaults.queries?.staleTime).toBe(5 * 60 * 1000);
    expect(queryClientOptions.defaultOptions?.queries?.staleTime).toBe(5 * 60 * 1000);
  });

  it('answers undefined for a key it did not cache', () => {
    expect(getQueryClient().getQueryData(keys.session('sess-1'))).toBeUndefined();
  });

  it('answers the cached value for a key it did cache', () => {
    const client = getQueryClient();
    client.setQueryData(keys.session('sess-1'), { id: 'sess-1', title: 'One' });
    expect(client.getQueryData(keys.session('sess-1'))).toEqual({
      id: 'sess-1',
      title: 'One',
    });
  });
});

describe('setQueryClientForTests', () => {
  it('injects a client that getQueryClient then answers', () => {
    const injected = new QueryClient(queryClientOptions);
    setQueryClientForTests(injected);
    expect(getQueryClient()).toBe(injected);
  });

  it('restores the module singleton when the test gives null', () => {
    const singleton = getQueryClient();
    setQueryClientForTests(new QueryClient(queryClientOptions));
    setQueryClientForTests(null);
    expect(getQueryClient()).toBe(singleton);
  });
});

describe('keys', () => {
  it('names one key per entity, with the identifier in the key', () => {
    expect(keys.kilns()).toEqual(['kilns']);
    expect(keys.sessions(true)).toEqual(['sessions', { includeArchived: true }]);
    expect(keys.session('sess-1')).toEqual(['session', 'sess-1']);
    expect(keys.sessionHistory('sess-1')).toEqual(['session', 'sess-1', 'history']);
    expect(keys.pluginOption('git', ['remote', 'name'])).toEqual([
      'plugins',
      'option',
      'git',
      'remote',
      'name',
    ]);
    expect(keys.pluginPublications('git')).toEqual([
      'plugins',
      'publications',
      'git',
      undefined,
    ]);
    expect(keys.searchGrep('/root', 'needle')).toEqual([
      'search',
      'grep',
      '/root',
      'needle',
      undefined,
    ]);
  });

  it('prefixes every session key with the session key itself', () => {
    const prefix = keys.session('sess-1');
    for (const key of [
      keys.sessionHistory('sess-1'),
      keys.sessionModels('sess-1'),
      keys.sessionModes('sess-1'),
      keys.sessionStatus('sess-1'),
      keys.sessionKnobs('sess-1'),
      keys.sessionScope('sess-1'),
    ]) {
      expect(key.slice(0, prefix.length)).toEqual(prefix);
    }
  });
});
