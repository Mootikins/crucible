/**
 * Lazy mermaid loader. Mermaid is ~1MB (pulls d3/dagre), so it is dynamically
 * imported the first time a ```mermaid fence actually renders — it never
 * touches the app-shell bundle. Mirrors the shiki lazy-singleton pattern.
 *
 * `render()` is async and returns an SVG STRING (mermaid renders into a
 * detached sandbox, no mounting needed), which the markdown pipeline sanitizes
 * with an SVG-capable DOMPurify pass and injects. `securityLevel: 'strict'`
 * makes mermaid sanitize its own output and drop click bindings; `htmlLabels:
 * false` keeps labels as SVG <text> (no <foreignObject>/HTML), which both
 * simplifies sanitization and survives the SVG profile intact.
 */
type MermaidModule = typeof import('mermaid');

let mermaidPromise: Promise<MermaidModule> | null = null;
let idCounter = 0;

async function getMermaid(): Promise<MermaidModule> {
  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then((mod) => {
      mod.default.initialize({
        startOnLoad: false,
        theme: 'dark',
        securityLevel: 'strict',
        fontFamily: "'IBM Plex Sans', system-ui, sans-serif",
        flowchart: { htmlLabels: false },
      });
      return mod;
    });
  }
  return mermaidPromise;
}

/**
 * Rewrite a rendered diagram's `viewBox` (and max-width) from the bounding box
 * of its actual content.
 *
 * Mermaid sizes dagre-based diagrams (flowchart, state, class, ER, git) from a
 * measurement pass that our own base stylesheet throws off: a three-node
 * flowchart whose content occupies 410×54 gets `viewBox="-62 -35 2070 2043"`,
 * so it renders as a speck in a huge empty box. Sequence and pie diagrams
 * compute their own dimensions and are unaffected. The LAYOUT is correct in
 * every case — only the frame is wrong — so refitting the box to the content
 * is a complete fix, and a no-op for diagrams that were already right.
 *
 * Measuring needs layout, so the markup is briefly attached to the document:
 * pass SANITIZED svg only.
 */
export function fitMermaidViewBox(svg: string): string {
  if (typeof document === 'undefined') return svg;
  const host = document.createElement('div');
  host.style.cssText =
    'position:absolute;left:-99999px;top:0;width:1200px;visibility:hidden;pointer-events:none';
  host.innerHTML = svg;
  const el = host.querySelector('svg');
  // The ROOT's bbox — the union of everything drawn. Measuring the first <g>
  // instead is wrong for diagram types whose root holds several sibling groups
  // (a sequence diagram would refit to just its first actor).
  if (!el || typeof el.getBBox !== 'function') return svg;

  document.body.appendChild(host);
  try {
    const box = el.getBBox();
    if (!Number.isFinite(box.width) || box.width <= 0 || box.height <= 0) return svg;
    const pad = 8;
    const w = box.width + pad * 2;
    const h = box.height + pad * 2;
    el.setAttribute('viewBox', `${box.x - pad} ${box.y - pad} ${w} ${h}`);
    el.setAttribute('width', '100%');
    el.removeAttribute('height');
    el.style.maxWidth = `${Math.ceil(w)}px`;
    return el.outerHTML;
  } catch {
    return svg;
  } finally {
    host.remove();
  }
}

/**
 * Render mermaid source to an SVG string. Returns null on a parse/render error
 * (caller renders the original source as a code block instead). Mermaid leaves
 * a temp measurement node in the DOM on failure — cleaned up here.
 */
export async function renderMermaid(code: string): Promise<string | null> {
  let mod: MermaidModule;
  try {
    mod = await getMermaid();
  } catch {
    return null;
  }
  const id = `crucible-mermaid-${++idCounter}`;
  try {
    const { svg } = await mod.default.render(id, code);
    return svg;
  } catch {
    if (typeof document !== 'undefined') {
      // mermaid names its throwaway measurement node `d<id>`.
      document.getElementById(`d${id}`)?.remove();
      document.getElementById(id)?.remove();
    }
    return null;
  }
}
