/** The settings popover. It holds one setting now: how an expanded panel ends. */
import type { Component } from 'solid-js';
import { CheckRow } from '../primitives/CheckRow';
import { SectionLabel } from '../primitives/SectionLabel';

export const SettingsPanel: Component<{ centreFocusExit: boolean; onCentreFocusExit: (on: boolean) => void }> = (props) => (
  <div class="mk-settings">
    <SectionLabel>Expand</SectionLabel>
    <CheckRow checked={props.centreFocusExit} onChange={props.onCentreFocusExit} note="Off: only its toggle ends it (Shift+Esc).">
      End an expanded panel when focus moves to the documents
    </CheckRow>
  </div>
);
