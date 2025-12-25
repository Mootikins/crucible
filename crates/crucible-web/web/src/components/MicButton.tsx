import { Component, createSignal } from 'solid-js';

interface MicButtonProps {
  onTranscription: (text: string) => void;
  disabled?: boolean;
}

type RecordingState = 'idle' | 'recording' | 'processing';

export const MicButton: Component<MicButtonProps> = (props) => {
  const [state, setState] = createSignal<RecordingState>('idle');

  // Placeholder - will be implemented with useMediaRecorder and WhisperContext
  const handleMouseDown = () => {
    if (props.disabled) return;
    setState('recording');
  };

  const handleMouseUp = async () => {
    if (state() !== 'recording') return;
    setState('processing');

    // TODO: Actual transcription
    // For now, simulate with a timeout
    await new Promise((resolve) => setTimeout(resolve, 500));
    props.onTranscription('[Voice input placeholder]');
    setState('idle');
  };

  const stateStyles = () => {
    switch (state()) {
      case 'recording':
        return 'bg-red-600 animate-pulse';
      case 'processing':
        return 'bg-yellow-600';
      default:
        return 'bg-neutral-700 hover:bg-neutral-600';
    }
  };

  return (
    <button
      type="button"
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onTouchStart={handleMouseDown}
      onTouchEnd={handleMouseUp}
      disabled={props.disabled}
      class={`p-2 rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${stateStyles()}`}
      data-testid="mic-button"
      data-state={state()}
      title={
        state() === 'recording'
          ? 'Recording... Release to stop'
          : state() === 'processing'
            ? 'Processing...'
            : 'Hold to record (or press Space)'
      }
    >
      {state() === 'processing' ? (
        <span class="flex items-center gap-0.5 w-5 h-5 justify-center">
          <span class="w-1 h-1 bg-white rounded-full animate-bounce" />
          <span class="w-1 h-1 bg-white rounded-full animate-bounce delay-75" />
          <span class="w-1 h-1 bg-white rounded-full animate-bounce delay-150" />
        </span>
      ) : (
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="currentColor"
          class="w-5 h-5 text-white"
        >
          <path d="M8.25 4.5a3.75 3.75 0 117.5 0v8.25a3.75 3.75 0 11-7.5 0V4.5z" />
          <path d="M6 10.5a.75.75 0 01.75.75v1.5a5.25 5.25 0 1010.5 0v-1.5a.75.75 0 011.5 0v1.5a6.751 6.751 0 01-6 6.709v2.291h3a.75.75 0 010 1.5h-7.5a.75.75 0 010-1.5h3v-2.291a6.751 6.751 0 01-6-6.709v-1.5A.75.75 0 016 10.5z" />
        </svg>
      )}
    </button>
  );
};
