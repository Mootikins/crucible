import type {
  DragSource,
  DropTarget,
  EdgePanel as EdgePanelType,
  EdgePanelPosition,
  FloatingWindow,
  FocusedRegion,
  LayoutNode,
  TabGroup,
} from '@/types/windowTypes';

export type PaneDropPosition = 'left' | 'right' | 'top' | 'bottom';

export interface WindowState {
  layout: LayoutNode;
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, EdgePanelType>;
  floatingWindows: FloatingWindow[];
  activePaneId: string | null;
  focusedRegion: FocusedRegion;
  dragState: {
    isDragging: boolean;
    source: DragSource | null;
    target: DropTarget | null;
  } | null;
  flyoutState: {
    isOpen: boolean;
    position: EdgePanelPosition;
    tabId: string | null;
  } | null;
  nextZIndex: number;
}
