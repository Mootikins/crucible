/**
 * A permission request for an edit, above the composer: the file, the lines
 * it adds, and the answers. The real app answers with `respondToInteraction`
 * (a `PermResponse`); its scopes are once, session, project and user.
 */
import type { Component } from 'solid-js';
import { Button } from '../primitives/Button';
import { DiffLines } from '../primitives/DiffLines';
import { SectionLabel } from '../primitives/SectionLabel';

export type PermissionChoice = 'once' | 'session' | 'deny';

export interface PermissionCardProps {
  /** The name of the file that the edit writes. */
  file: string;
  lines: string[];
  onAnswer: (choice: PermissionChoice) => void;
}

export const PermissionCard: Component<PermissionCardProps> = (props) => (
  <div class="mk-perm" role="alertdialog" aria-label="Permission request">
    <SectionLabel>
      Edit <code>{props.file}</code>?
    </SectionLabel>
    <pre>
      <DiffLines add={props.lines} />
    </pre>
    <div class="mk-pa">
      <Button variant="primary" onClick={() => props.onAnswer('once')}>
        Allow
      </Button>
      <Button onClick={() => props.onAnswer('session')}>Allow for session</Button>
      <span class="mk-grow" />
      <Button variant="ghost" onClick={() => props.onAnswer('deny')}>
        Deny
      </Button>
    </div>
  </div>
);
