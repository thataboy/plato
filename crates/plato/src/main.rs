mod app;
#[cfg(all(target_arch = "arm", target_env = "musl"))]
mod kindle;

use plato_core::anyhow::Error;
use crate::app::run;

fn main() -> Result<(), Error> {
    #[cfg(all(target_arch = "arm", target_env = "musl"))]
    {
        if std::env::var_os("PLATO_VOYAGE_SESSION").as_deref() != Some(std::ffi::OsStr::new("1")) {
            plato_core::anyhow::bail!("launch the Voyage alpha through its KUAL extension");
        }
        let profile = plato_core::device::kindle::current()?;
        if kindle::screen_command(profile)? {
            return Ok(());
        }
    }
    run()?;
    Ok(())
}
