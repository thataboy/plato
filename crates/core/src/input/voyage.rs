// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{ButtonCode, ButtonStatus, DeviceEvent, FingerStatus, PowerSource};
use crate::device::CURRENT_DEVICE;
use crate::device::kindle::{self, KindleProfile, InputSource as Source};
use crate::geom::Point;
use anyhow::{bail, Context, Error};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

const EVIOCGNAME: libc::c_int = 0x8080_4506_u32 as libc::c_int;
const EVIOCGRAB: libc::c_int = 0x4004_4590;

fn open_device(wanted: &[u8]) -> Result<File, Error> {
    for entry in fs::read_dir("/dev/input")? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with("event") {
            continue;
        }
        let Ok(file) = File::open(entry.path()) else {
            continue;
        };
        let mut name = [0u8; 128];
        let result = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGNAME, name.as_mut_ptr()) };
        if result >= 0 && name.split(|&byte| byte == 0).next() == Some(wanted) {
            if unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGRAB, 1 as libc::c_int) } < 0 {
                return Err(io::Error::last_os_error())
                    .with_context(|| format!("grabbing {}", entry.path().display()));
            }
            println!(
                "Grabbed {} at {}",
                String::from_utf8_lossy(wanted),
                entry.path().display()
            );
            return Ok(file);
        }
    }
    bail!(
        "Voyage input device {} was not found",
        String::from_utf8_lossy(wanted)
    )
}

pub fn events(
    profile: &'static KindleProfile,
    inverted: Arc<AtomicBool>,
    rotation: Arc<AtomicI8>,
    grab_power: bool,
) -> Result<Receiver<DeviceEvent>, Error> {
    // Open all required nodes before spawning any reader. If one grab fails,
    // dropping the files releases the earlier grabs before startup returns.
    let devices = profile.input.sources(grab_power)
        .map(|(source, name)| open_device(name).map(|file| (source, file)))
        .collect::<Result<Vec<_>, _>>()?;
    let (tx, rx) = mpsc::channel();
    for (source, file) in devices {
        let tx = tx.clone();
        let inverted = inverted.clone();
        let rotation = rotation.clone();
        thread::Builder::new().spawn(move || {
            if let Err(err) = read_events(file, profile, source, &inverted, &rotation, &tx) {
                eprintln!("Voyage evdev read failed: {err}");
            }
        })?;
    }
    Ok(rx)
}

#[derive(Clone, Copy, Default)]
struct Contact {
    id: Option<i32>,
    position: Point,
}

struct Touch {
    profile: &'static KindleProfile,
    slot: Option<usize>,
    slots: [Contact; 3],
    reported: [Contact; 3],
    last_motion: [f64; 3],
    desynced: bool,
}

impl Touch {
    fn new(profile: &'static KindleProfile) -> Self {
        Self {
            profile,
            slot: Some(0),
            slots: [Contact::default(); 3],
            reported: [Contact::default(); 3],
            last_motion: [0.0; 3],
            desynced: false,
        }
    }
}

impl Touch {
    fn feed(
        &mut self,
        kind: u16,
        code: u16,
        value: i32,
        time: f64,
        rotation: i8,
        tx: &Sender<DeviceEvent>,
    ) {
        if self.desynced {
            if (kind, code) == (0, 0) {
                self.desynced = false;
            }
            return;
        }
        match (kind, code) {
            (3, 47) => {
                self.slot = usize::try_from(value)
                    .ok()
                    .filter(|&slot| slot < self.slots.len())
            }
            (3, 53 | 54 | 57) => {
                let Some(slot) = self.slot else { return };
                let contact = &mut self.slots[slot];
                match code {
                    53 => contact.position.x = value.clamp(0, self.profile.display.width as i32 - 1),
                    54 => contact.position.y = value.clamp(0, self.profile.display.height as i32 - 1),
                    57 => contact.id = (value >= 0).then_some(value),
                    _ => unreachable!(),
                }
            }
            (0, 3) => {
                for (slot, reported) in self.slots.iter_mut().zip(&self.reported) {
                    *slot = Contact {
                        id: None,
                        position: reported.position,
                    };
                }
                self.report(time, rotation, tx);
                self.desynced = true;
            }
            (0, 0) => self.report(time, rotation, tx),
            _ => {}
        }
    }

    fn report(&mut self, time: f64, rotation: i8, tx: &Sender<DeviceEvent>) {
        for (slot, (old, new)) in self.reported.iter_mut().zip(&self.slots).enumerate() {
            let display = self.profile.display;
            let send = |id, status, position: Point| {
                let (x, y) = display.panel_to_view((position.x, position.y), rotation);
                let _ = tx.send(DeviceEvent::Finger {
                    id,
                    time,
                    status,
                    position: Point { x, y },
                });
            };
            if old.id != new.id {
                if let Some(id) = old.id {
                    let position = if new.id.is_none() {
                        new.position
                    } else {
                        old.position
                    };
                    if position != old.position {
                        send(id, FingerStatus::Motion, position);
                    }
                    send(id, FingerStatus::Up, position);
                }
                if let Some(id) = new.id {
                    send(id, FingerStatus::Down, new.position);
                }
                *old = *new;
                self.last_motion[slot] = time;
            } else if let Some(id) = new.id {
                let elapsed = time - self.last_motion[slot];
                if old.position != new.position && !(0.0..0.04).contains(&elapsed) {
                    send(id, FingerStatus::Motion, new.position);
                    *old = *new;
                    self.last_motion[slot] = time;
                }
            }
        }
    }
}

fn read_events(
    mut file: File,
    profile: &'static KindleProfile,
    source: Source,
    inverted: &AtomicBool,
    rotation: &AtomicI8,
    tx: &Sender<DeviceEvent>,
) -> io::Result<()> {
    let mut touch = Touch::new(profile);
    // The kernel's ARM32 timeval is eight bytes, independent of musl's
    // time64 layout. Decode the verified 16-byte kernel records explicitly.
    let mut event = [0; 16];
    loop {
        file.read_exact(&mut event)?;
        let time = f64::from(u32::from_le_bytes(event[0..4].try_into().unwrap()))
            + f64::from(u32::from_le_bytes(event[4..8].try_into().unwrap())) / 1e6;
        let kind = u16::from_le_bytes(event[8..10].try_into().unwrap());
        let code = u16::from_le_bytes(event[10..12].try_into().unwrap());
        let value = i32::from_le_bytes(event[12..16].try_into().unwrap());
        let orientation = rotation.load(Ordering::Relaxed);
        match source {
            Source::Touch => touch.feed(kind, code, value, time, orientation, tx),
            _ if kind == 1 => {
                let button = match (source, code) {
                    (Source::PagePress, code) if profile.input.page_keys.is_some_and(|keys| code == keys.previous || code == keys.next) => {
                        if profile.input.page_keys.is_some_and(|keys| code == keys.next)
                            ^ inverted.load(Ordering::Relaxed)
                            ^ CURRENT_DEVICE.should_invert_buttons(orientation)
                        {
                            ButtonCode::Forward
                        } else {
                            ButtonCode::Backward
                        }
                    }
                    (Source::Power, code) if code == profile.input.power_code => ButtonCode::Power,
                    _ => continue,
                };
                if let Some(status) = ButtonStatus::try_from_raw(value) {
                    if tx
                        .send(DeviceEvent::Button {
                            time,
                            code: button,
                            status,
                        })
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            }
            _ => {}
        }
    }
}

fn charger_value(attribute: &str) -> Option<u8> {
    let profile = kindle::current().ok()?;
    for device in profile.charger_devices {
        if let Ok(value) = fs::read_to_string(std::path::Path::new(device).join(attribute)) {
            return value.trim().parse().ok();
        }
    }
    for entry in fs::read_dir(profile.charger_driver).ok()?.flatten() {
        if let Ok(value) = fs::read_to_string(entry.path().join(attribute)) {
            return value.trim().parse().ok();
        }
    }
    None
}

pub fn cable_present() -> Option<bool> {
    charger_value("vbvolt")
        .or_else(|| charger_value("chgtyp"))
        .map(|value| value != 0)
}

pub fn usb_host_present() -> bool {
    if matches!(charger_value("chgtyp"), Some(1 | 2 | 9)) {
        return true;
    }
    // The controller reports address/configured states even when the
    // storage medium has been ejected by the host.
    let Ok(profile) = kindle::current() else { return false };
    fs::read_dir(profile.usb_driver)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            fs::read_to_string(entry.path().join("gadget/connected"))
                .is_ok_and(|value| value.trim() == "1")
        })
}

pub fn classify_power_connection() -> DeviceEvent {
    // Charging can be announced before the PMIC settles on a cable type.
    // Host codes follow max77696_uic_is_usbhost() in the Wario kernel.
    let mut last = None;
    for _ in 0..5 {
        thread::sleep(std::time::Duration::from_millis(300));
        let current = charger_value("chgtyp");
        if current == last {
            match current {
                Some(code @ (1 | 2 | 9)) => {
                    eprintln!("Voyage USB host detected (charger type {code}).");
                    return DeviceEvent::Plug(PowerSource::Host);
                }
                Some(code @ (3..=7 | 12)) => {
                    eprintln!("Voyage wall charger detected (charger type {code}).");
                    return DeviceEvent::Plug(PowerSource::Wall);
                }
                _ => {}
            }
        }
        last = current;
    }
    eprintln!("Voyage charger type unavailable or unsettled: {last:?}.");
    DeviceEvent::Plug(PowerSource::Host)
}

/// LIPC subscribers are tied to the app process, including watchdog kills.
pub struct StockEvents(Vec<Child>);

impl StockEvents {
    pub fn listen(tx: Sender<DeviceEvent>) -> Result<Self, Error> {
        let mut listeners = Self(Vec::new());
        for (service, events) in [
            (
                "com.lab126.powerd",
                "goingToScreenSaver,outOfScreenSaver,exitingScreenSaver,readyToSuspend,wakeupFromSuspend,charging,notCharging",
            ),
            ("com.lab126.hal", "usbPlugIn,usbConfigured,usbPlugOut"),
        ] {
            let parent = std::process::id() as libc::pid_t;
            let mut command = Command::new("lipc-wait-event");
            command.args(["-m", "-s", "0", service, events]).stdout(Stdio::piped());
            // The old Kindle kernel supports PR_SET_PDEATHSIG. Checking the
            // parent after the call also covers the fork/registration race.
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        libc::_exit(1);
                    }
                    Ok(())
                });
            }
            let mut child = command.spawn().with_context(|| format!("subscribing to {service}"))?;
            let stdout = child.stdout.take().expect("piped LIPC stdout");
            listeners.0.push(child);
            let tx = tx.clone();
            thread::Builder::new().spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    let mut fields = line.split_whitespace();
                    let Some(name) = fields.next() else { continue };
                    eprintln!("{service}: {name} {}", fields.next().unwrap_or(""));
                    let event = match (service, name) {
                        ("com.lab126.powerd", "goingToScreenSaver" | "readyToSuspend") => DeviceEvent::VoyageSleep,
                        ("com.lab126.powerd", "outOfScreenSaver" | "exitingScreenSaver") => DeviceEvent::VoyageWake,
                        ("com.lab126.powerd", "charging") | ("com.lab126.hal", "usbPlugIn") => {
                            let classification = tx.clone();
                            thread::spawn(move || {
                                classification.send(classify_power_connection()).ok();
                            });
                            continue;
                        }
                        ("com.lab126.powerd", "notCharging") => DeviceEvent::Unplug(PowerSource::Wall),
                        ("com.lab126.hal", "usbConfigured") => DeviceEvent::Plug(PowerSource::Host),
                        ("com.lab126.hal", "usbPlugOut") => DeviceEvent::Unplug(PowerSource::Host),
                        _ => continue,
                    };
                    if tx.send(event).is_err() {
                        return;
                    }
                }
                eprintln!("Voyage event subscription ended: {service}");
                tx.send(DeviceEvent::VoyageMonitorLost).ok();
            })?;
        }
        Ok(listeners)
    }
}

impl Drop for StockEvents {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn set_stock_window_manager(running: bool) -> Result<(), Error> {
    let pid = std::env::var("PLATO_VOYAGE_WM_PID")?.parse::<libc::pid_t>()?;
    if pid <= 1 || fs::read_to_string(format!("/proc/{pid}/comm"))?.trim() != "awesome" {
        bail!("stock window manager PID changed");
    }
    let signal = if running {
        libc::SIGCONT
    } else {
        libc::SIGSTOP
    };
    if unsafe { libc::kill(pid, signal) } != 0 {
        return Err(io::Error::last_os_error()).context("signalling the stock window manager");
    }
    Ok(())
}

/// BOOTTIME includes kernel suspend. MONOTONIC only advances while running.
pub fn suspended_seconds() -> Option<f64> {
    fn read(clock: libc::clockid_t) -> Option<f64> {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        (unsafe { libc::clock_gettime(clock, &mut time) } == 0)
            .then(|| time.tv_sec as f64 + time.tv_nsec as f64 / 1e9)
    }
    Some(read(libc::CLOCK_BOOTTIME)? - read(libc::CLOCK_MONOTONIC)?)
}
