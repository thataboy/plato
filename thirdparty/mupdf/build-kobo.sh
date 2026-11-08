#! /bin/sh

set -e

# The bundled MuPDF JPEG configuration uses one-byte booleans. It is not
# ABI-compatible with the external IJG library shipped with Plato, even
# when both report JPEG version 9. Use that library's configured headers.
for header in jpeglib.h jmorecfg.h jconfig.h; do
	if [ ! -f "../libjpeg/$header" ]; then
		printf 'Missing ../libjpeg/%s; build libjpeg before MuPDF.\n' "$header" >&2
		exit 1
	fi
done

[ -e .gitattributes ] && rm -rf .git*

BUILD_KIND=${1:-release}
# Make does not track changes to include flags. Discard JPEG consumers that
# may have been compiled using the bundled configuration in an earlier build.
for source in load-jpeg output-jpeg filter-dct jmemcust; do
	rm -f "build/$BUILD_KIND/source/fitz/$source.o"
done
make verbose=yes generate
make verbose=yes mujs=no tesseract=no extract=no archive=no brotli=no barcode=no commercial=no USE_SYSTEM_LIBS=yes USE_SYSTEM_LIBJPEG=yes SYS_LIBJPEG_CFLAGS=-I../libjpeg OS=kobo build="$BUILD_KIND" libs

arm-linux-gnueabihf-gcc -Wl,--gc-sections -o build/"$BUILD_KIND"/libmupdf.so $(find build/"$BUILD_KIND" -name '*.o' | grep -Ev '(SourceHanSerif-Regular|DroidSansFallbackFull|NotoSerifTangut|color-lcms)') -lm -L../freetype2/objs/.libs -lfreetype -L../harfbuzz/build/src -lharfbuzz -L../gumbo/.libs -lgumbo -L../jbig2dec/.libs -ljbig2dec -L../libjpeg/.libs -ljpeg -L../openjpeg/build/bin -lopenjp2 -L../zlib -lz -shared -Wl,-soname -Wl,libmupdf.so -Wl,--no-undefined
