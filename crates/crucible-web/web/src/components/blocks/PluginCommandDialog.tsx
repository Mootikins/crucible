import { Component, For, Show, createMemo, createSignal, onCleanup, createEffect } from 'solid-js';
import { X } from '@/lib/icons';
import type { PluginCommand } from '@/lib/types';
import { useRunPluginCommand } from '@/lib/query/plugins';
import {
  commandArgs,
  commandFields,
  type CommandField,
  type CommandFormValues,
} from '@/lib/command-form';

/**
 * An argument dialog for a plugin command, **generated** from what the plugin
 * declared.
 *
 * Nothing here names a command, a plugin or a parameter. `commandFields` reads
 * the JSON Schema the daemon ships as `parameters` and answers with a control
 * per parameter; this draws those controls and sends what
 * `commandArgs` makes of them. A command added to a plugin tomorrow gets this
 * dialog with no change here — which is the property the plugin plan asks for,
 * and the reason this is not five hand-written forms.
 *
 * ## The permission answer this does not have
 *
 * A `write` command invoked from a button is exactly the contract's item 7 — a
 * prompter for a person-invoked primitive — and it is unsequenced, so there is
 * no prompt here. What there is instead: the effect is shown as a **claim the
 * plugin makes**, and the Run button says which kind it is running. That is
 * labelling, not a gate, and it must not be mistaken for one:
 *
 * - the effect is declared by the plugin and verified by nothing;
 * - a block is same-origin script that can call `POST /api/plugins/command`
 *   directly, so this dialog is not on the only path to a write.
 *
 * When item 7 lands, the gate belongs on the daemon side of that route, and
 * this dialog becomes one of its callers rather than its enforcement.
 */
interface PluginCommandDialogProps {
  command: PluginCommand | null;
  onClose: () => void;
}

/** The label and tone for a declared effect. Nothing verifies the claim. */
function effectLabel(effect: PluginCommand['effect']): { text: string; tone: string } {
  return effect === 'read'
    ? { text: 'declares: read', tone: 'text-muted border-hairline' }
    : { text: 'declares: write', tone: 'text-attention border-attention/50' };
}

const FieldControl: Component<{
  field: CommandField;
  value: string | boolean | undefined;
  onInput: (value: string | boolean) => void;
}> = (props) => {
  const id = () => `plugin-command-field-${props.field.name}`;
  return (
    <Show
      when={props.field.control !== 'checkbox'}
      fallback={
        <input
          id={id()}
          type="checkbox"
          class="h-4 w-4"
          checked={props.value === true}
          onChange={(e) => props.onInput(e.currentTarget.checked)}
        />
      }
    >
      <Show
        when={props.field.control === 'lines' || props.field.control === 'json'}
        fallback={
          <input
            id={id()}
            type={props.field.control === 'number' ? 'number' : 'text'}
            class="w-full rounded border border-hairline bg-surface px-2 py-1 text-sm"
            value={typeof props.value === 'string' ? props.value : ''}
            onInput={(e) => props.onInput(e.currentTarget.value)}
          />
        }
      >
        <textarea
          id={id()}
          rows={3}
          class="w-full rounded border border-hairline bg-surface px-2 py-1 font-mono text-xs"
          placeholder={props.field.control === 'lines' ? 'one entry per line' : 'JSON'}
          value={typeof props.value === 'string' ? props.value : ''}
          onInput={(e) => props.onInput(e.currentTarget.value)}
        />
      </Show>
    </Show>
  );
};

export const PluginCommandDialog: Component<PluginCommandDialogProps> = (props) => {
  const runCommand = useRunPluginCommand();
  const [values, setValues] = createSignal<CommandFormValues>({});
  const [errors, setErrors] = createSignal<Record<string, string>>({});
  const [running, setRunning] = createSignal(false);
  const [result, setResult] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);

  const fields = createMemo(() => commandFields(props.command?.parameters));

  // A different command is a different form. Without this the second command
  // opened inherits the first one's answers, which for two commands that both
  // take `folder` is a wrong value that looks deliberate.
  createEffect(() => {
    props.command?.name;
    setValues({});
    setErrors({});
    setResult(null);
    setFailure(null);
  });

  createEffect(() => {
    if (!props.command) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      }
    };
    document.addEventListener('keydown', onKeyDown, true);
    onCleanup(() => document.removeEventListener('keydown', onKeyDown, true));
  });

  const run = async () => {
    const command = props.command;
    if (!command) return;
    const { args, errors: found } = commandArgs(fields(), values());
    setErrors(found);
    if (Object.keys(found).length > 0) return;

    setRunning(true);
    setResult(null);
    setFailure(null);
    try {
      const answer = await runCommand.mutateAsync({ command: command.name, args });
      setResult(JSON.stringify(answer, null, 2));
    } catch (error) {
      setFailure(error instanceof Error ? error.message : String(error));
    } finally {
      setRunning(false);
    }
  };

  return (
    <Show when={props.command}>
      {(command) => (
        <>
          <div class="fixed inset-0 z-[110] bg-black/65" onClick={() => props.onClose()} />
          <div
            class="fixed left-1/2 top-16 z-[120] flex max-h-[80vh] w-[min(560px,92vw)]
                   -translate-x-1/2 flex-col overflow-hidden rounded-xl border border-hairline
                   bg-surface-overlay shadow-2xl"
            role="dialog"
            aria-label={`Run ${command().name}`}
          >
            <div class="flex items-start gap-2 border-b border-hairline px-4 py-3">
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="font-mono text-sm font-medium">{command().name}</span>
                  <span
                    class={`rounded border px-1.5 py-0.5 text-floor uppercase tracking-wide ${
                      effectLabel(command().effect).tone
                    }`}
                    title="The plugin declares this about itself. Nothing verifies it."
                  >
                    {effectLabel(command().effect).text}
                  </span>
                </div>
                <Show when={command().description}>
                  <div class="mt-1 text-xs text-muted">{command().description}</div>
                </Show>
              </div>
              <button
                type="button"
                class="text-muted hover:text-shell-ink"
                aria-label="Close"
                onClick={() => props.onClose()}
              >
                <X size={16} />
              </button>
            </div>

            <div class="flex-1 overflow-y-auto px-4 py-3">
              <Show
                when={fields().length > 0}
                fallback={
                  <div class="text-sm text-muted italic">
                    This command declares no parameters.
                    <Show when={command().hint}>
                      {' '}
                      Its hint reads <code>{command().hint}</code>, which is free text a dialog
                      cannot be generated from — the plugin has to declare <code>params</code>.
                    </Show>
                  </div>
                }
              >
                <div class="flex flex-col gap-3">
                  <For each={fields()}>
                    {(field) => (
                      <div class="flex flex-col gap-1">
                        <label
                          for={`plugin-command-field-${field.name}`}
                          class="flex items-baseline gap-2 text-xs"
                        >
                          <span class="font-medium">{field.name}</span>
                          <span class="font-mono text-floor text-muted">{field.typeLabel}</span>
                          <Show when={field.required}>
                            <span class="text-floor uppercase text-attention">required</span>
                          </Show>
                        </label>
                        <Show when={field.description}>
                          <div class="text-floor text-muted">{field.description}</div>
                        </Show>
                        <FieldControl
                          field={field}
                          value={values()[field.name]}
                          onInput={(value) =>
                            setValues((previous) => ({ ...previous, [field.name]: value }))
                          }
                        />
                        <Show when={errors()[field.name]}>
                          <div class="text-floor text-error">{errors()[field.name]}</div>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>
              </Show>

              <Show when={failure()}>
                <pre class="mt-3 whitespace-pre-wrap rounded border border-error/40 p-2 text-xs text-error">
                  {failure()}
                </pre>
              </Show>
              <Show when={result()}>
                <pre class="mt-3 max-h-64 overflow-auto rounded border border-hairline bg-surface p-2 font-mono text-xs">
                  {result()}
                </pre>
              </Show>
            </div>

            <div class="flex items-center justify-end gap-2 border-t border-hairline px-4 py-3">
              <button
                type="button"
                class="rounded border border-hairline px-3 py-1 text-sm hover:border-primary"
                onClick={() => props.onClose()}
              >
                Close
              </button>
              <button
                type="button"
                class="rounded border border-primary px-3 py-1 text-sm text-primary
                       hover:bg-primary/10 disabled:opacity-50"
                disabled={running()}
                onClick={() => void run()}
              >
                {running() ? 'Running…' : 'Run'}
              </button>
            </div>
          </div>
        </>
      )}
    </Show>
  );
};
