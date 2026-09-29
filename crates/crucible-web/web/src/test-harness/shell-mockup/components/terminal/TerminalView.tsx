/** The terminal in the right rail's corner. The mockup shows a fixed session; it runs nothing. */
import type { Component } from 'solid-js';

export const TerminalView: Component = () => (
  <div class="mk-scroll mk-term">
    <div>
      <span class="mk-prompt">~/crucible</span> <span class="mk-cmd">cru process</span>
    </div>
    <div class="mk-quiet">Processing kiln via daemon...</div>
    <div class="mk-quiet">Discovered: 131 indexable files · Skipped (unchanged): 131</div>
    <div>
      <span class="mk-prompt">~/crucible</span> <span class="mk-caret" />
    </div>
  </div>
);
