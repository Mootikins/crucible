import { Component, For, Show, createSignal, createEffect, onCleanup } from 'solid-js';
import { executeShell, type ShellEvent } from '@/lib/api';

// =============================================================================
// Types
// =============================================================================

interface OutputLine {
  type: 'stdout' | 'stderr' | 'prompt' | 'exit';
  text: string;
}

interface CommandBlock {
  command: string;
  lines: OutputLine[];
  exitCode?: number;
  running: boolean;
}

// =============================================================================
// ShellPanel Component
// =============================================================================

export const ShellPanel: Component = () => {
  const [blocks, setBlocks] = createSignal<CommandBlock[]>([]);
  const [input, setInput] = createSignal('');
  const [history, setHistory] = createSignal<string[]>([]);
  const [historyIndex, setHistoryIndex] = createSignal(-1);
  const [isRunning, setIsRunning] = createSignal(false);

  let outputRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;
  let activeController: AbortController | null = null;

  // Auto-scroll to bottom when blocks change
  createEffect(() => {
    // Access blocks to track changes
    blocks();
    if (outputRef) {
      // Use queueMicrotask to scroll after DOM update
      queueMicrotask(() => {
        outputRef!.scrollTop = outputRef!.scrollHeight;
      });
    }
  });

  // Cleanup on unmount
  onCleanup(() => {
    activeController?.abort();
  });

  function handleSubmit() {
    const cmd = input().trim();
    if (!cmd || isRunning()) return;

    // Add to history (skip duplicates of last entry)
    setHistory((prev) => {
      if (prev.length > 0 && prev[prev.length - 1] === cmd) return prev;
      return [...prev, cmd];
    });
    setHistoryIndex(-1);
    setInput('');

    // Create new command block
    const newBlock: CommandBlock = {
      command: cmd,
      lines: [],
      running: true,
    };
    setBlocks((prev) => [...prev, newBlock]);
    setIsRunning(true);

    // Execute command and stream output
    activeController = executeShell(
      cmd,
      (event: ShellEvent) => {
        setBlocks((prev) => {
          const updated = [...prev];
          const current = { ...updated[updated.length - 1] };
          current.lines = [...current.lines];

          switch (event.type) {
            case 'stdout':
              if (event.data !== undefined) {
                current.lines.push({ type: 'stdout', text: event.data });
              }
              break;
            case 'stderr':
              if (event.data !== undefined) {
                current.lines.push({ type: 'stderr', text: event.data });
              }
              break;
            case 'exit':
              current.exitCode = event.code;
              current.running = false;
              current.lines.push({
                type: 'exit',
                text: `Process exited with code ${event.code}`,
              });
              break;
            case 'error':
              current.running = false;
              current.lines.push({
                type: 'stderr',
                text: `Error: ${event.message ?? 'Unknown error'}`,
              });
              break;
          }

          updated[updated.length - 1] = current;
          return updated;
        });
      },
      () => {
        // onDone callback
        setIsRunning(false);
        activeController = null;
        // Mark block as not running if not already
        setBlocks((prev) => {
          const updated = [...prev];
          if (updated.length > 0) {
            const last = { ...updated[updated.length - 1] };
            last.running = false;
            updated[updated.length - 1] = last;
          }
          return updated;
        });
      },
    );
  }

  function handleKeyDown(e: KeyboardEvent) {
    const hist = history();

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (hist.length === 0) return;

      const idx = historyIndex();
      if (idx === -1) {
        // Start from end of history
        setHistoryIndex(hist.length - 1);
        setInput(hist[hist.length - 1]);
      } else if (idx > 0) {
        setHistoryIndex(idx - 1);
        setInput(hist[idx - 1]);
      }
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (hist.length === 0) return;

      const idx = historyIndex();
      if (idx === -1) return;

      if (idx < hist.length - 1) {
        setHistoryIndex(idx + 1);
        setInput(hist[idx + 1]);
      } else {
        // Past end of history — clear
        setHistoryIndex(-1);
        setInput('');
      }
    } else if (e.key === 'Enter') {
      e.preventDefault();
      handleSubmit();
    } else if (e.key === 'c' && e.ctrlKey) {
      // Ctrl+C cancels running command
      if (isRunning() && activeController) {
        activeController.abort();
        activeController = null;
        setIsRunning(false);
        setBlocks((prev) => {
          const updated = [...prev];
          if (updated.length > 0) {
            const last = { ...updated[updated.length - 1] };
            last.running = false;
            last.lines = [...last.lines, { type: 'stderr', text: '^C' }];
            updated[updated.length - 1] = last;
          }
          return updated;
        });
      }
    } else if (e.key === 'l' && e.ctrlKey) {
      // Ctrl+L clears output
      e.preventDefault();
      setBlocks([]);
    }
  }

  function lineClass(type: OutputLine['type']): string {
    switch (type) {
      case 'stdout':
        return 'text-zinc-100';
      case 'stderr':
        return 'text-red-400';
      case 'exit':
        return ''; // handled per-block
      case 'prompt':
        return 'text-emerald-400';
      default:
        return 'text-zinc-100';
    }
  }

  function exitCodeClass(code: number | undefined): string {
    if (code === undefined) return 'text-zinc-500';
    return code === 0 ? 'text-emerald-400' : 'text-red-400';
  }

  return (
    <div class="h-full flex flex-col bg-zinc-950 font-mono text-sm">
      {/* Output area */}
      <div
        ref={outputRef}
        class="flex-1 min-h-0 overflow-y-auto px-3 py-2 select-text"
      >
        <Show
          when={blocks().length > 0}
          fallback={
            <div class="h-full flex items-center justify-center text-zinc-600 text-xs">
              <div class="text-center">
                <div class="text-lg mb-1">💻</div>
                <div>Shell</div>
                <div class="mt-1 text-zinc-700">
                  Type a command below • Ctrl+C to cancel • Ctrl+L to clear
                </div>
              </div>
            </div>
          }
        >
          <For each={blocks()}>
            {(block) => (
              <div class="mb-3">
                {/* Command prompt line */}
                <div class="flex items-baseline gap-1">
                  <span class="text-emerald-400 select-none">$</span>
                  <span class="text-zinc-100 font-semibold">{block.command}</span>
                  <Show when={block.running}>
                    <span class="text-amber-400 text-xs animate-pulse ml-2">
                      running…
                    </span>
                  </Show>
                </div>

                {/* Output lines */}
                <For each={block.lines}>
                  {(line) => (
                    <div
                      class={`whitespace-pre-wrap break-all pl-4 leading-snug ${
                        line.type === 'exit'
                          ? exitCodeClass(block.exitCode)
                          : lineClass(line.type)
                      }`}
                    >
                      {line.text}
                    </div>
                  )}
                </For>

                {/* Exit code summary (when not running and exit code present) */}
                <Show when={!block.running && block.exitCode !== undefined}>
                  <div
                    class={`text-xs mt-0.5 pl-4 ${exitCodeClass(block.exitCode)}`}
                  >
                    ⏎ exit {block.exitCode}
                  </div>
                </Show>
              </div>
            )}
          </For>
        </Show>
      </div>

      {/* Input area */}
      <div class="border-t border-zinc-800 px-3 py-2 flex items-center gap-2 bg-zinc-900/50">
        <span class="text-emerald-400 select-none font-semibold">$</span>
        <input
          ref={inputRef}
          type="text"
          value={input()}
          onInput={(e) => {
            setInput(e.currentTarget.value);
            setHistoryIndex(-1);
          }}
          onKeyDown={handleKeyDown}
          placeholder={isRunning() ? 'Command running…' : 'Enter command…'}
          disabled={isRunning()}
          class="flex-1 bg-transparent text-zinc-100 placeholder-zinc-600 outline-none caret-emerald-400 disabled:opacity-50"
          spellcheck={false}
          autocomplete="off"
        />
        <Show when={isRunning()}>
          <button
            onClick={() => {
              activeController?.abort();
              activeController = null;
              setIsRunning(false);
            }}
            class="text-xs text-red-400 hover:text-red-300 px-1.5 py-0.5 border border-red-800 rounded hover:border-red-600 transition-colors"
            title="Cancel (Ctrl+C)"
          >
            Cancel
          </button>
        </Show>
      </div>
    </div>
  );
};
