# CSS Tweaks

## Motivation

Even though it is possible to add CSS (Cascading Style Sheet) rules to epubs via `css/epub-user.css`, these rules don't allow for fine-grained control and must necessarily be kept rather general. For example, we may want to force "normal" text paragraphs to be left-aligned, but it's not ideal to put this in `css/epub-user.css`

```
p { text-align: left !important }
```

This will force all paragraphs to be left-aligned, even those that are meant to be center- or right-aligned.

What is needed is the ability to add CSS rules tailored to each epub. For example, given

```
<p class="chapter-header">Chapter 1</p>

<p class="first-para">It is a truth universally acknowledged,
that a single man in possession of a good fortune,
must be in want of a wife.</p>

<p class="para">However little known the feelings or views
of such a man may be on his first entering a neighbourhood,
this truth is so well fixed in the minds of the surrounding families,
that he is considered as the rightful property of some one or
other of their daughters.</p>

<p class="para">“My dear Mr. Bennet,” said his lady to him one day,
“have you heard that Netherfield Park is let at last?”</p>
```

we want to add these rules:

```
p.para { text-align: left;}
p.first-para { text-align: left;}
```

This is a rather trivial example that is not necessary per se, since there's built-in capability to change text alignment. However, the built-in text align adjuster does not work if the epub is coded a bit differently, e.g., if the `<p>` is wrapped in a `<div>` and the `<div>` is justified.

## CSS Tweaks

`CSS Tweaks` provides a way to add CSS styles to individual epubs and override their styles, without resorting to editing books in a program like Calibre.

Styles are defined in `Settings.toml` using the following format:

```
[[css-styles]]
name = "descriptive name 1"
css = "style declarations"

[[css-styles]]
name = "descriptive name 2"
css = "style declarations"

...etc
```

`style declarations` are `CSS-attribute: value` pairs separated by `;` per usual.  The `{ }` braces are optional and may be omitted.

Out of the box, there are 3 pre-defined styles you can use:

```
[[css-styles]]
name = "Main paragraph"
css = "margin:0; padding:0; text-indent:1.5em; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;"

[[css-styles]]
name = "Opening paragraph"
css = "margin:2em 0 0 0; padding:0; text-indent:0; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;"

[[css-styles]]
name = "Force preferences"
css = "font-family:serif; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;"
```

As you can see, values can be absolute (`2em`) or variables in the form of `%variable_name%`. They will be substituted with the values you set in the user interface or `Settings.toml`. (More later.)

## Applying the CSS styles

To apply a style to a paragraph (block element, to be more precise), select any word in the paragraph by holding down for a short interval. The word will be highlighted and a context menu will pop up. Now select `CSS Tweaks` > `style-name`. That style will then be applied to the paragraph and all other paragraphs having the same class name. (You can apply multiple styles to the same paragraph by repeating the procedure.)

If we apply `Opening paragraph` to the first paragraph and `Main paragraph` to the second paragraph (or third, take your pick) to the above example, we end up adding the following styles to the book:

```
p.first-para { margin:2em 0 0 0; padding:0; text-indent:0; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%; }
p.para { margin:0; padding:0; text-indent:1.5em; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%; }
```

So now, all `<p>` with class `first-para` will have top margin, no indent, and use our preferred text align, font size, and line height, while all `<p>` with class `para` will have no margins, small text indent, and use our preferred text align, font size, and line height.

Most narrative fiction / non-fiction books (as opposed to textbooks or technical books) will have two main types of paragraphs: those at the start of chapters or sections, and those in the main body. So just these two rules cover most of what we usually need to make books look consistently the way we like.

The `Force preferences` style does not set any margin or text-indent, but forces your choice of font size, line height, text align, *and* font family. The last is only needed when the publisher sets the main text font to a sans serif font. Setting `font-family` to serif does not mean the font will be a serif, only that it will be *your* chosen font.

BONUS: Many (most?) book publishers inexplicably like to set a font size and/or line height for the main text, forcing the reader to twiddle with font size and line height adjustment. Using CSS Tweaks lets us bypass all that.

## Variable substitution

CSS values may contain the following variables (*case sensitive*)

* `%FONTSIZE%`
* `%LINEHEIGHT%`
* `%TEXTALIGN%`
* `%fontsize%`
* `%lineheight%`
* `%textalign%`

`%FONTSIZE%`, `%LINEHEIGHT%`, and `%TEXTALIGN%` (all uppercase) will be substituted with their corresponding default values as defined in `Settings.toml`. `%fontsize%`, `%lineheight%`, and `%textalign%` (all lowercase) use values that you select in the user interface. These are more flexible as you can change them on the fly.

Note: do *not* specify units such as `em` or `pt`. That is only necessary for absolute values.

## Customizing styles

Of course you are free to change the pre-defined styles as well as add your own. For example, if you want to have a little bit more spacing between paragraphs, you can modify `Main paragraph` and `Opening paragraph` as follows:

```
[[css-styles]]
name = "Main paragraph"
css = "margin:0.1em 0; padding:0; text-indent:1.5em; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;"

[[css-styles]]
name = "Opening paragraph"
css = "margin:2em 0 0.1em 0; padding:0; text-indent:0; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;"
```

You can add a style to apply to chapter headings:

```
[[css-styles]]
name = "Chapter header"
css = "font-family:"Open Sans"; margin: 3em 0 2em 0; text-align:right; font-weight:bold; font-size:2.5em"
```

## Notes and caveats

* You can look at the underlying html code by making a selection then choosing `Inspect` from the pop up menu. You can also access the `CSS Tweaks` menu by tapping anywhere on the north strip (the upper part of the screen).

* The created CSS rules are saved externally in `.reading-states` (do not edit the files found there). The epubs are left unmodified.

* Applying the same style more than once to the same element will move the corresponding CSS rule to the end, allowing it to override other rules.

* When the text you select is inside a wrapper element (e.g., `<span>`) which in turn is inside a block element (e.g., `<div>`), Plato cannot determine which to apply styles to -- the `<div>`, the `<span>`, or some combination thereof. It will therefore ask you to decide. If you're not sure what to do, choose the most comprehensive CSS selector, i.e., the last one on the list.

* Use `Undo last` or `Undo all` option under the `CSS tweaks` menu when not getting the results you expected.

* Modifying a style in `Settings.toml` does not change previous applications of the style. You can use `Undo last` or `Undo all` then re-apply the modified style.

* Due to the vagaries of epub markups, this feature may fail in some instances. Hopefully, it works often enough to be worthwhile.