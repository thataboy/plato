#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$here/../.." && pwd)
build=${PLATO_KINDLE_BUILD:-"$repo/target/kindle"}
stage=$(mktemp -d "${TMPDIR:-/tmp}/plato-render-smoke.XXXXXX")
trap 'rm -rf "$stage"' EXIT
extension="$stage/extensions/plato-render-smoke"
mkdir -p "$extension"
cp -R "$here/smoke/." "$extension/"
cp -R "$repo/fonts" "$repo/css" "$extension/"
cp "$build/cargo/armv7-unknown-linux-musleabihf/release-lto/plato-render-smoke" "$extension/bin/"
python3 "$here/make-fixtures.py" "$extension/fixtures"
archive="$build/plato-render-smoke-0.1.0.zip"
rm -f "$archive"
(cd "$stage" && zip -qr "$archive" extensions -x '*.DS_Store' '*-user.css')
printf '%s\n' "$archive"
