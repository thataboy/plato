#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$HERE/.."
export PLATO_VOYAGE_SESSION=1
PLATO_VOYAGE_POWER=${1:-stock}
case "$PLATO_VOYAGE_POWER" in stock|bounded) ;; *) exit 2 ;; esac
export PLATO_VOYAGE_POWER
OUT=${PLATO_DIAG_OUT:-/mnt/us/plato-diagnostics}
mkdir -p "$OUT"
umask 077
LOCK=/var/tmp/plato-voyage-reader.lock
if ! mkdir "$LOCK" 2>/dev/null; then
    echo 'A Plato Voyage session is already running.' >&2
    exit 1
fi
export PLATO_VOYAGE_USB_REQUEST="$LOCK/connect-usb"
REPORT="$OUT/reader-$(date -u '+%Y%m%dT%H%M%SZ')-$$.txt"
PART="$REPORT.partial"
exec > "$PART" 2>&1

CHILD=
WATCH=
AWESOME=
VOLUMD=
PREVENT=
STOCK_SAVED=0

run_for() {
    seconds=$1
    shift
    "$@" &
    CHILD=$!
    (
        sleep "$seconds"
        if kill -KILL "$CHILD" 2>/dev/null; then
            printf 'Watchdog stopped %s after %s seconds.\n' "$1" "$seconds"
        fi
    ) &
    WATCH=$!
    status=0
    wait "$CHILD" || status=$?
    CHILD=
    kill -KILL "$WATCH" 2>/dev/null || :
    wait "$WATCH" 2>/dev/null || :
    WATCH=
    return "$status"
}

cleanup() {
    result=$?
    trap - EXIT
    trap '' HUP INT TERM
    [ -z "$WATCH" ] || kill -KILL "$WATCH" 2>/dev/null || :
    if [ -n "$CHILD" ]; then
        kill -KILL "$CHILD" 2>/dev/null || :
        wait "$CHILD" 2>/dev/null || :
    fi
    restore_screen=$STOCK_SAVED
    if [ "$restore_screen" = 1 ] && [ "$PLATO_VOYAGE_POWER" = stock ]; then
        if ! run_for 4 lipc-get-prop com.lab126.powerd state > "$LOCK/state" ||
            [ "$(cat "$LOCK/state")" != active ]; then
            restore_screen=0
            echo 'Leaving the stock screensaver on the panel.'
        fi
    fi
    if [ "$restore_screen" = 1 ]; then
        if run_for 15 "$HERE/plato" --restore-stock-screen "$LOCK/screen"; then
            echo 'Stock screen restored.'
        else
            echo 'Stock screen refresh failed. Resuming Kindle UI.'
            result=1
        fi
    fi
    if [ -n "$AWESOME" ]; then
        if kill -CONT "$AWESOME"; then echo 'Kindle window manager resumed.'; else result=1; fi
    fi
    if [ -n "$PREVENT" ]; then
        if ! run_for 4 lipc-set-prop -i com.lab126.powerd preventScreenSaver "$PREVENT"; then
            echo 'Could not restore the stock screensaver setting.'
            result=1
        fi
    fi
    usb_reconnect=0
    if [ -f "$PLATO_VOYAGE_USB_REQUEST" ]; then
        usb_reconnect=1
        echo 'Manual stock USB reconnect requested.'
    fi
    rm -f "$LOCK/screen" "$LOCK/prevent" "$LOCK/state" "$PLATO_VOYAGE_USB_REQUEST" || result=1
    rmdir "$LOCK" || result=1
    if [ -n "$VOLUMD" ]; then
        if ! run_for 8 sync; then
            echo 'Sync did not complete before handing storage back to stock.'
            result=1
        fi
        echo 'Reader files closed. Releasing Kindle USB storage service.'
    fi
    printf 'Reader session exit status: %s\nEND OF VOYAGE READER SESSION\n' "$result"
    if ! mv "$PART" "$REPORT"; then result=1; fi
    # Close the log and leave the exported filesystem before resuming volumd.
    cd /
    exec >/dev/null 2>&1
    if [ -n "$VOLUMD" ]; then
        kill -CONT "$VOLUMD" || result=1
    fi
    if [ "$usb_reconnect" = 1 ] && [ "$result" = 0 ] && [ -n "$VOLUMD" ]; then
        # Ask stock to re-enter USB storage after a host eject. All
        # reader files and report descriptors are already closed.
        for path in /sys/bus/platform/drivers/max77696-uic/*/vbvolt; do
            [ -r "$path" ] || continue
            if read -r vbus < "$path" && [ "$vbus" = 0 ]; then
                exit 0
            fi
            break
        done
        run_for 4 lipc-send-event com.lab126.hal usbPlugIn &&
            run_for 4 lipc-send-event com.lab126.hal usbConfigured || result=1
    fi
    exit "$result"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' HUP TERM

active_pid() {
    pid=$(pidof "$1") || return 1
    case "$pid" in ''|*[!0-9]*) return 1 ;; esac
    state=$(awk '/^State:/ { print $2 }' "/proc/$pid/status") || return 1
    case "$state" in R|S|D) printf '%s\n' "$pid" ;; *) return 1 ;; esac
}

echo "Plato Voyage portrait reader alpha. Power mode: $PLATO_VOYAGE_POWER."
version=$(cat /etc/prettyversion.txt 2>/dev/null || :)
case "$version" in
    'Kindle 5.10.3'|'Kindle 5.10.3 '*) ;;
    *) echo "Expected stock Kindle firmware 5.10.3, found: $version"; exit 1 ;;
esac

awesome=$(active_pid awesome) || { echo 'Could not identify an active Kindle window manager.'; exit 1; }
volumd=$(active_pid volumd) || { echo 'Could not identify the active USB storage service.'; exit 1; }
run_for 4 lipc-get-prop com.lab126.powerd preventScreenSaver > "$LOCK/prevent"
prevent=$(cat "$LOCK/prevent")
case "$prevent" in 0|1) ;; *) echo 'Unexpected screensaver property.'; exit 1 ;; esac
if [ "$PLATO_VOYAGE_POWER" = bounded ]; then
    PREVENT=$prevent
    run_for 4 lipc-set-prop -i com.lab126.powerd preventScreenSaver 1
elif [ "$prevent" != 0 ]; then
    echo 'Stock screensaver is inhibited by another setting. Use the bounded reader or release that setting first.'
    exit 1
fi
PLATO_VOYAGE_WM_PID=$awesome
export PLATO_VOYAGE_WM_PID
AWESOME=$awesome
kill -STOP "$AWESOME"
VOLUMD=$volumd
kill -STOP "$VOLUMD"

run_for 10 "$HERE/plato" --save-stock-screen "$LOCK/screen"
STOCK_SAVED=1
echo 'Stock screen saved. Starting reader.'
result=0
if [ "$PLATO_VOYAGE_POWER" = bounded ]; then
    run_for 205 "$HERE/plato" || result=$?
else
    "$HERE/plato" &
    CHILD=$!
    wait "$CHILD" || result=$?
    CHILD=
fi
printf 'Plato process exit status: %s\n' "$result"
exit "$result"
