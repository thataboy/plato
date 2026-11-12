#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT=${PLATO_DIAG_OUT:-/mnt/us/plato-diagnostics}
umask 077
mkdir -p "$OUT"
LOCK="$OUT/.display-probe-lock"
# The center probe used a different rectangle and backup name.
if [ -f "$OUT/.display-patch.bin" ]; then
    echo 'Restore the saved center patch with version 0.3.0 before running the top probe.' >&2
    exit 1
fi
BACKUP="$OUT/.display-patch-top.bin"
if ! mkdir "$LOCK" 2>/dev/null; then
    echo 'Display probe is running or its previous lock remains.' >&2
    exit 1
fi
CHILD=
WATCH=
cleanup() {
    [ -z "$WATCH" ] || kill "$WATCH" 2>/dev/null || :
    if [ -n "$CHILD" ]; then
        kill -KILL "$CHILD" 2>/dev/null || :
        wait "$CHILD" 2>/dev/null || :
    fi
    rmdir "$LOCK" 2>/dev/null || :
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' HUP TERM

run_bounded() {
    action=$1
    "$HERE/arm-probe" "$action" "$BACKUP" &
    CHILD=$!
    (
        sleep 20
        if kill -KILL "$CHILD" 2>/dev/null; then
            printf 'Watchdog stopped %s after 20 seconds.\n' "$action"
        fi
    ) &
    WATCH=$!
    status=0
    wait "$CHILD" || status=$?
    CHILD=
    kill "$WATCH" 2>/dev/null || :
    wait "$WATCH" 2>/dev/null || :
    WATCH=
    printf '%s exit status: %s\n' "$action" "$status"
    return "$status"
}

REPORT="$OUT/display-probe-$(date -u '+%Y%m%dT%H%M%SZ')-$$.txt"
PART="$REPORT.partial"
{
    echo 'Plato Voyage grayscale patch and restoration probe'
    if [ -f "$BACKUP" ]; then
        echo 'Found a saved patch from an earlier run. Retrying restoration only.'
    else
        run_bounded --draw-patch || :
    fi
    if [ -f "$BACKUP" ]; then
        if run_bounded --restore-patch; then
            echo 'Patch restored and refresh completed.'
        else
            echo 'Restoration did not complete. Saved patch retained for another attempt.'
        fi
    else
        echo 'No saved patch. Nothing to restore.'
    fi
    echo 'END OF DISPLAY PROBE'
} > "$PART" 2>&1
mv "$PART" "$REPORT"
