## Font substitutions

By default, Plato uses Noto Sans, Noto Serif, and Source Code Variable for sans serif, serif, and monospace fonts, respectively, in its own interface and in documents. If you wish to substitute any or all these fonts, do not ovewrite the files directly, as they will be overwritten again during upgrade. Instead, place your fonts here (in the fonts/ folder) and change their file names *exactly* as follows:

### Substitution for sans serif:

* sans-Regular.ttf
* sans-Italic.ttf
* sans-Bold.ttf
* sans-BoldItalic.ttf

### Substitution for serif:

* serif-Regular.ttf
* serif-Italic.ttf
* serif-Bold.ttf
* serif-BoldItalic.ttf

### Substitution for monospace:

* monospace-Regular.ttf
* monospace-Italic.ttf

### Notes

* File names are _case sensitive_. 
* All variants must be present as shown.
* There's no need to change the fonts' metadata, only the file names.
