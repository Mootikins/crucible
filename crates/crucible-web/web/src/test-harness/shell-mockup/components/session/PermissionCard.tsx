/**
 * A permission request for an edit, above the composer: the question, the
 * change as a diff, and the answers. The two common scopes are buttons; the
 * wider two wait in a quiet menu beside Deny. The real app answers with
 * `respondToInteraction` (a `PermResponse`); its scopes are once, session,
 * project and user (`PermissionInteraction.tsx`).
 */
import type { Component } from 'solid-js';
import { MoreHorizontal } from 'lucide-solid';
import { Button } from '../primitives/Button';
import { Menu } from '../primitives/Menu';
import { SectionLabel } from '../primitives/SectionLabel';
import { PermissionDiff, type PermissionDiffProps } from './PermissionDiff';

export type PermissionChoice = 'once' | 'session' | 'project' | 'user' | 'deny';

export interface PermissionCardProps {
  /** The name of the file that the edit writes. */
  file: string;
  diff: PermissionDiffProps;
  onAnswer: (choice: PermissionChoice) => void;
}

export const PermissionCard: Component<PermissionCardProps> = (props) => (
  <div class="mk-perm" role="alertdialog" aria-label="Permission request">
    <SectionLabel>
      Edit <code>{props.file}</code>?
    </SectionLabel>
    <PermissionDiff path={props.diff.path} add={props.diff.add} del={props.diff.del} rows={props.diff.rows} />
    <div class="mk-pa">
      <Button variant="primary" onClick={() => props.onAnswer('once')}>
        Allow
      </Button>
      <Button onClick={() => props.onAnswer('session')}>Allow for session</Button>
      <span class="mk-grow" />
      <Button variant="ghost" onClick={() => props.onAnswer('deny')}>
        Deny
      </Button>
      <Menu
        label="More ways to allow"
        trigger={<MoreHorizontal class="mk-i" />}
        align="end"
        up
        entries={[
          { kind: 'item', label: 'Allow for this project', onSelect: () => props.onAnswer('project') },
          { kind: 'item', label: 'Allow always', onSelect: () => props.onAnswer('user') },
        ]}
      />
    </div>
  </div>
);
