import { describe, it, expect } from 'vitest';
import {
  PanelLeft,
  PanelLeftClose,
  PanelRight,
  PanelRightClose,
  PanelBottom,
  PanelBottomClose,
  X,
  Maximize2,
  Minimize2,
  GripVertical,
  GripHorizontal,
  Settings,
  Zap,
  LayoutDashboard,
  ChevronDown,
  RefreshCw,
  Plus,
  FileText,
  FileCode,
  File,
  Folder,
  FolderOpen,
  FileJson,
  Palette,
  Globe,
  Moon,
  Cog,
  FolderTree,
  Search,
  GitBranch,
  ListTree,
  Bug,
  Terminal,
  AlertTriangle,
  FileOutput,
  AppWindow,
} from '../icons';

describe('Icon Exports', () => {
  it('should export all required panel icons', () => {
    expect(PanelLeft).toBeDefined();
    expect(PanelLeftClose).toBeDefined();
    expect(PanelRight).toBeDefined();
    expect(PanelRightClose).toBeDefined();
    expect(PanelBottom).toBeDefined();
    expect(PanelBottomClose).toBeDefined();
  });

  it('should export all required window control icons', () => {
    expect(X).toBeDefined();
    expect(Maximize2).toBeDefined();
    expect(Minimize2).toBeDefined();
  });

  it('should export all required drag handle icons', () => {
    expect(GripVertical).toBeDefined();
    expect(GripHorizontal).toBeDefined();
  });

  it('should export all required UI icons', () => {
    expect(Settings).toBeDefined();
    expect(Zap).toBeDefined();
    expect(LayoutDashboard).toBeDefined();
    expect(ChevronDown).toBeDefined();
    expect(RefreshCw).toBeDefined();
    expect(Plus).toBeDefined();
  });

  it('should export all required file type icons', () => {
    expect(FileText).toBeDefined();
    expect(FileCode).toBeDefined();
    expect(File).toBeDefined();
    expect(Folder).toBeDefined();
    expect(FolderOpen).toBeDefined();
    expect(FileJson).toBeDefined();
    expect(Palette).toBeDefined();
    expect(Globe).toBeDefined();
    expect(Moon).toBeDefined();
    expect(Cog).toBeDefined();
  });

  it('should export all required edge panel icons', () => {
    expect(FolderTree).toBeDefined();
    expect(Search).toBeDefined();
    expect(GitBranch).toBeDefined();
    expect(ListTree).toBeDefined();
    expect(Bug).toBeDefined();
    expect(Terminal).toBeDefined();
    expect(AlertTriangle).toBeDefined();
    expect(FileOutput).toBeDefined();
  });

  it('should export empty state icon', () => {
    expect(AppWindow).toBeDefined();
  });

  it('should export icons as SolidJS components', () => {
    // Icons should be callable components
    expect(typeof PanelLeft).toBe('function');
    expect(typeof Settings).toBe('function');
    expect(typeof Search).toBe('function');
  });
});
