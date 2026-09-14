import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { createSignal, type ParentProps } from 'solid-js';
import { ComposerCard } from '@/components/composer/ComposerCard';
import { defaultSettings, SETTINGS_STORAGE_KEY } from '@/lib/settings';
import App from './App';

// Keep the application's provider tree and the complete composer/mic/provider
// path real; replace unrelated shell panels and daemon-backed contexts.
vi.mock('@/components/AppShell', () => ({
  AppShell: () => {
    const [value, setValue] = createSignal('Draft');
    return <ComposerCard value={value} setValue={setValue} placeholder="Message"
      testid="voice-composer" onSubmit={() => {}} action={<button>Send</button>} />;
  },
}));
vi.mock('@/contexts/ProjectContext', async (original) => ({
  ...await original<object>(), ProjectProvider: (props: ParentProps) => props.children,
}));
vi.mock('@/contexts/SessionContext', async (original) => ({
  ...await original<object>(), SessionProvider: (props: ParentProps) => props.children,
}));
vi.mock('@/contexts/EditorContext', async (original) => ({
  ...await original<object>(), EditorProvider: (props: ParentProps) => props.children,
}));
vi.mock('@/components/CommandPalette', () => ({ CommandPalette: () => null }));
vi.mock('@/components/NotificationToast', () => ({ NotificationToast: () => null }));
vi.mock('@/components/ExportDialog', () => ({ ExportDialog: () => null }));
vi.mock('@/components/settings/SettingsModal', () => ({ SettingsModal: () => null }));
vi.mock('@/components/AuthTokenPrompt', () => ({ AuthTokenPrompt: () => null }));
vi.mock('@/lib/register-panels', () => ({ registerPanels: () => {} }));
vi.mock('@/lib/shell-boot', () => ({ startLayoutPersistence: () => {}, markShell: () => {} }));
vi.mock('@/stores/attentionStore', () => ({
  attentionActions: { startPolling: () => () => {} },
}));
vi.mock('@/hooks/useMediaRecorder', () => ({
  useMediaRecorder: () => ({
    isRecording: () => false, audioLevel: () => 0,
    startRecording: async () => {},
    stopRecording: async () => new Blob(['audio'], { type: 'audio/webm' }),
  }),
}));
vi.mock('@/lib/sounds', () => ({
  playRecordingStartSound: () => {}, playRecordingEndSound: () => {},
}));

const fetchMock = vi.fn();
beforeEach(() => {
  localStorage.clear();
  localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify({
    ...defaultSettings,
    transcription: { provider: 'server', serverUrl: 'https://speech.test', model: 'whisper-1', language: 'en' },
  }));
  fetchMock.mockReset();
  fetchMock.mockImplementation(async (url: string) => new Response(JSON.stringify(
    url.endsWith('/v1/audio/transcriptions') ? { text: 'spoken words' } : {},
  ), { status: 200, headers: { 'Content-Type': 'application/json' } }));
  vi.stubGlobal('fetch', fetchMock);
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); localStorage.clear(); });

async function record() {
  const mic = screen.getByTestId('mic-button');
  fireEvent.mouseDown(mic);
  await waitFor(() => expect(mic).toHaveAttribute('data-state', 'recording'));
  fireEvent.mouseUp(mic);
  return mic;
}

it('the application supplies transcription to its composer and preserves the draft', async () => {
  render(() => <App />);
  await record();
  await waitFor(() => expect(screen.getByTestId('voice-composer')).toHaveValue('Draft spoken words'));
  const request = fetchMock.mock.calls.find(([url]) => url === 'https://speech.test/v1/audio/transcriptions');
  expect(request).toBeDefined();
  expect(request![1].method).toBe('POST');
  const body = request![1].body as FormData;
  expect(body.get('model')).toBe('whisper-1');
  expect(body.get('language')).toBe('en');
  expect(body.get('file')).toBeInstanceOf(Blob);
});

it('a transcription failure stays visible and does not replace the draft', async () => {
  fetchMock.mockImplementation(async (url: string) =>
    url.endsWith('/v1/audio/transcriptions')
      ? new Response('unavailable', { status: 503, statusText: 'Unavailable' })
      : new Response('{}'));
  render(() => <App />);
  const mic = await record();
  await waitFor(() => expect(mic).toHaveAttribute('data-state', 'error'));
  expect(mic.title).toContain('503');
  expect(screen.getByTestId('voice-composer')).toHaveValue('Draft');
});
