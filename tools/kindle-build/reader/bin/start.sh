#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
umask 077
LOCK=/var/tmp/plato-voyage-reader.lock

# Execute from internal storage so no launcher descriptor pins /mnt/us.
if [ "$0" != "$LOCK/launcher" ]; then
    HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
    mode=${1:-stock}
    case "$mode" in stock|bounded) ;; *) exit 2 ;; esac
    if ! mkdir "$LOCK" 2>/dev/null; then
        echo 'A Plato Voyage session is already running.'
        exit 1
    fi
    if ! cp "$HERE/start.sh" "$LOCK/launcher"; then
        rm -f "$LOCK/launcher"
        rmdir "$LOCK"
        exit 1
    fi
    cd /
    exec /bin/sh "$LOCK/launcher" "$HERE" "$mode"
fi

HERE=$1
PLATO_VOYAGE_POWER=$2
export PLATO_VOYAGE_POWER
export PLATO_VOYAGE_SESSION=1
export PLATO_VOYAGE_USB_REQUEST="$LOCK/connect-usb"

mounted() {
    awk -v path="$1" '$2 == path { found = 1 } END { exit !found }' /proc/mounts
}

storage_ready() {
    mounted /mnt/us && { [ "$BASE_MOUNT" = 0 ] || mounted /mnt/base-us; }
}

# Some firmware keeps the public mount visible while removing the backing
# mount. A populated mass-storage LUN also means stock still owns the volume.
storage_exported() {
    for lun in /sys/devices/platform/*/gadget/lun*/file \
               /sys/class/android_usb/android0/f_mass_storage/lun*/file; do
        [ -r "$lun" ] || continue
        if read -r backing < "$lun" && [ -n "$backing" ]; then
            return 0
        fi
    done
    return 1
}

release_lock() {
    rm -f "$LOCK/launcher"
    rmdir "$LOCK"
}

if [ "${3:-}" = wait-usb ]; then
    BASE_MOUNT=$4
    trap 'release_lock' EXIT
    trap 'exit 1' HUP INT TERM
    # Require evidence of export before accepting a subsequent mount. This
    # prevents restarting during the interval before stock unmounts storage.
    attempts=0
    while storage_ready && ! storage_exported; do
        [ "$attempts" -lt 30 ] || exit 0
        attempts=$((attempts + 1))
        sleep 1
    done
    while ! storage_ready || storage_exported; do
        sleep 1
    done
    # Let stock finish restoring the UI and wait for the active power state.
    sleep 2
    attempts=0
    until storage_ready && ! storage_exported &&
        [ "$(timeout 2 lipc-get-prop com.lab126.powerd state 2>/dev/null)" = active ]; do
        [ "$attempts" -lt 30 ] || exit 0
        attempts=$((attempts + 1))
        sleep 1
    done
    trap - EXIT HUP INT TERM
    exec /bin/sh "$LOCK/launcher" "$HERE" "$PLATO_VOYAGE_POWER"
fi

trap 'release_lock' EXIT
cd "$HERE/.."
OUT=${PLATO_DIAG_OUT:-/mnt/us/plato-diagnostics}
mkdir -p "$OUT"
BASE_MOUNT=0
if mounted /mnt/base-us; then BASE_MOUNT=1; fi
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
    usb_request=
    if [ -f "$PLATO_VOYAGE_USB_REQUEST" ]; then
        usb_request=$(cat "$PLATO_VOYAGE_USB_REQUEST")
        echo "Stock USB handoff requested: $usb_request"
    fi
    rm -f "$LOCK/screen" "$LOCK/prevent" "$LOCK/state" "$PLATO_VOYAGE_USB_REQUEST" || result=1
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
    if [ "$usb_request" = reenter ] && [ "$result" = 0 ] && [ -n "$VOLUMD" ]; then
        # Ask stock to re-enter USB storage after a host eject. All
        # reader files and report descriptors are already closed.
        for path in /sys/bus/platform/drivers/max77696-uic/*/vbvolt; do
            [ -r "$path" ] || continue
            if read -r vbus < "$path" && [ "$vbus" = 0 ]; then
                usb_request=
            fi
            break
        done
        if [ "$usb_request" = reenter ]; then
            run_for 4 lipc-send-event com.lab126.hal usbPlugIn &&
                run_for 4 lipc-send-event com.lab126.hal usbConfigured || result=1
        fi
    fi
    if [ "$result" = 0 ] && [ -n "$VOLUMD" ] &&
        { [ "$usb_request" = connect ] || [ "$usb_request" = reenter ]; }; then
        /bin/sh "$LOCK/launcher" "$HERE" "$PLATO_VOYAGE_POWER" wait-usb "$BASE_MOUNT" </dev/null &
    else
        release_lock
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
# Validate hardware identity and firmware before pausing any stock service.
run_for 10 "$HERE/plato" --check-device

# Kindle firmware can supply either a TZif file or a POSIX timezone string.
# Set it before launching the reader so every local-time display uses it.
for tz_file in /var/local/system/tz /etc/localtime; do
    [ -s "$tz_file" ] || continue
    if [ "$(head -c 4 "$tz_file")" = TZif ]; then
        TZ=":$tz_file"
    else
        zone=$(cat "$tz_file")
        [ -n "$zone" ] || continue
        TZ=$zone
    fi
    export TZ
    break
done
echo "Timezone source: ${TZ:-system default}. Stock date: $(date +%Y-%m-%dT%H:%M:%S%z)"

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
