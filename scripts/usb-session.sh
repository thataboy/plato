#! /bin/sh

# Run from RAM after Plato has closed its files and exited.
STATUS_FIFO=/tmp/nickel-hardware-status
[ -p "$STATUS_FIFO" ] || exit 1
exec 3<>"$STATUS_FIFO"

grep -q ' /mnt/onboard ' /proc/mounts || exit 3
if ! ./usb-enable.sh; then
	if ! grep -q ' /mnt/onboard ' /proc/mounts; then
		./usb-disable.sh --recover || exit 3
	fi
	exit 1
fi

while IFS= read -r event <&3; do
	case "$event" in
		"usb plug remove"|"usb ac remove")
			./usb-disable.sh || exit 3
			exit 0
			;;
	esac
done

# Stop the export before remounting if the event stream fails.
./usb-disable.sh || exit 3
exit 1
