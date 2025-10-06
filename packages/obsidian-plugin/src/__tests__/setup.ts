/**
 * Vitest setup file for Obsidian plugin tests.
 */

import { vi } from "vitest";

// Mock Node.js http module
vi.mock("http", () => ({
  createServer: vi.fn(),
}));
