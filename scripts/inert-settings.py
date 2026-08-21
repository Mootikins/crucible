"""Settings a user can write that nothing reads — and that do not say so.

The failure this catches: a field deserializes, validates, and is then never
consulted, so a user who sets it gets silence. `[embedding.fastembed] cache_dir`
was one; it reached the provider and was overwritten with `None`.

Deliberately narrow, because the wide version is noise:

  - Only `Deserialize` structs. A `Serialize`-only field is read by a client,
    not by Rust, so "no Rust reads it" says nothing. This cut 162 raw hits to 113.
  - Only fields whose doc comment does NOT already admit they are inert.
    `ServerConfig::https` and its four neighbours say "Reserved for future use —
    not yet wired to server behavior", which is honest. An inert field that says
    so is documentation, not a bug.

Still textual, so a field read only by destructuring or by serde is a false
positive. This is a REPORT. Do not gate on it.

Run: python3 scripts/inert-settings.py
"""
import re
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = [p for p in (ROOT / 'crates').rglob('*.rs') if 'target' not in p.parts]

ADMITS_INERT = re.compile(
    r'reserved for future|not yet wired|currently inert|unused|no effect|'
    r'not read|placeholder|deprecated',
    re.I,
)

reads = defaultdict(int)
text = {}
for p in SRC:
    t = p.read_text(errors='ignore')
    text[p] = t
    for m in re.finditer(r'\.([a-z_][a-z0-9_]*)\b', t):
        reads[m.group(1)] += 1

struct_re = re.compile(r'^((?:#\[[^\n]*\]\n)*)(pub(?:\([^)]*\))?\s+)?struct\s+([A-Z]\w*)\s*\{', re.M)
field_re = re.compile(r'^\s{4}pub\s+([a-z_][a-z0-9_]*)\s*:')

rows = []
for p in SRC:
    rel = str(p.relative_to(ROOT))
    if '/tests/' in rel or rel.endswith('tests.rs') or 'mocks' in rel:
        continue
    t = text[p]
    lines = t.split('\n')
    for m in struct_re.finditer(t):
        attrs = m.group(1) or ''
        if 'Deserialize' not in attrs:
            continue
        start = t[:m.start()].count('\n') + attrs.count('\n')
        doc = []
        for i in range(start + 1, min(start + 90, len(lines))):
            line = lines[i]
            if line.startswith('}'):
                break
            stripped = line.strip()
            if stripped.startswith('///'):
                doc.append(stripped)
                continue
            fm = field_re.match(line)
            if fm:
                name = fm.group(1)
                if reads.get(name, 0) == 0 and not ADMITS_INERT.search(' '.join(doc)):
                    rows.append((rel, m.group(3), name))
            if not stripped.startswith('#['):
                doc = []

print(f"{len(rows)} settable fields that nothing reads and that do not say so\n")
by = defaultdict(list)
for rel, st, f in rows:
    by[rel].append(f'{st}.{f}')
for rel in sorted(by, key=lambda r: -len(by[r])):
    print(f'  {len(by[rel]):2d}  {rel}')
    for x in by[rel]:
        print(f'        {x}')
