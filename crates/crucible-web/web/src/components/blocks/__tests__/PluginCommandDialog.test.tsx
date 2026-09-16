import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';

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
const mocks = vi.hoisted(() => ({
  runPluginCommand: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  runPluginCommand: mocks.runPluginCommand,
}));

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
  mocks.runPluginCommand.mockReset();
  mocks.runPluginCommand.mockResolvedValue({ ok: true });
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

    // The third argument is the caller. This dialog names none, so it passes
    // `undefined` and `runPluginCommand`'s default names the app.
    await waitFor(() =>
      expect(mocks.runPluginCommand).toHaveBeenCalledWith(
        'spectrometer_calibrate',
        {
          sample: 'ref-12',
          passes: 3,
          dry_run: true,
          bands: ['red', 'green'],
        },
        undefined,
      ),
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
    expect(mocks.runPluginCommand).not.toHaveBeenCalled();
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
      expect(mocks.runPluginCommand).toHaveBeenCalledWith('bare', {}, undefined),
    );
  });
});
