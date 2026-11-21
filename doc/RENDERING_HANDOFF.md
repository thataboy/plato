# Rendering handoff

Build both Kobo and Voyage from `master`. Integration merge `313835b` combines
Kobo master `9c5ef62` with `codex/voyage-page-profiling` at `c50e327`.
The combined default build passed the user’s Libra 2 smoke tests on
2026-11-12, including Wi-Fi and rotation, with no reported regression. See `doc/KINDLE_VOYAGE_PORT.md` for the Voyage history,
device setup, and build details. The original Kindle branches, the
`codex/voyage-line-height-override` branch, and the stash named “Voyage
display experiments through 0.5.4” remain separate checkpoints.

Both platforms use one shared profiler with a compile-time `page-profiling` feature, disabled by
default. Add `--features page-profiling` to the existing Cargo build for
the `plato` package. A default build omits profiling timestamps, trace
events, logs, and diagnostic display waits. Kobo traces append to
`.adds/plato/info.log`. Voyage traces go to `plato-diagnostics/reader-*.txt`.
Finger release is the start of tap timing. On Kobo, ordinary partial turns
end at submission. Existing protected full refreshes report completion.
On Voyage, the profiling build waits for the page marker to report completion.
Allow pages to settle before the next tap when collecting comparative traces.

The EPUB anchor cache is on Kobo in `cede853` and in the current Voyage
branch. The Voyage measured 909 ms down to 0.25 ms for median anchor lookup
in Programming Rust, and 1,707 ms down to 793 ms for tap-to-display
completion. The original slow section also took about 3 seconds to lay out
on first open. That cost is separate from the repeated chapter anchor lookup.
The confirming Voyage log is `reader-20261111T015944Z-16933.txt`.

The Libra 2 launcher selects 8-bit depth for model `io:388` and restores
the prior depth on normal exit. The 8-bit framebuffer path fills and copies
whole rows. The device experiment logged a change from 32 to 8 bits. Median
drawing in Bertie's went from 203 ms to 11 ms, and in Programming Rust from
215 ms to 11 ms. Median tap-to-submit fell from 236 ms to 46 ms and from
247 ms to 54 ms, respectively. The protected full refreshes took about
433 ms after submission. The source trace is `.adds/plato/info.log`.
USB eject restarts the reader through the existing supervisor. Quit back
to Nickel and relaunch to exercise changes in the launcher itself.

The handoff builds use the default feature set. Voyage package version is
0.5.8. For a Kobo build with existing native libraries, run
`cargo build --offline --locked --release --target=arm-unknown-linux-gnueabihf -p plato`.
The active device binaries live at `.adds/plato/plato` on Kobo and
`extensions/plato-voyage/bin/plato` on Kindle.

## Integration validation

The merge retains master's Libra 2 launcher and Kobo framebuffer unchanged.
The EPUB anchor cache and gesture implementation also match master. Shared
profiling uses `tap_profile` and `ProfileTap`; platform labels remain Kobo
and Voyage. Voyage retains its framebuffer submission fences and diagnostic
page-completion waits. Default builds keep profiling disabled.

The user confirmed all requested Libra 2 checks passed: light/dark page
turns, dragging, frontlight, sleep/wake, USB cancellation and connection,
restart after eject, and saved position after quit/relaunch. Wi-Fi and
rotation also passed with no reported regression. The tested default binary
has SHA-256 `8258c587f604e82d1c65090629f0c86d447c41fdf0f605e27faed691229974c1`.
Voyage smoke testing remains desirable; existing device evidence predates
the integration. Compile checks alone do not establish hardware behavior.

The Kindle device profile is now on `master` and covers the existing
Voyage configuration. Paperwhite support still needs its own hardware backend.

Automated integration checks (2026-11-12): Kobo release builds with profiling
both disabled and enabled passed; desktop workspace checks with both feature
configurations passed. Static Kindle reader releases with profiling disabled and enabled passed;
the default rendering-smoke binary also built successfully. Kindle diagnostic fixtures, shell syntax checks,
and two shared-profiler lifecycle/thread-isolation tests passed. The Kindle
reader ZIP passed integrity checks and contains no user settings or metadata.
The Kobo runtime dependencies match the installed Libra 2 binary.

Integration artifacts are staged under
`/Users/luu/dev/plato/target/kindle-integration/`. The installed Libra 2 binary was checksum-verified before device testing.
The pre-integration device binary and the previously untracked port planning
document are preserved in that artifact directory.

## Device updates

Before replacing an installed binary on Kobo or Kindle, save the previous
binary on that device under a unique backup name and verify the backup.
Stage and checksum the replacement before switching it into place. If the
Kindle launcher changes, back it up and update it alongside the binary.
Preserve the device's settings, books, and reading state.

Both readers default to `resume-reading = true` in `Settings.toml`. A
relaunch resumes the book at its saved position if it was open at exit and
is still available. The library stays open if it was the last view. Set
`resume-reading = false` to always start at the library.

## Voyage mixed refresh polarity (2026-11-20)

Reader setting changes can queue a partial page repaint followed by GUI menu
and toolbar updates. In inverted mode, GL16_INV reads normal-polarity bytes
while GC16_FAST GUI updates read software-inverted bytes. Voyage now drains
pending updates before switching the mapped buffer’s byte polarity, including
before the software fallback for a rejected hardware-inverted refresh. Same
polarity updates retain the submission fence used for page-turn performance.
This targets the wrong-polarity rectangles seen after weight, margin, and font
selection; confirmation on the Voyage panel is still required.
