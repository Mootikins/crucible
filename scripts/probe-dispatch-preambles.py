#!/usr/bin/env python3
"""Does the RPC surface have a layer, written N times by hand?

The question: take every handler the dispatcher routes to, look at what it does
BEFORE its real work, and see whether that preamble is the same shape over and
over. If it is, that shape is the missing middleware and extracting it is
mechanical. If every handler is different, the match is honest.

Classifies the preamble, not the body: everything from `fn` up to the first line
that is neither a parameter extraction, a resolution, a guard, nor blank.
"""
import re
import subprocess
from collections import Counter, defaultdict
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
DISPATCH = ROOT / "crates/crucible-daemon/src/rpc/dispatch.rs"

# What a preamble step can be. Order matters only for reporting.
STEPS = [
    ("param", re.compile(r"require_param!|req\.params\.get\(")),
    ("kiln", re.compile(r"get_or_open\(|kiln_manager|km\.")),
    ("scope", re.compile(r"request_scope\(|Scope::|scope\b")),
    ("session", re.compile(r"session_id|get_session\(")),
    ("deser", re.compile(r"serde_json::from_value|from_value::<")),
    ("guard", re.compile(r"return Response::error|INVALID_PARAMS|internal_error\(")),
    ("permission", re.compile(r"permission|Permission")),
]


def dispatch_arms():
    """(method, is_forwarding) for every arm of the dispatcher's match."""
    text = DISPATCH.read_text()
    arms = []
    for m in re.finditer(r'^\s*"([a-z_]+\.[a-z_]+)" =>(.*)$', text, re.M):
        method, tail = m.group(1), m.group(2)
        # An arm that forwards is one whose whole body is a call to a handler.
        forwarding = bool(re.search(r"self\.handle_[a-z_]+\(", tail)) or tail.strip() in ("{", "")
        arms.append((method, forwarding, m.start()))
    return arms


def handler_bodies():
    """Every `handle_*` fn in the daemon, as (path, name, body_lines)."""
    out = subprocess.run(
        ["rg", "-n", r"^\s*(pub(\(crate\))? )?async fn handle_[a-z_]+", "--glob", "!*test*", "crates/crucible-daemon/src"],
        cwd=ROOT, capture_output=True, text=True,
    ).stdout
    found = []
    for line in out.splitlines():
        try:
            path, lineno, _ = line.split(":", 2)
        except ValueError:
            continue
        name = re.search(r"fn (handle_[a-z_]+)", line)
        if not name:
            continue
        src = (ROOT / path).read_text().splitlines()
        start = int(lineno) - 1
        # Walk to the end of the signature, then take the body until brace depth returns to 0.
        depth, body, started = 0, [], False
        for ln in src[start:start + 400]:
            depth += ln.count("{") - ln.count("}")
            if not started and "{" in ln:
                started = True
                continue
            if started:
                if depth <= 0:
                    break
                body.append(ln)
        found.append((path, name.group(1), body))
    return found


def preamble_of(body):
    """The leading run of steps, as an ordered list of step names."""
    steps = []
    for ln in body:
        s = ln.strip()
        if not s or s.startswith("//"):
            continue
        hit = [name for name, pat in STEPS if pat.search(s)]
        if not hit:
            # First line that is not a recognised preamble step ends the preamble,
            # unless it is pure scaffolding.
            if s in ("}", "};", ")", ");") or s.startswith("let ") and "=" not in s:
                continue
            break
        for h in hit:
            if h not in steps:
                steps.append(h)
    return steps


def main():
    arms = dispatch_arms()
    fwd = sum(1 for _, f, _ in arms if f)
    print(f"dispatch arms: {len(arms)}  forwarding: {fwd}  inline: {len(arms) - fwd}\n")

    handlers = handler_bodies()
    print(f"handle_* functions found: {len(handlers)}\n")

    shapes = Counter()
    step_freq = Counter()
    by_shape = defaultdict(list)
    empty = 0
    for path, name, body in handlers:
        pre = preamble_of(body)
        if not pre:
            empty += 1
            continue
        key = " -> ".join(pre)
        shapes[key] += 1
        by_shape[key].append(f"{Path(path).name}::{name}")
        for s in pre:
            step_freq[s] += 1

    print("=== how often each preamble step appears at all ===")
    for step, n in step_freq.most_common():
        print(f"  {step:12} {n:3}/{len(handlers)} handlers")

    print(f"\n  (no recognised preamble: {empty})")

    print("\n=== the preamble shapes, most common first ===")
    for shape, n in shapes.most_common(12):
        print(f"\n  {n:3}x  {shape}")
        for ex in by_shape[shape][:3]:
            print(f"         {ex}")
        if len(by_shape[shape]) > 3:
            print(f"         … and {len(by_shape[shape]) - 3} more")

    top = shapes.most_common(3)
    covered = sum(n for _, n in top)
    total = sum(shapes.values())
    print(f"\n=== VERDICT INPUT ===")
    print(f"  distinct shapes: {len(shapes)} across {total} handlers with a preamble")
    if total:
        print(f"  top 3 shapes cover {covered}/{total} = {100 * covered // total}%")


if __name__ == "__main__":
    main()
