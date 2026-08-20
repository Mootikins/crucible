"""Types declared in more than one crate, with the serde/alias noise removed."""
import json, re
from collections import defaultdict
from pathlib import Path
ROOT = Path('/home/moot/crucible')

# Associated types inside a serde impl, and per-crate Result aliases, are not
# duplicate domain types.
SERDE_LOCAL = {'Error','Ok','SerializeSeq','SerializeMap','SerializeStruct',
               'SerializeTuple','SerializeTupleStruct','SerializeTupleVariant',
               'SerializeStructVariant','Value','Item'}

pat = re.compile(r'^(\s*)(?:pub(?:\([^)]*\))?\s+)?(struct|enum|trait|type)\s+([A-Z][A-Za-z0-9_]*)')
FN = re.compile(r'(pub(\([^)]*\))?\s+)?(async\s+)?fn\s')


def is_function_local(lines, idx, indent):
    """True when the nearest enclosing open block is a `fn`, not a `mod`.

    A type declared inside a function body has NO module scope: nothing can
    import it, nothing can collide with it, it cannot shadow anything. The first
    version of this scan matched `^\\s*struct`, which counted them, and so
    reported `ShowRequest` and `ModelsResponse` in
    `agent_manager/context_length.rs` as cross-crate collisions. Both are
    four-space-indented locals, one inside each of two functions.
    """
    if indent == 0:
        return False
    for j in range(idx - 1, -1, -1):
        line = lines[j]
        stripped = line.lstrip()
        if not stripped or stripped.startswith(('//', '#[')):
            continue
        here = len(line) - len(stripped)
        if here < indent and line.rstrip().endswith('{'):
            return bool(FN.match(stripped))
    return False


decl = defaultdict(list)
for p in (ROOT/'crates').rglob('*.rs'):
    rel = str(p.relative_to(ROOT))
    if 'target' in rel or '/tests/' in rel or rel.endswith('tests.rs') or '/test' in rel:
        continue
    body = re.sub(r'#\[cfg\(test\)\][\s\S]*$', '', p.read_text(errors='ignore'))
    lines = body.split('\n')
    for i, line in enumerate(lines):
        m = pat.match(line)
        if not m:
            continue
        if is_function_local(lines, i, len(m.group(1))):
            continue
        decl[m.group(3)].append((rel, m.group(2)))

def crate(f): return f.split('/')[1]
rows = []
for name, locs in decl.items():
    if name in SERDE_LOCAL: continue
    kinds = {k for _, k in locs}
    files = sorted({f for f, _ in locs})
    if len({crate(f) for f in files}) < 2: continue
    # a per-crate Result/Error alias is the documented convention, not a duplicate
    if kinds == {'type'} and (name.endswith('Result') or name.endswith('Error')): continue
    rows.append((name, sorted(kinds), files))

rows.sort(key=lambda r: (-len(r[2]), r[0]))
print(f"{len(rows)} names declared in more than one crate\n")
for name, kinds, files in rows:
    print(f"{name}  [{'/'.join(kinds)}]")
    for f in files: print(f"    {f}")
Path('/tmp/fsize/g/dupes.json').write_text(json.dumps([{'name':n,'kinds':k,'files':f} for n,k,f in rows]))
