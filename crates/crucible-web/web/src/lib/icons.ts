// Every Lucide icon the Crucible web frontend uses, re-exported from ONE file.
//
// Each icon comes from its OWN module path, never from the `lucide-solid`
// barrel. The barrel re-exports 1943 icons, and a dev server has to serve
// every one of them as a separate request before the page can paint: it was
// 1948 of the 3427 module requests a cold load made, to use the 100 icons
// below. Deep paths cut that to 100.
//
// The production build tree-shook the barrel already, so this changes the
// bundle not at all — it changes how long the page takes to appear while you
// work on it.
//
// `optimizeDeps.include` cannot solve this instead: vite-plugin-solid keeps
// Solid component libraries out of the esbuild pre-bundle, because the JSX
// transform they need does not survive it.
//
// To add an icon, add a line here in the same form. The path is the icon's
// name in kebab-case, which is what `lucide-solid/icons/` is keyed by.

export { default as Activity } from 'lucide-solid/icons/activity';
export { default as ArrowRight } from 'lucide-solid/icons/arrow-right';
export { default as ArrowLeft } from 'lucide-solid/icons/arrow-left';
export { default as ArrowLeftRight } from 'lucide-solid/icons/arrow-left-right';
export { default as ArrowRightLeft } from 'lucide-solid/icons/arrow-right-left';
export { default as ExternalLink } from 'lucide-solid/icons/external-link';
export { default as Shield } from 'lucide-solid/icons/shield';
export { default as ShieldAlert } from 'lucide-solid/icons/shield-alert';
export { default as Archive } from 'lucide-solid/icons/archive';
export { default as ChartNetwork } from 'lucide-solid/icons/chart-network';
export { default as Crosshair } from 'lucide-solid/icons/crosshair';
export { default as PanelLeft } from 'lucide-solid/icons/panel-left';
export { default as PanelLeftClose } from 'lucide-solid/icons/panel-left-close';
export { default as PanelRight } from 'lucide-solid/icons/panel-right';
export { default as PanelRightClose } from 'lucide-solid/icons/panel-right-close';
export { default as X } from 'lucide-solid/icons/x';
export { default as Maximize2 } from 'lucide-solid/icons/maximize-2';
export { default as Minimize2 } from 'lucide-solid/icons/minimize-2';
export { default as GripVertical } from 'lucide-solid/icons/grip-vertical';
export { default as Settings } from 'lucide-solid/icons/settings';
export { default as Zap } from 'lucide-solid/icons/zap';
export { default as LayoutDashboard } from 'lucide-solid/icons/layout-dashboard';
export { default as Check } from 'lucide-solid/icons/check';
export { default as ChevronDown } from 'lucide-solid/icons/chevron-down';
export { default as ChevronLeft } from 'lucide-solid/icons/chevron-left';
export { default as ChevronRight } from 'lucide-solid/icons/chevron-right';
export { default as ChevronsDownUp } from 'lucide-solid/icons/chevrons-down-up';
export { default as ArrowUp } from 'lucide-solid/icons/arrow-up';
export { default as FlaskConical } from 'lucide-solid/icons/flask-conical';
export { default as FolderGit2 } from 'lucide-solid/icons/folder-git-2';
export { default as Bot } from 'lucide-solid/icons/bot';
export { default as ArrowUpDown } from 'lucide-solid/icons/arrow-up-down';
export { default as Copy } from 'lucide-solid/icons/copy';
export { default as RefreshCw } from 'lucide-solid/icons/refresh-cw';
export { default as Plus } from 'lucide-solid/icons/plus';
export { default as Package } from 'lucide-solid/icons/package';
export { default as Cpu } from 'lucide-solid/icons/cpu';
export { default as Plug } from 'lucide-solid/icons/plug';
export { default as Target } from 'lucide-solid/icons/target';
export { default as FileText } from 'lucide-solid/icons/file-text';
export { default as FileDiff } from 'lucide-solid/icons/file-diff';
export { default as Layers } from 'lucide-solid/icons/layers';
export { default as Puzzle } from 'lucide-solid/icons/puzzle';
export { default as FileCode } from 'lucide-solid/icons/file-code';
export { default as File } from 'lucide-solid/icons/file';
export { default as FileImage } from 'lucide-solid/icons/file-image';
export { default as FileArchive } from 'lucide-solid/icons/file-archive';
export { default as FileLock } from 'lucide-solid/icons/file-lock';
export { default as Film } from 'lucide-solid/icons/film';
export { default as Music } from 'lucide-solid/icons/music';
export { default as FileSpreadsheet } from 'lucide-solid/icons/file-spreadsheet';
export { default as FileType } from 'lucide-solid/icons/file-type';
export { default as FileKey } from 'lucide-solid/icons/file-key';
export { default as Binary } from 'lucide-solid/icons/binary';
export { default as Coffee } from 'lucide-solid/icons/coffee';
export { default as Gem } from 'lucide-solid/icons/gem';
export { default as Parentheses } from 'lucide-solid/icons/parentheses';
export { default as Hexagon } from 'lucide-solid/icons/hexagon';
export { default as BookMarked } from 'lucide-solid/icons/book-marked';
export { default as Database } from 'lucide-solid/icons/database';
export { default as Braces } from 'lucide-solid/icons/braces';
export { default as Palette } from 'lucide-solid/icons/palette';
export { default as Cloud } from 'lucide-solid/icons/cloud';
export { default as Globe } from 'lucide-solid/icons/globe';
export { default as Monitor } from 'lucide-solid/icons/monitor';
export { default as Network } from 'lucide-solid/icons/network';
export { default as Moon } from 'lucide-solid/icons/moon';
export { default as Sun } from 'lucide-solid/icons/sun';
export { default as Cog } from 'lucide-solid/icons/cog';
export { default as FolderTree } from 'lucide-solid/icons/folder-tree';
export { default as Search } from 'lucide-solid/icons/search';
export { default as GitBranch } from 'lucide-solid/icons/git-branch';
export { default as Terminal } from 'lucide-solid/icons/terminal';
export { default as AlertTriangle } from 'lucide-solid/icons/alert-triangle';
export { default as FileOutput } from 'lucide-solid/icons/file-output';
export { default as Bell } from 'lucide-solid/icons/bell';
export { default as MessageCircle } from 'lucide-solid/icons/message-circle';
export { default as ClipboardList } from 'lucide-solid/icons/clipboard-list';
export { default as CircleQuestionMark } from 'lucide-solid/icons/circle-question-mark';
export { default as Map } from 'lucide-solid/icons/map';
export { default as Trash2 } from 'lucide-solid/icons/trash-2';
export { default as Inbox } from 'lucide-solid/icons/inbox';
export { default as Link2 } from 'lucide-solid/icons/link-2';
export { default as Eye } from 'lucide-solid/icons/eye';
export { default as Pencil } from 'lucide-solid/icons/pencil';
export { default as Code } from 'lucide-solid/icons/code';
export { default as Pin } from 'lucide-solid/icons/pin';
export { default as PanelTop } from 'lucide-solid/icons/panel-top';
export { default as Brain } from 'lucide-solid/icons/brain';
export { default as Sliders } from 'lucide-solid/icons/sliders';
export { default as Key } from 'lucide-solid/icons/key';
export { default as Mic } from 'lucide-solid/icons/mic';
export { default as Sparkles } from 'lucide-solid/icons/sparkles';
export { default as StickyNote } from 'lucide-solid/icons/sticky-note';
export { default as Wrench } from 'lucide-solid/icons/wrench';
export { default as Undo2 } from 'lucide-solid/icons/undo-2';
export { default as Redo2 } from 'lucide-solid/icons/redo-2';
export { default as ZoomIn } from 'lucide-solid/icons/zoom-in';
export { default as ZoomOut } from 'lucide-solid/icons/zoom-out';
export { default as Frame } from 'lucide-solid/icons/frame';
export { default as MoreHorizontal } from 'lucide-solid/icons/more-horizontal';
