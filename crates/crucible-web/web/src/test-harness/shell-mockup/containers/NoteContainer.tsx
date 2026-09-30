/** A note tab on the mock store. A port reads EditorContext and `useGetFileContent`. */
import { Match, Show, Switch, type Component, type JSX } from 'solid-js';
import { useFloatingWindow } from '@/windowing';
import { windowActions, windowStore } from '@/windowing/store';
import { canGoBack, canGoForward, goHistory, openNote } from '../actions';
import { basename } from '../components/path';
import { HoverCornerControls } from '../components/note/HoverCornerControls';
import { HoverCrumbsBar } from '../components/note/HoverCrumbsBar';
import { HoverTitleBar } from '../components/note/HoverTitleBar';
import { NoteView } from '../components/note/NoteView';
import { pendingHunks, state } from '../state';
import { tweaks } from '../tweaks';
import { HunkContainer } from './HunkContainer';
import { noteLinks } from './wikilinks';

/** Takes the window's controls for the content, so the window draws no title bar. */
const ClaimControls: Component<{ claim: () => void; children: JSX.Element }> = (props) => {
  props.claim();
  return props.children;
};

export const NoteContainer: Component<{ tabId?: string; path: string }> = (props) => {
  const links = noteLinks(false);
  // A note in a floating window with no tab bar hosts that window's controls.
  const fw = useFloatingWindow();
  const hostsControls = () => !!fw && fw.chrome() === 'merged' && !fw.hasTabBar();
  // A hover popup (a transient window) shows a lighter bar until it is pinned.
  const isHover = () => !!fw && !!windowStore.floatingWindows.find((w) => w.id === fw.id)?.transient;
  const hover = {
    title: () => basename(props.path),
    onPin: () => fw && windowActions.pinFloatingWindow(fw.id),
    onClose: () => fw?.close(),
    onOpenInTab: () => {
      openNote(props.path, { where: 'tab' });
      fw?.close();
    },
  };
  return (
    <NoteView
      path={props.path}
      source={state.notes[props.path]}
      history={
        props.tabId && !isHover()
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
      windowControls={
        <Show when={hostsControls() && fw}>
          {(w) => {
            // The nav bar of a note has little room: no roll up, no maximize.
            const Controls = w().controls;
            return <Controls compact />;
          }}
        </Show>
      }
      bar={
        isHover() && fw
          ? (view) => (
              <ClaimControls claim={fw.claim}>
                <Switch>
                  <Match when={tweaks.hoverBar === 'title'}>
                    <HoverTitleBar title={hover.title()} onPin={hover.onPin} onClose={hover.onClose} />
                  </Match>
                  <Match when={tweaks.hoverBar === 'none'}>
                    <HoverCornerControls title={hover.title()} onPin={hover.onPin} onClose={hover.onClose} onOpenInTab={hover.onOpenInTab} />
                  </Match>
                  <Match when={tweaks.hoverBar === 'crumbs'}>
                    <HoverCrumbsBar
                      title={hover.title()}
                      path={props.path}
                      mode={view.mode}
                      onMode={view.setMode}
                      onPin={hover.onPin}
                      onClose={hover.onClose}
                      onOpenInTab={hover.onOpenInTab}
                    />
                  </Match>
                </Switch>
              </ClaimControls>
            )
          : undefined
      }
    />
  );
};
