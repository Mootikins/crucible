import { Component, createSignal, Show } from 'solid-js';
import { useMediaRecorder } from '@/hooks/useMediaRecorder';
import { useWhisper } from '@/contexts/WhisperContext';
import { playRecordingStartSound, playRecordingEndSound } from '@/lib/sounds';

interface MicButtonProps {
  onTranscription: (text: string) => void;
  disabled?: boolean;
}

type RecordingState = 'idle' | 'recording' | 'processing' | 'error';

export const MicButton: Component<MicButtonProps> = (props) => {
  const [state, setState] = createSignal<RecordingState>('idle');
  const [errorMessage, setErrorMessage] = createSignal<string | null>(null);
  const { isRecording, error: recorderError, startRecording, stopRecording } = useMediaRecorder();
  const { status: whisperStatus, transcribe, loadModel, progress, error: whisperError } = useWhisper();

  // Preload model on first interaction
  const ensureModelLoaded = async () => {
    if (whisperStatus() === 'idle') {
      try {
        await loadModel();
      } catch (err) {
        console.error('Failed to preload model:', err);
      }
    }
  };

  const handleMouseDown = async () => {
    if (props.disabled || state() !== 'idle') return;

    // Clear any previous error
    setErrorMessage(null);

    // Start loading model in background if not ready
    ensureModelLoaded();

    try {
      await startRecording();
      playRecordingStartSound();
      setState('recording');
    } catch (err) {
      console.error('Failed to start recording:', err);
      const message = err instanceof Error ? err.message : 'Failed to access microphone';
      setErrorMessage(message);
      setState('error');
      // Auto-clear error after 3 seconds
      setTimeout(() => {
        if (state() === 'error') {
          setState('idle');
          setErrorMessage(null);
        }
      }, 3000);
    }
  };

  const handleMouseUp = async () => {
    if (state() !== 'recording') return;

    setState('processing');
    playRecordingEndSound();

    try {
      const audioBlob = await stopRecording();

      // Ensure model is ready
      if (whisperStatus() !== 'ready') {
        await loadModel();
      }

      // Transcribe
      const text = await transcribe(audioBlob);

      if (text.trim()) {
        props.onTranscription(text.trim());
      }
    } catch (err) {
      console.error('Transcription failed:', err);
      const message = err instanceof Error ? err.message : 'Transcription failed';
      setErrorMessage(message);
      setState('error');
      // Auto-clear error after 3 seconds
      setTimeout(() => {
        if (state() === 'error') {
          setState('idle');
          setErrorMessage(null);
        }
      }, 3000);
      return;
    }
    setState('idle');
  };

  const stateStyles = () => {
    switch (state()) {
      case 'recording':
        return 'bg-red-600 animate-pulse';
      case 'processing':
        return 'bg-yellow-600';
      case 'error':
        return 'bg-red-800';
      default:
        return whisperStatus() === 'loading'
          ? 'bg-blue-600'
          : 'bg-neutral-700 hover:bg-neutral-600';
    }
  };

  const getTitle = () => {
    if (state() === 'error' && errorMessage()) {
      return `Error: ${errorMessage()}`;
    }
    if (whisperStatus() === 'loading') {
      return `Loading speech model... ${progress()}%`;
    }
    if (whisperStatus() === 'error' && whisperError()) {
      return `Model error: ${whisperError()}`;
    }
    if (state() === 'recording') {
      return 'Recording... Release to stop';
    }
    if (state() === 'processing') {
      return 'Transcribing...';
    }
    return 'Hold to record (or press Space)';
  };

  return (
    <button
      type="button"
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onTouchStart={handleMouseDown}
      onTouchEnd={handleMouseUp}
      disabled={props.disabled || whisperStatus() === 'loading'}
      class={`p-2 rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${stateStyles()}`}
      data-testid="mic-button"
      data-state={state()}
      title={getTitle()}
    >
      <Show when={state() === 'error'}>
        {/* Error icon - exclamation mark */}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="currentColor"
          class="w-5 h-5 text-white"
        >
          <path fill-rule="evenodd" d="M2.25 12c0-5.385 4.365-9.75 9.75-9.75s9.75 4.365 9.75 9.75-4.365 9.75-9.75 9.75S2.25 17.385 2.25 12zM12 8.25a.75.75 0 01.75.75v3.75a.75.75 0 01-1.5 0V9a.75.75 0 01.75-.75zm0 8.25a.75.75 0 100-1.5.75.75 0 000 1.5z" clip-rule="evenodd" />
        </svg>
      </Show>
      <Show when={state() !== 'error' && (state() === 'processing' || whisperStatus() === 'loading')}>
        <span class="flex items-center gap-0.5 w-5 h-5 justify-center">
          <span class="w-1 h-1 bg-white rounded-full animate-bounce" />
          <span class="w-1 h-1 bg-white rounded-full animate-bounce delay-75" />
          <span class="w-1 h-1 bg-white rounded-full animate-bounce delay-150" />
        </span>
      </Show>
      <Show when={state() !== 'error' && state() !== 'processing' && whisperStatus() !== 'loading'}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="currentColor"
          class="w-5 h-5 text-white"
        >
          <path d="M8.25 4.5a3.75 3.75 0 117.5 0v8.25a3.75 3.75 0 11-7.5 0V4.5z" />
          <path d="M6 10.5a.75.75 0 01.75.75v1.5a5.25 5.25 0 1010.5 0v-1.5a.75.75 0 011.5 0v1.5a6.751 6.751 0 01-6 6.709v2.291h3a.75.75 0 010 1.5h-7.5a.75.75 0 010-1.5h3v-2.291a6.751 6.751 0 01-6-6.709v-1.5A.75.75 0 016 10.5z" />
        </svg>
      </Show>
    </button>
  );
};
