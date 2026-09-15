import { describe, it, expect } from 'vitest';
import { HOST_RUNTIME, draftCreateParams, kilnsForCreate } from '@/lib/session-draft';

describe('kilnsForCreate', () => {
  it('attaches the chosen kiln by registry name', () => {
    expect(kilnsForCreate('notes', 'home')).toEqual(['notes']);
  });

  it('falls back to the configured default', () => {
    expect(kilnsForCreate('', 'home')).toEqual(['home']);
  });

  it('attaches nothing when the user says none', () => {
    expect(kilnsForCreate('none', 'home')).toEqual([]);
  });

  it('attaches nothing when there is no default to fall back to', () => {
    expect(kilnsForCreate('', null)).toEqual([]);
  });
});

describe('draftCreateParams', () => {
  const base = { kiln: '', defaultKiln: null, workspace: '', agentName: '', wsTarget: '', runtime: '' };

  // Untouched is NOT the same as an explicit "here": an untouched runtime lets
  // the project's setting decide, while `host` overrides it. Collapsing the two
  // would containerize a session that opted out, or unsandbox one that did not.
  it('says nothing about a runtime the user never touched', () => {
    expect('isolation' in draftCreateParams(base)).toBe(false);
  });

  it('turns "run here" into an explicit refusal to isolate', () => {
    expect(draftCreateParams({ ...base, runtime: HOST_RUNTIME }).isolation).toBe(false);
  });

  it('splits a provider spec into plugin and target', () => {
    expect(draftCreateParams({ ...base, runtime: 'oci:ubuntu:24.04' }).isolation).toEqual({
      plugin: 'oci',
      target: 'ubuntu:24.04',
    });
  });

  it('omits a workspace and a workspace target that were never chosen', () => {
    const params = draftCreateParams(base);
    expect(params.workspace).toBeUndefined();
    expect('workspace_target' in params).toBe(false);
  });

  it('carries an ACP agent as its own two fields', () => {
    const params = draftCreateParams({ ...base, agentName: 'claude' });
    expect(params.agent_type).toBe('acp');
    expect(params.agent_name).toBe('claude');
  });

  it('names no agent when the internal one is used', () => {
    expect('agent_type' in draftCreateParams(base)).toBe(false);
  });

  it('forwards a named card without confusing it with an ACP profile', () => {
    expect(draftCreateParams({ ...base, agentCard: ' researcher ' })).toMatchObject({ agent_card: 'researcher' });
    expect(draftCreateParams({ ...base, agentName: 'claude', agentCard: 'researcher' })).not.toHaveProperty('agent_card');
    expect(draftCreateParams({ ...base, agentCard: ' ' })).not.toHaveProperty('agent_card');
  });
});
