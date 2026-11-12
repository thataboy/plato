#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT=${PLATO_DIAG_OUT:-/mnt/us/plato-diagnostics}
cd "$HERE/.."
mkdir -p "$OUT"
LOCK="$OUT/.render-smoke-lock"
if ! mkdir "$LOCK" 2>/dev/null; then
    echo 'Rendering test already running or its previous lock remains.' >&2
    exit 1
fi
CHILD=
WATCH=
cleanup() {
    [ -z "$WATCH" ] || kill "$WATCH" 2>/dev/null || :
    [ -z "$CHILD" ] || kill -KILL "$CHILD" 2>/dev/null || :
    rmdir "$LOCK" 2>/dev/null || :
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' HUP TERM
REPORT="$OUT/render-smoke-$(date -u '+%Y%m%dT%H%M%SZ')-$$.txt"
PART="$REPORT.partial"
{
    echo 'Launching Plato static ARMv7 rendering test.'
    "$HERE/plato-render-smoke" &
    CHILD=$!
    (
        sleep 40
        if kill -KILL "$CHILD" 2>/dev/null; then
            echo 'Render watchdog stopped the process after 40 seconds.'
        fi
    ) &
    WATCH=$!
    status=0
    wait "$CHILD" || status=$?
    CHILD=
    kill "$WATCH" 2>/dev/null || :
    wait "$WATCH" 2>/dev/null || :
    WATCH=
    printf '\nRender test exit status: %s\n' "$status"
} > "$PART" 2>&1
mv "$PART" "$REPORT"
