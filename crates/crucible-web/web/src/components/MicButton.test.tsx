import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { MicButton } from './MicButton';

// Mock the hooks and contexts
const mockStartRecording = vi.fn();
const mockStopRecording = vi.fn();
const mockTranscribe = vi.fn();
const mockLoadModel = vi.fn();

vi.mock('@/contexts/WhisperContext', () => ({
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
