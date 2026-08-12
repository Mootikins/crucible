/**
 * The frontend's single answer to "is this file a markdown note?".
 *
 * WHY A COPY EXISTS AT ALL: the canonical predicate is Rust —
 * `KilnFileKind::of` / `is_note_file` in `crates/crucible-core/src/kiln.rs`.
 * TypeScript cannot call it, and asking the daemon is not an option: this
 * question is answered per keystroke in the editor and per node per render in
 * the canvas, so a round trip would put a network call in a hot path to learn
 * something a filename already tells us. So the frontend keeps exactly one
 * duplicate — this module — and `crates/crucible-cli/tests/architecture_tests.rs`
 * (A2f) fails the build if a second one appears.
 *
 * **`kiln.rs` is the source of truth; the two must change together.** Widening
 * this list without widening `KilnFileKind::of` means the UI offers markdown
 * editing for a file the indexer treats as an asset; narrowing it means the
 * reverse. Either way the symptom is a file that half-exists.
 *
 * Deliberately importing nothing. `lib/markdown.ts` pulls markdown-it,
 * dompurify, shiki and mermaid; a drop target asking about a filename has no
 * business dragging a renderer into its bundle.
 */

/** Extensions that are notes, mirroring `md | markdown` in `KilnFileKind::of`. */
const MARKDOWN_EXTENSIONS = ['md', 'markdown'];

/**
 * The lowercased final extension of `path`, or null if it has none — matching
 * Rust's `Path::extension()`, which the canonical predicate is built on.
 *
 * The `dot <= 0` guard is that match: a leading dot is a stem, not an extension
 * separator, so a file literally named `.md` has no extension and is an asset
 * (`kiln.rs`'s `a_dotfile_without_an_extension_is_an_asset`). `.hidden.md` does
 * have one. Only the last extension counts, so `notes.md.bak` is a `bak`.
 */
function extensionOf(path: string): string | null {
  const base = path.slice(path.lastIndexOf('/') + 1);
  const dot = base.lastIndexOf('.');
  return dot <= 0 ? null : base.slice(dot + 1).toLowerCase();
}

/**
 * Whether `path` is a markdown note. Case-insensitive, because a vault synced
 * off a case-preserving filesystem really does contain `Daily.MD`.
 *
 * `.mdx` and `.mdc` are NOT markdown: MDX embeds JSX, which our parser would
 * render as prose. (`lib/file-icons.ts` still gives them a document icon —
 * that map is cosmetic and answers "what does this look like", not "can we
 * edit it as a note". A recognizable icon on a file you open as source is
 * correct; it is not precedent for widening this predicate.)
 */
export function isMarkdownPath(path: string): boolean {
  const ext = extensionOf(path);
  return ext !== null && MARKDOWN_EXTENSIONS.includes(ext);
}

/**
 * A note's wikilink stem: the filename with its markdown extension removed.
 *
 * Paired with `isMarkdownPath` on purpose — anything this recognizes as a note
 * must also be strippable, or a `.markdown` file becomes `[[Reading
 * List.markdown]]`. This is only for names `isMarkdownPath` accepted; the
 * link-*resolution* stem rules (which strip `.md` from wikilink targets) are a
 * separate question and live at their own call sites.
 */
export function noteStem(name: string): string {
  return isMarkdownPath(name) ? name.slice(0, name.lastIndexOf('.')) : name;
}
