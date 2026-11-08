# Static Kindle build and rendering test

This build targets 32-bit ARMv7 Linux with musl on the Kindle Voyage. It builds
Plato and a separate rendering test. The rendering test is the KUAL action
ready for device validation. The reader launcher still needs Kindle hardware
integration.

## Build

Install Rust with the `armv7-unknown-linux-musleabihf` target and provide Zig
0.15.2, GNU Make, Python 3, curl, and zip. From the repository root:

```sh
ZIG=/absolute/path/to/zig sh tools/kindle-build/build.sh
sh tools/kindle-build/package-smoke.sh
```

`PLATO_KINDLE_BUILD` overrides the default `target/kindle` build directory.
Use the same value for both commands. The build checks the pinned source
archives and links MuPDF, bundled FreeType and codecs, HarfBuzz, and DjVuLibre
as static archives. MuPDF and Plato use a single HarfBuzz build.

## Run the rendering test

Extract the generated `plato-render-smoke-0.1.0.zip` at the Kindle drive root.
Eject and unplug, then select **KUAL → Plato Rendering Test → Render sample
PDF and EPUB**. Wait up to 45 seconds before reconnecting USB. Share the newest
`plato-diagnostics/render-smoke-*.txt`. Each fixture should report page
dimensions and nonzero dark and light pixel counts, followed by exit status 0.

The test rasterizes the two bundled documents in memory and records results
in the report. Its KUAL extension is separate from the display and input probes.

## Portrait reader alpha

Build the ARM target as above and run `sh tools/kindle-build/package-reader.sh`.
The reader package needs `resources/blue_noise-128.png` from Plato's normal
resource bundle. Set `PLATO_KINDLE_RESOURCES` to its directory if the resources
are in a different checkout.

Install the generated ZIP at the root of the Kindle drive. Eject and unplug
before selecting **KUAL → Plato Voyage Alpha → Start reader with stock sleep**.
Open a bundled book and try the following in order:

1. Tap the middle of the page to show the bars, then tap the frontlight icon.
   Change brightness and check the physical light responds.
2. Press the power button briefly. Leave the Kindle unplugged and untouched
   for three minutes, then press it again. Check the page and brightness return.
3. Turn a page and connect the Kindle to the computer. Cancel the USB prompt
   and check that the book stays open. Choose **Menu → USB Connect** and
   confirm that the Kindle returns to stock and the USB drive mounts.
4. Retrieve the newest `plato-diagnostics/reader-*.txt` report. After unplugging,
   reopen the book to check that the last page survived the USB handoff.

To leave manually in stock sleep mode, show the bars and select **Menu → Quit**.
The power button controls stock sleep in this mode. The separate **Start
bounded reader (fallback)** action retains the three-minute session and uses a
short power-button press to quit.

This alpha targets portrait on Voyage firmware 5.10.3. It uses the isolated
sample library and retains its settings and progress on upgrade. The stock
sleep mode listens to powerd and keeps the stock screensaver. During reading
the launcher pauses the stock window manager and USB storage service.
Wall charging leaves Plato open. A new USB host connection prompts once
for confirmation, and a cable already attached at launch stays quiet.
Use **USB Connect** to request USB storage with an ejected cable attached.
The launcher releases stock storage handling after the reader closes its
files. Sleep, wake, and automatic USB
handoff in this build still need device validation. Brightness uses Kindle's
stock level plus a physical off setting.

## Wi-Fi and manual rotation

In the top-right Plato menu, use **Enable Wi-Fi** to control the stock
Kindle radio. Plato connects through saved Kindle Wi-Fi profiles. Set up a
new network in the stock Kindle settings first.

Use **Menu → Rotate** to choose 0°, 90°, 180°, or 270°. The touch targets
and PagePress orientation follow the display. The Kindle framebuffer
stays in its stock mode during the session so the launcher can restore
the stock screen on exit.
