import { createSignal, createEffect, onMount, onCleanup } from 'solid-js';

export interface OverflowDropdownOptions {
  containerRef: () => HTMLDivElement | undefined;
  deps: () => unknown;
}

export interface OverflowDropdownResult {
  isOverflowing: () => boolean;
  showDropdown: () => boolean;
  setShowDropdown: (v: boolean) => void;
  toggleDropdown: () => void;
}

export function useOverflowDropdown(opts: OverflowDropdownOptions): OverflowDropdownResult {
  const [isOverflowing, setIsOverflowing] = createSignal(false);
  const [showDropdown, setShowDropdown] = createSignal(false);

  const toggleDropdown = () => {
    setShowDropdown(!showDropdown());
  };

  onMount(() => {
    const containerEl = opts.containerRef();
    if (!containerEl) return;

    const checkOverflow = () => {
      const el = opts.containerRef();
      if (el) {
        setIsOverflowing(el.scrollWidth > el.clientWidth);
      }
    };

    const observer = new ResizeObserver(checkOverflow);
    observer.observe(containerEl);

    createEffect(() => {
      opts.deps();
      checkOverflow();
    });

    onCleanup(() => observer.disconnect());
  });

  createEffect(() => {
    if (!showDropdown()) return;

    const handleClickOutside = () => {
      setShowDropdown(false);
    };

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setShowDropdown(false);
    };

    // Intentional setTimeout race workaround: prevents the click-that-opens
    // from immediately closing the dropdown
    setTimeout(() => {
      document.addEventListener('click', handleClickOutside);
      document.addEventListener('keydown', handleEscape);
    }, 0);

    onCleanup(() => {
      document.removeEventListener('click', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    });
  });

  return {
    isOverflowing,
    showDropdown,
    setShowDropdown,
    toggleDropdown,
  };
}
