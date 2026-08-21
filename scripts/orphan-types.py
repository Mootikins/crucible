#!/usr/bin/env python3
"""Public types that no Rust code names outside their own declaration.

WHY THIS AND NOT scip-refs.py
    `scip-refs.py` answers "is this FIELD ever read". This answers "is this
    TYPE ever named". They are different questions and they need different
    evidence: a type has no read/write split to classify, and rust-analyzer
    resolves a name the same way ripgrep does once you know there is exactly
    one declaration of it.

WHAT AN ORPHAN IS
    A type whose name appears EXACTLY ONCE in all of `crates/`: on the line
    that declares it. No `impl` block, no field, no import, no test.

    The loose test -- "no reference outside the declaring file" -- does NOT
    work. Every RPC request type in `rpc_client/client/session.rs` is built by
    a method in that same file, and the loose test called all fourteen of them
    dead. Count total occurrences, not foreign ones.

WHAT IT CANNOT TELL YOU
    The same blind spots as `scip-refs.py`. A name reached only from Lua,
    Fennel, the SolidJS frontend, a serde wire payload or a macro that builds
    it by concatenation has zero Rust occurrences and is still load-bearing.
    The report flags a hit in `runtime/`, `web/` or `docs/` for that reason.
    Read the declaration before you delete it.

USAGE
    scripts/orphan-types.py
    scripts/orphan-types.py --include-doubles   # also names declared twice
"""

from __future__ import annotations

import argparse
import collections
import pathlib
import re
import subprocess
import sys

DECL = re.compile(r"^pub (?:struct|enum|trait) ([A-Z][A-Za-z0-9_]*)")

# A double is a test stand-in by convention, not an orphan to delete.
TEST_DOUBLE_PREFIXES = ("Test", "Mock", "Fake", "Noop", "NoOp", "Stub")

# Places a Rust name can be reached from without any Rust occurrence.
FOREIGN_ROOTS = ["runtime", "crates/crucible-web/web", "docs"]


def declarations(root: pathlib.Path) -> dict[str, list[tuple[str, int]]]:
    found: dict[str, list[tuple[str, int]]] = collections.defaultdict(list)
    for path in root.rglob("*.rs"):
        for lineno, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
            match = DECL.match(line)
            if match:
                found[match.group(1)].append((str(path), lineno))
    return found


def occurrence_counts(names: list[str], root: pathlib.Path) -> collections.Counter:
    """One ripgrep pass over every name. Per-name greps take minutes."""
    pattern = r"\b(" + "|".join(names) + r")\b"
    result = subprocess.run(
        ["rg", "--no-heading", "-o", "-n", "-t", "rust", "-e", pattern, str(root)],
        capture_output=True,
        text=True,
    )
    counts: collections.Counter = collections.Counter()
    for line in result.stdout.splitlines():
        counts[line.split(":", 2)[2]] += 1
    return counts


def foreign_hit(name: str) -> str | None:
    roots = [r for r in FOREIGN_ROOTS if pathlib.Path(r).exists()]
    if not roots:
        return None
    result = subprocess.run(
        ["rg", "-l", "-w", name, *roots], capture_output=True, text=True
    )
    files = result.stdout.split()
    return files[0] if files else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--include-doubles",
        action="store_true",
        help="report names declared more than once (a count is ambiguous for them)",
    )
    args = parser.parse_args()

    root = pathlib.Path("crates")
    if not root.is_dir():
        print("run this from the repository root", file=sys.stderr)
        return 2

    declared = declarations(root)
    unique = sorted(n for n, v in declared.items() if len(v) == 1)
    counts = occurrence_counts(unique, root)

    orphans = 0
    for name in unique:
        if counts[name] != 1 or name.startswith(TEST_DOUBLE_PREFIXES):
            continue
        path, lineno = declared[name][0]
        foreign = foreign_hit(name)
        note = f"   [named in {foreign} -- check before deleting]" if foreign else ""
        print(f"{path}:{lineno}  {name}{note}")
        orphans += 1

    if args.include_doubles:
        print("\n-- declared more than once, count is ambiguous --")
        for name, sites in sorted(declared.items()):
            if len(sites) > 1:
                print(f"{name}: " + ", ".join(f"{p}:{n}" for p, n in sites))

    print(f"\n{orphans} orphan(s).", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
