/** A note tab on the mock store. A port reads EditorContext and `useGetFileContent`. */
import { Show, type Component } from 'solid-js';
import { useFloatingWindow } from '@/windowing';
import { canGoBack, canGoForward, focusComposer, goHistory } from '../actions';
import { NoteView } from '../components/note/NoteView';
import { pendingHunks, state } from '../state';
import { HunkContainer } from './HunkContainer';
import { noteLinks } from './wikilinks';

export const NoteContainer: Component<{ tabId?: string; path: string }> = (props) => {
  const links = noteLinks(false);
  // A note in a floating window with no tab bar hosts that window's controls.
  const fw = useFloatingWindow();
  const hostsControls = () => !!fw && fw.chrome() === 'merged' && !fw.hasTabBar();
  return (
    <NoteView
      path={props.path}
      source={state.notes[props.path]}
      history={
        props.tabId
          ? {
              canBack: canGoBack(props.tabId),
              canForward: canGoForward(props.tabId),
              onGo: (step) => props.tabId && goHistory(props.tabId, step),
            }
          : undefined
      }
      links={links}
      renderHunk={(id) => <HunkContainer id={id} />}
      orphanHunks={pendingHunks().filter((id) => state.hunks[id]!.path === props.path)}
      onAsk={focusComposer}
      windowControls={
        <Show when={hostsControls() && fw}>
          {(w) => {
            const Controls = w().controls;
            return <Controls />;
          }}
        </Show>
      }
    />
  );
};
