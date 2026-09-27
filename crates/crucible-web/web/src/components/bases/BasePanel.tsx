import { createSignal, Show, type Component } from 'solid-js';
import { BaseView } from './BaseView';
import FileViewerPanel from '../FileViewerPanel';
export const BasePanel: Component<{ filePath?: string }> = (props) => {
  const [source,setSource] = createSignal(false);
  return <div class="h-full overflow-auto"><button class="m-2 text-sm underline" onClick={() => setSource(!source())}>{source() ? 'Show views' : 'Edit source'}</button>
    <Show when={source()} fallback={<BaseView filePath={props.filePath} />}><FileViewerPanel filePath={props.filePath} /></Show>
  </div>;
};
