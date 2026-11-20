# FreeType font thickness prototype

Run from any directory with a C compiler, pkg-config, FreeType, HarfBuzz,
Python 3, and Pillow installed:

```sh
bash tools/font-weight-prototype/render.sh /tmp/plato-font-weight
```

The standalone renderer uses bundled Noto Sans, Noto Serif, and Source Code
Variable (its default variation coordinates). It shapes text with HarfBuzz,
loads unhinted outlines, applies `FT_Outline_EmboldenXY`, then rasterizes with
FreeType grayscale antialiasing. Sample text is 11 pt at 300 DPI. Labels are
unmodified. This is an experiment, not a change to Plato's reader.

Strengths in pixels: -3, -2, -1.5, -1, -0.5, -0.25, 0, +0.25, +0.5, +1,
+1.5, +2, +3. These are API strengths, not a promise that each edge moves by
that amount. The same strength is applied in X and Y, then the outline is
translated by negative half the strength to keep the adjustment centered.
Shaping and glyph advances are independent of strength. Zero skips outline
transformation entirely.

Outputs: `sans.png`, `serif.png`, `mono.png`, `comparison.png`, and
`fine-comparison.png` (five samples around zero). Original PGM images and the
compiled renderer are retained in the output directory. Individual sheets
are 1072 x 1920 pixels; view at 100% to judge native rasterization. A scaled
overview is useful for comparison but resampling changes apparent weight.
These are desktop grayscale renders, not a simulation of Kindle panel
quantization or refresh behavior. The eventual range needs device testing.
