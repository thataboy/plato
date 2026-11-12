#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Read-only inventory; the only device writes are report files and a run lock.
set -u
umask 077

OUT=${PLATO_DIAG_OUT:-/mnt/us/plato-diagnostics}
# Test fixtures replace absolute hardware paths and disable device commands.
ROOT=${PLATO_DIAG_FIXTURE_ROOT:-}
case "$OUT" in /*) ;; *) echo 'Output path must be absolute.' >&2; exit 2 ;; esac
case "$ROOT" in ''|/*) ;; *) echo 'Fixture root must be absolute.' >&2; exit 2 ;; esac
mkdir -p "$OUT" || exit 1
LOCK="$OUT/.inventory-lock"
if ! mkdir "$LOCK" 2>/dev/null; then
    echo 'Inventory already running. If a previous run was interrupted, remove .inventory-lock over USB.' >&2
    exit 1
fi
STAMP=$(date -u '+%Y%m%dT%H%M%SZ')
REPORT="$OUT/inventory-$STAMP-$$.txt"
PART="$REPORT.partial"
CHILD=
WATCH=
cleanup() {
    [ -z "$CHILD" ] || kill "$CHILD" 2>/dev/null || :
    [ -z "$WATCH" ] || kill "$WATCH" 2>/dev/null || :
    rm -f "$LOCK/command.txt" "$LOCK/timed-out"
    rmdir "$LOCK" 2>/dev/null || :
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

# Fixed, short read commands only. A hung command is killed after three seconds.
run() {
    printf '\n$ %s\n' "$*"
    if ! command -v "$1" >/dev/null 2>&1; then
        printf '[unavailable]\n'
        return
    fi
    rm -f "$LOCK/timed-out"
    "$@" > "$LOCK/command.txt" 2>&1 &
    CHILD=$!
    (
        sleep 3
        : > "$LOCK/timed-out"
        kill -KILL "$CHILD" 2>/dev/null || :
    ) &
    WATCH=$!
    wait "$CHILD" 2>/dev/null
    result=$?
    CHILD=
    kill "$WATCH" 2>/dev/null || :
    wait "$WATCH" 2>/dev/null || :
    WATCH=
    head -c 8192 "$LOCK/command.txt"
    printf '\n[exit %s' "$result"
    [ ! -f "$LOCK/timed-out" ] || printf '; timed out after 3 seconds'
    printf ']\n'
}
read_attr() {
    path=$1
    printf '\n%s: ' "$path"
    if [ -r "$ROOT$path" ]; then
        run head -c 1024 "$ROOT$path"
    else
        printf '[unavailable]\n'
    fi
}
inventory() {
    printf 'Plato Kindle inventory v1\nUTC: %s\n' "$STAMP"
    printf 'Owner-reported baseline: Voyage, firmware 5.10.3, KUAL installed.\n'
    printf 'Read-only inventory. No display, input, network, or service settings changed.\n'
    printf 'Missing commands/attributes are expected and are recorded below.\n'
    printf 'No serial, MAC, SSID, account data, logs, books, or waveform files requested.\n'
    if [ -z "$ROOT" ]; then
        run uname -srm
    else
        printf 'Fixture mode: stock commands disabled.\n'
    fi
    read_attr /etc/prettyversion
    read_attr /etc/version
    printf '\nCPU (selected fields only):\n'
    run awk '/^(Processor|model name|Features|CPU architecture|CPU implementer|CPU variant|CPU part|CPU revision|Hardware)[[:space:]]*:/ {print}' "$ROOT/proc/cpuinfo"
    printf '\nMemory (kB):\n'
    run awk '/^(MemTotal|MemFree|MemAvailable|Buffers|Cached|SwapTotal|SwapFree):/ {print}' "$ROOT/proc/meminfo"
    printf '\nInput devices (IDs, names, handlers, capabilities; no unique IDs or physical paths):\n'
    run awk '/^[INHB]:/ {print}' "$ROOT/proc/bus/input/devices"
    for name in name virtual_size bits_per_pixel stride rotate modes; do
        read_attr "/sys/class/graphics/fb0/$name"
    done
    printf '\nInput node availability:\n'
    for node in "$ROOT"/dev/input/event*; do
        [ -e "$node" ] || continue
        printf '%s\n' "${node#"$ROOT"}"
    done
    for path in \
        /sys/class/backlight/max77696-bl/brightness \
        /sys/class/backlight/max77696-bl/max_brightness \
        /sys/devices/system/wario_battery/wario_battery0/battery_capacity \
        /sys/devices/system/wario_charger/wario_charger0/charging \
        /sys/devices/system/wario_hall/wario_hall0/hall_enable; do
        read_attr "$path"
    done
    printf '\nCommand availability (does not establish service connectivity):\n'
    for cmd in lipc-get-prop lipc-wait-event lipc-probe initctl fbset evtest eips timeout sshd dropbear; do
        if command -v "$cmd" >/dev/null 2>&1; then
            printf '%s: present\n' "$cmd"
        else
            printf '%s: not in PATH\n' "$cmd"
        fi
    done
    printf '\nUSBNetwork installation locations (existence only; no configuration read):\n'
    for path in /mnt/us/usbnet /mnt/us/extensions/usbnet /mnt/us/extensions/usbnetwork; do
        if [ -d "$ROOT$path" ]; then printf '%s: present\n' "$path"; else printf '%s: absent\n' "$path"; fi
    done
    if [ -z "$ROOT" ]; then
        printf '\nSelected stock service status:\n'
        for service in framework powerd deviced volumd; do run initctl status "$service"; done
        printf '\nSelected LIPC read-only properties:\n'
        for prop in state battLevel isCharging flIntensity preventScreenSaver; do
            run lipc-get-prop com.lab126.powerd "$prop"
        done
        for prop in fsrkeypadEnable fsrkeypadPrevEnable fsrkeypadNextEnable; do
            run lipc-get-prop com.lab126.deviced "$prop"
        done
    fi
    printf '\nEND OF INVENTORY\n'
}
inventory > "$PART" 2>&1
if mv "$PART" "$REPORT"; then
    printf 'Inventory saved: %s\n' "$REPORT"
else
    exit 1
fi
