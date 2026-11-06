#! /bin/sh

set -e

# Status 2 means no filesystems were detached and no export was attempted.
grep -q ' /mnt/onboard ' /proc/mounts || exit 2
if grep -qE '^g_(file|mass)_storage ' /proc/modules ||
   [ -n "$(cat /sys/kernel/config/usb_gadget/g1/UDC 2>/dev/null)" ]; then
	echo "A USB storage gadget is already active."
	exit 2
fi

DISK=/dev/mmcblk
PARTITIONS="${DISK}0p3"

[ -e "${DISK}1p1" ] && PARTITIONS="${PARTITIONS},${DISK}1p1"

sync || exit 2
umount /mnt/onboard || exit 2
if grep -q ' /mnt/sd ' /proc/mounts; then
	umount /mnt/sd
fi

VENDOR_ID=0x2237
PRODUCT_ID=${PRODUCT_ID:-"0x6666"}
FIRMWARE_VERSION=${FIRMWARE_VERSION:-"9.8.76543"}
SERIAL_NUMBER=${SERIAL_NUMBER:-"N666999666999"}

legacy() {
	ANDROID_MODULE=/drivers/${PLATFORM}/g_mass_storage.ko

	if [ -e "$ANDROID_MODULE" ]; then
		MODULE_PARAMETERS="idVendor=${VENDOR_ID} idProduct=${PRODUCT_ID} iManufacturer=Kobo iProduct=eReader-${FIRMWARE_VERSION} iSerialNumber=${SERIAL_NUMBER}"
		# shellcheck disable=SC2086
		insmod "$ANDROID_MODULE" file="$PARTITIONS" stall=1 removable=1 $MODULE_PARAMETERS
	else
		GADGETS=/drivers/${PLATFORM}/usb/gadget

		case "$PLATFORM" in
			mx6[su]ll-ntx)
				MODULE_PARAMETERS="idVendor=${VENDOR_ID} idProduct=${PRODUCT_ID} iManufacturer=Kobo iProduct=eReader-${FIRMWARE_VERSION} iSerialNumber=${SERIAL_NUMBER}"
				insmod "$GADGETS"/configfs.ko || :
				insmod "$GADGETS"/libcomposite.ko || :
				insmod "$GADGETS"/usb_f_mass_storage.ko || :
				;;
			*)
				MODULE_PARAMETERS="vendor=${VENDOR_ID} product=${PRODUCT_ID} vendor_id=Kobo product_id=eReader-${FIRMWARE_VERSION} SN=${SERIAL_NUMBER}"
				if [ "$PLATFORM" != mx6sl-ntx ] ; then
					insmod "$GADGETS"/arcotg_udc.ko || :
					sleep 2
				fi
				;;
		esac

		# shellcheck disable=SC2086
		insmod "$GADGETS"/g_file_storage.ko file="$PARTITIONS" stall=1 removable=1 $MODULE_PARAMETERS
	fi

	sleep 1
}

mtk() {
	DIR=/sys/kernel/config/usb_gadget/g1

	mkdir -p "$DIR"/strings/0x409
	echo "$VENDOR_ID" > "$DIR"/idVendor
	echo "$PRODUCT_ID" > "$DIR"/idProduct
	echo "$SERIAL_NUMBER" > "$DIR"/strings/0x409/serialnumber
	echo Kobo > "$DIR"/strings/0x409/manufacturer
	echo "eReader-${FIRMWARE_VERSION}" > "$DIR"/strings/0x409/product

	mkdir -p "$DIR"/configs/c.1/strings/0x409
	echo KOBOeReader > "$DIR"/configs/c.1/strings/0x409/configuration

	mkdir -p "$DIR"/functions/mass_storage.0/lun.0
	echo "${DISK}0p12" > "$DIR"/functions/mass_storage.0/lun.0/file
	ln -s "$DIR"/functions/mass_storage.0 "$DIR"/configs/c.1
	echo 11211000.usb > "$DIR"/UDC
}

case "$PLATFORM" in
	mt8113t-ntx)
		mtk
		;;
	*)
		legacy
		;;
esac
