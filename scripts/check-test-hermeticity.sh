#!/usr/bin/env bash
# Prove the test suite writes nothing outside its own temporary directories.
#
# A test that reads `dirs::config_dir()` instead of an injected home writes
# into `~/.config/crucible`. That passes CI, passes locally, and silently
# edits the developer's real configuration. One such leak wrote 7,503 files
# over seven months. The compiler cannot see it; only this can.
#
# Two places are watched, because a leak has two destinations.
#
# The developer's own directories, watched recursively. This is the original
# case: a test that finds the real home.
#
# The SYSTEM TEMPORARY DIRECTORY, watched for the daemon's state filenames
# only. `/tmp` is shared by every user on the machine and is not cleaned
# between runs, so a test that hardcodes `/tmp` instead of a `TempDir` writes
# state that collides across runs and across users. This half exists because
# `test_context()` in `rpc/dispatch.rs` once left a `/tmp/projects.json`
# behind, and later runs read it back as real state.
#
# KNOW WHAT THIS PROVES. It observes what a run actually wrote. It cannot see
# a hardcoded path that the current suite never exercises: the leak above was
# re-introduced verbatim and this gate still said OK, because no test in the
# tier writes through that context. A pass means "this run leaked nothing",
# never "no test can leak". The durable fix for that class is an API that
# cannot be handed a shared path in the first place -- a helper returning the
# `TempDir` guard rather than taking a directory argument. This gate catches
# the leak that fires; the type system has to catch the one that sleeps.
#
# Watching all of the temp directory would be useless — every process on the
# box writes there. So the temp half checks EXACT NAMES: the files the daemon
# persists. Add a name here whenever the daemon learns to persist another.
set -euo pipefail

home_roots=("$HOME/.config/crucible" "$HOME/.local/share/crucible" "$HOME/.crucible")

# The daemon's persistent state files. A `RegistryStore` also writes a
# `<file>.lock` sidecar beside each, so both spellings are listed.
state_files=(
    projects.json projects.json.lock
    kilns.json kilns.json.lock
    llm.json llm.json.lock
)

tmp_root="${TMPDIR:-/tmp}"

before=$(mktemp) && after=$(mktemp)
trap 'rm -f "$before" "$after"' EXIT

# `if`, not `[ -e ... ] && find`. Under `set -e` the `&&` form returns 1 from
# the last loop iteration when that path is absent, which aborts the whole
# script before it compares anything -- and an aborted gate still exits
# non-zero, so it looks exactly like a caught leak. `return 0` makes the
# function's status its own, never the last test's.
snapshot() {
    {
        for root in "${home_roots[@]}"; do
            if [ -e "$root" ]; then
                find "$root" -printf '%p %T@ %s\n' 2>/dev/null
            fi
        done
        # Exact names at the temp root, never a recursive walk: a test's own
        # `TempDir` lives under here too, and that is the correct thing to do.
        for name in "${state_files[@]}"; do
            if [ -e "$tmp_root/$name" ]; then
                find "$tmp_root/$name" -maxdepth 0 -printf '%p %T@ %s\n' 2>/dev/null
            fi
        done
    } | sort
    return 0
}

snapshot > "$before"

# `CRU_HERMETIC_CMD` replaces the suite, so this gate can be red-proofed
# without a build: run it with a command that deliberately leaks and check
# that the gate FAILS. A gate nobody has watched fail proves nothing.
if [ -n "${CRU_HERMETIC_CMD:-}" ]; then
    echo "Running \$CRU_HERMETIC_CMD instead of the suite..."
    status=0
    eval "$CRU_HERMETIC_CMD" || status=$?
else
    echo "Running the test suite..."
    status=0
    just test "${1:-quick}" || status=$?
fi

snapshot > "$after"

if diff -q "$before" "$after" > /dev/null; then
    echo "OK: the suite wrote nothing under \$HOME or into $tmp_root."
    exit "$status"
fi

echo "FAIL: the test suite wrote outside its own temporary directories."
echo "Every test must inject its data home and use a TempDir, never a"
echo "hardcoded path (see AGENTS.md, Hermeticity)."
diff "$before" "$after" | head -40
exit 1
