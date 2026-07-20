#!/usr/bin/env bash
# Post-install smoke check for the cru web stack.
#
# Verifies the installed binary, the daemon socket, and the web server's
# embedded UI + API surface — including that the kiln link index is populated
# (graph view / backlinks) and that non-loopback requests are NOT wide open.
#
# Usage: scripts/sanity-web.sh [PORT] [CRU_BIN]
#   PORT     web server port (default 3000)
#   CRU_BIN  binary expected to own the stack (default ~/.cargo/bin/cru)
set -u

PORT="${1:-3000}"
CRU_BIN="${2:-$HOME/.cargo/bin/cru}"
BASE="http://127.0.0.1:${PORT}"
PASS=0
FAIL=0
WARN=0

ok()   { printf '  \033[32mPASS\033[0m %s\n' "$1"; PASS=$((PASS+1)); }
bad()  { printf '  \033[31mFAIL\033[0m %s\n' "$1"; FAIL=$((FAIL+1)); }
warn() { printf '  \033[33mWARN\033[0m %s\n' "$1"; WARN=$((WARN+1)); }

code() { curl -s -o /dev/null -w '%{http_code}' --max-time 5 "$1"; }

echo "== binary =="
if VERSION=$("$CRU_BIN" --version 2>/dev/null); then
  ok "$CRU_BIN → $VERSION"
else
  bad "$CRU_BIN not runnable"
fi

echo "== daemon =="
SOCK="${CRUCIBLE_SOCKET:-${XDG_RUNTIME_DIR:-/tmp}/crucible.sock}"
if [ -S "$SOCK" ]; then
  OWNER=$(lsof -t "$SOCK" 2>/dev/null | sort -u | head -1)
  if [ -n "$OWNER" ]; then
    EXE=$(readlink "/proc/$OWNER/exe" 2>/dev/null || echo '?')
    if [ "$EXE" = "$CRU_BIN" ]; then
      ok "socket $SOCK owned by $CRU_BIN (pid $OWNER)"
    else
      warn "socket $SOCK owned by $EXE — expected $CRU_BIN (stale daemon?)"
    fi
  else
    warn "socket $SOCK exists but no owner found"
  fi
else
  bad "no daemon socket at $SOCK"
fi

echo "== web (localhost) =="
[ "$(code "$BASE/health")" = 200 ] && ok "/health 200" || bad "/health not 200"
INDEX=$(curl -s --max-time 5 "$BASE/")
if echo "$INDEX" | grep -q 'id="root"'; then ok "UI index served"; else bad "UI index missing root div"; fi

# Embedded hashed assets: every /assets/*.js referenced by the index must load.
ASSET=$(echo "$INDEX" | grep -o '/assets/[A-Za-z0-9._-]*\.js' | head -1)
if [ -n "$ASSET" ]; then
  [ "$(code "$BASE$ASSET")" = 200 ] && ok "bundle $ASSET 200" || bad "bundle $ASSET not 200"
  # Fonts are url()-referenced from the emitted CSS bundle (fontsource
  # imports in index.tsx → vite emits hashed woff2 + CSS @font-face).
  CSSA=$(echo "$INDEX" | grep -o '/assets/[A-Za-z0-9._-]*\.css' | head -1)
  WOFF=""
  [ -n "$CSSA" ] && WOFF=$(curl -s --max-time 5 "$BASE$CSSA" | grep -o '/assets/[A-Za-z0-9._-]*\.woff2' | head -1)
  if [ -n "$WOFF" ]; then
    [ "$(code "$BASE$WOFF")" = 200 ] && ok "font $WOFF 200" || bad "font $WOFF not 200"
  else
    bad "no woff2 reference in CSS bundle — font emission regressed"
  fi
else
  bad "index references no /assets/*.js"
fi

echo "== kiln graph / link index =="
KILN=$(curl -s --max-time 5 "$BASE/api/config" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("kiln_path",""))' 2>/dev/null)
if [ -n "$KILN" ]; then
  GRAPH=$(curl -s --max-time 30 "$BASE/api/kiln/graph?kiln=$(python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))" "$KILN")")
  COUNTS=$(echo "$GRAPH" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d["notes"]), len(d["links"]))' 2>/dev/null)
  if [ -n "$COUNTS" ]; then
    NOTES=${COUNTS% *}; LINKS=${COUNTS#* }
    [ "$NOTES" -gt 0 ] && ok "graph: $NOTES notes" || warn "graph: kiln $KILN has no notes"
    if [ "$NOTES" -gt 0 ]; then
      [ "$LINKS" -gt 0 ] && ok "graph: $LINKS links (link index populated)" \
        || warn "graph: 0 links — link index empty (relink pending, or kiln truly has no wikilinks)"
    fi
  else
    bad "graph API returned no parseable JSON for kiln $KILN"
  fi
else
  warn "/api/config gave no kiln_path — skipping graph check"
fi

echo "== LAN binds =="
for IP in $(hostname -I 2>/dev/null); do
  case "$IP" in 127.*|::1) continue;; *:*) continue;; esac
  if [ "$(code "http://$IP:$PORT/")" = 200 ]; then
    ok "UI reachable on http://$IP:$PORT"
    # Same-machine curl to the LAN IP arrives from a non-loopback peer, so
    # the API must NOT answer without auth (localhost exemption only).
    RC=$(code "http://$IP:$PORT/api/config")
    case "$RC" in
      401|403) ok "remote /api/config requires auth ($RC)";;
      200)     bad "remote /api/config answered WITHOUT auth — fail-open!";;
      *)       warn "remote /api/config → $RC (expected 401/403)";;
    esac
  else
    warn "UI not reachable on http://$IP:$PORT (not bound to this interface?)"
  fi
done

echo
printf '%d passed, %d warnings, %d failed\n' "$PASS" "$WARN" "$FAIL"
[ "$FAIL" -eq 0 ]
