/**
 * The settings popover: how an expanded panel ends, and a stub for the
 * projects and kilns. The real app manages those in its settings modal
 * (`components/settings/WorkspaceSettings.tsx`).
 */
import type { Component } from 'solid-js';
import { CheckRow } from '../primitives/CheckRow';
import { EmptyText } from '../primitives/EmptyText';
import { SectionLabel } from '../primitives/SectionLabel';

export const SettingsPanel: Component<{ centreFocusExit: boolean; onCentreFocusExit: (on: boolean) => void }> = (props) => (
  <div class="mk-settings">
    <SectionLabel>Expand</SectionLabel>
    <CheckRow checked={props.centreFocusExit} onChange={props.onCentreFocusExit} note="Off: only its toggle ends it (Shift+Esc).">
      End an expanded panel when focus moves to the documents
    </CheckRow>
    <SectionLabel>Projects and kilns</SectionLabel>
    <EmptyText as="p" pad>
      docs (kiln) and crucible (project). The mockup cannot add or remove them.
    </EmptyText>
  </div>
);
