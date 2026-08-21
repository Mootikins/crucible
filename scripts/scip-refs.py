#!/usr/bin/env python3
"""Count SCIP definitions, reads and writes per symbol; find never-read fields.

WHAT IT DOES
    Decodes a SCIP index (protobuf) and reports, for any symbol:
      def    - occurrences whose `symbol_roles` has the Definition bit (0x1)
      read   - occurrences that read the symbol:  `expr.field`
      init   - occurrences that only name the symbol while building or
               matching a value:  `Type { field: value }`
      other  - occurrences it cannot classify, almost all of them macro
               arguments that rust-analyzer mapped back to the call site

    The def/reference split comes from the role bit. The read/init split does
    NOT: rust-analyzer emits `symbol_roles = 0` for every non-definition
    occurrence, so ReadAccess and WriteAccess are never set. This tool
    therefore takes each occurrence's source range and inspects the line:
    a `.` before the name means a read, a `:` after it means an initializer.
    For the shorthand `Type { field }`, where punctuation says nothing, it
    uses the `local N` symbol sharing the same range: a pattern defines a
    local and so reads the field, an expression references one and so writes
    it.

    That distinction is the whole point. A field that is deserialized, given a
    default and then never consulted still has references - every
    `Type { field: None }` in every constructor is one. It has zero reads.

    `strings(1)` on the index can do neither split: it sees symbol text but
    no roles and no ranges.

INDEX FILE
    Default: /tmp/scip/index.scip  (override with --index)
    Source root defaults to the repository root (override with --root).

TO REGENERATE THE INDEX
    cd /home/moot/crucible
    rust-analyzer scip . --output /tmp/scip/index.scip
    (About 160 s for this workspace. Produces about 69 MB.)
    The index is a snapshot; it does not follow later edits. This tool checks
    the text at each range against the symbol name and reports `stale=N` when
    they disagree, which means: reindex before you trust the numbers.

WHAT IT CANNOT TELL YOU
    - Whether a read is live. Dead code that still compiles counts as a read.
      Code behind a `#[cfg]` that was off during indexing is absent entirely.
    - Anything about non-Rust call sites. Lua, Fennel, TypeScript, JSON-RPC
      payloads and serde wire names never appear as SCIP references. A field
      with zero reads can still be load-bearing across a wire boundary. Check
      `#[serde]` attributes and the web and Lua layers before you delete one.
    - Macro-generated reads. rust-analyzer maps some macro output back to
      source, not all. Treat zero as "look closer", not as proof.
    - Which impl a `dyn` call reaches. The occurrence lands on the trait
      symbol, not on each impl.
    - Read against write inside a macro. When rust-analyzer maps an expansion
      back to a macro argument, no local shares the range and the occurrence
      lands in `other`. Check those by hand.
    - Struct field against enum variant, when the index carries no
      SymbolInformation.kind. The fallback matches the symbol shape
      `Type#name.`, which both share.

USAGE
    scripts/scip-refs.py --symbol 'FastEmbedConfig#'   # counts + files
    scripts/scip-refs.py --unread-fields               # never-read fields
    scripts/scip-refs.py --unread-fields --crate crucible-core --json
    scripts/scip-refs.py --self-test                   # prove the split works
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from collections import defaultdict
from pathlib import Path

DEFAULT_INDEX = "/tmp/scip/index.scip"

# scip.proto field numbers.
IDX_DOCUMENTS = 2
DOC_RELATIVE_PATH = 1
DOC_OCCURRENCES = 2
DOC_SYMBOLS = 3
DOC_TEXT = 5
OCC_RANGE = 1
OCC_SYMBOL = 2
OCC_SYMBOL_ROLES = 3
SYMINFO_SYMBOL = 1
SYMINFO_KIND = 5

# scip.proto SymbolRole bit flags.
ROLE_DEFINITION = 0x1
ROLE_NAMES = [
    (0x1, "definition"),
    (0x2, "import"),
    (0x4, "write"),
    (0x8, "read"),
    (0x10, "generated"),
    (0x20, "test"),
    (0x40, "forward_definition"),
]

# scip.proto SymbolInformation.Kind values that name a struct-like member.
MEMBER_KINDS = {12, 15, 41}  # EnumMember, Field, Property

# `crate path/Type#field.` but not `Type#method().`, not `Type#`, not `local 0`.
FIELD_SHAPE = re.compile(r"#[A-Za-z_][A-Za-z0-9_]*\.$")
SYMBOL_TAIL = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)\.$")

CLASSES = ("read", "init", "other", "stale")


# --------------------------------------------------------------------------
# Minimal protobuf wire-format reader.
#
# SCIP uses only wire type 0 (varint) and 2 (length-delimited) in the messages
# this tool reads. The reader still skips 1 and 5 so a schema change cannot
# desynchronise the stream.
# --------------------------------------------------------------------------


def read_varint(buf: bytes, pos: int) -> tuple[int, int]:
    """Return (value, new_pos)."""
    result = 0
    shift = 0
    while True:
        byte = buf[pos]
        pos += 1
        result |= (byte & 0x7F) << shift
        if byte < 0x80:
            return result, pos
        shift += 7


def iter_fields(buf: bytes, pos: int, end: int):
    """Yield (field_number, wire_type, value).

    For wire type 2 the value is a (start, stop) span into `buf`.
    For wire type 0 it is the integer. Other wire types yield None.
    """
    while pos < end:
        key, pos = read_varint(buf, pos)
        field = key >> 3
        wire = key & 7
        if wire == 2:
            length, pos = read_varint(buf, pos)
            stop = pos + length
            yield field, wire, (pos, stop)
            pos = stop
        elif wire == 0:
            value, pos = read_varint(buf, pos)
            yield field, wire, value
        elif wire == 5:
            pos += 4
            yield field, wire, None
        elif wire == 1:
            pos += 8
            yield field, wire, None
        else:
            raise ValueError(f"unsupported wire type {wire} at offset {pos}")


def read_packed_varints(buf: bytes, start: int, stop: int) -> list[int]:
    out = []
    pos = start
    while pos < stop:
        value, pos = read_varint(buf, pos)
        out.append(value)
    return out


def parse_occurrence(buf: bytes, start: int, stop: int):
    """Return (symbol_bytes, symbol_roles, range_list)."""
    symbol = b""
    roles = 0
    rng: list[int] = []
    for field, wire, value in iter_fields(buf, start, stop):
        if field == OCC_SYMBOL and wire == 2:
            symbol = bytes(buf[value[0] : value[1]])
        elif field == OCC_SYMBOL_ROLES and wire == 0:
            roles = value
        elif field == OCC_RANGE and wire == 2:
            rng = read_packed_varints(buf, value[0], value[1])
    return symbol, roles, rng


def parse_symbol_information(buf: bytes, start: int, stop: int) -> tuple[bytes, int]:
    """Return (symbol_bytes, kind)."""
    symbol = b""
    kind = 0
    for field, wire, value in iter_fields(buf, start, stop):
        if field == SYMINFO_SYMBOL and wire == 2:
            symbol = bytes(buf[value[0] : value[1]])
        elif field == SYMINFO_KIND and wire == 0:
            kind = value
    return symbol, kind


def iter_documents(buf: bytes):
    """Yield (relative_path, occurrence_spans, symbol_spans, text_span)."""
    for field, wire, value in iter_fields(buf, 0, len(buf)):
        if field != IDX_DOCUMENTS or wire != 2:
            continue
        doc_start, doc_stop = value
        path = ""
        occurrences: list[tuple[int, int]] = []
        symbols: list[tuple[int, int]] = []
        text_span = None
        for dfield, dwire, dvalue in iter_fields(buf, doc_start, doc_stop):
            if dwire != 2:
                continue
            if dfield == DOC_RELATIVE_PATH:
                path = buf[dvalue[0] : dvalue[1]].decode("utf-8", "replace")
            elif dfield == DOC_OCCURRENCES:
                occurrences.append(dvalue)
            elif dfield == DOC_SYMBOLS:
                symbols.append(dvalue)
            elif dfield == DOC_TEXT:
                text_span = dvalue
        yield path, occurrences, symbols, text_span


# --------------------------------------------------------------------------
# Read against write classification.
#
# rust-analyzer never sets the ReadAccess or WriteAccess role bits, so the
# only evidence left is the source line under the occurrence range.
# --------------------------------------------------------------------------


def normalise_range(rng: list[int]) -> tuple[int, int, int, int] | None:
    """SCIP packs a range as [line, col, endCol] or [line, col, endLine, endCol]."""
    if len(rng) == 3:
        return rng[0], rng[1], rng[0], rng[2]
    if len(rng) == 4:
        return rng[0], rng[1], rng[2], rng[3]
    return None



# `match`, `if let` and `while let` introduce pattern position.
_PATTERN_INTRO = re.compile(rb"\b(match|if\s+let|while\s+let)\b")


def in_pattern_position(lines: list[bytes], line_no: int, start_col: int) -> bool:
    """True when this occurrence sits inside a pattern rather than an expression.

    `Type { field: x }` is a WRITE in an expression and a READ in a pattern, and
    the punctuation is identical. Getting it backwards reported
    `PermissionDecision::Ask { rule_matched: false }` — the fail-closed guard
    that stops an explicit `ask` rule being auto-approved — as never read. That
    is the one mistake this tool must not make.

    The rule: find the `{` that opens this struct pattern or literal, then look
    back from it. A `=>` first means an arm body, so an expression. A `match`,
    `if let` or `while let` first means a pattern.

    Textual and deliberately conservative: ambiguity answers False, which keeps
    the cautious `init` and at worst hides a dead field.
    """
    text = b"\n".join(lines[max(0, line_no - 60) : line_no] + [lines[line_no][:start_col]])
    brace = text.rfind(b"{")
    if brace == -1:
        return False
    head = text[:brace]
    if not _PATTERN_INTRO.search(head):
        return False
    # Inside a match, every arm before this one has already contributed a `=>`,
    # so "is there an arrow above me" is useless. The arm BOUNDARY is what
    # matters: an arm ends at `,` and the next arm's pattern starts there. So a
    # comma closer to us than the last arrow means we are in a fresh pattern; an
    # arrow closer means we are in the previous arm's body.
    arrow = head.rfind(b"=>")
    comma = head.rfind(b",")
    brace_open = head.rfind(b"{")
    return max(comma, brace_open) > arrow


def classify(lines: list[bytes], rng: list[int], name: bytes, local_roles: int | None) -> str:
    """Return one of CLASSES for a non-definition occurrence.

    `lines` holds raw UTF-8 bytes because SCIP columns count UTF-8 code units,
    not code points. A line with an emoji in it shifts every later column.

    `local_roles` is the OR of the roles of any `local N` symbol that shares
    this exact range, or None when no local sits there. That resolves the
    shorthand `Type { field }`, which the punctuation alone cannot:
      - a pattern, `let Type { field } = x`, DEFINES a local, and extracts
        the field value to do it, so it reads the field;
      - an expression, `Type { field }`, REFERENCES an existing local and
        feeds it in, so it writes the field.
    """
    span = normalise_range(rng)
    if span is None:
        return "other"
    line_no, start_col, end_line, end_col = span
    if line_no >= len(lines):
        return "stale"
    line = lines[line_no]
    if end_line != line_no:
        end_col = len(line)
    if line[start_col:end_col] != name:
        return "stale"

    before = line[:start_col].rstrip()
    # A field access may wrap:  self\n    .config\n    .cache_dir
    look = line_no
    while not before and look > 0:
        look -= 1
        before = lines[look].rstrip()
    if before.endswith(b".") and not before.endswith(b".."):
        return "read"

    after = line[end_col:].lstrip()
    if after.startswith(b":") and not after.startswith(b"::"):
        # `field:` means a write in an expression and a READ in a pattern —
        # `PermissionDecision::Ask { rule_matched: false }` in a match arm
        # inspects the field, it does not set it. Getting this backwards
        # reported the fail-closed permission guard as never read, which is the
        # one mistake this tool must not make.
        return "init" if not in_pattern_position(lines, line_no, start_col) else "read"

    if local_roles is not None:
        return "read" if local_roles & ROLE_DEFINITION else "init"

    # No local at this range. Usually a macro argument that rust-analyzer
    # mapped back to the call site, where the expansion decides read or write.
    return "other"


class SourceCache:
    """Raw UTF-8 lines for the files under the index, read at most once each."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self._lines: dict[str, list[bytes] | None] = {}
        self.missing: set[str] = set()

    def lines(self, relative_path: str) -> list[bytes] | None:
        cached = self._lines.get(relative_path, False)
        if cached is not False:
            return cached  # type: ignore[return-value]
        path = Path(relative_path)
        if not path.is_absolute():
            path = self.root / path
        try:
            raw = path.read_bytes()
        except OSError:
            self.missing.add(relative_path)
            self._lines[relative_path] = None
            return None
        value = raw.split(b"\n")
        self._lines[relative_path] = value
        return value


# --------------------------------------------------------------------------
# Aggregation
# --------------------------------------------------------------------------


class Totals:
    __slots__ = ("defs", "counts", "def_files", "files", "role_hist")

    def __init__(self) -> None:
        self.defs = 0
        self.counts: dict[str, int] = dict.fromkeys(CLASSES, 0)
        self.def_files: dict[str, int] = defaultdict(int)
        self.files: dict[str, dict[str, int]] = defaultdict(lambda: dict.fromkeys(CLASSES, 0))
        self.role_hist: dict[int, int] = defaultdict(int)

    @property
    def refs(self) -> int:
        return sum(self.counts.values())

    @property
    def reads(self) -> int:
        return self.counts["read"]


def scan(buf: bytes, sources: SourceCache, want: bytes | None, field_shape_only: bool):
    """One pass over the index.

    `want` keeps only symbols containing that substring; None keeps all.
    `field_shape_only` additionally keeps only `Type#name.` symbols.
    Returns (symbol -> Totals, symbol -> kind, n_documents, n_occurrences).
    """
    counts: dict[bytes, Totals] = {}
    kinds: dict[bytes, int] = {}
    keep: dict[bytes, bytes | None] = {}  # symbol -> trailing name, or None to skip
    n_docs = 0
    n_occ = 0

    for path, occ_spans, sym_spans, text_span in iter_documents(buf):
        n_docs += 1
        lines: list[bytes] | None = None
        if text_span is not None:
            lines = bytes(buf[text_span[0] : text_span[1]]).split(b"\n")
        loaded = lines is not None

        # A `local N` occurrence that shares a range with a field occurrence
        # tells shorthand pattern from shorthand expression, so collect those
        # first. The two can appear in either order in the stream.
        local_roles: dict[tuple[int, ...], int] = {}
        pending: list[tuple[Totals, bytes, list[int]]] = []

        for start, stop in occ_spans:
            symbol, roles, rng = parse_occurrence(buf, start, stop)
            n_occ += 1
            if not symbol:
                continue
            if symbol.startswith(b"local "):
                key = tuple(rng)
                local_roles[key] = local_roles.get(key, 0) | roles
                continue
            name = keep.get(symbol, False)
            if name is False:
                text = symbol.decode("utf-8", "replace")
                ok = (want is None or want in symbol) and (
                    not field_shape_only or FIELD_SHAPE.search(text) is not None
                )
                match = SYMBOL_TAIL.search(text) if ok else None
                name = match.group(1).encode() if match else None
                keep[symbol] = name
            if name is None:
                continue

            entry = counts.get(symbol)
            if entry is None:
                entry = counts[symbol] = Totals()
            entry.role_hist[roles] += 1
            if roles & ROLE_DEFINITION:
                entry.defs += 1
                entry.def_files[path] += 1
                continue
            pending.append((entry, name, rng))

        if pending and not loaded:
            lines = sources.lines(path)
            loaded = True
        for entry, name, rng in pending:
            if lines is None:
                kind = "stale"
            else:
                kind = classify(lines, rng, name, local_roles.get(tuple(rng)))
            entry.counts[kind] += 1
            entry.files[path][kind] += 1

        for start, stop in sym_spans:
            symbol, kind_value = parse_symbol_information(buf, start, stop)
            if symbol and kind_value and symbol in keep and keep[symbol] is not None:
                kinds[symbol] = kind_value

    return counts, kinds, n_docs, n_occ


def role_summary(hist: dict[int, int]) -> str:
    parts = []
    for roles, n in sorted(hist.items()):
        if roles == 0:
            names = "no-role"
        else:
            names = "|".join(name for bit, name in ROLE_NAMES if roles & bit) or hex(roles)
        parts.append(f"{names}={n}")
    return ", ".join(parts)


# --------------------------------------------------------------------------
# Commands
# --------------------------------------------------------------------------


def cmd_symbol(buf: bytes, sources: SourceCache, needle: str, as_json: bool, max_files: int) -> int:
    counts, kinds, _, _ = scan(buf, sources, needle.encode(), field_shape_only=False)
    if not counts:
        print(f"no symbol matches {needle!r}", file=sys.stderr)
        return 1

    ordered = sorted(counts.items(), key=lambda kv: kv[0])
    if as_json:
        print(
            json.dumps(
                [
                    {
                        "symbol": sym.decode("utf-8", "replace"),
                        "kind": kinds.get(sym),
                        "definitions": t.defs,
                        "references": t.refs,
                        "reads": t.counts["read"],
                        "inits": t.counts["init"],
                        "other": t.counts["other"],
                        "stale": t.counts["stale"],
                        "definition_files": dict(t.def_files),
                        "files": {p: c for p, c in t.files.items()},
                    }
                    for sym, t in ordered
                ],
                indent=2,
            )
        )
        return 0

    print(f"{'def':>4} {'read':>5} {'init':>5} {'other':>6} {'stale':>6}  symbol")
    for sym, t in ordered:
        c = t.counts
        verdict = "INERT (never read)" if c["read"] == 0 else "read"
        print(
            f"{t.defs:>4} {c['read']:>5} {c['init']:>5} {c['other']:>6} {c['stale']:>6}"
            f"  {sym.decode('utf-8', 'replace')}   -> {verdict}"
        )
        print(f"       roles: {role_summary(t.role_hist)}")
        for path, n in sorted(t.def_files.items()):
            print(f"       def         {n:4d}  {path}")
        rows = sorted(t.files.items(), key=lambda kv: (-sum(kv[1].values()), kv[0]))
        for path, per in rows[:max_files]:
            label = " ".join(f"{k}={v}" for k, v in per.items() if v)
            print(f"       {label:<24}  {path}")
        if len(rows) > max_files:
            print(f"       ... {len(rows) - max_files} more file(s)")
    return 0



# Fields that are never read ON PURPOSE. Each class was confirmed by reading the
# code before it was filtered; `--all` still shows them.
_NOISE = re.compile(
    r"""
      \#_[a-z]                 # `_guard`, `_temp_dir`: an RAII handle held only
                               # to keep something alive. The leading underscore
                               # is the author saying so.
    | \ (Test|Mock)[A-Za-z]*\#  # test scaffolding, not shipped behaviour
    | \ markdown-it\           # vendored; not ours to triage
    """,
    re.X,
)


def _is_deliberate_noise(name: str) -> bool:
    return bool(_NOISE.search(name))


def cmd_unread_fields(
    buf: bytes, sources: SourceCache, crate: str | None, as_json: bool, show_all: bool
) -> int:
    counts, kinds, n_docs, n_occ = scan(buf, sources, None, field_shape_only=True)
    have_kinds = bool(kinds)

    rows = []
    stale_total = 0
    for symbol, t in counts.items():
        stale_total += t.counts["stale"]
        if not t.defs or (not show_all and t.counts["read"]):
            continue
        name = symbol.decode("utf-8", "replace")
        kind = kinds.get(symbol)
        if have_kinds and kind is not None and kind not in MEMBER_KINDS:
            continue
        if crate and f" {crate} " not in name:
            continue
        if not show_all and _is_deliberate_noise(name):
            continue
        rows.append((name, t))

    rows.sort(key=lambda r: r[0])

    if as_json:
        print(
            json.dumps(
                {
                    "documents": n_docs,
                    "occurrences": n_occ,
                    "stale_occurrences": stale_total,
                    "count": len(rows),
                    "fields": [
                        {
                            "symbol": n,
                            "definitions": t.defs,
                            "references": t.refs,
                            "reads": t.counts["read"],
                            "inits": t.counts["init"],
                            "other": t.counts["other"],
                            "files": sorted(t.def_files),
                        }
                        for n, t in rows
                    ],
                },
                indent=2,
            )
        )
        return 0

    print(f"{'def':>4} {'read':>5} {'init':>5} {'other':>6}  symbol")
    for name, t in rows:
        c = t.counts
        print(f"{t.defs:>4} {c['read']:>5} {c['init']:>5} {c['other']:>6}  {name}")

    print(
        f"\n{len(rows)} field-shaped symbol(s) never read, "
        f"over {n_docs} documents and {n_occ} occurrences.",
        file=sys.stderr,
    )
    if stale_total:
        print(
            f"WARNING: {stale_total} occurrence(s) did not match the source text. "
            "The index is older than the tree. Reindex before you trust this.",
            file=sys.stderr,
        )
    if sources.missing:
        print(f"WARNING: {len(sources.missing)} file(s) not found under --root.", file=sys.stderr)
    print(
        "Never read does not prove dead. Read the module docstring for what "
        "this cannot see: serde, Lua, the web layer, macros.",
        file=sys.stderr,
    )
    return 0


# Five symbols whose read/init split is known from independent inspection of
# the source. `num_threads` and `https` have references but no reads: every
# occurrence is a `field: value` initializer. If a change to `classify` counts
# an initializer as a read, these two flip and the test fails.
SELF_TEST = [
    ("session/types/session/Session#kilns.", True),
    ("config/enrichment/FastEmbedConfig#batch_size.", True),
    ("config/enrichment/FastEmbedConfig#cache_dir.", True),
    ("config/enrichment/FastEmbedConfig#num_threads.", False),
    ("config/config/server/ServerConfig#https.", False),
    # A field matched against a LITERAL inside a match arm — `Ask { rule_matched:
    # false }`. The punctuation is identical to a struct-literal write, so the
    # first classifier called this a write and reported the field never read.
    # It is the fail-closed guard that stops an explicit `ask` rule being
    # auto-approved, and calling a live security guard dead is the one failure
    # this tool must never have. Pinned so a change to `in_pattern_position`
    # cannot quietly bring it back.
    ("permissions/types/PermissionDecision#Ask#rule_matched.", True),
    # A field bound to a DIFFERENT name in a pattern — `output: cmd_output`.
    ("observe/events/LogEvent#BashCompleted#output.", True),
]


def cmd_self_test(buf: bytes, sources: SourceCache) -> int:
    failures = 0
    print(f"{'ok':<5}{'def':>4}{'read':>6}{'init':>6}{'other':>7}{'stale':>7}  symbol")
    for needle, want_read in SELF_TEST:
        counts, _, _, _ = scan(buf, sources, needle.encode(), field_shape_only=False)
        exact = [t for sym, t in counts.items() if sym.decode("utf-8", "replace").endswith(needle)]
        if len(exact) != 1:
            print(f"FAIL  {needle}: matched {len(exact)} symbols, wanted 1")
            failures += 1
            continue
        t = exact[0]
        c = t.counts
        good = t.defs > 0 and (c["read"] > 0) == want_read
        failures += not good
        print(
            f"{'ok' if good else 'FAIL':<5}{t.defs:>4}{c['read']:>6}{c['init']:>6}"
            f"{c['other']:>7}{c['stale']:>7}  {needle}"
            f"   -> {'read' if c['read'] else 'INERT (never read)'}"
        )
    print(
        "\nexpected: kilns/batch_size/cache_dir read, num_threads/https inert",
        file=sys.stderr,
    )
    return 1 if failures else 0


def repo_root() -> Path:
    here = Path(__file__).resolve().parent
    return here.parent if (here.parent / ".git").exists() else Path.cwd()


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Count SCIP definitions, reads and writes per symbol.",
        epilog="See the module docstring for what this tool cannot tell you.",
    )
    ap.add_argument("--index", default=DEFAULT_INDEX, help=f"SCIP index (default {DEFAULT_INDEX})")
    ap.add_argument("--root", help="source root for the index paths (default: repository root)")
    ap.add_argument("--symbol", help="report on symbols containing this substring")
    ap.add_argument(
        "--unread-fields", action="store_true", help="list every Type#field. that is never read"
    )
    ap.add_argument("--crate", help="restrict --unread-fields to one crate name")
    ap.add_argument(
        "--all-fields", action="store_true", help="with --unread-fields, list read fields too"
    )
    ap.add_argument("--json", action="store_true", help="emit JSON")
    ap.add_argument("--max-files", type=int, default=20, help="files to list per symbol")
    ap.add_argument(
        "--self-test", action="store_true", help="check the read/init split on five known symbols"
    )
    ap.add_argument("--time", action="store_true", help="print elapsed seconds to stderr")
    args = ap.parse_args(argv)

    if not args.symbol and not args.unread_fields and not args.self_test:
        ap.error("give --symbol, --unread-fields or --self-test")

    index_path = Path(args.index)
    if not index_path.is_file():
        print(
            f"index not found: {index_path}\nregenerate it with:\n"
            f"  rust-analyzer scip . --output {index_path}",
            file=sys.stderr,
        )
        return 2

    started = time.monotonic()
    sources = SourceCache(Path(args.root).resolve() if args.root else repo_root())
    buf = index_path.read_bytes()
    if args.self_test:
        rc = cmd_self_test(buf, sources)
    elif args.symbol:
        rc = cmd_symbol(buf, sources, args.symbol, args.json, args.max_files)
    else:
        rc = cmd_unread_fields(buf, sources, args.crate, args.json, args.all_fields)
    if args.time:
        print(f"[{time.monotonic() - started:.1f}s]", file=sys.stderr)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
