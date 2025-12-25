/**
 * Audio feedback utilities using Web Audio API.
 * Generates simple tones without needing external audio files.
 */

let audioContext: AudioContext | null = null;

function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();
  }
  return audioContext;
}

/**
 * Play a short "blip" sound to indicate recording has stopped.
 * Uses Web Audio API to generate a simple tone.
 */
export function playRecordingEndSound(): void {
  try {
    const ctx = getAudioContext();

    // Resume context if suspended (required for user gesture)
    if (ctx.state === 'suspended') {
      ctx.resume();
    }

    const oscillator = ctx.createOscillator();
    const gainNode = ctx.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(ctx.destination);

    // Configure the tone
    oscillator.type = 'sine';
    oscillator.frequency.setValueAtTime(880, ctx.currentTime); // A5 note
    oscillator.frequency.setValueAtTime(1320, ctx.currentTime + 0.05); // E6 note (rising)

    // Quick fade in/out for a pleasant blip
    gainNode.gain.setValueAtTime(0, ctx.currentTime);
    gainNode.gain.linearRampToValueAtTime(0.3, ctx.currentTime + 0.02);
    gainNode.gain.linearRampToValueAtTime(0, ctx.currentTime + 0.12);

    oscillator.start(ctx.currentTime);
    oscillator.stop(ctx.currentTime + 0.12);
  } catch (err) {
    // Audio not available, silently fail
    console.debug('Could not play sound:', err);
  }
}

/**
 * Play a subtle sound when recording starts.
 */
export function playRecordingStartSound(): void {
  try {
    const ctx = getAudioContext();

    if (ctx.state === 'suspended') {
      ctx.resume();
    }

    const oscillator = ctx.createOscillator();
    const gainNode = ctx.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(ctx.destination);

    // Lower, shorter tone for start
    oscillator.type = 'sine';
    oscillator.frequency.setValueAtTime(440, ctx.currentTime); // A4

    gainNode.gain.setValueAtTime(0, ctx.currentTime);
    gainNode.gain.linearRampToValueAtTime(0.2, ctx.currentTime + 0.02);
    gainNode.gain.linearRampToValueAtTime(0, ctx.currentTime + 0.08);

    oscillator.start(ctx.currentTime);
    oscillator.stop(ctx.currentTime + 0.08);
  } catch (err) {
    console.debug('Could not play sound:', err);
  }
}
