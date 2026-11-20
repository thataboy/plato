# Themes

Themes provide a quick way to save and re-apply several reader settings at once. Currently, themes are supported for epub and html files. Themes work pretty much the same way as they do on the Kindle, but better! You can select what settings to save, and in addition to the usual font and layout settings, you can also save your front light levels and inverted mode status. The following settings can be saved in a theme:

- `Font family`
- `Font size`
- `Font weight`
- `Relative font size`
- `Margin width`
- `Line spacing`
- `Text align`
- `Front light`
- `Inverted mode`

To create a theme:

- tap middle of the screen to bring up the menu
- click the theme icon (to the left of the location seek bar)
- select `New theme...`
- select setting(s) you want to include in your theme
- click Save

## Notes

- You can rename or delete an existing theme by tapping the 3 dot menu next to the theme's name.
- Creating a new theme with the same name as an existing theme will replace it. Out of the box, there are some example themes which you are free to use, overwrite, or delete.
- `Relative font size` saves the difference between the current font size and the default font size (the option is disabled if both are the same). This is useful for having a theme which increases or decreases the font size by some amount (e.g., +/- 3pt) rather than setting a fixed size (e.g., 15pt).

## Special theme names

There are two special theme names:

- `__inverted`
- `__uninverted`

If you want to have a theme automatically applied whenever you turn *on* (*off*) inverted mode, name it
`__inverted` (`__uninverted`). Other special theme names beginning with two undescores `__` may be added later.


Font weight saves an absolute stroke adjustment from -3.0 to +3.0 in 0.5 steps. Negative values lighten text; positive values thicken it. Themes without font weight leave the book’s current weight unchanged. Existing manually configured `dismiss = false` themes remain supported, although this option is no longer offered in the save dialog.
