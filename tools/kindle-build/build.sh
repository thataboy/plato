#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Static ARMv7 Kindle build. Set ZIG to an absolute Zig 0.15.2 executable.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
build=${PLATO_KINDLE_BUILD:-"$repo/target/kindle"}
mkdir -p "$build"
build=$(CDPATH= cd -- "$build" && pwd)
zig=${ZIG:-$(command -v zig)}
case "$zig" in
    /*) ;;
    *) echo 'Set ZIG to an absolute Zig executable path.' >&2; exit 1 ;;
esac
[ "$("$zig" version)" = 0.15.2 ] || { echo 'This build expects Zig 0.15.2.' >&2; exit 1; }
jobs=${JOBS:-4}
target=armv7-unknown-linux-musleabihf
export ZIG_GLOBAL_CACHE_DIR="$build/zig-cache"
export ZIG_LOCAL_CACHE_DIR="$build/zig-local"
export PLATO_KINDLE_NATIVE="$build/native"
export CARGO_TARGET_DIR="$build/cargo"
mkdir -p "$build/bin" "$build/downloads" "$PLATO_KINDLE_NATIVE"

fetch() {
    file="$build/downloads/$1"
    if [ ! -f "$file" ]; then
        curl -fL --retry 2 "$2" -o "$file.part"
        printf '%s  %s\n' "$3" "$file.part" | shasum -a 256 -c
        mv "$file.part" "$file"
    fi
    printf '%s  %s\n' "$3" "$file" | shasum -a 256 -c
}

fetch mupdf-1.27.0.tar.gz \
    https://casper.mupdf.com/downloads/archive/mupdf-1.27.0-source.tar.gz \
    ae2442416de499182d37a526c6fa2bacc7a3bed5a888d113ca04844484dfe7c6
fetch djvulibre-3.5.29.tar.gz \
    https://downloads.sourceforge.net/djvu/djvulibre-3.5.29.tar.gz \
    d3b4b03ae2bdca8516a36ef6eb27b777f0528c9eda26745d9962824a3fdfeccf

for tool in cc c++; do
    cat > "$build/bin/arm-$tool" <<WRAPPER
#!/bin/sh
for arg do
    shift
    case "\$arg" in --target=*) continue ;; esac
    set -- "\$@" "\$arg"
done
exec "$zig" "$tool" -target arm-linux-musleabihf -mcpu=cortex_a9 "\$@"
WRAPPER
done
for tool in ar ranlib; do
    printf '#!/bin/sh\nexec "%s" %s "$@"\n' "$zig" "$tool" > "$build/bin/arm-$tool"
done
host=$(rustc -vV | sed -n 's/^host: //p')
lld="$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld"
printf '#!/bin/sh\nexec "%s" -flavor gnu -m armelf_linux_eabi "$@"\n' "$lld" > "$build/bin/arm-ld"
chmod +x "$build"/bin/arm-*
cc="$build/bin/arm-cc"
cxx="$build/bin/arm-c++"
ar="$build/bin/arm-ar"
ranlib="$build/bin/arm-ranlib"
ld="$build/bin/arm-ld"

mupdf="$build/mupdf"
if [ ! -f "$mupdf/Makefile" ]; then
    mkdir -p "$mupdf"
    tar -xzf "$build/downloads/mupdf-1.27.0.tar.gz" --strip-components=1 -C "$mupdf"
fi
# Plato uses variable font APIs that the default slim FreeType build omits.
python3 - "$mupdf" <<'PYTHON'
from pathlib import Path
import sys

root = Path(sys.argv[1])
options = root / "scripts/freetype/slimftoptions.h"
source = options.read_text()
updated = source.replace("#undef TT_CONFIG_OPTION_GX_VAR_SUPPORT\n", "")
if updated != source:
    options.write_text(updated)
makelists = root / "Makelists"
source = makelists.read_text()
module = "FREETYPE_SRC += thirdparty/freetype/src/base/ftmm.c\n"
if module not in source:
    anchor = "FREETYPE_SRC += thirdparty/freetype/src/base/ftinit.c\n"
    if anchor not in source:
        raise SystemExit("unexpected MuPDF FreeType source list")
    makelists.write_text(source.replace(anchor, anchor + module))
PYTHON

# The MuPDF archive prefixes HarfBuzz symbols and supplies a MuPDF-specific
# allocator. Plato needs a normal HarfBuzz instance using the same FreeType.
harfbuzz="$build/harfbuzz"
if [ ! -f "$PLATO_KINDLE_NATIVE/libharfbuzz.a" ]; then
    mkdir -p "$harfbuzz"
    cp -R "$mupdf/thirdparty/harfbuzz/src/." "$harfbuzz/"
    sed '/#include "hb-rename.h"/d' "$harfbuzz/hb-common.h" > "$harfbuzz/hb-common.h.tmp"
    mv "$harfbuzz/hb-common.h.tmp" "$harfbuzz/hb-common.h"
    "$cxx" -std=c++17 -O2 -ffunction-sections -fdata-sections \
        -DHAVE_FREETYPE -DHAVE_OT -DHAVE_FALLBACK -DHAVE_FT_GET_TRANSFORM \
        -DHAVE_FT_SET_VAR_BLEND_COORDINATES -DHAVE_FT_DONE_MM_VAR \
        -I"$mupdf/thirdparty/freetype/include" -c "$harfbuzz/harfbuzz.cc" -o "$build/harfbuzz.o"
    "$ar" rcs "$PLATO_KINDLE_NATIVE/libharfbuzz.a" "$build/harfbuzz.o"
fi

make -C "$mupdf" generate
make -C "$mupdf" -j "$jobs" OS=Linux build=release OUT=build/kindle \
    CC="$cc" CXX="$cxx" AR="$ar" LD="$ld" USE_SYSTEM_LIBS=no \
    USE_SYSTEM_HARFBUZZ=yes SYS_HARFBUZZ_CFLAGS="-I$harfbuzz" \
    HAVE_OBJCOPY=yes HAVE_LIBCRYPTO=no HAVE_GLUT=no HAVE_X11=no \
    mujs=no tesseract=no extract=no archive=no brotli=no barcode=no commercial=no libs
cp "$mupdf"/build/kindle/libmupdf*.a "$PLATO_KINDLE_NATIVE/"

djvu="$build/djvu"
if [ ! -f "$djvu/configure" ]; then
    mkdir -p "$djvu"
    tar -xzf "$build/downloads/djvulibre-3.5.29.tar.gz" --strip-components=1 -C "$djvu"
fi
if [ ! -f "$djvu/libdjvu/Makefile" ]; then
    (cd "$djvu" && ./configure --host=arm-linux-musleabihf \
        CC="$cc" CXX="$cxx" AR="$ar" RANLIB="$ranlib" \
        --disable-xmltools --disable-desktopfiles --without-jpeg --disable-shared --enable-static)
fi
make -C "$djvu/libdjvu" -j "$jobs" libdjvulibre.la
# Do not trust the archive produced by host libtool on macOS. Archive
# the target objects directly with the same cross tool used by MuPDF.
rm -f "$PLATO_KINDLE_NATIVE/libdjvulibre.a"
"$ar" rcs "$PLATO_KINDLE_NATIVE/libdjvulibre.a" "$djvu"/libdjvu/libdjvulibre_la-*.o

printf 'int main() { return 0; }\n' > "$build/cxx.cpp"
"$cxx" -static -v "$build/cxx.cpp" -o "$build/cxx" > "$build/cxx.log" 2>&1
for lib in libc++.a libc++abi.a; do
    # Zig's verbose link line quotes archive paths.
    path=$(tr ' "' '\n' < "$build/cxx.log" | awk -F / -v lib="$lib" '$NF == lib { path = $0 } END { print path }')
    test -f "$path"
    cp "$path" "$PLATO_KINDLE_NATIVE/$lib"
done
"$cc" -O2 -I"$mupdf/include" -c "$repo/mupdf_wrapper/mupdf_wrapper.c" \
    -o "$build/mupdf_wrapper.o"
"$ar" rcs "$PLATO_KINDLE_NATIVE/libmupdf_wrapper.a" "$build/mupdf_wrapper.o"

export CC_armv7_unknown_linux_musleabihf="$cc" AR_armv7_unknown_linux_musleabihf="$ar"
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER=rust-lld
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUSTFLAGS='-C target-feature=+crt-static -C relocation-model=static'
cd "$repo"
cargo build --locked --profile release-lto --target "$target" -p plato --bins "$@"
printf '%s\n' "$CARGO_TARGET_DIR/$target/release-lto"
