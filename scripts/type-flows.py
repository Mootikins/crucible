"""What types each feature flow crosses, read from handler signatures.

The call graph cannot answer this: graphify records one `calls` edge for
`handle_session_send_message`, which plainly calls many things, because Rust
call resolution across modules is not something the AST extractor does. Handler
signatures do parse reliably, and for a type-alignment question they are the
better source anyway — a signature is where a type crosses a boundary.
"""
import json, re
from collections import defaultdict
from pathlib import Path

ROOT = Path('/home/moot/crucible')
SKIP = {'String','str','Vec','Option','Result','Arc','Box','HashMap','BTreeMap',
        'Value','PathBuf','Path','bool','usize','u64','u32','i64','f32','f64',
        'Sender','Receiver','Self','Duration','Instant','JsonValue','Mutex',
        'RwLock','HashSet','BTreeSet','Cow','Ordering','Range','IndexMap'}

# Where every type is declared, from source.
decl = defaultdict(set)
declpat = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|type)\s+([A-Z][A-Za-z0-9_]*)', re.M)
for p in (ROOT/'crates').rglob('*.rs'):
    rel = str(p.relative_to(ROOT))
    if 'target' in rel or '/tests/' in rel or rel.endswith('tests.rs'):
        continue
    body = re.sub(r'#\[cfg\(test\)\][\s\S]*$', '', p.read_text(errors='ignore'))
    for m in declpat.finditer(body):
        decl[m.group(1)].add(rel)

def owner(t):
    files = decl.get(t)
    if not files: return None
    return sorted(files)

# Every handler signature in the daemon.
sigpat = re.compile(r'pub(?:\(crate\))?\s+(?:async\s+)?fn\s+(handle_[a-z0-9_]+)\s*\(([^{]*?)\)\s*(?:->\s*([^{]+?))?\s*\{', re.S)
handlers = {}
for p in (ROOT/'crates/crucible-daemon/src').rglob('*.rs'):
    rel = str(p.relative_to(ROOT))
    if '/tests/' in rel or rel.endswith('tests.rs'): continue
    src = p.read_text(errors='ignore')
    for m in sigpat.finditer(src):
        handlers[m.group(1)] = (rel, m.group(2), (m.group(3) or '').strip())

print(f"handlers parsed: {len(handlers)}\n")

def types_in(text):
    out = []
    for t in re.findall(r'\b([A-Z][A-Za-z0-9_]*)\b', text or ''):
        if t in SKIP or t in out: continue
        out.append(t)
    return out

# Group by the RpcMethod family the handler serves.
FAMILY = [('session_send_message','turn'),('session_','session'),('kiln_','kiln'),
          ('note_','note'),('search','search'),('plugin_','plugin'),
          ('workflow_','workflow'),('skill','skill'),('agent','agent'),
          ('job','job'),('embed','embed'),('ui_','ui'),('config','config')]
def family(name):
    for pre, fam in FAMILY:
        if name.replace('handle_','').startswith(pre): return fam
    return 'other'

fams = defaultdict(list)
for name,(rel,params,ret) in sorted(handlers.items()):
    fams[family(name)].append((name, rel, types_in(params), types_in(ret)))

report = {}
for fam in sorted(fams):
    rows = fams[fam]
    seen = defaultdict(int)
    for _, _, pin, pout in rows:
        for t in pin + pout: seen[t] += 1
    report[fam] = {'handlers': len(rows), 'types': seen}
    print(f"=== {fam}  ({len(rows)} handlers) ===")
    for t, n in sorted(seen.items(), key=lambda kv: -kv[1])[:12]:
        loc = owner(t)
        if not loc:
            where = "(external)"
        elif len(loc) == 1:
            where = loc[0]
        else:
            where = f"{len(loc)} DECLARATIONS: " + " · ".join(loc)
        print(f"   {n:3d}x {t:28s} {where}")
    print()
Path('/tmp/fsize/g/typeflow.json').write_text(json.dumps(
    {k: {'handlers': v['handlers'], 'types': v['types']} for k, v in report.items()}))
