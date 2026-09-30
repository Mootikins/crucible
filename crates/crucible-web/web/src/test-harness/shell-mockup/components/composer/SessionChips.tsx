/**
 * The session facts under the capsule: the permission mode (an icon, at the
 * far left), the model, the working folder and the kiln. The real app fills
 * them from `useSessionModes`, `useSessionModels` and `useSessionScopeChips`.
 */
import type { Component } from 'solid-js';
import { ChevronDown, CircleHelp, FlaskConical, FolderGit2, ListChecks, Zap } from '@/lib/icons';
import { KnobButton } from './KnobButton';

export type ModeId = 'Ask' | 'Auto' | 'Plan';
const MODE_ICON = { Ask: CircleHelp, Auto: Zap, Plan: ListChecks } as const;

export interface SessionChipsProps {
  mode: ModeId;
  model: string;
  /** The project name, or "Session folder" for a session with no project. */
  workspace: string;
  /** The attached kiln, or "No kiln". */
  kiln: string;
}

export const SessionChips: Component<SessionChipsProps> = (props) => {
  const ModeIcon = () => {
    const Icon = MODE_ICON[props.mode];
    return <Icon class="mk-i" />;
  };
  return (
    <div class="mk-chips">
      <KnobButton title={`Mode: ${props.mode}`}>
        <ModeIcon />
      </KnobButton>
      <KnobButton title="Model">
        {props.model}
        <ChevronDown class="mk-i" />
      </KnobButton>
      <KnobButton title="Working folder">
        <FolderGit2 class="mk-i" />
        {props.workspace}
      </KnobButton>
      <KnobButton title="Kiln">
        <FlaskConical class="mk-i" />
        {props.kiln}
        <ChevronDown class="mk-i" />
      </KnobButton>
    </div>
  );
};
