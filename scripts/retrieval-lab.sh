#!/usr/bin/env bash
# Run every retrieval strategy of the retrieval lab over two golden sets.
#
# For each strategy the script starts its own daemon under a temporary
# CRUCIBLE_HOME, with `[plugins.retrieval-lab] enabled = true` and
# `strategy = "<name>"` in a temporary config. It processes two temporary
# kilns, the adversarial fixture and the Help plus Guides docs, then runs
# `cru eval precognition --json` over three golden sets: the adversarial
# classes, the transition queries and the multi-hop queries. The last two
# run over the docs kiln. One row per strategy per class lands in
# `$OUT/results.tsv`, and the table prints at the end. A docs row names
# its golden set in the class column (`transition_queries`,
# `multihop_queries`), because the eval names the class after the golden
# file and both files are `golden.toml`.
#
# EMBEDDER. The lab needs a REAL embedder. The script writes
# `[enrichment.provider] type = "<provider>"` into its config. The default
# provider is `fastembed`, which the `cru` binary must have compiled in
# (`cru --standalone doctor -f json` must report "Embeddings available
# (fastembed)"; the
# model downloads into the fastembed cache on first use). `--provider ollama
# --model nomic-embed-text` works when Ollama is reachable. `mock` is refused:
# hash vectors carry no meaning, so every number would be noise.
#
# STRATEGY CONTRACT. The plugin reads `[plugins.retrieval-lab]`:
#   enabled  = true
#   strategy = "points" | "arc_post" | "arc_pre" | "bezier_post"
#            | "meta_pre" | "ppr_post" | "meta_ppr"
# A strategy in INDEX_STRATEGIES changes what the index stage writes, so the
# kilns are reprocessed with `--force` when the run enters or leaves one.
# The docs kiln is registered under the name `docs` on every daemon, because
# a strategy reads a hit's blocks by kiln name and a kiln opened by path
# alone has none.
# Before each eval the script zeroes the plugin's counters, and after it
# reads them with `cru lua '=RETRIEVAL_LAB_COUNTERS'`; a table `{ queries =
# N, interior_wins = N }` fills two columns, and its absence prints `n/a`.
# The transition set also scores the hit rows at block granularity
# (`block@1`, `block@k`, `passage@k`) over the queries with `expect_text`.
#
# Usage: scripts/retrieval-lab.sh [--dry-run] [--out DIR] [--cru BIN]
#                                 [--provider NAME] [--model NAME]
#                                 [--cache-dir DIR] [--strategies "a b c"]
#   --dry-run     Print the plan and check the embedder. Starts no daemon.
#   --out DIR     Where configs, kilns, JSON and results.tsv go
#                 (default: a fresh mktemp directory).
#   --cru BIN     The binary to run (default: target/debug/cru, else `cru`).
#   --provider    fastembed (default), ollama, or openai.
#   --model       Embedding model name; omitted means the provider's default.
#   --cache-dir   Where fastembed keeps its models (default: ~/.fastembed_cache,
#                 shared across runs so the model downloads once).
#   --strategies  Space-separated list (default: "points meta_pre ppr_post
#                 meta_ppr arc_pre").
#   --exclude-kinds "a b"  Block kinds the strategies never pair (default: the
#                 plugin's own default, `heading` and `transition`).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STRATEGIES="points slate_post arc_pre meta_pre ppr_post"
INDEX_STRATEGIES="arc_pre meta_pre meta_ppr"
DRY_RUN=0
OUT=""
CRU=""
PROVIDER="fastembed"
MODEL=""
CACHE_DIR="${HOME}/.fastembed_cache"
EXCLUDE_KINDS=""
DAEMON_WAIT_SECS=90

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --out) OUT="$2"; shift ;;
        --cru) CRU="$2"; shift ;;
        --provider) PROVIDER="$2"; shift ;;
        --model) MODEL="$2"; shift ;;
        --cache-dir) CACHE_DIR="$2"; shift ;;
        --strategies) STRATEGIES="$2"; shift ;;
        --exclude-kinds) EXCLUDE_KINDS="$2"; shift ;;
        -h|--help) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

say()  { printf '%s\n' "$*"; }
fail() { printf 'retrieval-lab: %s\n' "$*" >&2; exit 1; }

# --- The binary and the embedder ------------------------------------------

if [ -z "$CRU" ]; then
    if [ -x "$REPO/target/debug/cru" ]; then
        CRU="$REPO/target/debug/cru"
    elif command -v cru >/dev/null 2>&1; then
        CRU="$(command -v cru)"
    else
        fail "no cru binary: build one with 'cargo build -p crucible-cli' or pass --cru"
    fi
fi
[ -x "$CRU" ] || fail "not executable: $CRU"
command -v python3 >/dev/null 2>&1 || fail "python3 is needed to read the JSON reports"

case "$PROVIDER" in
    fastembed|ollama|openai) ;;
    mock) fail "the mock embedder is refused: its vectors carry no meaning. Use fastembed, ollama or openai" ;;
    *) fail "unknown provider '$PROVIDER': use fastembed, ollama or openai" ;;
esac

if [ -z "$OUT" ]; then
    OUT="$(mktemp -d "${TMPDIR:-/tmp}/retrieval-lab.XXXXXX")"
fi
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
LAB_HOME="$OUT/home"
KILNS="$OUT/kilns"
ADVERSARIAL_SRC="$REPO/assets/fixtures/adversarial_kiln/corpus"
ADVERSARIAL_GOLDEN="$REPO/assets/fixtures/adversarial_kiln/golden"
TRANSITION_GOLDEN="$REPO/assets/fixtures/transition_queries/golden.toml"
MULTIHOP_GOLDEN="$REPO/assets/fixtures/multihop_queries/golden.toml"
RESULTS="$OUT/results.tsv"
# A used home carries an index from an earlier run, so its first strategy
# would measure that index. Refuse it.
if [ -d "$LAB_HOME" ] && [ -n "$(ls -A "$LAB_HOME")" ]; then
    echo "retrieval-lab: $LAB_HOME is not empty; pass a fresh --out" >&2
    exit 2
fi
mkdir -p "$LAB_HOME" "$KILNS"
# Unix socket paths are capped near 108 bytes, so the sockets live in a short
# directory of their own rather than under $OUT.
SOCK_DIR="$(mktemp -d "${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}/rl.XXXXXX")"

# Every cru invocation runs with a child-scoped data root, config dir, data
# dir and socket, so nothing here reads the developer's plugins or writes
# under their ~/.local/share/crucible.
LAB_ENV=(CRUCIBLE_HOME="$LAB_HOME" XDG_CONFIG_HOME="$LAB_HOME/config" XDG_DATA_HOME="$LAB_HOME/data")
cru() {
    local cfg="$1"; shift
    env "${LAB_ENV[@]}" CRUCIBLE_SOCKET="$SOCK" "$CRU" --config "$cfg" "$@"
}

# One config per strategy. The daemon opens `kiln_path` at boot; the docs
# kiln is named on the command line (`cru process <dir>`, `cru eval --kiln`),
# because a daemon-backed command reads the daemon's effective config, not
# the file the CLI was pointed at.
write_config() {
    local path="$1" kiln="$2" strategy="$3"
    {
        printf 'kiln_path = "%s"\n\n' "$kiln"
        printf '[llm]\ndefault = "ollama"\n\n[llm.providers.ollama]\ntype = "ollama"\ndefault_model = "llama3.2"\n\n'
        printf '[enrichment.provider]\ntype = "%s"\n' "$PROVIDER"
        [ -n "$MODEL" ] && printf 'model = "%s"\n' "$MODEL"
        [ "$PROVIDER" = "fastembed" ] && printf 'cache_dir = "%s"\n' "$CACHE_DIR"
        printf '\n[plugins.retrieval-lab]\nenabled = true\nstrategy = "%s"\n' "$strategy"
        if [ -n "$EXCLUDE_KINDS" ]; then
            printf 'exclude_kinds = [%s]\n' "$(printf '"%s", ' $EXCLUDE_KINDS | sed 's/, $//')"
        fi
    } > "$path"
}

# `cru doctor` reports whether an embedding backend exists. Plain doctor
# auto-starts a daemon for its registry checks; `--standalone` boots an
# in-process one that ends with the command, so the dry run leaves nothing
# behind and needs no daemon.
check_embedder() {
    local cfg="$OUT/config-doctor.toml"
    SOCK="$SOCK_DIR/doctor.sock"
    write_config "$cfg" "$KILNS/docs" "points"
    local report
    report="$(XDG_RUNTIME_DIR="$SOCK_DIR" cru "$cfg" --standalone doctor -f json 2>/dev/null || true)"
    if ! printf '%s' "$report" | grep -q "Embeddings available ($PROVIDER)"; then
        if [ "$PROVIDER" = "openai" ] && printf '%s' "$report" | grep -q "Embeddings available"; then
            return 0
        fi
        printf '%s\n' "$report" >&2
        fail "cru doctor does not report 'Embeddings available ($PROVIDER)'. Build cru with the fastembed feature, or pass --provider ollama with Ollama running"
    fi
}

# --- Kilns ----------------------------------------------------------------

prepare_kilns() {
    rm -rf "$KILNS/adversarial" "$KILNS/docs"
    mkdir -p "$KILNS/adversarial/.crucible" "$KILNS/docs/.crucible"
    cp -R "$ADVERSARIAL_SRC/." "$KILNS/adversarial/"
    cp -R "$REPO/docs/Help" "$KILNS/docs/Help"
    cp -R "$REPO/docs/Guides" "$KILNS/docs/Guides"
    : > "$KILNS/adversarial/.crucible/kiln.toml"
    : > "$KILNS/docs/.crucible/kiln.toml"
}

# --- The daemon -----------------------------------------------------------

DAEMON_PID=""
stop_daemon() {
    if [ -n "$DAEMON_PID" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    DAEMON_PID=""
}
cleanup() {
    stop_daemon
    rm -rf "$SOCK_DIR"
}
trap cleanup EXIT

start_daemon() {
    local cfg="$1" log="$2"
    # The daemon's cwd is $OUT, so any relative path it writes stays out of
    # the repository.
    (cd "$OUT" && exec env "${LAB_ENV[@]}" CRUCIBLE_SOCKET="$SOCK" \
        "$CRU" --config "$cfg" daemon serve) >"$log" 2>&1 &
    DAEMON_PID=$!
    local waited=0
    until python3 - "$SOCK" <<'EOF' 2>/dev/null
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(0.2)
s.connect(sys.argv[1])
EOF
    do
        kill -0 "$DAEMON_PID" 2>/dev/null || { cat "$log" >&2; fail "the daemon exited before its socket came up"; }
        sleep 1
        waited=$((waited + 1))
        [ "$waited" -lt "$DAEMON_WAIT_SECS" ] || { cat "$log" >&2; fail "the daemon socket did not come up in ${DAEMON_WAIT_SECS}s"; }
    done
}

# --- One strategy ---------------------------------------------------------

in_list() { case " $2 " in *" $1 "*) return 0 ;; esac; return 1; }

# Appends the rows of one JSON report to results.tsv. A non-empty `label`
# replaces the report's class name, so one kiln can carry several golden
# sets that share a file name.
append_rows() {
    local strategy="$1" corpus="$2" report="$3" ms_per_query="$4" counters="$5" label="${6:-}"
    python3 - "$strategy" "$corpus" "$report" "$ms_per_query" "$counters" "$RESULTS" "$label" <<'EOF'
import json, sys
strategy, corpus, report, ms, counters, out, label = sys.argv[1:8]
data = json.load(open(report))
try:
    c = json.load(open(counters))
    queries = c.get("queries", "n/a")
    wins = c.get("interior_wins", "n/a")
except Exception:
    queries = wins = "n/a"
rows = list(data["classes"])
if len(rows) > 1:
    rows.append(data["total"])
with open(out, "a") as f:
    for r in rows:
        f.write("\t".join(str(x) for x in [
            strategy, corpus, label or r["class"], r["n"],
            f'{r["hit_at_1"]:.3f}', f'{r["hit_at_k"]:.3f}',
            f'{r["mrr"]:.3f}', f'{r["recall_at_k"]:.3f}',
            r["passage_n"], f'{r["block_hit_at_1"]:.3f}',
            f'{r["block_hit_at_k"]:.3f}', f'{r["passage_hit_at_k"]:.3f}',
            queries, wins, ms,
        ]) + "\n")
EOF
}

run_strategy() {
    local strategy="$1" previous="$2"
    local cfg="$OUT/config-$strategy.toml"
    SOCK="$SOCK_DIR/$strategy.sock"
    write_config "$cfg" "$KILNS/adversarial" "$strategy"

    say "== $strategy: starting the daemon"
    start_daemon "$cfg" "$OUT/daemon-$strategy.log"
    # A kiln opened by path alone is nameless, and a nameless hit carries no
    # `kiln` for the plugin to read its blocks through. The adversarial kiln
    # is named `default` by the config; the docs kiln needs a registration.
    cru "$cfg" kiln register docs "$KILNS/docs" > /dev/null

    local force=""
    if in_list "$strategy" "$INDEX_STRATEGIES" || in_list "$previous" "$INDEX_STRATEGIES"; then
        force="--force"
    fi
    say "== $strategy: processing the kilns ${force:+(reprocess: $force)}"
    cru "$cfg" process "$KILNS/adversarial" --json $force > "$OUT/$strategy-process-adversarial.json"
    cru "$cfg" process "$KILNS/docs" --json $force > "$OUT/$strategy-process-docs.json"

    # One eval per golden set: `corpus:label:path`. An empty label is the
    # adversarial directory, whose classes the report names itself.
    local start end ms n
    for run in "adversarial::$ADVERSARIAL_GOLDEN" \
               "docs:transition_queries:$TRANSITION_GOLDEN" \
               "docs:multihop_queries:$MULTIHOP_GOLDEN"; do
        local corpus label golden golden_args report
        corpus="${run%%:*}"
        label="${run#*:}"; label="${label%%:*}"
        golden="${run#*:*:}"
        if [ -z "$label" ]; then
            golden_args=(--golden-dir "$golden")
        else
            golden_args=(--golden "$golden")
        fi
        report="$OUT/$strategy-$corpus${label:+-$label}.json"
        say "== $strategy: eval over $corpus${label:+ ($label)}"
        # The counters accumulate across evals; zero them so each corpus
        # reports its own. No plugin, no counters.
        cru "$cfg" lua 'local c = RETRIEVAL_LAB_COUNTERS; if c then c.queries = 0; c.interior_wins = 0 end' > /dev/null 2>&1 || true
        start=$(date +%s%N)
        cru "$cfg" eval precognition --kiln "$KILNS/$corpus" "${golden_args[@]}" --json > "$report"
        end=$(date +%s%N)
        n=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["total"]["n"])' "$report")
        ms=$(( (end - start) / 1000000 / (n > 0 ? n : 1) ))
        # The plugin's counters, when it exposes them. No plugin, no counters.
        cru "$cfg" lua '=RETRIEVAL_LAB_COUNTERS' > "${report%.json}-counters.json" 2>/dev/null || true
        append_rows "$strategy" "$corpus" "$report" "$ms" "${report%.json}-counters.json" "$label"
    done

    stop_daemon
}

# --- Main -----------------------------------------------------------------

say "retrieval lab"
say "  cru:        $CRU"
say "  provider:   $PROVIDER${MODEL:+ ($MODEL)}"
[ "$PROVIDER" = "fastembed" ] && say "  cache:      $CACHE_DIR"
say "  strategies: $STRATEGIES"
say "  reprocess:  $INDEX_STRATEGIES"
say "  out:        $OUT"
say "  home:       $LAB_HOME (child-scoped CRUCIBLE_HOME)"
say "  kilns:      $KILNS/adversarial <- $ADVERSARIAL_SRC"
say "              $KILNS/docs        <- $REPO/docs/Help + $REPO/docs/Guides"
say "  golden:     $ADVERSARIAL_GOLDEN"
say "              $TRANSITION_GOLDEN"
say "              $MULTIHOP_GOLDEN"

[ -d "$ADVERSARIAL_SRC" ] || fail "missing $ADVERSARIAL_SRC"
[ -f "$TRANSITION_GOLDEN" ] || fail "missing $TRANSITION_GOLDEN"
[ -f "$MULTIHOP_GOLDEN" ] || fail "missing $MULTIHOP_GOLDEN"

prepare_kilns
check_embedder
say "  embedder:   ok ($PROVIDER)"

if [ "$DRY_RUN" -eq 1 ]; then
    say ""
    say "plan (dry run, no daemon started):"
    previous=""
    for s in $STRATEGIES; do
        force=""
        if in_list "$s" "$INDEX_STRATEGIES" || in_list "$previous" "$INDEX_STRATEGIES"; then
            force=" --force"
        fi
        say "  [$s] config: [plugins.retrieval-lab] enabled = true, strategy = \"$s\""
        say "  [$s] cru daemon serve  (CRUCIBLE_SOCKET=$SOCK_DIR/$s.sock)"
        say "  [$s] cru kiln register docs $KILNS/docs"
        say "  [$s] cru process $KILNS/adversarial --json$force"
        say "  [$s] cru process $KILNS/docs --json$force"
        say "  [$s] cru eval precognition --kiln $KILNS/adversarial --golden-dir $ADVERSARIAL_GOLDEN --json"
        say "  [$s] cru eval precognition --kiln $KILNS/docs --golden $TRANSITION_GOLDEN --json"
        say "  [$s] cru eval precognition --kiln $KILNS/docs --golden $MULTIHOP_GOLDEN --json"
        say "  [$s] cru lua '=RETRIEVAL_LAB_COUNTERS'"
        previous="$s"
    done
    say "  results: $RESULTS"
    exit 0
fi

printf 'strategy\tcorpus\tclass\tn\thit@1\thit@k\tmrr\trecall@k\tpassage_n\tblock@1\tblock@k\tpassage@k\tqueries\tinterior_wins\tms_per_query\n' > "$RESULTS"
previous=""
for s in $STRATEGIES; do
    run_strategy "$s" "$previous"
    previous="$s"
done

say ""
say "results: $RESULTS"
column -t -s "$(printf '\t')" "$RESULTS"
