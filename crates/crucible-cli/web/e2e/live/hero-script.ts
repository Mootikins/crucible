import type { FakeOllamaOptions } from './fake-ollama';

/**
 * The scripted LLM turns for the hero flow. Shared by hero-setup (which feeds
 * the fake server) and hero.live.spec (which asserts the replies render). Match
 * substrings are chosen to be unambiguous across the three legs.
 */
export const HERO_REPLIES = {
  turn1: 'Seed summary: the seed note body establishes the project baseline.',
  turn2: 'Your from-tui note records the terminal write and the browser edit.',
  turn3: 'Acknowledged: the note now carries both the terminal and browser lines.',
} as const;

export const HERO_SCRIPT: FakeOllamaOptions = {
  rules: [
    { contains: 'summarize', reply: HERO_REPLIES.turn1 },
    { contains: 'from-tui', reply: HERO_REPLIES.turn2 },
    { contains: 'confirm', reply: HERO_REPLIES.turn3 },
  ],
  fallback: 'Acknowledged.',
  modelName: 'hero-model',
};
