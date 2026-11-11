# Rendering handoff

Kobo work is on `master` in `/Users/luu/dev/plato`. Voyage work is on
`codex/voyage-page-profiling` in `/Users/luu/dev/plato-kindle-voyage`.
See `doc/KINDLE_VOYAGE_PORT.md` in the Voyage worktree for the port history,
device setup, and build details. The line-height override is preserved on
`codex/voyage-line-height-override`. The stash named “Voyage display
experiments through 0.5.4” preserves that checkpoint.

Both branches have a compile-time `page-profiling` feature, disabled by
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
