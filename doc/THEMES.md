# Themes

Themes provide a quick way to set several reader settings at once.  Currently, you must create themes by adding manually to ```Settings.toml```. A theme should contain a name and one or more of these settings

- ```font-family```
- ```font-size```
- ```text-align```
- ```margin-width```
- ```line-height```
- ```ignore-document-css```
- ```inverted```
- ```frontlight``` (```true``` for on, ```false``` for off)
- ```frontlight-levels``` (see example below)

These settings have the same functions as formats as their counterparts with the same names.

## Examples

A theme with comfortable font size and line spacing. Note that settings you omit will remain unchanged.

```
[[themes]]
name = "Comfy"
font-family = "Georgia"
font-size = 14.5
margin-width = 5
line-height = 1.4
```

More examples. Note the format for setting the front light levels

```
[[themes]]
name = "Night"
font-family = "Georgia"
inverted = true

[themes.frontlight-levels]
intensity = 30.0
warmth = 8.0

[[themes]]
name = "Day"
font-family = "Arial"
inverted = false

[themes.frontlight-levels]
intensity = 60.0
warmth = 0.0
```

## Setting relative font size

- ```font-size-relative``` (true or false, default is false)

Add ```font-size-relative =  true``` to increase / decrease the current font size by the value of ```font-size```. For example, this theme turns off the front light and increase the font size by 1.5.

```
[[themes]]
name = "Outside"
frontlight = false
font-size = 1.5
font-size-relative = true
```

If ```font-size``` is a negative number, that implies ```font-size-relative = true``` (you can't have a negative font size!) so there is no need to specify it in that case.

## Misc

To see the list of themes, tap the menu icon next to the search button in the toolbar. By default, after you select a theme, the toolbar and menu will be dismissed. If you want the toolbar and menu to stay on the screen, add this line to the theme

```
dismiss = false
```

## Special theme names

Finally, there are two special theme names

- ```__inverted```
- ```__uninverted```

A theme named ```__inverted``` will be applied automatically whenever you toggle *on* inverted mode. Likewise, a theme named ```__uninverted``` will be applied automatically whenever you toggle *off* inverted mode.  Note that any theme whose name begins with two underscores will be hidden and not shown on the themes menu.
