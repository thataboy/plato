## Font substitutions

By default, Plato uses Noto Sans, Noto Serif, and Source Code Variable for sans serif, serif, and monospace fonts, respectively, in its user interface (UI). (In epub books and html documents, it uses Libertinus Serif instead of Noto Serif.) If you wish to substitute any or all these fonts, do not overwrite the files directly, as they will be overwritten again during upgrade. Instead, place your fonts here (in the fonts/ folder) and change their file names *exactly* as follow:

### Substitution for UI sans serif:

* `sans-Regular.ttf`
* `sans-Italic.ttf`
* `sans-Bold.ttf`
* `sans-BoldItalic.ttf`

### Substitution for UI serif:

* `serif-Regular.ttf`
* `serif-Italic.ttf`
* `serif-Bold.ttf`
* `serif-BoldItalic.ttf`

### Substitution for UI monospace:

* `monospace-Regular.ttf`
* `monospace-Italic.ttf`

To substitute for fonts used in epub books and html documents, use the names above but prefix with `book-`. For example:

* `book-serif-Regular.ttf`
* `book-serif-Italic.ttf`
* `book-serif-Bold.ttf`
* `book-serif-BoldItalic.ttf`

### Notes

* File names are _case sensitive_. 
* All variants must be present as shown.
* There's no need to change the fonts' metadata, only the file names.
* Be careful when substituting for fonts used in epub and html. If your fonts do not have all the required glyphs, you may see missing characters.