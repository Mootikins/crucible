#!/usr/bin/env bash
# Usage: scripts/record-gif.sh <fixture.jsonl> <output.gif> [--speed N]
# Records a Crucible TUI replay as a GIF using tmux + asciinema + agg
set -euo pipefail

FIXTURE="${1:?Usage: $0 <fixture.jsonl> <output.gif> [--speed N]}"
OUTPUT="${2:?Usage: $0 <fixture.jsonl> <output.gif> [--speed N]}"
SPEED=5

# Parse optional --speed flag
if [[ $# -gt 2 ]]; then
  if [[ "$3" == "--speed" ]]; then
    SPEED="${4:?--speed requires a value}"
  fi
fi

CAST_FILE="$(mktemp /tmp/crucible-demo-XXXXXX.cast)"
SESSION="crucible-demo-$$"

cleanup() {
  tmux kill-session -t "$SESSION" 2>/dev/null || true
  rm -f "$CAST_FILE"
}
trap cleanup EXIT

echo "→ Starting tmux session: $SESSION"
tmux new-session -d -s "$SESSION" -x 120 -y 35

echo "→ Recording replay (speed=$SPEED) to $CAST_FILE"
export OPENAI_API_KEY=dummy

# Start asciinema recording in tmux with the replay command
tmux send-keys -t "$SESSION" \
  "asciinema rec '$CAST_FILE' --overwrite -c 'COLORTERM=truecolor COLORFGBG=\"15;0\" OPENAI_API_KEY=dummy cru chat --replay \"$FIXTURE\" --replay-speed $SPEED --replay-auto-exit 500 -C assets/demo-config.toml'; exit" \
  Enter

# Wait for tmux session to exit (replay complete)
# The session will stay open until asciinema finishes recording
MAX_WAIT=300
WAITED=0
echo "→ Waiting for replay to complete (max ${MAX_WAIT}s)..."
while tmux has-session -t "$SESSION" 2>/dev/null && [ $WAITED -lt $MAX_WAIT ]; do
  sleep 2
  WAITED=$((WAITED + 2))
  [ $((WAITED % 10)) -eq 0 ] && echo "  ...${WAITED}s elapsed"
done

if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "⚠ Timeout after ${MAX_WAIT}s, forcing exit"
  tmux kill-session -t "$SESSION" 2>/dev/null || true
fi

echo "→ Converting cast to GIF: $OUTPUT"
if [ ! -f "$CAST_FILE" ] || [ ! -s "$CAST_FILE" ]; then
  echo "✗ Cast file missing or empty: $CAST_FILE"
  exit 1
fi

~/.cargo/bin/agg --theme dracula --font-size 16 --idle-time-limit 3 "$CAST_FILE" "$OUTPUT"
echo "✓ GIF created: $OUTPUT ($(du -sh "$OUTPUT" | cut -f1))"
