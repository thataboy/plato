#! /bin/sh

set -e

method=${1:-"fast"}

[ -e libs -a $# -eq 0 ] && method=skip

case "$method" in
	fast)
		./download.sh 'libs/*'
		cd thirdparty
		./download.sh mupdf
		cd ..
		;;

	slow)
		shift
		cd thirdparty
		./download.sh "$@"
		./build.sh "$@"
		cd ..

		[ -e libs ] || mkdir libs

		cp thirdparty/zlib/libz.so libs
		cp thirdparty/bzip2/libbz2.so libs

		cp thirdparty/libpng/.libs/libpng16.so libs
		cp thirdparty/libjpeg/.libs/libjpeg.so libs
		cp thirdparty/openjpeg/build/bin/libopenjp2.so libs
		cp thirdparty/jbig2dec/.libs/libjbig2dec.so libs

		cp thirdparty/freetype2/objs/.libs/libfreetype.so libs
		cp thirdparty/harfbuzz/build/src/libharfbuzz.so libs

		cp thirdparty/gumbo/.libs/libgumbo.so libs
		cp thirdparty/djvulibre/libdjvu/.libs/libdjvulibre.so libs
		cp thirdparty/mupdf/build/release/libmupdf.so libs
		;;

	skip)
		;;
	*)
		printf "Unknown build method: %s.\n" "$method" 1>&2
		exit 1
		;;
esac

# Linker names may be absent when reusing previously downloaded libraries.
for name in z bz2 png16 jpeg openjp2 jbig2dec freetype harfbuzz gumbo djvulibre; do
	link="libs/lib${name}.so"
	[ -e "$link" ] && continue
	set -- "$link".*
	if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
		printf 'Expected one versioned library for %s; rebuild with ./build.sh slow.\n' "$link" >&2
		exit 1
	fi
	ln -sf "${1##*/}" "$link"
done

cd mupdf_wrapper
./build-kobo.sh
cd ..

cargo build --release --target=arm-unknown-linux-gnueabihf -p plato
