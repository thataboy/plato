// SPDX-License-Identifier: AGPL-3.0-or-later
use plato_core::anyhow::{Context, Error};
use plato_core::document::{open, Location};
use std::path::Path;

fn render(path: &Path) -> Result<(), Error> {
    let mut document = open(path).context("cannot open document")?;
    document.layout(1072, 1448, 11.0, 300);
    let (page, _) = document
        .pixmap(Location::Exact(0), 1.0, 1)
        .context("cannot render first page")?;
    let dark = page.data().iter().filter(|&&pixel| pixel < 128).count();
    let light = page.data().iter().filter(|&&pixel| pixel >= 224).count();
    println!(
        "{}: {} x {}, {dark} dark pixels, {light} light pixels",
        path.display(),
        page.width,
        page.height
    );
    if dark == 0 || light == 0 {
        return Err(Error::msg("fixture rendered without visible text contrast"));
    }
    Ok(())
}

fn main() -> Result<(), Error> {
    println!("Plato native rendering smoke test");
    for filename in ["fixtures/smoke.pdf", "fixtures/smoke.epub"] {
        render(Path::new(filename)).with_context(|| format!("rendering {filename}"))?;
    }
    println!("END OF PLATO RENDER SMOKE");
    Ok(())
}
