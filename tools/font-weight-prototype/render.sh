#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
out="${1:-/tmp/plato-font-weight}"
mkdir -p "$out"
cc -Wall -Wextra -Werror -O2 tools/font-weight-prototype/render.c \
  $(pkg-config --cflags --libs freetype2 harfbuzz) -lm -o "$out/render"
for spec in 'NotoSans-Regular.ttf|Noto Sans|sans' 'NotoSerif-Regular.ttf|Noto Serif|serif' 'SourceCodeVariable-Roman.otf|Source Code Variable (default weight)|mono'; do
  IFS='|' read -r font title name <<< "$spec"
  "$out/render" "fonts/$font" fonts/NotoSans-Regular.ttf "$title" "$out/$name.pgm"
done
python3 - "$out" <<'PY'
from pathlib import Path
import sys
from PIL import Image

out = Path(sys.argv[1])
images = []
for name in ('sans', 'serif', 'mono'):
    im = Image.open(out / f'{name}.pgm')
    im.save(out / f'{name}.png')
    images.append(im.copy())
overview = Image.new('L', (1072 * 3, 1920), 255)
for i, im in enumerate(images):
    overview.paste(im, (1072 * i, 0))
overview.save(out / 'comparison.png')
# Pixel-for-pixel crop: original and the neighboring +/- 0.25 / 0.5 samples.
detail = Image.new('L', (1072 * 3, 150 + 133 * 5), 255)
for i, im in enumerate(images):
    detail.paste(im.crop((0, 0, 1072, 150)), (1072 * i, 0))
    detail.paste(im.crop((0, 150 + 4 * 133, 1072, 150 + 9 * 133)), (1072 * i, 150))
detail.save(out / 'fine-comparison.png')
print(f'Rendered PNG comparisons in {out}')
PY
