# Plato diagnostics for Kindle Voyage

KUAL diagnostics for the jailbroken Voyage on firmware 5.10.3. The inventory
action has run successfully on this device. Version 0.2.0 adds a static ARMv7
Rust probe for the runtime, framebuffer geometry, and input events.

## Run the grayscale display probe

1. Install version 0.3.1, then eject and disconnect USB.
2. Open **KUAL → Plato Diagnostics → Test top-screen restoration**.
3. Note the text or menu items near the top of the screen. Keep the screen
   untouched while six grayscale bars appear for about five seconds. Check
   whether the same text or menu items return.
4. Wait 45 seconds before reconnecting USB. Retrieve the newest
   `plato-diagnostics/display-probe-*.txt` and report what appeared and
   whether the original patch returned.

The action checks for the Voyage framebuffer mode observed by the earlier
probe. It checks that the 720 × 160 pixel patch near the top contains both dark
and light pixels, saves it before writing, and uses separate
processes for drawing and restoring so restoration can run after a failed
or timed-out draw. If restoration fails, rerunning the action retries the
saved patch first. A hard interruption can leave `.display-probe-lock`.
Remove that folder over USB only after the interrupted run has stopped, then
rerun the action to restore the saved patch.

## Install and run the ARM probe

1. Extract `plato-diagnostics-0.3.1.zip` into the Kindle drive root in normal
   USB storage mode. Replace files under `extensions/plato-diagnostics`
   when prompted and keep the other extensions.
2. Eject and disconnect the Kindle. Open **KUAL → Plato Diagnostics →
   Run ARM probe and capture input**.
3. During the next 20 seconds, tap two points in blank areas, make one short
   swipe, then press the forward and back PagePress controls on both sides.
   Input remains active in KUAL during the capture, so avoid tapping menu items.
4. After 45 seconds, reconnect USB and retrieve the newest
   `plato-diagnostics/arm-probe-*.txt` report. Share it even if the binary
   failed to launch or one input device logged no events.

The probe opens the framebuffer and event devices for reading. It logs actual
framebuffer geometry and a bounded sample from the touch and PagePress
devices. If the binary stalls, the launcher stops it after 40 seconds and
records the result. A `.partial` report means the launcher was interrupted.
If an interrupted run left `.arm-probe-lock`, remove that folder over USB
once the run has stopped. The workflow uses USB storage and KUAL.

The original **Save read-only device inventory** action still writes an
`inventory-*.txt` report in the same directory. Allow up to two minutes for
that action to finish. To uninstall, remove `extensions/plato-diagnostics`.

## Build and host checks

```sh
rustup target add armv7-unknown-linux-musleabihf
sh tools/kindle-diagnostics/test.sh
sh tools/kindle-diagnostics/package.sh
```

The ZIP is written under `tools/kindle-diagnostics/out/`. The inventory fixture
checks cover report completion, identifier filtering, missing attributes,
run locking, and recovery from a stalled read command.

`PLATO_DIAG_OUT` overrides the output directory for development.
`PLATO_DIAG_FIXTURE_ROOT` supplies synthetic hardware files to the inventory
script for host tests.
