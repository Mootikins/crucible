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

/**
 * Content the agent writes via a real `write_file` tool call, and the reply
 * streamed once the tool result round-trips back to the fake. Two separate
 * files/prompts (TUI vs web) so both legs can run in the same fake-ollama
 * process without racing each other's tool-call state.
 */
export const AGENT_FS_WRITE = {
  tui: {
    trigger: 'write-via-tui-agent',
    path: 'notes/agent-tui.md',
    content: 'written by the agent from the TUI leg\n',
    replyAfterTool: 'Done — I wrote notes/agent-tui.md for you.',
  },
  web: {
    trigger: 'write-via-web-agent',
    path: 'notes/agent-web.md',
    content: 'written by the agent from the web leg\n',
    replyAfterTool: 'Done — I wrote notes/agent-web.md for you.',
  },
} as const;

export const HERO_SCRIPT: FakeOllamaOptions = {
  rules: [
    { contains: 'summarize', reply: HERO_REPLIES.turn1 },
    { contains: 'from-tui', reply: HERO_REPLIES.turn2 },
    { contains: 'confirm', reply: HERO_REPLIES.turn3 },
    {
      contains: AGENT_FS_WRITE.tui.trigger,
      toolCall: {
        name: 'write_file',
        arguments: { path: AGENT_FS_WRITE.tui.path, content: AGENT_FS_WRITE.tui.content },
      },
      replyAfterTool: AGENT_FS_WRITE.tui.replyAfterTool,
    },
    {
      contains: AGENT_FS_WRITE.web.trigger,
      toolCall: {
        name: 'write_file',
        arguments: { path: AGENT_FS_WRITE.web.path, content: AGENT_FS_WRITE.web.content },
      },
      replyAfterTool: AGENT_FS_WRITE.web.replyAfterTool,
    },
  ],
  fallback: 'Acknowledged.',
  modelName: 'hero-model',
};
