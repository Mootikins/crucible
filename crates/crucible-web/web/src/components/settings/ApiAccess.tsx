// src/components/settings/ApiAccess.tsx
import { Component, Show, createSignal } from 'solid-js';
import { Key } from '@/lib/icons';

import { SectionHeader } from './primitives';
import { useLogin } from '@/lib/query/auth';

/**
 * API access for non-localhost clients. The pasted key is exchanged for an
 * HttpOnly session cookie (POST /api/auth/login) — the key never travels in
 * a URL and is never stored where page JS can read it. It lives in
 * `~/.config/crucible/api_key` on the machine running `cru web`.
 */
export const ApiAccessSection: Component = () => {
  const loginMutation = useLogin();
  const [draft, setDraft] = createSignal('');
  const [rejected, setRejected] = createSignal(false);

  const save = async () => {
    const key = draft().trim();
    if (!key) return;
    if (await loginMutation.mutateAsync(key)) {
      window.location.reload();
    } else {
      setRejected(true);
    }
  };

  return (
    <>
      <SectionHeader title="API Access" icon={Key} />
      <tr class="border-b border-hairline">
        <td class="py-3 text-shell-body text-sm">
          Sign in with API key
          <div class="text-xs text-muted-dark">
            Required for non-localhost access; sets a session cookie.
          </div>
          <Show when={rejected()}>
            <div class="text-xs text-error" data-testid="settings-api-token-rejected">
              The server rejected that key.
            </div>
          </Show>
        </td>
        <td class="py-3 text-right">
          <input
            type="password"
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
            placeholder="Paste API key"
            class="bg-control border border-hairline rounded px-2 py-1 text-sm text-shell-ink focus:border-primary focus-ring w-56"
            data-testid="settings-api-token-input"
          />
          <button
            type="button"
            onClick={() => void save()}
            disabled={!draft().trim()}
            class="ml-2 rounded bg-primary px-2 py-1 text-sm text-on-primary hover:bg-primary-hover disabled:opacity-50"
            data-testid="settings-api-token-save"
          >
            Sign in
          </button>
        </td>
      </tr>
    </>
  );
};
