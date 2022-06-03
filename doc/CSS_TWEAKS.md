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
.para { text-align: left !important;}
.first-para { text-align: left !important;}
```

## CSS Tweaks

The `CSS Tweaks` feature provides a way to do just this -- adding specific CSS rules to individual epubs without having to edit them in a program like Calibre.

First, add styles into `Settings.toml` using the following format:

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

## Examples

The following define two styles intended for indent and non-indented paragraphs, with zero margins and paddings. The `indent` style may be used for normal prose paragraphs, while the `no indent` style may be used for paragraphs at start of chapters or sections.

```
[[css-style]]
name = "indent"
css = "text-indent: 1.5em !important; margin: 0 !important; padding: 0 !important;"

[[css-style]]
name = "no indent"
css = "text-indent: 0 !important; margin: 0 !important; padding: 0 !important;"
```

A style to force left alignment:

```
[[css-style]]
name = "left align"
css = "text-align: left !important;"
```

## Applying the CSS styles

To apply a style to a paragraph (block element, to be more precise), select any word in the paragraph by holding down for a short interval. The word will be highlighted and the context menu will pop up. Now select `CSS Tweaks` > `style-name`. That style will then be applied to the paragraph and all other paragraphs having the same class name.

Repeat said procedure to apply multiple styles to the same paragraph. In the example above, after we apply `indent` and `left align` to the second paragraph, the following CSS rules will be added to the book's stylesheet

```
.para { text-indent: 1.5em !important; margin: 0 !important; padding: 0 !important; }
.para { text-align: left !important; }
```

Voilà! This will make all paragraphs with class  `.para` to be left aligned, first line indented, with no margin or padding.

## Notes and caveats

* The created CSS rules are saved externally in `.reading-states`. The epubs are left unmodified.

* Applying the same style more than once to the same class will simple move the corresponding rule to the end, allowing it to override any other styles.

* Currently, only block elements (p, div, h1, h2, ..., etc) can have styles applied to them.

* The markup in most epubs is not so clean and simple as our example, so the feature may fail in many instances. Hopefully, it works often enough to be worthwhile.
