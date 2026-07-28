#!/usr/bin/env python3
"""Generate THIRD-PARTY-NOTICES.md from the dependency graph that actually ships.

The released `cru` binary embeds `crates/crucible-web/web/dist` wholesale via
rust-embed, which means it redistributes 69 IBM Plex font files (OFL-1.1), the
KaTeX fonts, and Lucide's icon paths — all of which condition redistribution on
carrying their notice. None of that is visible in the source tree, because
`dist/` is gitignored and built during release, so the gap was invisible.

This is generated rather than hand-maintained for the obvious reason: 1100 Rust
crates and several hundred npm packages cannot be tracked by hand, and a notices
file that silently goes stale is worse than none — it asserts compliance it no
longer has.

Run via `just notices`.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WEB = ROOT / "crates/crucible-web/web"

# The feature set `[workspace.metadata.dist]` builds the release binary with.
# Enumerating anything else would list crates that never ship.
DIST_FEATURES = "fastembed,web"
DIST_TARGET = "x86_64-unknown-linux-gnu"


def rust_packages() -> list[tuple[str, str, str]]:
    """Every crate linked into the shipped binary, as (name, version, license)."""
    out = subprocess.run(
        [
            "cargo", "tree",
            "-p", "crucible-cli",
            "--no-default-features",
            "--features", DIST_FEATURES,
            "-e", "normal",  # normal deps only: build/dev deps are not redistributed
            "--target", DIST_TARGET,
            "--prefix", "none",
            "--format", "{p}|{l}",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout

    seen: dict[tuple[str, str], str] = {}
    for line in out.splitlines():
        line = line.strip()
        if not line or "|" not in line:
            continue
        pkg, license_ = line.rsplit("|", 1)
        parts = pkg.split()
        if len(parts) < 2 or not parts[1].startswith("v"):
            continue
        name, version = parts[0], parts[1][1:]
        # A crate reached by several paths appears repeatedly; and a proc-macro
        # listed without a license string is recorded as such rather than
        # silently dropped.
        # cargo appends its "already shown" marker after the whole format
        # string, so it lands inside the licence field rather than the package.
        license_ = license_.strip().removesuffix("(*)").strip()
        seen[(name, version)] = license_ or "NOT DECLARED"
    return sorted((n, v, l) for (n, v), l in seen.items())


def js_packages() -> list[tuple[str, str, str]]:
    """Installed npm packages in the web tree.

    Deliberately over-inclusive: it lists build-time packages too. Working out
    which modules a bundler actually inlined is not reliably decidable from the
    outside, and an extra notice costs nothing while a missing one is the whole
    problem.
    """
    modules = WEB / "node_modules"
    if not modules.is_dir():
        sys.exit(f"node_modules missing at {modules} — run `bun install` in the web tree first")

    found: dict[tuple[str, str], str] = {}
    for manifest in modules.glob("*/package.json"):
        _record(manifest, found)
    for manifest in modules.glob("@*/*/package.json"):
        _record(manifest, found)
    return sorted((n, v, l) for (n, v), l in found.items())


def _record(manifest: Path, into: dict[tuple[str, str], str]) -> None:
    try:
        data = json.loads(manifest.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return
    name, version = data.get("name"), data.get("version")
    if not name or not version:
        return

    license_ = data.get("license")
    if isinstance(license_, dict):  # the deprecated object form
        license_ = license_.get("type")
    if not license_ and isinstance(data.get("licenses"), list):
        license_ = " OR ".join(
            l.get("type", "") for l in data["licenses"] if isinstance(l, dict)
        )
    into[(name, version)] = license_ or _license_from_disk(manifest.parent) or "NOT DECLARED"


SPDX_HINTS = [
    ("Apache License", "Apache-2.0"),
    ("SIL Open Font License", "OFL-1.1"),
    ("Mozilla Public License", "MPL-2.0"),
    ("ISC License", "ISC"),
    ("MIT License", "MIT"),
    ("MIT ", "MIT"),
]


def _license_from_disk(pkg_dir: Path) -> str | None:
    """Infer a licence for a package whose manifest omits the field.

    Reported as `<SPDX> (from file)` so a reader can tell an inference from a
    declaration — the two are not equally trustworthy.
    """
    for pattern in ("LICENSE*", "LICENCE*", "license*", "licence*", "readme.md", "README.md"):
        for candidate in sorted(pkg_dir.glob(pattern)):
            text = read_text(candidate)
            if not text:
                continue
            head = text[:4000]
            for needle, spdx in SPDX_HINTS:
                if needle in head:
                    return f"{spdx} (from file)"
    return None


def read_text(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8").strip()
    except OSError:
        return None


# Assets the binary redistributes verbatim, whose notices must travel with them.
VERBATIM = [
    ("IBM Plex Sans", "OFL-1.1", WEB / "node_modules/@fontsource/ibm-plex-sans/LICENSE"),
    ("IBM Plex Mono", "OFL-1.1", WEB / "node_modules/@fontsource/ibm-plex-mono/LICENSE"),
    ("KaTeX (including its fonts)", "MIT", WEB / "node_modules/katex/LICENSE"),
    ("Lucide icons", "ISC", WEB / "node_modules/lucide-solid/LICENSE"),
    ("JSON Canvas sample", "MIT", ROOT / "crates/crucible-core/tests/fixtures/canvas/LICENSE"),
]

# Copyleft that reaches the binary. Both licenses are file-level: they bind the
# covered files, not Crucible's own source, but both require that recipients be
# told where to get that source.
SOURCE_OFFER = """\
The following components are covered by file-level copyleft licences. Their
obligations attach to those components' own files and not to Crucible's source,
but recipients must be told where the covered source can be obtained.

| Component | Licence | Source |
|-----------|---------|--------|
| `colored` | MPL-2.0 | https://crates.io/crates/colored |
| `nucleo-matcher` | MPL-2.0 | https://crates.io/crates/nucleo-matcher |
| `option-ext` | MPL-2.0 | https://crates.io/crates/option-ext |
| `inferno` | CDDL-1.0 | https://crates.io/crates/inferno |

`inferno` is present unintentionally: LanceDB declares `lance-testing` as a
normal dependency rather than a dev-dependency, which drags a flamegraph
profiler into every consumer's release build. Tracked for an upstream fix.

The full MPL-2.0 text is at https://mozilla.org/MPL/2.0/ and CDDL-1.0 at
https://opensource.org/license/cddl-1-0. `dompurify` is offered as
`MPL-2.0 OR Apache-2.0`; Crucible elects **Apache-2.0**. `self_cell` is offered
as `Apache-2.0 OR GPL-2.0-only`; Crucible elects **Apache-2.0**.
"""


def table(rows: list[tuple[str, str, str]]) -> str:
    lines = ["| Package | Version | Licence |", "|---------|---------|---------|"]
    lines += [f"| `{n}` | {v} | {l} |" for n, v, l in rows]
    return "\n".join(lines)


def main() -> None:
    rust = rust_packages()
    js = js_packages()

    parts = [
        "# Third-party notices",
        "",
        "Crucible is distributed under `MIT OR Apache-2.0` (see `LICENSE-MIT` and",
        "`LICENSE-APACHE`). It also redistributes third-party software, listed here",
        "with the notices those licences require.",
        "",
        "**This file is generated** — run `just notices` to regenerate it. Do not edit",
        "it by hand; a notices file that has quietly gone stale asserts a compliance it",
        "no longer has.",
        "",
        f"Covers the release build: `--no-default-features --features {DIST_FEATURES}`,",
        f"target `{DIST_TARGET}`, normal dependencies only — build-time and",
        "development dependencies are not redistributed. The npm list is deliberately",
        "over-inclusive, since which modules a bundler inlines is not reliably",
        "decidable from the outside.",
        "",
        "---",
        "",
        "## Redistributed verbatim",
        "",
        "The released binary embeds the built web UI, which carries these files as-is.",
        "Their notices are reproduced in full below because their licences require the",
        "notice to travel with the files.",
        "",
    ]

    for name, spdx, path in VERBATIM:
        text = read_text(path)
        parts += [f"### {name} — {spdx}", ""]
        if text is None:
            parts += [
                f"> Notice file not found at `{path.relative_to(ROOT)}`.",
                "> Regenerate after installing dependencies.",
                "",
            ]
        else:
            parts += ["```", text, "```", ""]

    parts += ["---", "", "## Source availability", "", SOURCE_OFFER, "---", ""]
    parts += [
        f"## Rust crates ({len(rust)})",
        "",
        "Linked into the released binary.",
        "",
        table(rust),
        "",
        "---",
        "",
        f"## npm packages ({len(js)})",
        "",
        "Installed in the web tree; a subset is bundled into the shipped UI.",
        "",
        table(js),
        "",
    ]

    out = "\n".join(parts) + "\n"

    root_file = ROOT / "THIRD-PARTY-NOTICES.md"
    root_file.write_text(out, encoding="utf-8")

    # Also into the web tree's public/ so Vite copies it into dist/, which
    # rust-embed then bakes into the binary — the notice has to accompany the
    # fonts, and the fonts are inside the executable.
    public_file = WEB / "public/THIRD-PARTY-NOTICES.md"
    public_file.write_text(out, encoding="utf-8")

    print(f"wrote {root_file.relative_to(ROOT)} and {public_file.relative_to(ROOT)}")
    print(f"  {len(rust)} Rust crates, {len(js)} npm packages")
    missing = [n for n, _, p in VERBATIM if read_text(p) is None]
    if missing:
        print(f"  WARNING: notice text missing for: {', '.join(missing)}")
        sys.exit(1)


if __name__ == "__main__":
    main()
