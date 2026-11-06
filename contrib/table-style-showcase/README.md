# Table borders and backgrounds

Run `python3 contrib/table-style-showcase/generate.py` to rebuild
`contrib/table-style-showcase.epub` using Python's standard library.

The four chapters cover table backgrounds and outer borders, individual `td` and
`th` backgrounds and per-side borders, unequal-height and empty cells, `colspan`,
and a row continuing over multiple pages. Cell decoration fills the row height;
backgrounds paint behind content and borders reserve space for padding and text.
The table's outer decoration follows the actual table width.

Solid, dotted, dashed, and fallback styles follow the box-style showcase.
Adjacent cell edges are separate: `border-collapse`, `border-spacing`, `rowspan`,
and repeated page headers are not implemented by this change.

Enable publisher/document CSS in Plato. Turn through the long-table chapter in
both directions and check that the short cell's background continues alongside
the long cell. The generated EPUB validates its XML, ZIP integrity, and mimetype
entry, and is reproducible.

Validation: 53 core tests passed, including three table regression tests; all six
rendered pages were reviewed, and the Kobo ARM release build was checked.
As with the prior box-style work, builds use isolated copies with valid lockfiles
because the checkout's existing Cargo.lock contains duplicate keys. Native tests
match Homebrew MuPDF 1.27.2; the Kobo executable uses MuPDF 1.27.0, matching the
installed device library.

Deployment: the binary at `target/table-style-showcase/plato` was installed as
`/Volumes/KOBOeReader/.adds/plato/plato`. The previous binary is preserved beside
it as `plato-before-table-styles-20261106`. Both sample EPUBs are in the device's
`library` directory. SHA-256 checks verified all copies. On-device interaction
still needs to be checked after USB eject and restarting Plato.
