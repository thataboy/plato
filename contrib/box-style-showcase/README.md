# Rules, borders, and backgrounds

Run `python3 contrib/box-style-showcase/generate.py` from the repository root to
rebuild `contrib/box-style-showcase.epub`. The generator uses only Python's standard
library, validates the XML and ZIP, and produces reproducible output.

The four chapters demonstrate:

- `<hr>` with `align`, pixel/percentage `width`, and pixel `size`; CSS width/height
  override the legacy dimensions.
- Block and float borders, including individual sides, widths, colors, and the
  one-to-four-value shorthands. Solid, dotted, and dashed are rendered; double,
  groove, ridge, inset, and outset fall back to solid. None/hidden suppress an edge.
- Block, float, and inline text background colors. Color-only `background` and
  `background-color` accept hex, the renderer's named colors, and comma-separated
  `rgb()` values. Monochrome screens display colors in grayscale.
- Nested backgrounds and a long box with side borders continued over page breaks.
  Top/bottom edges appear on the first/last fragment respectively.

Enable publisher/document CSS in Plato. Turn through the last chapter in both
directions to check navigation. Inline text borders, background images, alpha
compositing, and rounded corners are outside this implementation.

The sample copied to USB storage is `library/box-style-showcase.epub`. Rendering
these new features requires a Plato executable built with these source changes;
copying the EPUB alone does not upgrade the installed application.

Validation: 50 core tests passed, six rendered sample pages were visually reviewed,
and the Kobo ARM release build succeeded. Validation used isolated copies with
working lockfiles because the checkout's existing Cargo.lock has duplicate keys.
The native test copy used MuPDF 1.27.2 to match Homebrew; the Kobo build retained
the repository's MuPDF 1.27.0 binding. The built executable is available locally
at `target/box-style-showcase/plato`. The subsequent table-style build includes
these features and is installed on the Kobo; see the table-style showcase README.
