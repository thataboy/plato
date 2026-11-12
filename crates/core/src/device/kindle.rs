// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kindle identity and device data. A build target is not a device identity.
//! Only the verified Voyage configuration is currently supported.
use std::{fmt, fs, sync::OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayBackend {
    Voyage,
}

#[derive(Debug, Clone, Copy)]
pub struct DisplayProfile {
    pub backend: DisplayBackend,
    pub path: &'static str,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub rotation: i8,
    pub dpi: u16,
}

impl DisplayProfile {
    pub fn map_len(self) -> usize {
        self.stride * self.height as usize
    }
    pub fn quarter_turns(self, rotation: i8) -> i8 {
        (rotation - self.rotation).rem_euclid(4)
    }
    pub fn view_dims(self, rotation: i8) -> (u32, u32) {
        if self.quarter_turns(rotation) % 2 == 0 {
            (self.width, self.height)
        } else {
            (self.height, self.width)
        }
    }
    pub fn view_to_panel(self, (x, y): (i32, i32), rotation: i8) -> (i32, i32) {
        let (w, h) = (self.width as i32, self.height as i32);
        match self.quarter_turns(rotation) {
            1 => (w - 1 - y, x),
            2 => (w - 1 - x, h - 1 - y),
            3 => (y, h - 1 - x),
            _ => (x, y),
        }
    }
    pub fn panel_to_view(self, (x, y): (i32, i32), rotation: i8) -> (i32, i32) {
        let (w, h) = (self.width as i32, self.height as i32);
        match self.quarter_turns(rotation) {
            1 => (y, w - 1 - x),
            2 => (w - 1 - x, h - 1 - y),
            3 => (h - 1 - y, x),
            _ => (x, y),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PageKeys {
    pub name: &'static [u8],
    pub previous: u16,
    pub next: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSource {
    Touch,
    PagePress,
    Power,
}

#[derive(Debug, Clone, Copy)]
pub struct InputProfile {
    pub touch: &'static [u8],
    pub page_keys: Option<PageKeys>,
    pub power: &'static [u8],
    pub power_code: u16,
}

impl InputProfile {
    /// A buttonless profile never attempts to open a page-key device.
    /// Declared devices remain required: discovery/grab errors are not hidden.
    pub fn sources(self, grab_power: bool) -> impl Iterator<Item = (InputSource, &'static [u8])> {
        [
            Some((InputSource::Touch, self.touch)),
            self.page_keys
                .map(|keys| (InputSource::PagePress, keys.name)),
            grab_power.then_some((InputSource::Power, self.power)),
        ]
        .into_iter()
        .flatten()
    }
}

#[derive(Debug)]
pub struct KindleProfile {
    pub product: &'static str,
    pub name: &'static str,
    pub firmware: &'static str,
    pub display: DisplayProfile,
    pub input: InputProfile,
    pub battery_capacity: &'static str,
    pub battery_charging: &'static str,
    pub brightness: &'static str,
    pub max_brightness: u8,
    pub charger_devices: &'static [&'static str],
    pub charger_driver: &'static str,
    pub usb_driver: &'static str,
}

pub static VOYAGE: KindleProfile = KindleProfile {
    product: "kindle-voyage",
    name: "Kindle Voyage",
    firmware: "Kindle 5.10.3",
    display: DisplayProfile {
        backend: DisplayBackend::Voyage,
        path: "/dev/fb0",
        width: 1072,
        height: 1448,
        stride: 1088,
        rotation: 3,
        dpi: 300,
    },
    input: InputProfile {
        touch: b"cyttsp4_mt",
        page_keys: Some(PageKeys {
            name: b"fsr_keypad",
            previous: 104,
            next: 109,
        }),
        power: b"max77696-onkey",
        power_code: 116,
    },
    battery_capacity: "/sys/devices/system/wario_battery/wario_battery0/battery_capacity",
    battery_charging: "/sys/devices/system/wario_charger/wario_charger0/charging",
    brightness: "/sys/class/backlight/max77696-bl/brightness",
    max_brightness: 24,
    charger_devices: &[
        "/sys/bus/platform/devices/max77696-uic",
        "/sys/class/power_supply/max77696-uic/device",
    ],
    charger_driver: "/sys/bus/platform/drivers/max77696-uic",
    usb_driver: "/sys/bus/platform/drivers/fsl-usb2-udc",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionError(pub &'static str);
impl fmt::Display for DetectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for DetectionError {}

/// Model-code positions and Voyage codes follow KOReader's Kindle device.lua
/// at 739291bfbb52b23e430cf1ff92b3a0e2e16d3e6a. Never log the full serial.
pub fn identify(serial: &str) -> Result<&'static KindleProfile, DetectionError> {
    let serial = serial.trim().as_bytes();
    if serial.len() != 16 || !serial.iter().all(u8::is_ascii_alphanumeric) {
        return Err(DetectionError("invalid Kindle device identity"));
    }
    if matches!(serial[0], b'B' | b'9')
        && matches!(&serial[2..4], b"13" | b"54" | b"2A" | b"4F" | b"52" | b"53")
    {
        Ok(&VOYAGE)
    } else {
        Err(DetectionError(
            "unsupported Kindle model; only Voyage is supported",
        ))
    }
}

pub fn validate_firmware(profile: &KindleProfile, version: &str) -> Result<(), DetectionError> {
    let version = version.trim();
    if version == profile.firmware
        || version
            .strip_prefix(profile.firmware)
            .is_some_and(|rest| rest.starts_with(' '))
    {
        Ok(())
    } else {
        Err(DetectionError(
            "unsupported Kindle firmware; Voyage requires 5.10.3",
        ))
    }
}

pub fn current() -> Result<&'static KindleProfile, DetectionError> {
    static PROFILE: OnceLock<Result<&'static KindleProfile, DetectionError>> = OnceLock::new();
    *PROFILE.get_or_init(|| {
        let serial = fs::read_to_string("/proc/usid")
            .map_err(|_| DetectionError("cannot read Kindle device identity"))?;
        let profile = identify(&serial)?;
        let version = fs::read_to_string("/etc/prettyversion.txt")
            .map_err(|_| DetectionError("cannot read Kindle firmware version"))?;
        validate_firmware(profile, &version)?;
        Ok(profile)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifies_voyage_variants_without_accepting_other_kindles() {
        for lead in ["B", "9"] {
            for code in ["13", "54", "2A", "4F", "52", "53"] {
                assert_eq!(
                    identify(&format!("{lead}0{code}000000000000\n"))
                        .unwrap()
                        .product,
                    "kindle-voyage"
                );
            }
        }
        for serial in [
            "",
            "B013",
            "B0D4000000000000",
            "G0033W0000000000",
            "G013000000000000",
            "B01300000000000!",
            "éééééééé",
        ] {
            assert!(identify(serial).is_err(), "unexpected accepted fixture");
        }
    }
    #[test]
    fn firmware_guard_preserves_exact_version_boundary() {
        for version in ["Kindle 5.10.3", "Kindle 5.10.3 (build)\n"] {
            assert!(validate_firmware(&VOYAGE, version).is_ok());
        }
        for version in ["", "Kindle 5.10.30", "Kindle 5.10.3.1", "Kindle 5.18.1"] {
            assert!(validate_firmware(&VOYAGE, version).is_err());
        }
    }
    #[test]
    fn buttonless_input_profile_skips_pagepress() {
        let input = InputProfile {
            page_keys: None,
            ..VOYAGE.input
        };
        assert_eq!(
            input.sources(false).collect::<Vec<_>>(),
            vec![(InputSource::Touch, input.touch)]
        );
        assert_eq!(
            input
                .sources(true)
                .map(|(source, _)| source)
                .collect::<Vec<_>>(),
            vec![InputSource::Touch, InputSource::Power]
        );
        assert_eq!(
            VOYAGE
                .input
                .sources(false)
                .map(|(source, _)| source)
                .collect::<Vec<_>>(),
            vec![InputSource::Touch, InputSource::PagePress]
        );
    }
    #[test]
    fn rotations_round_trip_every_voyage_pixel() {
        let display = VOYAGE.display;
        for (rotation, expected_origin) in [
            (3, (0, 0)),
            (0, (1071, 0)),
            (1, (1071, 1447)),
            (2, (0, 1447)),
        ] {
            assert_eq!(display.view_to_panel((0, 0), rotation), expected_origin);
            let (w, h) = display.view_dims(rotation);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let panel = display.view_to_panel((x, y), rotation);
                    assert!((0..display.width as i32).contains(&panel.0));
                    assert!((0..display.height as i32).contains(&panel.1));
                    assert_eq!(display.panel_to_view(panel, rotation), (x, y));
                }
            }
        }
    }
}
