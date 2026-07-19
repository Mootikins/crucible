import http from 'node:http';
import net from 'node:net';

/**
 * Fake Ollama server — makes the daemon's LLM turns deterministic.
 *
 * The daemon talks to Ollama through genai 0.6's NATIVE Ollama adapter (verified
 * against the crate source), NOT the OpenAI-compatible surface:
 *   - chat is POST {endpoint}/api/chat with a JSON body { model, messages, stream }
 *   - the response is newline-delimited JSON (NDJSON), one object per line, each
 *     { message: { content } }, terminated by a line with { done: true }
 *   - there is no SSE framing and no `data: [DONE]` sentinel
 * The daemon force-appends `/v1/` to a custom Ollama endpoint before handing it
 * to genai (provider/adapter_mapping.rs), so chat actually lands on
 * `POST {endpoint}/v1/api/chat`. Model listing (provider/model_listing.rs) hits
 * the RAW `GET {endpoint}/api/tags`. We therefore route by URL *suffix* so both
 * the `/v1`-prefixed and bare paths are served.
 *
 * Replies are scripted: the last user message is matched (substring) against an
 * ordered rule list; first match wins, else `fallback`.
 */

export interface OllamaRule {
  /** Substring matched (case-insensitively) against the last user message. */
  contains: string;
  /** The assistant reply streamed back for a match (text-only rules). */
  reply?: string;
  /**
   * When set, the FIRST time this rule matches (the request carries no tool
   * result yet) the fake streams a tool_calls round instead of `reply`. Once
   * the daemon's follow-up request comes back carrying that tool's result
   * (a message with role "tool"), the fake streams `replyAfterTool` instead.
   */
  toolCall?: { name: string; arguments: Record<string, unknown> };
  /** Text reply streamed once the tool result round-trips back. */
  replyAfterTool?: string;
}

export interface FakeOllamaOptions {
  rules: OllamaRule[];
  fallback: string;
  /** Model name advertised by GET /api/tags (default "hero-model"). */
  modelName?: string;
  /** Records each chat prompt seen, for test assertions/debugging. */
  onChat?: (lastUserMessage: string, reply: string) => void;
  /** Called for EVERY request (method, url) — useful for debugging routing. */
  onRequest?: (method: string, url: string) => void;
}

export interface FakeOllama {
  port: number;
  /** Chat prompts seen, in order. */
  prompts: string[];
  /** Tool-call rounds streamed, in order (name + arguments sent to the model). */
  toolRounds: Array<{ name: string; arguments: Record<string, unknown> }>;
  close(): Promise<void>;
}

interface OllamaMessage {
  role?: string;
  content?: string;
}

function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let data = '';
    req.on('data', (c) => (data += c));
    req.on('end', () => resolve(data));
    req.on('error', reject);
  });
}

function parseMessages(body: string): OllamaMessage[] {
  try {
    const parsed = JSON.parse(body) as { messages?: OllamaMessage[] };
    return parsed.messages ?? [];
  } catch {
    return [];
  }
}

function lastUserMessage(messages: OllamaMessage[]): string {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i]?.role === 'user' && messages[i]?.content) {
      return messages[i].content as string;
    }
  }
  // Fall back to the last message of any role.
  return messages[messages.length - 1]?.content ?? '';
}

/**
 * True once the daemon's follow-up request carries a tool result. genai's
 * Ollama adapter serializes a `ContentPart::ToolResponse` as a message with
 * role "tool" (adapter_shared.rs — `ContentPart::ToolResponse(tr) => ...
 * "Ollama native API expects role 'tool' for tool response"`), appended
 * after the assistant's tool_calls message. The trigger user message stays
 * unchanged across the round-trip, so this is the only reliable signal that
 * we're in the second half of a tool-calling turn.
 */
function hasToolResult(messages: OllamaMessage[]): boolean {
  return messages.some((m) => m.role === 'tool');
}

function pickRule(opts: FakeOllamaOptions, prompt: string): OllamaRule | undefined {
  const needle = prompt.toLowerCase();
  return opts.rules.find((rule) => needle.includes(rule.contains.toLowerCase()));
}

/** Stream an assistant reply as Ollama NDJSON: per-word chunks, then done. */
function streamChat(res: http.ServerResponse, model: string, reply: string): void {
  res.writeHead(200, { 'Content-Type': 'application/x-ndjson' });
  // Split into word chunks so streaming is observable but deterministic.
  const words = reply.split(/(\s+)/).filter((w) => w.length > 0);
  for (const w of words) {
    res.write(
      JSON.stringify({ model, message: { role: 'assistant', content: w }, done: false }) + '\n',
    );
  }
  res.write(
    JSON.stringify({
      model,
      message: { role: 'assistant', content: '' },
      done: true,
      done_reason: 'stop',
      prompt_eval_count: 1,
      eval_count: Math.max(1, words.length),
    }) + '\n',
  );
  res.end();
}

/**
 * Stream a tool-call round as Ollama NDJSON: one chunk carrying
 * `message.tool_calls` (genai's `OllamaStreamer` reads `/message/tool_calls`,
 * takes `/function/name` + `/function/arguments` — arguments is a JSON
 * object, not a string — and auto-generates a call id since the native
 * Ollama wire shape doesn't require one), then a `done:true` line with
 * `done_reason: "tool_calls"` (maps to genai's `StopReason::ToolCall`).
 */
function streamToolCall(
  res: http.ServerResponse,
  model: string,
  toolCall: { name: string; arguments: Record<string, unknown> },
): void {
  res.writeHead(200, { 'Content-Type': 'application/x-ndjson' });
  res.write(
    JSON.stringify({
      model,
      message: {
        role: 'assistant',
        tool_calls: [{ function: { name: toolCall.name, arguments: toolCall.arguments } }],
      },
      done: false,
    }) + '\n',
  );
  res.write(
    JSON.stringify({
      model,
      message: { role: 'assistant', content: '' },
      done: true,
      done_reason: 'tool_calls',
      prompt_eval_count: 1,
      eval_count: 1,
    }) + '\n',
  );
  res.end();
}

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const port = (srv.address() as net.AddressInfo).port;
      srv.close(() => resolve(port));
    });
  });
}

export async function startFakeOllama(opts: FakeOllamaOptions): Promise<FakeOllama> {
  const model = opts.modelName ?? 'hero-model';
  const prompts: string[] = [];
  const toolRounds: Array<{ name: string; arguments: Record<string, unknown> }> = [];

  const server = http.createServer((req, res) => {
    const url = req.url ?? '';
    const method = req.method ?? 'GET';
    opts.onRequest?.(method, url);

    // Model listing — GET .../api/tags (native Ollama shape).
    if (method === 'GET' && url.endsWith('/api/tags')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ models: [{ name: model }] }));
      return;
    }
    // OpenAI-compat model listing — GET .../models (context-length probing).
    if (method === 'GET' && url.endsWith('/models')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ id: model }], models: [{ name: model }] }));
      return;
    }
    // Model info — .../api/show (context-length probing). Minimal but valid.
    if (url.endsWith('/api/show')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ model_info: { 'general.context_length': 8192 } }));
      return;
    }
    // Chat — POST .../api/chat (bare or /v1-prefixed).
    if (method === 'POST' && url.endsWith('/api/chat')) {
      void readBody(req).then((body) => {
        const messages = parseMessages(body);
        const prompt = lastUserMessage(messages);
        prompts.push(prompt);
        const rule = pickRule(opts, prompt);

        if (rule?.toolCall) {
          if (hasToolResult(messages)) {
            const reply = rule.replyAfterTool ?? opts.fallback;
            opts.onChat?.(prompt, reply);
            streamChat(res, model, reply);
          } else {
            toolRounds.push(rule.toolCall);
            opts.onChat?.(prompt, `[tool_call:${rule.toolCall.name}]`);
            streamToolCall(res, model, rule.toolCall);
          }
          return;
        }

        const reply = rule?.reply ?? opts.fallback;
        opts.onChat?.(prompt, reply);
        streamChat(res, model, reply);
      });
      return;
    }

    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: `fake-ollama: unhandled ${method} ${url}` }));
  });

  const port = await freePort();
  await new Promise<void>((resolve) => server.listen(port, '127.0.0.1', resolve));

  return {
    port,
    prompts,
    toolRounds,
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections?.();
        server.close(() => resolve());
      }),
  };
}
