import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
import { DraftSessionPanel } from '../DraftSessionPanel';

const createSessionMock = vi.fn().mockResolvedValue({ id: 'sess-1' });
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ createSession: createSessionMock }),
}));

const closeDraftTabMock = vi.fn();
const consumeDraftPrefillMock = vi.fn<() => string | null>(() => null);
vi.mock('@/lib/draft-session', () => ({
  closeDraftTab: (...args: unknown[]) => closeDraftTabMock(...args),
  consumeDraftPrefill: () => consumeDraftPrefillMock(),
}));

vi.mock('@/lib/api', () => ({
  getConfig: vi.fn().mockResolvedValue({ kiln_path: '/home/user/kilns/helios' }),
  listAgents: vi.fn().mockResolvedValue([
    {
      name: 'claude',
      description: 'Claude Code via ACP',
      command: 'npx',
      is_builtin: true,
      available: true,
    },
    {
      name: 'cursor',
      description: 'Cursor IDE via ACP',
      command: 'cursor-acp',
      is_builtin: true,
      available: false,
    },
  ]),
  listAllModels: vi.fn().mockResolvedValue(['ollama/llama3.2', 'openai/gpt-4o', '[error] zai: boom']),
  listKilns: vi
    .fn()
    .mockResolvedValue([{ path: '/home/user/kilns/other', name: 'other' }]),
  listProjects: vi.fn().mockResolvedValue([{ path: '/repos/crucible', name: 'crucible', kilns: [] }]),
  listProviders: vi.fn().mockResolvedValue([
    { name: 'ollama', provider_type: 'ollama', available: true, default_model: 'llama3.2', models: [] },
  ]),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const setup = async () => {
  const utils = render(() => <DraftSessionPanel draftTabId="tab-draft-1" />);
  await waitFor(() => {
    expect(
      (utils.getByTestId('draft-agent') as HTMLSelectElement).options.length,
    ).toBeGreaterThan(1);
  });
  return utils;
};

describe('DraftSessionPanel', () => {
  it('renders grouped controls with internal agent + defaults preselected', async () => {
    const { getByTestId } = await setup();

    const agent = getByTestId('draft-agent') as HTMLSelectElement;
    expect(agent.value).toBe('');
    expect(agent.options[0].text).toContain('Internal agent');

    const kiln = getByTestId('draft-kiln') as HTMLSelectElement;
    expect(kiln.value).toBe('');
    expect(kiln.options[0].text).toContain('helios');
    expect(kiln.options[0].text).toContain('default');

    const model = getByTestId('draft-model') as HTMLSelectElement;
    expect(model.value).toBe('');
    // Provider error entries are filtered out of the picker.
    const modelTexts = Array.from(model.options).map((o) => o.value);
    expect(modelTexts).toContain('ollama/llama3.2');
    expect(modelTexts.some((t) => t.startsWith('[error]'))).toBe(false);
  });

  it('marks unavailable ACP agents as disabled options', async () => {
    const { getByTestId } = await setup();
    const agent = getByTestId('draft-agent') as HTMLSelectElement;
    const cursor = Array.from(agent.options).find((o) => o.value === 'cursor');
    expect(cursor?.disabled).toBe(true);
    expect(cursor?.text).toContain('not installed');
    const claude = Array.from(agent.options).find((o) => o.value === 'claude');
    expect(claude?.disabled).toBe(false);
  });

  it('hides the model picker when an ACP agent is selected', async () => {
    const { getByTestId, queryByTestId } = await setup();
    fireEvent.change(getByTestId('draft-agent'), { target: { value: 'claude' } });
    expect(queryByTestId('draft-model')).toBeNull();
  });

  it('does not create a session while the message is empty', async () => {
    const { getByTestId } = await setup();
    const send = getByTestId('draft-send') as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    fireEvent.click(send);
    expect(createSessionMock).not.toHaveBeenCalled();
  });

  it('creates an internal session with the first message and closes the draft', async () => {
    const { getByTestId } = await setup();
    fireEvent.change(getByTestId('draft-model'), { target: { value: 'openai/gpt-4o' } });
    fireEvent.input(getByTestId('draft-input'), { target: { value: 'hello world' } });
    fireEvent.click(getByTestId('draft-send'));

    await waitFor(() => expect(createSessionMock).toHaveBeenCalledTimes(1));
    const [params, opts] = createSessionMock.mock.calls[0];
    expect(params.kiln).toBe('/home/user/kilns/helios');
    expect(params.agent_type).toBeUndefined();
    expect(opts.initialMessage).toBe('hello world');
    expect(opts.model).toBe('openai/gpt-4o');

    await waitFor(() => expect(closeDraftTabMock).toHaveBeenCalledWith('tab-draft-1'));
  });

  it('attaches additional kilns and passes them as connect_kilns', async () => {
    const { getByTestId } = await setup();
    fireEvent.change(getByTestId('draft-extra-kilns'), {
      target: { value: '/home/user/kilns/other' },
    });
    fireEvent.input(getByTestId('draft-input'), { target: { value: 'multi-kiln idea' } });
    fireEvent.click(getByTestId('draft-send'));

    await waitFor(() => expect(createSessionMock).toHaveBeenCalledTimes(1));
    const [params] = createSessionMock.mock.calls[0];
    expect(params.connect_kilns).toEqual(['/home/user/kilns/other']);
  });

  it('creates an ACP session without a model override', async () => {
    const { getByTestId } = await setup();
    fireEvent.change(getByTestId('draft-agent'), { target: { value: 'claude' } });
    fireEvent.input(getByTestId('draft-input'), { target: { value: 'refactor auth' } });
    fireEvent.click(getByTestId('draft-send'));

    await waitFor(() => expect(createSessionMock).toHaveBeenCalledTimes(1));
    const [params, opts] = createSessionMock.mock.calls[0];
    expect(params.agent_type).toBe('acp');
    expect(params.agent_name).toBe('claude');
    expect(opts.model).toBeUndefined();
    expect(opts.initialMessage).toBe('refactor auth');
  });

  it('prefills the message box from the Home composer handoff', async () => {
    consumeDraftPrefillMock.mockReturnValueOnce('carried over from home');
    const { getByTestId } = await setup();
    expect((getByTestId('draft-input') as HTMLTextAreaElement).value).toBe(
      'carried over from home',
    );
  });

  it('focuses the message textarea on mount', async () => {
    const { getByTestId } = await setup();
    expect(document.activeElement).toBe(getByTestId('draft-input'));
  });

  it('submits on Enter (without Shift)', async () => {
    const { getByTestId } = await setup();
    const input = getByTestId('draft-input');
    fireEvent.input(input, { target: { value: 'quick idea' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(createSessionMock).toHaveBeenCalledTimes(1));
    expect(createSessionMock.mock.calls[0][1].initialMessage).toBe('quick idea');
  });

  it('keeps the draft open when creation fails', async () => {
    createSessionMock.mockRejectedValueOnce(new Error('daemon down'));
    const { getByTestId } = await setup();
    fireEvent.input(getByTestId('draft-input'), { target: { value: 'hello' } });
    fireEvent.click(getByTestId('draft-send'));

    await waitFor(() => expect(createSessionMock).toHaveBeenCalledTimes(1));
    expect(closeDraftTabMock).not.toHaveBeenCalled();
    // Button recovers so the user can retry.
    await waitFor(() =>
      expect((getByTestId('draft-send') as HTMLButtonElement).disabled).toBe(false),
    );
  });
});

describe('DraftSessionPanel — instant submit preview', () => {
  it('shows the user message + working dots the moment Enter is pressed, before createSession resolves', async () => {
    // A slow daemon create (cold kiln open can take seconds) must not leave
    // the user staring at the dead form — the panel becomes the conversation
    // immediately.
    let resolveCreate: (v: { id: string }) => void = () => {};
    createSessionMock.mockReturnValueOnce(
      new Promise((r) => (resolveCreate = r)),
    );

    const { getByTestId, queryByTestId } = await setup();
    fireEvent.input(getByTestId('draft-input'), { target: { value: 'first message' } });
    fireEvent.click(getByTestId('draft-send'));

    // Synchronously visible — no awaits between click and preview.
    const pending = getByTestId('draft-pending');
    expect(pending.textContent).toContain('first message');
    // The form is hidden while pending.
    expect((getByTestId('draft-input').closest('.hidden'))).not.toBeNull();

    resolveCreate({ id: 'sess-9' });
    await waitFor(() => expect(closeDraftTabMock).toHaveBeenCalled());
    void queryByTestId;
  });
});
