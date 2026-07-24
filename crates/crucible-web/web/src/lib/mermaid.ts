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
