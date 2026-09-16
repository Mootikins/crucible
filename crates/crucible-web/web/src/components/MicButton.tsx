import { Component, createSignal, Show, Accessor } from 'solid-js';
import { useWhisperSafe, reportedToUser } from '@/contexts/WhisperContext';
import { playRecordingStartSound, playRecordingEndSound } from '@/lib/sounds';
import { Mic } from '@/lib/icons';
import { notificationActions } from '@/stores/notificationStore';

interface MicButtonProps {
  onTranscription: (text: string) => void;
  disabled?: boolean;
  // Recording controls passed from parent (ChatInput owns the recorder)
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<Blob>;
  isRecording: Accessor<boolean>;
}

type RecordingState = 'idle' | 'recording' | 'processing' | 'error';

export const MicButton: Component<MicButtonProps> = (props) => {
  const [state, setState] = createSignal<RecordingState>('idle');
  const { status: whisperStatus, transcribe, loadModel, progress, error: whisperError } = useWhisperSafe();

  /**
   * Show the failure on the button and put its cause in the notification
   * area.
   *
   * The button used to keep the message in a local signal and show it only
   * as a tooltip on a red circle for three seconds, which nobody hovering a
   * microphone icon reads. The red circle stays, as the visual; the words go
   * where every other error goes. A failure the provider already reported
   * (a model that would not load, a server that answered 503) is not
   * reported twice.
   */
  const fail = (prefix: string, err: unknown, fallback: string) => {
    console.error(`${prefix}:`, err);
    if (!reportedToUser(err)) {
      const cause = err instanceof Error ? err.message : fallback;
      notificationActions.addNotification('error', `${prefix}: ${cause}`);
    }
    setState('error');
    // Back to idle after a moment; the notification keeps the words.
    setTimeout(() => {
      if (state() === 'error') setState('idle');
    }, 3000);
  };

  // Preload model on first interaction
  const ensureModelLoaded = async () => {
    if (whisperStatus() === 'idle') {
      try {
        await loadModel();
      } catch (err) {
        // The provider reports its own load failure; nothing to add here.
        console.error('Failed to preload model:', err);
      }
    }
  };

  const handleMouseDown = async () => {
    if (props.disabled || state() !== 'idle') return;

    // Start loading model in background if not ready
    ensureModelLoaded();

    try {
      await props.startRecording();
      playRecordingStartSound();
      setState('recording');
    } catch (err) {
      fail('Could not start recording', err, 'Failed to access microphone');
    }
  };

  const handleMouseUp = async () => {
    if (state() !== 'recording') return;

    setState('processing');
    playRecordingEndSound();

    try {
      const audioBlob = await props.stopRecording();

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
      fail('Transcription failed', err, 'Transcription failed');
      return;
    }
    setState('idle');
  };

  const stateStyles = () => {
    switch (state()) {
      case 'recording':
        return 'bg-shell-ink'; // Inverted: ink bg, black icon
      case 'processing':
        return 'bg-primary-hover';
      case 'error':
        return 'bg-error-dark';
      default:
        return whisperStatus() === 'loading'
          ? 'bg-primary'
          : 'bg-transparent hover:bg-hover-wash';
    }
  };

  const iconColor = () => {
    if (state() === 'recording') return 'text-shell-bg';
    if (state() === 'idle' && whisperStatus() !== 'loading') return 'text-muted';
    return 'text-white';
  };

  const getTitle = () => {
    if (state() === 'error') {
      return 'Voice input failed — see notifications';
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
      // Its own shape now. It used to inherit one from a `rounded-full
      // overflow-hidden` pill it shared with the send button, which welded an
      // input method to a commit action and left send as a 28px sliver of a
      // capsule. The two are separate controls and they look it.
      class={`focus-ring relative flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${stateStyles()}`}
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
          class={`w-4 h-4 ${iconColor()}`}
        >
          <path fill-rule="evenodd" d="M2.25 12c0-5.385 4.365-9.75 9.75-9.75s9.75 4.365 9.75 9.75-4.365 9.75-9.75 9.75S2.25 17.385 2.25 12zM12 8.25a.75.75 0 01.75.75v3.75a.75.75 0 01-1.5 0V9a.75.75 0 01.75-.75zm0 8.25a.75.75 0 100-1.5.75.75 0 000 1.5z" clip-rule="evenodd" />
        </svg>
      </Show>
      <Show when={state() !== 'error' && (state() === 'processing' || whisperStatus() === 'loading')}>
        {/* Spinner */}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          class={`w-4 h-4 ${iconColor()} animate-spin`}
        >
          <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2" stroke-opacity="0.25" />
          <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        </svg>
      </Show>
      <Show when={state() !== 'error' && state() !== 'processing' && whisperStatus() !== 'loading'}>
        <Mic class={`w-4 h-4 ${iconColor()}`} />
      </Show>
    </button>
  );
};
