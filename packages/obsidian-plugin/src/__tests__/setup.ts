/**
 * Vitest setup file for Obsidian plugin tests.
 */

import { vi } from "vitest";

// Mock Node.js http module
vi.mock("http", () => ({
  default: {
    createServer: vi.fn(),
    request: vi.fn(),
  },
  createServer: vi.fn(),
  request: vi.fn(),
}));

// Mock Node.js https module
vi.mock("https", () => ({
  default: {
    request: vi.fn(),
  },
  request: vi.fn(),
}));
