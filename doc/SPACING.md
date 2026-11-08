# EPUB spacing audit

The regression case is *Three Men in a Boat*, `OEBPS/boat005_ch02.xhtml`:
`p.blockquote { margin:1em 3em }`, followed by a floated `.lettrine` image and
`p.initial`. Its `span.first` also specifies `margin-left:-0.5em`.

## What an em means

For margins, padding and text indentation, `1em` is the element's computed font
size, not the width of the letter M, a space, or its line-height. For example,
at a 16pt font size, `1em = 16pt = 21⅓ CSS px`; at 300dpi this rasterizes to about
67 device pixels. The book's `3em` side margin is therefore about 200 device
pixels at that font size. These side margins were already computed correctly.
A `font-size` expressed in em uses the parent's font size; `rem` uses the computed
root element font size. The reader's font-size setting supplies the initial size.

CSS absolute units retain their ratios: `96px = 72pt = 6pc = 1in = 2.54cm`.
A CSS pixel is not necessarily one hardware pixel. Margins and padding in percent
use the containing block's **width**, even on the top and bottom. Percentage
height instead needs a definite containing-block height; otherwise it is auto.

## Findings and changes

| Area | Previously | Corrected behavior |
| --- | --- | --- |
| Block floats | Lost the preceding sibling's pending bottom margin | Preserve that margin for both the float's position and the following normal-flow block |
| Image-only floats | An inline-image baseline inserted space above the image | Lay out the image from the float content top |
| Inline margins/padding | Ignored on ordinary spans | Apply horizontal spacing, including negative margins; vertical inline margins do not change line boxes |
| Collapsed margins | Collapsed through padding/borders; pairwise arithmetic lost negative-margin information through empty blocks | Stop at top/bottom padding and borders; retain positive/negative extrema across adjoining groups |
| Hidden siblings | Could consume pending sibling spacing | `display:none` preserves it |
| Block widths | Ignored most explicit widths; interpreted horizontal auto margins as a whole containing-block width | Honor content widths and distribute available space to auto margins; preserve negative margins |
| Padding | Could become negative | Clamp negative used padding to zero |
| Heights | Percentage height used containing width; explicit height counted glyph ink rather than flow height | Resolve against definite height and measure line-box flow, excluding top margin |
| Root units | `rem` always used the reader setting | Use computed `html` font size, including when EPUB layout starts at `body` |
| Line-height | Always scaled inherited values with the child's font; leading split by font ascent ratio | Inherit numbers as ratios and lengths/percentages as computed lengths; divide leading equally above/below |
| Percentage indentation | Inherited a previously resolved pixel count | Preserve percentages until the child's containing block is known |
| Center/right indentation | Center discarded it; right shifted the right edge | Preserve first-line indentation while retaining the alignment edge |
| Inline containing blocks | Nested spans could replace the width with zero | Preserve the containing width; explicitly sized floats supply their own content width for child percentages |
| Image constraints | Ignored min/max dimensions | Apply min/max bounds while preserving intrinsic aspect ratio; resolve block-image auto margins |
| Block constraints | Ignored min/max width and min-height | Apply these constraints to content dimensions |
| Whitespace | Used the paragraph's whitespace/font settings for inline runs; normalized typographic spaces | Honor inline `white-space`, permit `normal` to reset inherited `pre`, measure spaces in the run font, preserve Unicode space widths |
| Length syntax | Misread exponent notation and some zero spellings | Share finite numeric/unit conversion for font sizes and lengths; accept signed/fractional zero and case-insensitive units |

These fixes are in the HTML/EPUB renderer, not overrides specific to this book.
No change was made to the book's authored `3em` margin or `1.5em` paragraph indent.

## Validation and limits

All 68 core tests pass (15 added spacing tests). Core regression tests cover the chapter's float/margin sequence, border and
padding collapse barriers, mixed-sign empty-block margin chains, hidden siblings,
auto/negative margins, computed em/rem, percentage indentation/heights, inherited
line-height, whitespace, image percentages/constraints and prior table styling.
Native tests use an isolated working lockfile and Homebrew MuPDF 1.27.2; the Kobo
build retains MuPDF 1.27.0. The checkout's pre-existing malformed Cargo.lock is
unchanged.

Controlled chapter renders use identical fonts/settings before and after:
1264×1680, 15.2pt, 300dpi, line-height 1.1755, reader margin 1mm. They demonstrate
geometry changes, not an exact font match to the supplied device screenshot.
Local review artifacts are in `target/spacing-review/` (not tracked).

This is not a complete browser CSS implementation. Remaining unsupported or
approximate areas include `ex`/`ch`, viewport units, `calc()`, `box-sizing`, full
`clear`/formatting-context rules, arbitrary inline vertical alignment and line-box
expansion for mixed font sizes, block max-height/overflow, and CSS fragmentation
rules at page edges. These require broader layout work; the fixes above do not
claim conformance for those features. Replaced images retain aspect ratio rather
than implementing independently distorted width/height sizing. Padding validation
operates on used values; the cascade does not yet discard every invalid declaration
in favor of an earlier valid one.

## Reference rules

- [CSS Values and Units: font-relative lengths](https://www.w3.org/TR/css-values-3/#font-relative-lengths)
- [CSS 2.2 box model and collapsing margins](https://www.w3.org/TR/CSS22/box.html#collapsing-margins)
- [CSS 2.2 widths, heights, and leading](https://www.w3.org/TR/CSS22/visudet.html)
