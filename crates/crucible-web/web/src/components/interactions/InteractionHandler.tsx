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
      <Match when={props.request.type === 'ask'}>
        <AskInteraction
          request={props.request as Extract<InteractionRequest, { type: 'ask' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.type === 'popup'}>
        <PopupInteraction
          request={props.request as Extract<InteractionRequest, { type: 'popup' }>}
          onRespond={props.onRespond}
        />
      </Match>
      <Match when={props.request.type === 'permission'}>
        <PermissionInteraction
          request={props.request as Extract<InteractionRequest, { type: 'permission' }>}
          onRespond={props.onRespond}
        />
      </Match>
    </Switch>
  );
};
