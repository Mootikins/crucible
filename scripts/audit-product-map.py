#!/usr/bin/env python3
"""Report every falsifiable citation in Meta/Product.md that no longer holds.

The map's proof lines cite code four ways:

    `path/to/file.rs`            a file
    `path/to/file.rs:123`        a file and line, or `:12-34` a range
    `path/to/dir/`               a directory
    `path`::test_name            a test that demonstrates the claim

All four rot: files move, modules split, line numbers drift, tests get renamed.
Prose still needs a human — this says WHICH prose to look at, which is the part
that was costing an hour of greps.

Everything is indexed once and checked in memory; spawning a search per claim
takes minutes and this takes seconds.
"""
import re
import subprocess
import sys
from pathlib import Path

# The repo root, resolved from git rather than hardcoded — an absolute
# developer path in a committed script is a script that only runs on one
# machine.
ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
)
DOC = ROOT / "docs/Meta/Product.md"

TRACKED = [
    t
    for t in subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split("\n")
    if t
]

CITE = re.compile(r"`([A-Za-z0-9_./-]+\.(?:rs|ts|tsx|lua|fnl|md|toml))(?::(\d+)(?:-(\d+))?)?`")
DIRCITE = re.compile(r"`([A-Za-z0-9_./-]+/)`")
TESTREF = re.compile(r"::([a-z_][a-z0-9_]{6,})")
IDENT = re.compile(r"`([a-z_][a-z0-9_]{4,})(?:\(\))?`")

AMBIGUOUS = object()

# Paths naming a file in the user's kiln or home, not in this repo.
CONCEPTUAL = re.compile(r"^\.crucible/|^~|^AGENTS\.md$|^\.rules$|/\.crucible/")

# Citations that are patterns or generated artifacts, not tracked paths.
NOT_A_PATH = {"_test.fnl", "cru.lua", "plugins.toml", "cru-docs.json"}


def build_corpus():
    """Every searchable byte of the source tree, once."""
    out = subprocess.run(
        ["git", "grep", "-h", "", "--", "crates/", "runtime/", "scripts/"],
        cwd=ROOT, capture_output=True, text=True,
    )
    return out.stdout


def resolve(path: str):
    direct = ROOT / path
    if direct.is_file():
        return direct
    matches = [t for t in TRACKED if t == path or t.endswith("/" + path)]
    if len(matches) == 1:
        return ROOT / matches[0]
    return AMBIGUOUS if matches else None


def entry_titles(lines):
    titles, current = {}, "(preamble)"
    for i, line in enumerate(lines):
        m = re.match(r"^- \[[x\- ]\] \*\*(.+?)\*\*", line)
        if m:
            current = m.group(1)
        titles[i] = current
    return titles


def report(heading, rows):
    print(f"\n=== {heading} ({len(rows)}) ===")
    for line, title, detail in rows:
        print(f"  L{line}  {title}\n        {detail}")


def main():
    lines = DOC.read_text().split("\n")
    titles = entry_titles(lines)
    corpus = build_corpus()

    missing, drifted, dirs, tests, idents = [], [], [], [], []
    seen_ident, seen_test = set(), set()

    for i, line in enumerate(lines):
        n = i + 1

        for path, start, end in CITE.findall(line):
            if path in NOT_A_PATH or CONCEPTUAL.search(path):
                continue
            if path.endswith(".md") and not path.startswith(("docs/", "crates/")):
                continue
            resolved = resolve(path)
            if resolved is None:
                missing.append((n, titles[i], path))
            elif resolved is not AMBIGUOUS and start:
                total = len(resolved.read_text(errors="replace").split("\n"))
                last = int(end or start)
                if last > total:
                    cite = f"{start}-{end}" if end else start
                    drifted.append((n, titles[i], f"{path}:{cite}  (file has {total} lines)"))

        for d in set(DIRCITE.findall(line)):
            if CONCEPTUAL.search(d) or not any(c.isalpha() for c in d):
                continue
            if not any(f"/{d}" in f"/{t}" for t in TRACKED):
                dirs.append((n, titles[i], d))

        if "Proof:" in line:
            for name in TESTREF.findall(line):
                if (name, titles[i]) in seen_test:
                    continue
                seen_test.add((name, titles[i]))
                if name not in corpus:
                    tests.append((n, titles[i], f"::{name}"))

        if "Proof:" in line or "Gets you:" in line:
            for name in IDENT.findall(line):
                if name in seen_ident:
                    continue
                seen_ident.add(name)
                if name not in corpus:
                    idents.append((n, titles[i], f"`{name}`"))

    report("CITED FILES THAT NO LONGER EXIST", missing)
    report("LINE CITATIONS PAST END OF FILE", drifted)
    report("CITED DIRECTORIES THAT NO LONGER EXIST", dirs)
    report("NAMED PROOF TESTS NOT FOUND IN THE TREE", tests)
    report("BACKTICKED IDENTIFIERS NOT FOUND IN THE TREE", idents)
    print(f"\nchecked {len(seen_test)} proof tests, {len(seen_ident)} identifiers")


if __name__ == "__main__":
    sys.exit(main())
