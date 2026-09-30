/** One transcript item, drawn by its kind. */
import type { Component } from 'solid-js';
import type { WikilinkEvents } from '../primitives/wikilinks';
import { AssistantText } from './AssistantText';
import { PrecogCard } from './PrecogCard';
import { RecordLine } from './RecordLine';
import { ThinkingLine } from './ThinkingLine';
import { ToolLine } from './ToolLine';
import { UserMessage } from './UserMessage';
import type { ToolLineHandlers, TranscriptItem, TurnHandlers } from './types';

export interface TranscriptItemViewProps {
  it: TranscriptItem;
  last: boolean;
  copyText?: string;
  links: WikilinkEvents;
  tools: ToolLineHandlers;
  turn: TurnHandlers;
}

export const TranscriptItemView: Component<TranscriptItemViewProps> = (props) => {
  // An item never changes its kind, so the view reads it once.
  const it = props.it;
  if (it.t === 'user') return <UserMessage text={it.text} time={it.time} links={props.links} onEdit={props.turn.onEdit} />;
  if (it.t === 'precog') return <PrecogCard notes={it.notes} onOpen={props.tools.onOpenPath} />;
  if (it.t === 'thinking') return <ThinkingLine secs={it.secs} />;
  if (it.t === 'record') return <RecordLine>{it.text}</RecordLine>;
  if (it.t === 'tool') return <ToolLine it={it} tools={props.tools} />;
  return <AssistantText md={it.md} elapsed={it.elapsed} tokens={it.tokens} last={props.last} copyText={props.copyText} links={props.links} onRegenerate={props.turn.onRegenerate} />;
};
