import { Component, Switch, Match } from 'solid-js';
import type { InteractionRequest, InteractionResponse } from '@/lib/types';
import { AskInteraction } from './AskInteraction';
import { PopupInteraction } from './PopupInteraction';
import { PermissionInteraction } from './PermissionInteraction';

interface Props {
  request: InteractionRequest;
  onRespond: (response: InteractionResponse) => void;
}

export const InteractionHandler: Component<Props> = (props) => {
  return (
    <Switch>
      <Match when={props.request.kind === 'ask'}>
        <AskInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'ask' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.kind === 'popup'}>
        <PopupInteraction
          request={props.request as Extract<InteractionRequest, { kind: 'popup' }>}
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
