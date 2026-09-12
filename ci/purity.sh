#!/usr/bin/env bash
# The purity grep of architecture.md §2:
#   - bevy appears in no dependency tree except pool-app's;
#   - ort appears only in pool-ai, and never in its default (feature-off) graph;
#   - rand appears nowhere: all randomness is pool-rng.
#
# Reads the workspace's own crate list from cargo metadata, so it needs no maintenance as crates land.
set -euo pipefail

fail=0

crates=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' | sort)

has_crate() {
    printf '%s\n' "$crates" | grep -qx "$1"
}

for crate in $crates; do
    tree=$(cargo tree -p "$crate" -e normal --prefix none)

    if [ "$crate" != "pool-app" ] && grep -qE '^bevy v' <<<"$tree"; then
        echo "purity: bevy leaks into $crate"
        fail=1
    fi
    if [ "$crate" != "pool-app" ] && grep -qE '^rand v' <<<"$tree"; then
        echo "purity: rand appears in $crate (all randomness is pool-rng)"
        fail=1
    fi
    if [ "$crate" != "pool-ai" ] && grep -qE '^ort v' <<<"$tree"; then
        echo "purity: ort appears in $crate"
        fail=1
    fi
done

if has_crate pool-ai; then
    tree=$(cargo tree -p pool-ai -e normal --prefix none)
    if grep -qE '^ort v' <<<"$tree"; then
        echo "purity: ort is in pool-ai's default graph (it must sit behind the non-default ort feature)"
        fail=1
    fi
fi

if [ "$fail" -eq 0 ]; then
    echo "purity: ok"
fi
exit "$fail"
