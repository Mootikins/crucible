/** The look toolbox on the tweaks store. It shows while the rail's Look button has it open. */
import { Show, type Component } from 'solid-js';
import { ToolboxPanel } from '../components/toolbox/ToolboxPanel';
import { accentOptions, resetTweaks, setToolboxOpen, setTweak, toolboxOpen, tweaks, tweaksCss } from '../tweaks';

export const ToolboxContainer: Component = () => (
  <Show when={toolboxOpen()}>
    <ToolboxPanel
      tweaks={tweaks}
      accents={accentOptions}
      onSet={setTweak}
      onReset={resetTweaks}
      onClose={() => setToolboxOpen(false)}
      css={tweaksCss}
    />
  </Show>
);
