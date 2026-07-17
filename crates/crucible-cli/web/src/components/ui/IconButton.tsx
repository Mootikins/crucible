import { Component, JSX, splitProps } from 'solid-js';

/**
 * The one chrome icon-button. Every square icon button in the shell
 * (titlebars, headers, tab bars, status bar) renders through this so
 * box size, icon size, radius, and hover treatment stay uniform —
 * before it existed the chrome had 8 divergent box/icon/hover combos.
 *
 * Sizes: sm = 24px box (dense strips: titlebars, tab bars),
 *        md = 28px box (headers, toolbars). Icons are always w-4.
 * The ribbon keeps its own fixed Obsidian-style slots (w-10) — it is
 * a layout element, not a chrome button.
 */
export const IconButton: Component<
  JSX.ButtonHTMLAttributes<HTMLButtonElement> & { size?: 'sm' | 'md' }
> = (props) => {
  const [local, rest] = splitProps(props, ['size', 'class', 'children']);
  return (
    <button
      type="button"
      class={`${local.size === 'sm' ? 'w-6 h-6' : 'w-7 h-7'} flex items-center justify-center flex-shrink-0 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors ${local.class ?? ''}`}
      {...rest}
    >
      {local.children}
    </button>
  );
};
