// SPDX-License-Identifier: AGPL-3.0-or-later
use plato_core::anyhow::{bail, Error};
use plato_core::battery::{Battery, Status};
use plato_core::framebuffer::voyage::VoyageFramebuffer;
use plato_core::settings::Settings;
use std::fs;
use std::path::Path;

pub fn screen_command() -> Result<bool, Error> {
    let mut args = std::env::args_os().skip(1);
    let Some(action) = args.next() else {
        return Ok(false);
    };
    let Some(path) = args.next() else {
        bail!("missing screen backup path")
    };
    if args.next().is_some() {
        bail!("unexpected screen command argument");
    }
    match action.to_str() {
        Some("--save-stock-screen") => {
            VoyageFramebuffer::open()?.save_stock_screen(Path::new(&path))?
        }
        Some("--restore-stock-screen") => {
            VoyageFramebuffer::open()?.restore_stock_screen(Path::new(&path))?
        }
        _ => bail!("unknown Voyage command"),
    }
    Ok(true)
}

pub fn prepare_settings(settings: &mut Settings) {
    settings.import.startup_trigger = true;
    settings.auto_suspend = 0.0;
    settings.auto_power_off = 0.0;
    settings.wifi = false;
    settings.frontlight = false;
}

pub struct VoyageBattery;

impl Battery for VoyageBattery {
    fn capacity(&mut self) -> Result<Vec<f32>, Error> {
        let level = fs::read_to_string(
            "/sys/devices/system/wario_battery/wario_battery0/battery_capacity",
        )?;
        Ok(vec![level.trim().parse()?])
    }

    fn status(&mut self) -> Result<Vec<Status>, Error> {
        let charging =
            fs::read_to_string("/sys/devices/system/wario_charger/wario_charger0/charging")?;
        Ok(vec![if charging.trim() == "1" {
            Status::Charging
        } else {
            Status::Discharging
        }])
    }
}

const BRIGHTNESS: &str = "/sys/class/backlight/max77696-bl/brightness";
const POWERD: &str = "com.lab126.powerd";
const STOCK_MAX: u8 = 24;

fn run_lipc(program: &str, args: &[&str]) -> Result<String, Error> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            state => {
                let _ = child.kill();
                let _ = child.wait();
                state?;
                bail!("{program} timed out");
            }
        }
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("{program} failed: {}", output.status);
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub struct VoyageFrontlight {
    // Zero means physically off. The other 25 steps map to stock levels 0–24.
    step: u8,
    hardware: Option<u32>,
}

impl VoyageFrontlight {
    pub fn read() -> Result<Self, Error> {
        let native: u8 = run_lipc("lipc-get-prop", &[POWERD, "flIntensity"])?
            .trim()
            .parse()?;
        if native > STOCK_MAX {
            bail!("unexpected stock frontlight level {native}");
        }
        let hardware: u32 = fs::read_to_string(BRIGHTNESS)?.trim().parse()?;
        Ok(Self {
            step: if hardware == 0 { 0 } else { native + 1 },
            hardware: Some(hardware),
        })
    }

    fn apply(step: u8) -> Result<(), Error> {
        let native = step.saturating_sub(1).to_string();
        run_lipc("lipc-set-prop", &["-i", POWERD, "flIntensity", &native])?;
        if step == 0 {
            // On Voyage, stock level 0 leaves a faint light.
            fs::write(BRIGHTNESS, b"0\n")?;
        }
        Ok(())
    }
}

impl plato_core::frontlight::Frontlight for VoyageFrontlight {
    fn set_intensity(&mut self, value: f32) {
        let step = (value.clamp(0.0, 100.0) * (f32::from(STOCK_MAX) + 1.0) / 100.0).round() as u8;
        if step == self.step {
            return;
        }
        match Self::apply(step) {
            Ok(()) => {
                self.step = step;
                self.hardware = fs::read_to_string(BRIGHTNESS)
                    .ok()
                    .and_then(|value| value.trim().parse().ok());
                eprintln!("Voyage frontlight step: {step}/25");
            }
            Err(error) => eprintln!("Voyage frontlight update failed: {error:#}"),
        }
    }

    fn restore(&mut self) {
        let result = Self::apply(self.step).and_then(|()| {
            if let Some(brightness) = self.hardware {
                let current: u32 = fs::read_to_string(BRIGHTNESS)?.trim().parse()?;
                if current != brightness {
                    // Stock can dim the hardware while retaining the same
                    // LIPC level, so setting that level may be a no-op.
                    fs::write(BRIGHTNESS, format!("{brightness}\n"))?;
                }
            }
            Ok(())
        });
        match result {
            Ok(()) => eprintln!("Voyage frontlight restored: {}/25", self.step),
            Err(error) => eprintln!("Voyage frontlight restore failed: {error:#}"),
        }
    }

    fn set_warmth(&mut self, _: f32) {}

    fn levels(&self) -> plato_core::frontlight::LightLevels {
        plato_core::frontlight::LightLevels {
            intensity: f32::from(self.step) * 100.0 / (f32::from(STOCK_MAX) + 1.0),
            warmth: 0.0,
        }
    }
}

pub fn note_activity() {
    if let Err(error) = run_lipc(
        "lipc-set-prop",
        &["-i", POWERD, "touchScreenSaverTimeout", "1"],
    ) {
        eprintln!("Voyage activity notification failed: {error:#}");
    }
}

const WIFI_SERVICE: &str = "com.lab126.wifid";
const WIRELESS_SERVICE: &str = "com.lab126.cmd";

fn set_stock_wifi(enable: bool) -> Result<(), Error> {
    let radio = (WIRELESS_SERVICE, "wirelessEnable");
    let wifi = (WIFI_SERVICE, "enable");
    let (value, properties) = if enable {
        ("1", [radio, wifi])
    } else {
        ("0", [wifi, radio])
    };
    for (service, property) in properties {
        run_lipc("lipc-set-prop", &["-i", service, property, value])?;
    }
    Ok(())
}

fn wifi_status() -> Result<(bool, bool), Error> {
    let enabled = match run_lipc("lipc-get-prop", &["-i", WIFI_SERVICE, "enable"])?.trim() {
        "0" => false,
        "1" => true,
        other => bail!("unexpected Kindle Wi-Fi setting {other:?}"),
    };
    let online =
        enabled && run_lipc("lipc-get-prop", &[WIFI_SERVICE, "cmState"])?.trim() == "CONNECTED";
    Ok((enabled, online))
}

pub fn wifi_worker(
    events: std::sync::mpsc::Sender<plato_core::view::Event>,
) -> std::sync::mpsc::Sender<bool> {
    use plato_core::input::DeviceEvent;
    use plato_core::view::Event;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::Duration;

    let (requests, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut last = None;
        let mut delay = Duration::ZERO;
        let mut reported_error = false;
        loop {
            match receiver.recv_timeout(delay) {
                Ok(enable) => match set_stock_wifi(enable) {
                    Ok(()) => {
                        let message = if enable {
                            "Wi-Fi enabled. Looking for a saved network."
                        } else {
                            "Wi-Fi disabled."
                        };
                        events.send(Event::Notify(message.into())).ok();
                    }
                    Err(error) => {
                        eprintln!("Voyage Wi-Fi control failed: {error:#}");
                        events
                            .send(Event::Notify("Could not change Kindle Wi-Fi.".into()))
                            .ok();
                    }
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            match wifi_status() {
                Ok((enabled, online)) => {
                    reported_error = false;
                    delay = Duration::from_secs(if enabled { 3 } else { 15 });
                    if last != Some((enabled, online)) {
                        eprintln!("Voyage Wi-Fi: enabled={enabled}, connected={online}.");
                        if events
                            .send(Event::Device(DeviceEvent::VoyageWifi { enabled, online }))
                            .is_err()
                        {
                            return;
                        }
                        last = Some((enabled, online));
                    }
                }
                Err(error) => {
                    delay = Duration::from_secs(10);
                    if !reported_error {
                        eprintln!("Voyage Wi-Fi status unavailable: {error:#}");
                        reported_error = true;
                    }
                }
            }
        }
    });
    requests
}
