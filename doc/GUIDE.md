## Install

If this is your first time installing Plato, the easiest way is to get the
[Plato one-click install package from mobileread.com](https://www.mobileread.com/forums/showthread.php?t=314220).

This will install the original Plato and the necessary launchers.

Next, go to the [Releases page](https://github.com/thataboy/plato/releases) and download the latest. Unzip to location where Plato is installed (usually `.adds/plato/`) and overwrite all files. Your settings will be preserved.

## Configure

The settings are saved in and read from `Settings.toml`. You can edit this file when *Plato* isn't running or is in shared mode. You can enter the shared mode by connecting your device to a computer.

You can also edit `Settings-sample.toml` and rename it to `Settings.toml` before you first run *Plato*.

`plato.sh` has a few settings that you can override by with `config.sh` (use `config-sample.sh` as a starting point).

The following style sheets : `css/{epub,html,dictionary,toc}.css` can be overridden via `css/{epub,html,dictionary,toc}-user.css`.

Read [THEMES.md](THEMES.md) to learn about themes, which provide a quick way to set several reader settings at once.

Read [CSS_TWEAKS.md](CSS_TWEAKS.md) to learn how to tweak CSS styles for individual epubs.

The hyphenation bounds for a particular language can be overridden by creating a file name `LANGUAGE_CODE.bounds` in the `hyphenation-patterns` directory. The content of this file must the minimum number of letters before the hyphenation point relative to the beginning and end of the word, separated by a space. You can disable hyphenation all together by uncommenting the corresponding line in `config.sh`.

Dictionaries in the *StarDict* and *dictd* formats can be placed in the `dictionaries` directory. *StarDict* dictionaries should be placed as uncompressed folders containing an `.ifo` file. *Plato* doesn't support *StarDict* natively and will therefore convert all the *StarDict* dictionaries it might find in the `dictionaries` directory during startup. You can disable this behavior by uncommenting the corresponding line in `config.sh`.

The four scripts `scripts/wifi-{pre,post}-{up,down}.sh` can be created with commands to run before or after the WiFi is enabled or disabled, respectively.

## Upgrade

Go to the [Releases page](https://github.com/thataboy/plato/releases) and download the latest. Unzip to location where Plato is installed (usually `.adds/plato/`) and overwrite all files. Your settings will be preserved.

Always read the release notes as they contain actual, useful information!
