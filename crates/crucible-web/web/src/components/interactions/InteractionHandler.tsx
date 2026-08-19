import { Component, Switch, Match } from 'solid-js';
import type { InteractionRequest, InteractionResponse } from '@/lib/types';
import { AskInteraction } from './AskInteraction';
import { AskBatchInteraction } from './AskBatchInteraction';
import { EditInteraction } from './EditInteraction';
import { ShowInteraction } from './ShowInteraction';
import { PopupInteraction } from './PopupInteraction';
import { PanelInteraction } from './PanelInteraction';
import { PermissionInteraction } from './PermissionInteraction';

interface Props {
  request: InteractionRequest;
  onRespond: (response: InteractionResponse) => void;
}

/**
 * Every `InteractionRequest.kind` must have an arm here.
 *
 * `interaction-coverage.test.ts` fails when one does not, because a request
 * the browser cannot draw is a caller parked until its timeout with nothing on
 * screen to explain why. The kind list is Rust's
 * `InteractionRequest::KINDS`, mirrored in `lib/types.ts`.
 */
export const InteractionHandler: Component<Props> = (props) => {
  return (
    <Switch>
      <Match when={props.request.kind === 'ask'}>
        <AskInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'ask' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.kind === 'ask_batch'}>
        <AskBatchInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'ask_batch' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.kind === 'edit'}>
        <EditInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'edit' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.kind === 'show'}>
        <ShowInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'show' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.kind === 'popup'}>
        <PopupInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'popup' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.kind === 'panel'}>
        <PanelInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'panel' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.kind === 'permission'}>
        <PermissionInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'permission' }>}
          onRespond={props.onRespond}
        />
      </Match>
    </Switch>
  );
};
