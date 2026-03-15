#!/usr/bin/env bash
set -euo pipefail

FIXTURE="$1"
OUTPUT="$2"
SPEED="${3:-5}"
CAST_FILE="$(mktemp /tmp/crucible-demo-XXXXXX.cast)"
SESSION="crucible-demo-$$"

tmux kill-session -t "$SESSION" 2>/dev/null || true
tmux new-session -d -s "$SESSION" -x 120 -y 35

COLORTERM=truecolor COLORFGBG="15;0" tmux send-keys -t "$SESSION" \
  "asciinema rec '$CAST_FILE' --overwrite -c 'OPENAI_API_KEY=dummy /home/moot/crucible/target/debug/cru chat --replay $FIXTURE --replay-speed $SPEED --replay-auto-exit 500 -C assets/demo-config.toml'; exit" Enter

MAX_WAIT=180
WAITED=0
while tmux has-session -t "$SESSION" 2>/dev/null && [ $WAITED -lt $MAX_WAIT ]; do
  sleep 2
  WAITED=$((WAITED + 2))
done

tmux kill-session -t "$SESSION" 2>/dev/null || true

if [ ! -f "$CAST_FILE" ] || [ ! -s "$CAST_FILE" ]; then
  echo "ERROR: Cast file not created or empty: $CAST_FILE"
  exit 1
fi

/home/moot/.cargo/bin/agg --theme dracula --font-size 16 --idle-time-limit 3 "$CAST_FILE" "$OUTPUT"
rm -f "$CAST_FILE"
SIZE="$(du -sh "$OUTPUT" | cut -f1)"
echo "GIF created: $OUTPUT ($SIZE)"
