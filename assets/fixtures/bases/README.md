# Bases reference fixtures

The compatibility target is **Obsidian 1.14.2**, English locale,
`America/Chicago` timezone. `expressions.json` is the input corpus;
`obsidian-1.14.2-expressions.json` was captured from that running app's
registered `base:query` CLI handler using `format=json`, named view `Case`,
and a disposable vault. The capture script records the version and timezone.
Outputs preserve CLI display strings and nulls, including evaluation errors.

The seed note is `notes/a.md` with `status: todo`, `tags: [work/deep]`, and
`[[notes/a]]`. Every case uses a separate base selecting that exact file and
one formula column named `result`. No property-type overrides are installed.
`Tasks.base` and `First.md` are additional portable query inputs.

To regenerate, open a disposable vault named `crucible-bases-conformance*`
in Obsidian 1.14.2 with `--remote-debugging-port=19222`, enable Bases and CLI,
then run `node scripts/capture-bases-conformance.mjs` from the repository.
Set `OBSIDIAN_CDP` to use another CDP endpoint. The script refuses other vaults,
except the isolated `/tmp/crucible-obsidian-oracle/vault` used for this capture.
It overwrites fixture notes only inside that disposable vault. Review the
resulting diff; do not silently update the pinned target.

Offline comparison runs in `bases_matches_captured_obsidian_expressions`.
Live regeneration is explicitly ignored because it requires the desktop app
and its CDP endpoint. Relative-time cases assert a type or freshly-created
relative value rather than recording a wall-clock timestamp.

The query corpus covers filter composition, sorting/limits, missing values,
list groups, backlinks and default views. Every query is captured in JSON,
CSV, TSV, Markdown and paths formats. The CLI reference supplies no host for
`this`; Crucible's native saved views separately follow Obsidian's documented
main-pane context. Obsidian's CLI scans Markdown; unfiltered native views also
include attachments, as required by the Bases syntax.

Creation captures retain raw file bytes. Comparisons require identical body
bytes, paths and frontmatter values; YAML indentation and null spelling are
serializer choices. Explicit content takes precedence over template content.
Summary captures evaluate the running native table view, covering every
built-in summary for all rows, each group, and an empty set.

`obsidian-1.14.2-moves.json` records the native kanban card-drop method’s
resulting file bytes for scalar, empty, list, missing-list and boolean groups.
The tests compare frontmatter values and exact body bytes, allowing YAML
serializer formatting differences as with creation.

The expression comparison pins the captured `TZ` with `EnvVarGuard` in its
isolated nextest process. Epoch, offset and daylight-saving cases therefore
run against the recorded zone even when the build machine uses another zone.
