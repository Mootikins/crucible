#!/usr/bin/env bash
# Prove the test suite writes nothing under the developer's own directories.
#
# A test that reads `dirs::config_dir()` instead of an injected home writes
# into `~/.config/crucible`. That passes CI, passes locally, and silently
# edits the developer's real configuration. One such leak wrote 7,503 files
# over seven months. The compiler cannot see it; only this can.
set -euo pipefail

roots=("$HOME/.config/crucible" "$HOME/.local/share/crucible" "$HOME/.crucible")
before=$(mktemp) && after=$(mktemp)
trap 'rm -f "$before" "$after"' EXIT

snapshot() {
    for root in "${roots[@]}"; do
        [ -e "$root" ] && find "$root" -printf '%p %T@ %s\n' 2>/dev/null
    done | sort
}

snapshot > "$before"
echo "Running the test suite..."
status=0
just test "${1:-quick}" || status=$?
snapshot > "$after"

if diff -q "$before" "$after" > /dev/null; then
    echo "OK: the suite wrote nothing under $HOME."
    exit "$status"
fi

echo "FAIL: the test suite changed files under \$HOME."
echo "Every test must inject its data home (see AGENTS.md, Hermeticity)."
diff "$before" "$after" | head -40
exit 1
