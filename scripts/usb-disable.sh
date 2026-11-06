#! /bin/sh

set -e

RECOVER=${1:-}

grep -q ' /mnt/onboard ' /proc/mounts && exit 1

DISK=/dev/mmcblk

legacy() {
	PARTITION=${DISK}0p3
	LOADED_MODULE=$(grep -oE '^g_(file|mass)_storage\b' /proc/modules || :)

	case "$LOADED_MODULE" in
		g_file_storage)
			rmmod g_file_storage

			case "$PLATFORM" in
				mx6[su]ll-ntx)
					rmmod usb_f_mass_storage || :
					rmmod libcomposite || :
					rmmod configfs || :
					;;
				*)
					[ "$PLATFORM" != mx6sl-ntx ] && rmmod arcotg_udc || :
					;;
			esac
			;;
		g_mass_storage)
			rmmod g_mass_storage
			;;
		*)
			[ "$RECOVER" = --recover ] || exit 1
			;;
	esac

	sleep 1
}

mtk() {
	PARTITION=${DISK}0p12
	DIR=/sys/kernel/config/usb_gadget/g1

	if [ -e "$DIR"/UDC ]; then
		if [ -n "$(cat "$DIR"/UDC)" ]; then
			echo "" > "$DIR"/UDC
		fi
	else
		[ "$RECOVER" = --recover ] || exit 1
	fi

	rm -f "$DIR"/configs/c.1/mass_storage.0
	for entry in configs/c.1/strings/0x409 configs/c.1 functions/mass_storage.0 strings/0x409; do
		[ ! -d "$DIR/$entry" ] || rmdir "$DIR/$entry" || :
	done
	[ ! -d "$DIR" ] || rmdir "$DIR" || :
}

case "$PLATFORM" in
	mt8113t-ntx)
		mtk
		;;
	*)
		legacy
		;;
esac

MOUNT_ARGS="noatime,nodiratime,shortname=mixed,utf8"

dosfsck -a -w "$PARTITION" || dosfsck -a -w "$PARTITION"
mount -o "$MOUNT_ARGS" -t vfat "$PARTITION" /mnt/onboard

PARTITION=${DISK}1p1

if [ -e "$PARTITION" ] && ! grep -q ' /mnt/sd ' /proc/mounts; then
	mount -o "$MOUNT_ARGS" -t vfat "$PARTITION" /mnt/sd
fi
