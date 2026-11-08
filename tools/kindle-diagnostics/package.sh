#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT="$HERE/out"
mkdir -p "$OUT"
STAGE=$(mktemp -d "${TMPDIR:-/tmp}/plato-diag.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
mkdir -p "$STAGE/extensions/plato-diagnostics"
cp -R "$HERE/extension/." "$STAGE/extensions/plato-diagnostics/"
cp "$HERE/README.md" "$STAGE/extensions/plato-diagnostics/README.md"
rustc --edition 2024 --target armv7-unknown-linux-musleabihf \
    -C linker=rust-lld -C target-feature=+crt-static -C relocation-model=static \
    -C opt-level=s -C panic=abort -C strip=symbols \
    "$HERE/probe.rs" -o "$STAGE/extensions/plato-diagnostics/bin/arm-probe"
ARCHIVE="$OUT/plato-diagnostics-0.3.1.zip"
rm -f "$ARCHIVE"
(cd "$STAGE" && zip -qr "$ARCHIVE" extensions -x "*.DS_Store")
printf '%s\n' "$ARCHIVE"
