#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$here/../.." && pwd)
build=${PLATO_KINDLE_BUILD:-"$repo/target/kindle"}
resources=${PLATO_KINDLE_RESOURCES:-"$repo/resources"}
stage=$(mktemp -d "${TMPDIR:-/tmp}/plato-voyage-reader.XXXXXX")
trap 'rm -rf "$stage"' EXIT
extension="$stage/extensions/plato-voyage"
mkdir -p "$extension/resources" "$extension/dictionaries"
cp -R "$here/reader/." "$extension/"
cp -R "$repo/fonts" "$repo/css" "$repo/icons" "$repo/keyboard-layouts" "$repo/artworks" "$extension/"
cp "$resources/blue_noise-128.png" "$extension/resources/"
cp "$build/cargo/armv7-unknown-linux-musleabihf/release-lto/plato" "$extension/bin/"
python3 "$here/make-fixtures.py" "$extension/books" --reader
archive="$build/plato-voyage-alpha-0.3.1.zip"
rm -f "$archive"
(cd "$stage" && zip -qr "$archive" extensions -x '*.DS_Store' '*-user.css')
printf '%s\n' "$archive"
