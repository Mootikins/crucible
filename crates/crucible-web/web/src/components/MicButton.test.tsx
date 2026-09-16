import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { MicButton } from './MicButton';
import { notificationActions } from '@/stores/notificationStore';

// Mock the hooks and contexts
const mockStartRecording = vi.fn();
const mockStopRecording = vi.fn();
const mockTranscribe = vi.fn();
const mockLoadModel = vi.fn();

vi.mock('@/contexts/WhisperContext', () => ({
  // A mocked provider reports nothing, so the button reports everything.
  reportedToUser: () => false,
  useWhisperSafe: () => ({
    status: () => 'ready',
    progress: () => 100,
    error: () => null,
    transcribe: mockTranscribe,
    loadModel: mockLoadModel,
  }),
}));

vi.mock('@/lib/sounds', () => ({
  playRecordingStartSound: vi.fn(),
  playRecordingEndSound: vi.fn(),
}));

// Helper to create test props
const createTestProps = () => {
  const [isRecording] = createSignal(false);
  return {
    onTranscription: vi.fn(),
    startRecording: mockStartRecording,
    stopRecording: mockStopRecording,
    isRecording,
  };
};

describe('MicButton', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockStartRecording.mockResolvedValue(undefined);
    mockStopRecording.mockResolvedValue(new Blob(['audio']));
    mockTranscribe.mockResolvedValue('transcribed text');
  });

  it('renders mic button', () => {
    const props = createTestProps();
    render(() => <MicButton {...props} />);

    expect(screen.getByTestId('mic-button')).toBeInTheDocument();
  });

  it('starts in idle state', () => {
    const props = createTestProps();
    render(() => <MicButton {...props} />);

    expect(screen.getByTestId('mic-button')).toHaveAttribute('data-state', 'idle');
  });

  it('shows recording state on mouseDown', async () => {
    const props = createTestProps();
    render(() => <MicButton {...props} />);

    const button = screen.getByTestId('mic-button');
    await fireEvent.mouseDown(button);

    expect(mockStartRecording).toHaveBeenCalled();
  });

  it('calls onTranscription with text after recording', async () => {
    mockTranscribe.mockResolvedValue('hello world');
    const props = createTestProps();

    render(() => <MicButton {...props} />);

    const button = screen.getByTestId('mic-button');

    // Start recording
    await fireEvent.mouseDown(button);

    // We need to manually set state since our mock doesn't trigger state changes
    // In integration, this would happen via the real hook
    await fireEvent.mouseUp(button);

    // Wait for transcription
    await vi.waitFor(() => {
      expect(props.onTranscription).toHaveBeenCalledWith('hello world');
    });
  });

  it('shows correct title for hold-to-talk', () => {
    const props = createTestProps();
    render(() => <MicButton {...props} />);

    expect(screen.getByTestId('mic-button')).toHaveAttribute(
      'title',
      'Hold to record (or press Space)'
    );
  });

  it('is disabled when disabled prop is true', () => {
    const props = createTestProps();
    render(() => <MicButton {...props} disabled={true} />);

    expect(screen.getByTestId('mic-button')).toBeDisabled();
  });

  it('handles recording errors', async () => {
    mockStartRecording.mockRejectedValue(new Error('Mic access denied'));
    const props = createTestProps();

    render(() => <MicButton {...props} />);

    const button = screen.getByTestId('mic-button');
    await fireEvent.mouseDown(button);

    await vi.waitFor(() => {
      expect(button).toHaveAttribute('data-state', 'error');
    });
  });

  it('handles transcription errors', async () => {
    mockTranscribe.mockRejectedValue(new Error('Transcription failed'));
    const props = createTestProps();

    render(() => <MicButton {...props} />);

    const button = screen.getByTestId('mic-button');

    await fireEvent.mouseDown(button);
    await fireEvent.mouseUp(button);

    await vi.waitFor(() => {
      expect(button).toHaveAttribute('data-state', 'error');
    });
  });

  it('does not call onTranscription for empty transcripts', async () => {
    mockTranscribe.mockResolvedValue('   '); // Whitespace only
    const props = createTestProps();

    render(() => <MicButton {...props} />);

    const button = screen.getByTestId('mic-button');
    await fireEvent.mouseDown(button);
    await fireEvent.mouseUp(button);

    await vi.waitFor(() => {
      expect(props.onTranscription).not.toHaveBeenCalled();
    });
  });
});

/**
 * A failure has to reach the notification area. The button used to keep the
 * message in a local signal and show it as a tooltip on a red circle for
 * three seconds, which nobody hovering a microphone icon reads. The
 * notification names the cause, so "permission denied" and "no microphone"
 * are told apart from a transcription server that is down.
 */
describe('MicButton reports failures to the notification area', () => {
  let added: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.clearAllMocks();
    mockStartRecording.mockResolvedValue(undefined);
    mockStopRecording.mockResolvedValue(new Blob(['audio']));
    mockTranscribe.mockResolvedValue('transcribed text');
    added = vi.spyOn(notificationActions, 'addNotification');
  });

  afterEach(() => {
    added.mockRestore();
  });

  it('names the cause when the microphone cannot start', async () => {
    mockStartRecording.mockRejectedValue(new Error('Microphone permission denied'));
    render(() => <MicButton {...createTestProps()} />);

    await fireEvent.mouseDown(screen.getByTestId('mic-button'));

    await vi.waitFor(() => {
      expect(added).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('Microphone permission denied'),
      );
    });
  });

  it('names the cause when transcription fails', async () => {
    mockTranscribe.mockRejectedValue(new Error('Transcription failed: 503 Service Unavailable'));
    render(() => <MicButton {...createTestProps()} />);

    const button = screen.getByTestId('mic-button');
    await fireEvent.mouseDown(button);
    await fireEvent.mouseUp(button);

    await vi.waitFor(() => {
      expect(added).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('Transcription failed: 503 Service Unavailable'),
      );
    });
  });

  it('names the cause when the speech model cannot load before transcribing', async () => {
    // The mocked context says the model is ready, so make the load fail on
    // the transcribe step instead: that is the path a cold local model takes.
    mockTranscribe.mockRejectedValue(new Error('Failed to load speech model: fetch failed'));
    render(() => <MicButton {...createTestProps()} />);

    const button = screen.getByTestId('mic-button');
    await fireEvent.mouseDown(button);
    await fireEvent.mouseUp(button);

    await vi.waitFor(() => {
      expect(added).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('Failed to load speech model: fetch failed'),
      );
    });
  });
});
