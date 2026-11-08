#!/bin/sh
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/plato-diag-test.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
mkdir -p "$TMP/root/proc/bus/input" "$TMP/root/etc" "$TMP/bin"
printf 'Hardware : Wario\nSerial : SECRET_CPU_SERIAL\nFeatures : neon vfp\n' > "$TMP/root/proc/cpuinfo"
printf 'MemTotal: 512000 kB\nMemFree: 200000 kB\n' > "$TMP/root/proc/meminfo"
printf 'N: Name="touch"\nU: Uniq=SECRET_INPUT_ID\nP: Phys=SECRET_PHYS\nH: Handlers=event1\nB: EV=b\n' > "$TMP/root/proc/bus/input/devices"
printf 'Kindle 5.10.3\n' > "$TMP/root/etc/prettyversion"
export PLATO_DIAG_FIXTURE_ROOT="$TMP/root"
export PLATO_DIAG_OUT="$TMP/reports"
sh -n "$HERE/extension/bin/inventory.sh"
sh -n "$HERE/package.sh"
sh "$HERE/extension/bin/inventory.sh"
set -- "$TMP/reports"/*.txt
REPORT=$1
grep -q 'END OF INVENTORY' "$REPORT"
grep -q 'Wario' "$REPORT"
grep -q '\[unavailable\]' "$REPORT"
if grep -q 'SECRET_' "$REPORT"; then echo 'Identifier filtering failed' >&2; exit 1; fi
[ ! -d "$TMP/reports/.inventory-lock" ]
mkdir "$TMP/reports/.inventory-lock"
if sh "$HERE/extension/bin/inventory.sh" >/dev/null 2>&1; then
    echo 'Concurrent-run lock failed' >&2; exit 1
fi
rmdir "$TMP/reports/.inventory-lock"
# A mock awk stalls only the CPU read; exec makes the timed-out PID the sleeper.
REAL_AWK=$(command -v awk)
printf '#!/bin/sh\ncase "$*" in *cpuinfo*) exec sleep 20 ;; esac\nexec "%s" "$@"\n' "$REAL_AWK" > "$TMP/bin/awk"
chmod +x "$TMP/bin/awk"
PLATO_DIAG_OUT="$TMP/timeout-reports" PATH="$TMP/bin:$PATH" sh "$HERE/extension/bin/inventory.sh"
set -- "$TMP/timeout-reports"/*.txt
grep -q 'timed out after 3 seconds' "$1"
grep -q 'END OF INVENTORY' "$1"
[ ! -d "$TMP/timeout-reports/.inventory-lock" ]
printf 'Host fixture checks passed.\n'
