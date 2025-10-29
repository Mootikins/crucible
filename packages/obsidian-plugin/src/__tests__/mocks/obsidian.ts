/**
 * Mock Obsidian API for testing.
 */

import { vi } from "vitest";

export class TFile {
  path: string;
  name: string;
  extension: string;
  parent: any;
  stat: {
    ctime: number;
    mtime: number;
    size: number;
  };

  constructor(path: string) {
    this.path = path;
    this.name = path.split("/").pop() || "";
    this.extension = "md";
    this.parent = { path: path.split("/").slice(0, -1).join("/") };
    this.stat = {
      ctime: Date.now(),
      mtime: Date.now(),
      size: 100,
    };
  }
}

export class Kiln {
  getMarkdownFiles = vi.fn(() => []);
  read = vi.fn(async (file: TFile) => "# Test\n\nContent");
  modify = vi.fn(async (file: TFile, data: string) => {});
  getAbstractFileByPath = vi.fn((path: string) => new TFile(path));
}

export class MetadataCache {
  getFileCache = vi.fn((file: TFile) => ({
    frontmatter: {
      status: "active",
      tags: ["test"],
    },
    tags: [{ tag: "#test" }],
    links: [],
    embeds: [],
  }));
}

export class App {
  kiln = new Kiln();
  metadataCache = new MetadataCache();
}

export class Plugin {
  app: App;
  manifest: any;

  constructor(app: App, manifest: any) {
    this.app = app;
    this.manifest = manifest;
  }

  loadData = vi.fn(async () => ({}));
  saveData = vi.fn(async (data: any) => {});
  addSettingTab = vi.fn();
}

export class PluginSettingTab {
  app: App;
  plugin: any;
  containerEl: HTMLElement;

  constructor(app: App, plugin: any) {
    this.app = app;
    this.plugin = plugin;
    this.containerEl = document.createElement("div");
  }

  display() {}
  hide() {}
}

export class Setting {
  constructor(containerEl: HTMLElement) {}

  setName(name: string) {
    return this;
  }

  setDesc(desc: string) {
    return this;
  }

  addText(callback: (text: any) => any) {
    const text = {
      setPlaceholder: vi.fn(() => text),
      setValue: vi.fn(() => text),
      onChange: vi.fn(),
    };
    callback(text);
    return this;
  }

  addDropdown(callback: (dropdown: any) => any) {
    const dropdown = {
      addOption: vi.fn(() => dropdown),
      setValue: vi.fn(() => dropdown),
      onChange: vi.fn(),
    };
    callback(dropdown);
    return this;
  }
}

export class Notice {
  constructor(message: string) {}
}

export function getAllTags(cache: any) {
  return cache?.tags?.map((t: any) => t.tag) || [];
}

export function parseFrontMatterTags(frontmatter: any) {
  return frontmatter?.tags || [];
}
