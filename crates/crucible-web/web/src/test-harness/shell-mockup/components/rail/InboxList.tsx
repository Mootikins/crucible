/**
 * The inbox: every permission request, then every session whose edits wait
 * for review. The real app reads the requests from the attention store's
 * `pendingInteraction` and the counts from `reviewStore.unreviewedCount`.
 */
import { For, Show, type Component } from 'solid-js';
import { Button } from '../primitives/Button';
import { EmptyText } from '../primitives/EmptyText';
import { SectionLabel } from '../primitives/SectionLabel';
import { InboxItem } from './InboxItem';

export interface InboxPermissionView {
  id: string;
  title: string;
  color: string;
  /** The name of the file that the edit writes. */
  file: string;
}

export interface InboxReviewView {
  id: string;
  title: string;
  color: string;
  pending: number;
}

export interface InboxListProps {
  permissions: InboxPermissionView[];
  reviews: InboxReviewView[];
  onAllow: (id: string) => void;
  onDeny: (id: string) => void;
  onOpen: (id: string) => void;
  onReview: (id: string) => void;
}

export const InboxList: Component<InboxListProps> = (props) => (
  <div class="mk-inbox">
    <SectionLabel>Waiting for you</SectionLabel>
    <For each={props.permissions}>
      {(p) => (
        <InboxItem title={p.title} color={p.color} text={<>Edit <code>{p.file}</code>?</>}>
          <Button variant="primary" onClick={() => props.onAllow(p.id)}>Allow</Button>
          <Button variant="ghost" onClick={() => props.onDeny(p.id)}>Deny</Button>
          <Button variant="ghost" onClick={() => props.onOpen(p.id)}>Open</Button>
        </InboxItem>
      )}
    </For>
    <For each={props.reviews}>
      {(r) => (
        <InboxItem title={r.title} color={r.color} text={`${r.pending} to review`}>
          <Button onClick={() => props.onReview(r.id)}>Review</Button>
        </InboxItem>
      )}
    </For>
    <Show when={!(props.permissions.length + props.reviews.length)}>
      <EmptyText pad>Nothing waits for you.</EmptyText>
    </Show>
  </div>
);
