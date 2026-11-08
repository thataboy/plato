# Kindle Voyage port: investigation and implementation plan

Prepared 2026-11-03. This is a source-level plan, not a claim of device compatibility. No Kindle was connected or tested during this investigation.

## Recommendation

Develop the port on the dedicated `codex/kindle-voyage` branch in `/Users/luu/dev/plato-kindle-voyage`, keeping the Kobo checkout separate. Preserve the reader, EPUB engine, themes, cover library, scrubber, progress bar, translation, and Wikipedia features. Replace the hardware-specific wiring on this branch first; reconsider a shared platform backend after device validation establishes the actual scope. Use Aristotle for static Rust/native-library build techniques and selected lifecycle patterns; use KOReader's explicit Voyage implementation for hardware behavior.

Do not rebase the product onto Aristotle. Its current application has its own UI, library, and hardware crates and consumes a modified Plato submodule for rendering. Moving this fork's features into that application would become a second UI port.

The first milestone should be a small executable that runs on the Voyage, draws and refreshes a test image, records touch/PagePress input, and exits cleanly. Prove the ABI, kernel, and screen assumptions before investing in the complete application build.

## Scope and assumptions

- First supported device: jailbroken Kindle Voyage, launched manually from KUAL.
- Preserve the existing Kobo branch and desktop emulator support. On the Kindle branch, make Kindle selection explicit rather than interpreting every ARM target as Kobo. The shared-platform design below is a consolidation option, not a prerequisite for the initial port.
- First reading milestone: portrait EPUB/PDF reading, existing fork UI, touch, saved progress, and return to stock UI. PagePress follows in the same usable-alpha milestone.
- Daily-use milestone additionally requires reliable sleep/wake, frontlight control, battery reporting, and safe USB handling.
- Later: landscape, automatic brightness, haptic preferences, network controls, and the fork's online features. Boot replacement, other Kindle models, Amazon format/DRM integration, and stock reading-position import are separate projects.
- User-confirmed baseline: firmware **5.10.3**, KUAL installed, and a USBNetwork option visible in KUAL. SSH availability and USB networking operation are unconfirmed; a menu entry alone does not establish either. The completed inventory below records the kernel and input nodes. KUAL version remains unconfirmed. Target the existing firmware; no firmware replacement is required by this plan.

## What the local fork needs

Inspected local revision: `e4a5530682a6718e286b262b8fdaf9c6c536ceb0` (Plato 0.9.45).

| Area | Existing code | Porting consequence |
| --- | --- | --- |
| Rendering abstraction | `crates/core/src/framebuffer/mod.rs` exposes `Framebuffer` and five `UpdateMode`s | Implement a Kindle backend without changing view drawing APIs. |
| Battery/frontlight | Existing `Battery`, `Frontlight`, and `LightSensor` traits | Add Kindle implementations; reuse existing context injection. |
| Application wiring | `crates/plato/src/app.rs` selects Kobo framebuffer using `mark()`, constructs Kobo devices, and handles power/USB directly | Extract device construction and platform actions; changing device names alone is insufficient. |
| Device identity | `crates/core/src/device.rs` reads Kobo `PRODUCT`/`MODEL_NUMBER`, defaults unknown products to a Kobo Touch | Add explicit Voyage detection and reject unsupported devices. Separate Kobo generation from platform capabilities. |
| Input | `crates/core/src/input.rs` mixes raw events, Kobo touch transforms, button codes, and Nickel's USB FIFO | Preserve normalized `DeviceEvent`s; add a Kindle decoder and event source. |
| Build | `.cargo/config`, `crates/core/build.rs`, `build.sh`, native dependency scripts | Current ARM build is hard-float GNU with Kobo shared libraries. Build all Kindle native dependencies for one matching ABI. |
| Packaging/lifecycle | `contrib/plato.sh`, `dist.sh`, `bundle.sh`, `scripts/*` | New Kindle package and KUAL launcher; existing scripts manipulate Nickel, Kobo partitions, drivers, and power nodes. |
| Paths and settings | `crates/core/src/settings/mod.rs` defaults to `/mnt/onboard`, `/mnt/sd`, and Kobo library folders | Kindle defaults should use `/mnt/us/documents` and a separate application directory. |
| Diagnostics | `crates/core/src/document/mod.rs` calls `ntx_hwconfig` and Kobo network scripts | Make platform-specific diagnostics optional and replace them on Kindle. |

The document/rendering code should need little hardware-specific change. The higher-risk work is startup, display updates, input ABI, and power ownership.

## Reference findings

### Aristotle: useful components, different target

Inspected revision [`77c5ca8`](https://github.com/piotrkowski/aristotle/tree/77c5ca846b6850ed9d8ad24c7a5e2ed205cf9a35), with its exact Plato submodule [`6a4fc45`](https://github.com/piotrkowski/plato/tree/6a4fc453c1766787560db2d5dd5b48d88b3e383c).

- Its [build script](https://github.com/piotrkowski/aristotle/blob/77c5ca846b6850ed9d8ad24c7a5e2ed205cf9a35/tools/build-plato.sh) builds Rust and the native rendering dependencies as static ARMv7 musl code. This avoids linking the application against Kindle's stock soft-float userspace libraries.
- Its [Plato build integration](https://github.com/piotrkowski/plato/blob/6a4fc453c1766787560db2d5dd5b48d88b3e383c/crates/core/build.rs) is a concrete starting point for this fork's build changes. Adapt library-directory configuration without adopting unrelated font/format reductions.
- Its [framebuffer](https://github.com/piotrkowski/aristotle/blob/77c5ca846b6850ed9d8ad24c7a5e2ed205cf9a35/crates/aristotle-hal/src/framebuffer.rs) uses the PW4/Rex 80-byte update structure and Zelda waveform numbering. These are not the Voyage interface.
- Its [input backend](https://github.com/piotrkowski/aristotle/blob/77c5ca846b6850ed9d8ad24c7a5e2ed205cf9a35/crates/aristotle-hal/src/input.rs) explicitly handles the 16-byte ARM kernel event layout instead of assuming musl's time types match it. This fork currently embeds `libc::timeval` in its input structure: audit that before switching libc.
- Its [power-validation record](https://github.com/piotrkowski/aristotle/blob/77c5ca846b6850ed9d8ad24c7a5e2ed205cf9a35/docs/power-validation.md) distinguishes screensaver transitions from actual suspend and lists remaining physical-device checks. Treat its lifecycle code as reference material, not Voyage validation.

### KOReader: the Voyage hardware reference

Inspected revision [`3d40440`](https://github.com/koreader/koreader/tree/3d40440e7f07a156dfee6cffc50400654d2336da), with its pinned base [`1c96825`](https://github.com/koreader/koreader-base/tree/1c9682506757d5fd30cfa01c5698929dcc35340f).

Its [Voyage device implementation](https://github.com/koreader/koreader/blob/3d40440e7f07a156dfee6cffc50400654d2336da/frontend/device/kindle/device.lua) supplies concrete starting values:

- 300 DPI, touch, frontlight, ambient sensor, and page keys.
- Touch node `/dev/input/event1` in its device profile; discover names/capabilities on our device rather than assume numbering.
- PagePress codes `104` (previous) and `109` (next), versus this fork's Kobo codes `193`/`194`.
- Frontlight node `/sys/class/backlight/max77696-bl/brightness`.
- Battery fallback `/sys/devices/system/wario_battery/wario_battery0/battery_capacity`; charging fallback `/sys/devices/system/wario_charger/wario_charger0/charging`.
- `com.lab126.deviced` properties `fsrkeypadEnable`, `fsrkeypadPrevEnable`, and `fsrkeypadNextEnable` restore PagePress when the framework is stopped. It also filters touch near the pressure-pad areas; inspect whether the same suppression is needed in Plato.

Its [Kindle power backend](https://github.com/koreader/koreader/blob/3d40440e7f07a156dfee6cffc50400654d2336da/frontend/device/kindle/powerd.lua) uses LIPC for light level, battery, and suspend coordination. Voyage's nominal light level zero is not necessarily physically off; preserve a distinct off state and verify restoration after wake.

The [display implementation](https://github.com/koreader/koreader-base/blob/1c9682506757d5fd30cfa01c5698929dcc35340f/ffi/framebuffer_mxcfb.lua) and [Kindle ABI definitions](https://github.com/koreader/koreader-base/blob/1c9682506757d5fd30cfa01c5698929dcc35340f/ffi/mxcfb_kindle_h.lua) route Voyage through the older Kindle/REAGL path, not Rex. The older update structure has histogram waveform fields and a 72-byte layout. Its waveform values include A2=4 and REAGL=8; PW4's corresponding values differ. Preserve submission/completion ordering and the REAGL full-update semantics.

The [launcher](https://github.com/koreader/koreader/blob/3d40440e7f07a156dfee6cffc50400654d2336da/platform/kindle/koreader.sh) is also valuable for framework coexistence, restoring UI state, and inhibiting USB mass storage while files are open. Adapt the required operations, not the entire script with its KOReader-specific dependencies.

## Proposed design

### Keep the platform boundary small

The following shared-backend layout was the initial proposal. After discussion, the agreed first implementation uses a separate Kindle branch/worktree and may replace Kobo wiring directly there. Extract these abstractions only where useful; decide on consolidation after the reading and lifecycle probes.

Add a `Platform`/device factory responsible for framebuffer, normalized input/system events, battery, frontlight, and paths. Add platform methods for lifecycle operations such as requesting suspend, network changes, and handing control back for USB storage. Keep the current traits and shared event loop where possible.

Suggested layout (names are provisional):

```text
crates/core/src/platform/{mod,kobo,kindle}.rs
crates/core/src/framebuffer/{kindle,kindle_sys}.rs
crates/core/src/battery/kindle.rs
crates/core/src/frontlight/kindle.rs
crates/core/src/input/kindle.rs
contrib/kindle/{config.xml,menu.json,start.sh,stop.sh}
scripts/kindle/...
build-kindle.sh
dist-kindle.sh
```

Use Cargo features or explicit build configuration for `kobo` and `kindle`, with Kobo kept as the default. CPU target and device platform are separate concepts. Avoid spreading model checks throughout the view code. Platform-specific defaults must not rewrite existing Kobo settings.

### Build and ABI strategy

First candidate: fully static `armv7-unknown-linux-musleabihf`, adapting Aristotle's Zig/native-library pipeline. Build MuPDF 1.27.0 and its wrapper, FreeType, HarfBuzz, DjVuLibre, compression/image libraries, C++ runtime, and Cargo native dependencies consistently. Isolate output under Kindle-specific directories so Kobo's `libs/` and `dist/` cannot be accidentally reused or overwritten.

Static hard-float can coexist with soft-float stock processes because they communicate through files, kernel interfaces, or subprocesses. It cannot directly load stock soft-float `liblipc` into that process. Initially use the stock LIPC commands or a small supervised stock-runtime helper for subscriptions. Cache battery/light values; do not launch a command for every pointer movement or render.

Static linking does **not** establish old-kernel compatibility. The [Voyage kernel source family](https://github.com/katadelos/linux-3.0.35-lab126/blob/master/BUILD_HOW-TO.txt) is Linux 3.0.35/Wario, whereas Aristotle's input code documents a 4.1 kernel. Confirm the device's actual version. The [current Rust target table](https://doc.rust-lang.org/rustc/platform-support.html) lists ARMv7 musl but does not establish compatibility with this specific vendor kernel. Exercise threads, clocks, polling, entropy, file I/O, and process spawning before choosing a toolchain version.

If the static approach fails on required kernel behavior, investigate a compatible pinned Rust/libc build or a Kindle soft-float GNU sysroot/custom Rust build. KOReader's `kindlepw2` softfp toolchain is the reference for the latter; merely switching to a modern GNU Rust target is not sufficient. Record this as a decision gate, not an assumed easy fallback.

### Display strategy

Implement a narrow Rust `KindleFramebuffer` using the Voyage ABI from KOReader's pinned definitions, retaining Plato's drawing interface. Add size/offset assertions against the device kernel headers. Query resolution, stride, offsets, memory length, and pixel depth; the device advertises portrait modes of 1072×1448 and a landscape mode of 1448×1072. Use the active geometry and offsets returned by the ioctls for memory access.

Begin with grayscale drawing and conservative full GC16 updates. Then map `Gui`, `Partial`, `Full`, `Fast`, and `FastMono` to tested Voyage modes. Keep waveform choice separate from the driver's partial/full update flag. Introduce REAGL and faster feedback only after image fidelity and marker waits pass. Preserve framebuffer state on exit; defer rotation initially and reject unsupported format assumptions clearly.

FBInk is an alternative if direct ABI maintenance becomes burdensome. Evaluate it behind the same trait rather than introducing per-pixel subprocess calls or replacing the renderer. Do not copy Aristotle's PW4 constants into the Voyage backend.

### Input and lifecycle strategy

Decode raw kernel records into existing `DeviceEvent`s with an explicit kernel ABI type. Capture the actual touch protocol, axis ranges, release events, slots, and pressure-pad events. Test transformations independently from the framebuffer rotation. Reuse gesture recognition above the decoder.

Keep Amazon `powerd` as suspend authority. For the first prototype, use a temporary, bounded sleep inhibition that is restored when the launcher exits. For daily use, subscribe to powerd screensaver/suspend/wake events, save state before sleep, stop drawing while asleep, and invalidate/redraw after resume. Do not run Kobo's `/sys/power/state-extended` or RTC flow on Kindle.

Start by retaining stock services and suppressing competing UI drawing as narrowly as practical. Only stop the full framework if device testing demonstrates it is necessary; that path adds framework-readiness, PagePress, and screensaver restoration obligations. A parent launcher must clean up after child failure, including abort-style Rust panics; Rust destructors alone are insufficient. Provide an idempotent recovery/stop action for interrupted launchers.

For the first package, books live under `/mnt/us/documents` and app resources/configuration under `/mnt/us/plato`, with the KUAL extension under `/mnt/us/extensions/plato`. Use a small isolated test library first. For USB mass storage, flush and close application files and hand control back to stock before export; distinguish a host storage connection from charging or USB networking. Never invoke the Kobo partition-export scripts.

## Implementation sequence and acceptance gates

| Phase | Deliverable | Done when |
| --- | --- | --- |
| 0. Device baseline | Firmware/kernel/CPU, memory, framebuffer and evdev inventory; LIPC property/service inventory; stock/KOReader baseline if available | We can launch a temporary program and recover to stock UI; actual device properties are recorded without full serial numbers. |
| 1. Compatibility probe | Minimal static Rust binary, framebuffer pattern, raw-input logger, cleanup launcher | Threads/clocks/polling/files/entropy/subprocesses work; grayscale and full refresh work; touch and PagePress are decoded; repeated exit/crash returns to stock. |
| 2. Build and platform separation | Reproducible Kindle native build; platform factory; paths/capabilities; dedicated packaging | Full Plato links with matching native libraries; ELF inspection confirms intended ABI and dependencies; Kobo/emulator builds retain existing behavior. |
| 3. Reading alpha | Kindle framebuffer and input integrated with existing library/reader | EPUB and PDF open, touch gestures and both PagePress sides work, themes/scrubber/cover view render correctly, and progress survives exit/relaunch. |
| 4. Daily-use lifecycle | Powerd events, brightness/battery adapters, USB policy, robust recovery | Physical button, cover and idle suspend/wake work repeatedly unplugged; actual suspend is observed; light and reading state restore; USB does not race metadata writes. |
| 5. Polish and release | Refresh tuning, rotation, network integration, diagnostics, installation docs | Regression matrix passes, overnight drain is measured, online features work over HTTPS, and a fresh KUAL ZIP install/upgrade/uninstall is repeatable. |

Phases 1 and 4 are the major uncertainty gates. A first rendered book is not evidence that phase 4 is complete. Do not estimate a release date until the compatibility probe and an unplugged sleep/wake experiment have run.

## Validation plan

- **ABI/build:** inspect ELF interpreter/dependencies, ARM attributes, and native library versions; check framebuffer structure offsets and raw input record sizes on the target build. Run probes on the real kernel, not just an emulator.
- **Input:** replay recorded taps, swipes, long press, multi-touch if exposed, releases, and PagePress repeat events. Check edges and accidental pressure-pad touches. Verify no duplicate turns from touch plus keys.
- **Display:** grayscale ramps, text, covers, small inverted highlights, overlapping updates, 100 page turns, menus, and exit restoration. Measure touch-to-settled-page latency and ghosting before choosing refresh defaults.
- **Reader regression:** EPUB typography, PDF zoom, dictionary, highlights/bookmarks, themes/CSS, cover grid, scrubber, and progress bar. Verify supported secondary formats before packaging claims of parity.
- **Lifecycle:** at least 20 unplugged sleep/wake cycles; power button and magnetic cover; idle sleep; exit, startup failure, child abort, and interrupted launcher recovery. Compare suspend counters/logs where available and overnight battery drop against a same-device baseline.
- **Storage/network:** USB charging versus storage/networking, clean handoff before export, persistent settings, failed network requests, DNS/TLS on the old kernel, and translation/Wikipedia after Wi-Fi changes. Preserve stock network settings on exit.
- **Resource budget:** measure startup, peak RSS on a large PDF, repeated cover-grid browsing, and page-cache growth; set limits from the Voyage's actual available memory.
- **Kobo/emulator:** compile both, exercise device selection/settings defaults and existing event behavior, and use targeted regression tests around the extracted platform boundary.

## Suggested reviewable changes

1. Device probe, reproducible build spike, and recorded compatibility decision.
2. Platform selection and settings/path separation, preserving Kobo behavior.
3. Kindle framebuffer with ABI checks and display fixtures.
4. Kindle input adapter and captured-event tests; initial KUAL reading package.
5. Powerd/frontlight/battery/USB lifecycle with recovery tests.
6. Refresh tuning, online features, rotation, package documentation, and measured release criteria.

Preserve license headers and source attribution when adapting reference code. Both projects identify as AGPL; document exact borrowed files and revisions in the distribution's source notices.

## Immediate next action

The Voyage diagnostics verified the static ARM runtime, raw touch and PagePress events, and drawing and restoring a small e-ink region. The static build's PDF and EPUB rendering test passed on the Voyage. The portrait alpha now integrates the Voyage framebuffer and evdev with the existing Plato library and reader. The device test confirmed touch paging, PagePress, and reading-position persistence. The launcher reported clean exit, screen restoration, and resumption of the Kindle window manager and USB storage service. The device test confirmed stock sleep and wake, brightness changes and restoration, and return to KUAL. Automatic USB handoff also passed with an EPUB open. Version 0.2.1 also passed wall charging and computer USB handoff in the same session. Next, exercise the isolated library with real books and verify the longer sleep and release workflows.

## Stage 0 device results

The KUAL inventory ran to completion on the Voyage. Firmware 5.10.3 is owner-reported because neither version file queried by the script was available.

| Area | Observed on this unit |
| --- | --- |
| Kernel and CPU | `3.0.35-lab126`, ARMv7 with NEON and VFPv3, i.MX 6SoloLite Wario board |
| Memory | 514748 kB total and no swap |
| Framebuffer | `mxc_epdc_fb`, 8 bpp, 1088-byte stride, virtual size 1088×6144, rotation 3, advertised portrait mode 1072×1448 |
| Input | `max77696-onkey` on `event0`, `cyttsp4_mt` on `event1`, `fsr_keypad` on `event2` |
| PagePress | Key capabilities are consistent with codes 104 and 109. At inventory time the global `fsrkeypadEnable` property read 0, with previous/next individually set to 1. Capture real presses before changing any properties. |
| Battery and frontlight | Voyage sysfs reads worked. Battery capacity was 87% through both sysfs and LIPC. Brightness read 310 of 4095 in sysfs and level 13 in LIPC. |
| Stock services and tools | Framework, powerd, deviced, and volumd were running. LIPC reads worked, and `fbset`, `evtest`, and `timeout` were found. |
| USBNetwork | Extension directories and SSH server commands were present. Connectivity remains untested. |

The framebuffer modes corrected the earlier 1080×1440 assumption. The ARM probe then confirmed the active geometry.

## Read-only ARM probe results

- The static ARMv7 Rust executable ran through KUAL and exited 0 on Linux `3.0.35-lab126`. Threads, channel timeouts, clocks, entropy reads, subprocesses, pipes, and evdev polling worked in the probe. The complete Plato build still needs device validation.
- Framebuffer ioctls reported 1072×1448 active, 1088×6144 virtual, offsets 0/0, 8 bpp grayscale, rotation 3, 1088-byte stride, and 6,782,976 bytes of framebuffer memory. The subsequent display probe verified GC16 update submission, completion, and framebuffer readback. The user confirmed that visible content returned after the top-screen patch.
- The 16-byte evdev decoder captured 312 touch records from `cyttsp4_mt`. The device uses Type B slots (0–2), tracking IDs, and per-axis updates bounded by X 0–1071 and Y 0–1447. Track slot state across synchronization frames and release on tracking ID −1. Verify physical screen orientation with known touch points.
- The `fsr_keypad` emitted two press/release pairs for each of 104 and 109. No enable property was changed between probes by these diagnostics. The user confirmed the extra touch events were deliberate. This capture does not show that PagePress itself generates touch events.


## Display probe results

The center test drew grayscale bars over a blank patch. The top-screen test then covered visible content with a 720×160 patch at (48, 24). Both the drawing and restoration GC16 updates completed in about 0.7 seconds with collision reported as 0. The restored bytes matched the saved patch, both processes exited 0, and the user visually confirmed the original screen content returned. These timings are for the diagnostic patches.

## Static build milestone

The Kindle branch builds the reader and a separate rendering test as static ARMv7 musl executables. The build combines MuPDF 1.27.0 and its bundled codecs, FreeType with variable font support, a shared HarfBuzz build for MuPDF and Plato, and DjVuLibre. The earlier branch lockfile contained duplicate keys. The build uses the valid lockfile from the Kobo checkout, whose Cargo manifests match the Kindle branch.

The KUAL rendering test passed on the Voyage with exit status 0. The PDF fixture produced a 280×200 raster with 402 dark pixels and the EPUB fixture produced a 1072×1448 raster with 15,415 dark pixels. Both had nonzero light pixels. The test rendered the fixtures in memory. The interactive reader was subsequently tested on the same Voyage. Build and run instructions are in `tools/kindle-build/README.md`.

## Portrait alpha milestone

The static ARMv7 reader and desktop compile check passed. The alpha uses a
shadow grayscale framebuffer with synchronous GC16 updates, Type B touch,
and the Voyage PagePress keys. It uses bundled sample books in a separate
library. Its KUAL launcher pauses the Kindle window manager and USB storage
service for a bounded session, saves the stock screen, and restores the
screen and service state on exit. A host launcher simulation exercised
cleanup after a failing reader process.

On the Voyage, the user confirmed that both touch and PagePress turned pages
and that reading progress persisted across launches. The session log recorded
opening both bundled formats, successful stock screen restoration, the Kindle
window manager and USB storage service resuming, and reader and launcher exit
status 0. The session used the bounded portrait alpha launcher.

## Stock sleep and USB test build

Version 0.2.0 adds a stock sleep mode alongside the proven bounded session.
The launcher retains the stock framework and screensaver. Plato saves reading
state before sleep, yields display control to stock, and redraws after a
powerd wake event. The log includes kernel suspend time when the two kernel
clocks are available. Brightness uses the stock powerd levels and a physical
off step. A HAL USB event or a charging event closes the reader, then the
launcher syncs, closes its log, leaves the USB filesystem, and resumes stock
storage handling. Wall charging may also close the reader in this test build.

The event vocabulary and brightness behavior are based on KOReader's
[Kindle event backend](https://github.com/koreader/koreader-base/blob/1c9682506757d5fd30cfa01c5698929dcc35340f/input/input-kindle.h)
and [Kindle frontlight implementation](https://github.com/koreader/koreader/blob/3d40440e7f07a156dfee6cffc50400654d2336da/frontend/device/kindle/powerd.lua).
The ARM and desktop builds passed. Host launcher checks exercised active and
sleeping cleanup, bounded fallback, and USB storage release after leaving the
volume.

On the Voyage, two unplugged power-button cycles recorded 117.67 s and
145.51 s of kernel suspend, followed by reader redraws and restoration of
the selected frontlight levels. The user confirmed brightness changes and
restoration after wake. The reader and launcher exited with status 0, and
the user confirmed return to KUAL. That session ended without a cable event.

A separate USB test with the EPUB open passed. Stock powerd emitted
`charging`, which triggered a clean reader exit. The launcher restored the
stock screen, resumed the window manager, and released the USB storage
service after closing the reader files. Both exit statuses were 0, and the
user observed normal Kindle USB drive mode on the computer. This verifies
handoff from the charging notification. HAL-only detection, behavior at
full battery, and distinguishing wall power from a computer remain untested.

### Wall charging follow-up

The 0.2.0 build exited when a wall charger was attached, as both wall
chargers and computers emit the stock charging event. Version 0.2.1 uses
the MAX77696 charger type to distinguish them. It lets dedicated chargers
continue charging in Plato and returns to stock on a host connection or
unknown classification. A USB configuration event also returns to stock.
The classification values follow the
[Wario charger driver](https://github.com/katadelos/linux-3.0.35-lab126/blob/master/drivers/power/max77696-uic.c)
and its
[constants](https://github.com/katadelos/linux-3.0.35-lab126/blob/master/include/linux/mfd/max77696.h).
Both connections passed on the Voyage in the same reader session. The
wall charger emitted both charging and USB plug-in events and was classified
as type 12. Plato remained open. The computer later emitted USB configured,
which triggered handoff and a clean exit. USB at full battery remains untested.

The current alpha already imports books from `extensions/plato-voyage/books/`.
The user verified real books, font changes, font size, margins, and dark mode
in that isolated library. Verify reading position across quit and USB handoff, followed by
idle and cover wake, overnight battery drain, and an upgrade that preserves
the library and its metadata.

## Wi-Fi and manual rotation

Version 0.3.0 uses the stock Kindle Wi-Fi connection manager and reports
connection changes to the reader and its online tools. Set up new Wi-Fi
profiles in stock settings. The enable/disable property order follows
[KOReader's Kindle Wi-Fi implementation](https://github.com/koreader/koreader/blob/master/frontend/device/kindle/device.lua).

Manual rotation now supports all four angles with a software framebuffer
transform and inverse touch transform. The physical Kindle framebuffer
stays in the verified stock portrait mode. The four mappings passed
pixel round-trip and refresh bounds checks. The Wi-Fi worker passed a
mocked radio enable, connection, and disable sequence. Device testing
should cover a saved network, an online lookup, landscape touch and
PagePress, and stock screen restoration after quitting in landscape.

Once these pass, checkpoint the tested Kindle changes and merge local
master into the Kindle branch. Master is five commits ahead of the
Kindle branch's base. The overlapping files are the app event loop and
Cargo lockfile. Keep the Kindle lifecycle behavior and master’s reader
and Kobo changes when resolving those edits. Validate both targets before
deciding whether to merge the platform support back into master.

The user confirmed manual rotation, and the Voyage log confirmed Wi-Fi
reached connected state on a saved network. The user then requested a USB
prompt and a quiet launch with an already-ejected cable. Version 0.3.1
remembers the cable present at startup, prompts once for a new host
connection, and resets the prompt on unplug. **USB Connect** requests
stock USB mode after progress and settings are saved and files are closed.
The launcher sends stock HAL events from outside the user volume after
resuming volumd. This uses the sequence reported on
[Kindle Touch](https://www.mobileread.com/forums/showthread.php?t=179555).
Mocked launcher checks verified event ordering and suppression on ordinary
or failed exits. Confirm cancellation, replug, and manual USB re-entry on
the Voyage before merging master.
