import { Component } from 'solid-js';

interface VolumeMeterProps {
  /** Audio level from 0-1 */
  level: () => number;
  /** Size in pixels */
  size?: number;
  /** Whether currently recording */
  active?: boolean;
}

/**
 * Circular volume meter that fills based on audio level.
 * Uses SVG for smooth rendering and CSS for animations.
 */
export const VolumeMeter: Component<VolumeMeterProps> = (props) => {
  const size = () => props.size ?? 40;
  const strokeWidth = 3;
  const radius = () => (size() - strokeWidth) / 2;
  const circumference = () => 2 * Math.PI * radius();

  // Calculate stroke-dashoffset based on level (0-1)
  // Full circle when level is 1, empty when level is 0
  const dashOffset = () => {
    const level = props.level();
    return circumference() * (1 - level);
  };

  return (
    <div
      class="absolute inset-0 flex items-center justify-center pointer-events-none"
      style={{ opacity: props.active ? 1 : 0, transition: 'opacity 150ms' }}
    >
      <svg
        width={size()}
        height={size()}
        viewBox={`0 0 ${size()} ${size()}`}
        class="transform -rotate-90"
      >
        {/* Background ring */}
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          fill="none"
          stroke="rgba(255, 255, 255, 0.2)"
          stroke-width={strokeWidth}
        />
        {/* Level indicator ring */}
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          fill="none"
          stroke="rgb(239, 68, 68)" /* red-500 */
          stroke-width={strokeWidth}
          stroke-linecap="round"
          stroke-dasharray={String(circumference())}
          stroke-dashoffset={dashOffset()}
          style={{ transition: 'stroke-dashoffset 50ms ease-out' }}
        />
      </svg>
    </div>
  );
};

/**
 * Alternative: Vertical bar meter
 */
export const VolumeMeterBar: Component<VolumeMeterProps> = (props) => {
  return (
    <div
      class="absolute left-0 bottom-0 w-1 bg-neutral-700 rounded-full overflow-hidden"
      style={{
        height: '100%',
        opacity: props.active ? 1 : 0,
        transition: 'opacity 150ms',
      }}
    >
      <div
        class="absolute bottom-0 w-full bg-red-500 rounded-full"
        style={{
          height: `${props.level() * 100}%`,
          transition: 'height 50ms ease-out',
        }}
      />
    </div>
  );
};

/**
 * Pulsing glow effect based on audio level
 * No CSS transition for maximum reactivity - updates at 60fps via requestAnimationFrame
 */
export const VolumeGlow: Component<VolumeMeterProps> = (props) => {
  // Dramatic glow that really pops on loud audio
  const glowIntensity = () => props.level() * 40; // 0-40px blur
  const glowSpread = () => props.level() * 20; // 0-20px spread
  const opacity = () => 0.4 + props.level() * 0.6; // 0.4-1.0 opacity

  return (
    <div
      class="absolute inset-0 rounded-lg pointer-events-none"
      style={{
        'box-shadow': props.active
          ? `0 0 ${glowIntensity()}px ${glowSpread()}px rgba(239, 68, 68, ${opacity()})`
          : 'none',
        // No transition - direct updates for instant response
      }}
    />
  );
};
