#!/usr/bin/env bash
# Fails if any Rust file under crates/ exceeds MAX_LINES unless the path is
# listed in .file-size-whitelist. The whitelist documents existing files
# slated for decomposition; entries are removed as files are split up.
#
# Intent: no *new* file may be born over the ceiling. Existing offenders are
# grandfathered — the whitelist shrinks over time, never grows without a plan.

set -euo pipefail

MAX_LINES=${MAX_LINES:-1500}
WHITELIST_FILE="${WHITELIST_FILE:-.file-size-whitelist}"

cd "$(git rev-parse --show-toplevel)"

if [[ ! -f "$WHITELIST_FILE" ]]; then
    echo "error: $WHITELIST_FILE not found at repo root" >&2
    exit 2
fi

mapfile -t whitelist < <(grep -vE '^\s*(#|$)' "$WHITELIST_FILE" | awk '{print $1}')

is_whitelisted() {
    local target=$1
    for entry in "${whitelist[@]}"; do
        [[ "$entry" == "$target" ]] && return 0
    done
    return 1
}

violations=0
while IFS= read -r file; do
    loc=$(wc -l < "$file")
    if (( loc > MAX_LINES )); then
        if ! is_whitelisted "$file"; then
            printf 'NEW OFFENDER: %s (%d lines, limit %d)\n' "$file" "$loc" "$MAX_LINES" >&2
            violations=$((violations + 1))
        fi
    fi
done < <(find crates -name "*.rs" -not -path "*/target/*")

stale=0
for entry in "${whitelist[@]}"; do
    if [[ ! -f "$entry" ]]; then
        printf 'STALE WHITELIST: %s no longer exists — remove from %s\n' "$entry" "$WHITELIST_FILE" >&2
        stale=$((stale + 1))
    else
        current_loc=$(wc -l < "$entry")
        if (( current_loc <= MAX_LINES )); then
            printf 'STALE WHITELIST: %s is now %d lines (<= %d) — remove from %s\n' \
                "$entry" "$current_loc" "$MAX_LINES" "$WHITELIST_FILE" >&2
            stale=$((stale + 1))
        fi
    fi
done

if (( violations > 0 )) || (( stale > 0 )); then
    echo "" >&2
    echo "File-size check failed: $violations new offender(s), $stale stale entry/entries." >&2
    exit 1
fi

echo "File-size check passed. ${#whitelist[@]} file(s) whitelisted for decomposition (limit: $MAX_LINES)."
