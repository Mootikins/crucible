import { Component } from 'solid-js';

interface PlaceholderPanelProps {
  name?: string;
}

const PlaceholderPanel: Component<PlaceholderPanelProps> = (props) => {
  return (
    <div class="flex flex-col items-center justify-center h-full gap-4 text-center p-4">
      <div class="text-lg font-semibold text-zinc-300">{props.name || 'Panel'}</div>
      <div class="text-sm text-zinc-500">Coming soon</div>
    </div>
  );
};

export default PlaceholderPanel;
