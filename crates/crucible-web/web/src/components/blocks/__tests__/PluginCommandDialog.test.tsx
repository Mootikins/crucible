import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { PLUGIN_CALLER_HEADER } from '@/lib/api';

/**
 * The done-when of the plugin plan's step 3: **a dialog is generated for a
 * command the dialog code has never seen.**
 *
 * So `spectrometer_calibrate` does not exist, is not a plugin, and appears
 * nowhere outside this file. If the dialog can draw it — the right control per
 * declared type, the right value per control on the wire — then it is
 * generated. If it could only draw `graph_neighborhood`, that would be a
 * hand-written form with a schema-shaped comment.
 */

// No `vi.mock('@/lib/api')`: the dialog issues the real `runPluginCommand`
// against the `POST /api/plugins/command` route below, so what the plugin is
// told — command name, typed args, and who is calling on its behalf — is read
// off the wire.
/** Every run the dialog issued, as it went out. */
const sent: { name: string; args: unknown; caller: string }[] = [];

let env: TestQueryEnv;

import { PluginCommandDialog } from '../PluginCommandDialog';

/** A command invented here, declaring one parameter of every drawable shape. */
const INVENTED = {
  plugin: 'spectrometer',
  name: 'spectrometer_calibrate',
  description: 'Calibrate against a reference sample',
  hint: null,
  effect: 'write' as const,
  parameters: {
    type: 'object',
    properties: {
      sample: { type: 'string', description: 'Reference sample id' },
      passes: { type: 'number', description: 'How many passes' },
      dry_run: { type: 'boolean', description: 'Report without applying' },
      bands: { type: 'array', items: { type: 'string' }, description: 'Bands to include' },
      overrides: { type: 'object', description: 'Raw overrides' },
    },
    required: ['sample'],
  },
};

beforeEach(() => {
  sent.length = 0;
  env = createTestQueryEnv({
    'POST /api/plugins/command': async (request) => {
      const { name, args } = (await request.json()) as { name: string; args: unknown };
      sent.push({ name, args, caller: request.headers.get(PLUGIN_CALLER_HEADER) ?? '' });
      return { ok: true };
    },
  });
});

afterEach(() => {
  env.restore();
});

/** The control each field was drawn with, by tag and type attribute. */
function controlsByName(container: HTMLElement): Record<string, string> {
  const out: Record<string, string> = {};
  for (const element of container.querySelectorAll('input, textarea')) {
    const id = element.getAttribute('id') ?? '';
    const name = id.replace('plugin-command-field-', '');
    if (!name || name === id) continue;
    out[name] =
      element.tagName === 'TEXTAREA' ? 'textarea' : (element.getAttribute('type') ?? 'text');
  }
  return out;
}

describe('PluginCommandDialog', () => {
  it('draws a control per declared type for a command it has never seen', () => {
    const { container } = render(() => (
      <PluginCommandDialog command={INVENTED} onClose={() => {}} />
    ));

    expect(controlsByName(container)).toEqual({
      sample: 'text',
      passes: 'number',
      dry_run: 'checkbox',
      bands: 'textarea',
      overrides: 'textarea',
    });
  });

  it('sends each answer as the type the declaration named', async () => {
    const { container, getByText } = render(() => (
      <PluginCommandDialog command={INVENTED} onClose={() => {}} />
    ));

    const field = (name: string) =>
      container.querySelector(`#plugin-command-field-${name}`) as HTMLInputElement;

    fireEvent.input(field('sample'), { target: { value: 'ref-12' } });
    fireEvent.input(field('passes'), { target: { value: '3' } });
    fireEvent.click(field('dry_run'));
    fireEvent.input(field('bands'), { target: { value: 'red\ngreen' } });
    fireEvent.click(getByText('Run'));

    // The dialog names no caller, so `runPluginCommand`'s default speaks for
    // the app on the header the plugin is reached through.
    await waitFor(() =>
      expect(sent[0]).toEqual({
        name: 'spectrometer_calibrate',
        args: {
          sample: 'ref-12',
          passes: 3,
          dry_run: true,
          bands: ['red', 'green'],
        },
        caller: 'app',
      }),
    );
  });

  /**
   * A required field left blank must stop the call, not send a partial one.
   * The plugin would answer with its own refusal, but only after a write
   * command had already been dispatched.
   */
  it('refuses to run while a required field is blank', async () => {
    const { getByText, container } = render(() => (
      <PluginCommandDialog command={INVENTED} onClose={() => {}} />
    ));

    fireEvent.click(getByText('Run'));

    // The error, not the label: the field's own `required` marker is drawn
    // before anyone presses anything, so matching on that word alone would
    // pass with the check deleted.
    await waitFor(() =>
      expect(container.querySelector('.text-error')?.textContent).toBe('required'),
    );
    expect(sent).toHaveLength(0);
  });

  /**
   * The effect is shown as a claim, never as a verdict. A badge reading "read"
   * flat would tell a user the daemon checked something; nothing did.
   */
  it('presents the declared effect as a declaration', () => {
    const { getByText } = render(() => <PluginCommandDialog command={INVENTED} onClose={() => {}} />);
    const badge = getByText('declares: write');
    expect(badge.getAttribute('title')).toMatch(/nothing verifies it/i);
  });

  it('offers a Run button for a command that declares no parameters', async () => {
    const { getByText, container } = render(() => (
      <PluginCommandDialog
        command={{
          plugin: 'p',
          name: 'bare',
          description: '',
          hint: null,
          effect: 'read',
          parameters: null,
        }}
        onClose={() => {}}
      />
    ));

    expect(controlsByName(container)).toEqual({});
    fireEvent.click(getByText('Run'));
    await waitFor(() =>
      expect(sent[0]).toEqual({ name: 'bare', args: {}, caller: 'app' }),
    );
  });
});
