#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT=${PLATO_DIAG_OUT:-/mnt/us/plato-diagnostics}
mkdir -p "$OUT"
LOCK="$OUT/.arm-probe-lock"
if ! mkdir "$LOCK" 2>/dev/null; then
    echo 'ARM probe already running or its previous lock remains.' >&2
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
REPORT="$OUT/arm-probe-$(date -u '+%Y%m%dT%H%M%SZ')-$$.txt"
PART="$REPORT.partial"
{
    echo 'Launching static ARMv7 Rust probe.'
    "$HERE/arm-probe" &
    CHILD=$!
    (
        sleep 40
        if kill -KILL "$CHILD" 2>/dev/null; then
            echo 'ARM probe watchdog stopped the process after 40 seconds.'
        fi
    ) &
    WATCH=$!
    status=0
    wait "$CHILD" || status=$?
    CHILD=
    kill "$WATCH" 2>/dev/null || :
    wait "$WATCH" 2>/dev/null || :
    WATCH=
    printf '\nARM probe exit status: %s\n' "$status"
} > "$PART" 2>&1
mv "$PART" "$REPORT"
