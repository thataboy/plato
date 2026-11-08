// SPDX-License-Identifier: AGPL-3.0-or-later
use super::linuxfb_sys::{fix_screen_info, var_screen_info};
use super::transform::transform_dither_g16;
use super::{Framebuffer, Pixmap, UpdateMode};
use crate::color::Color;
use crate::geom::{Point, Rectangle};
use anyhow::{bail, Context, Error};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicI8, Ordering};
use std::sync::Arc;

const WIDTH: u32 = 1072;
const HEIGHT: u32 = 1448;
const ROTATION: i8 = 3;
const STRIDE: usize = 1088;
const MAP_LEN: usize = STRIDE * HEIGHT as usize;

// Use the existing Plato rotation IDs, with 3 corresponding to portrait.
fn quarter_turns(rotation: i8) -> i8 {
    (rotation - ROTATION).rem_euclid(4)
}

fn view_to_panel(p: Point, turns: i8) -> Point {
    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    match turns {
        1 => Point {
            x: w - 1 - p.y,
            y: p.x,
        },
        2 => Point {
            x: w - 1 - p.x,
            y: h - 1 - p.y,
        },
        3 => Point {
            x: p.y,
            y: h - 1 - p.x,
        },
        _ => p,
    }
}

pub(crate) fn panel_to_view(p: Point, rotation: i8) -> Point {
    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    match quarter_turns(rotation) {
        1 => Point {
            x: p.y,
            y: w - 1 - p.x,
        },
        2 => Point {
            x: w - 1 - p.x,
            y: h - 1 - p.y,
        },
        3 => Point {
            x: h - 1 - p.y,
            y: p.x,
        },
        _ => p,
    }
}

fn panel_rect(rect: Rectangle, turns: i8) -> Rectangle {
    let a = view_to_panel(rect.min, turns);
    let b = view_to_panel(
        Point {
            x: rect.max.x - 1,
            y: rect.max.y - 1,
        },
        turns,
    );
    Rectangle {
        min: Point {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
        },
        max: Point {
            x: a.x.max(b.x) + 1,
            y: a.y.max(b.y) + 1,
        },
    }
}

// Voyage uses the older 72-byte Kindle EPDC update ABI.
const SEND_UPDATE: libc::c_int = 0x4048_462e;
const WAIT_FOR_UPDATE: libc::c_int = 0xc008_462f_u32 as libc::c_int;
const DU: u32 = 1;
const GC16: u32 = 2;
const GC16_FAST: u32 = 3;
const FORCE_MONOCHROME: u32 = 2;

#[repr(C)]
struct Update {
    top: u32,
    left: u32,
    width: u32,
    height: u32,
    waveform: u32,
    mode: u32,
    marker: u32,
    hist_bw_waveform: u32,
    hist_gray_waveform: u32,
    temperature: i32,
    flags: u32,
    alt_buffer: [u32; 7],
}

const _: () = assert!(size_of::<Update>() == 72);

pub struct VoyageFramebuffer {
    file: File,
    pixels: NonNull<u8>,
    image: Pixmap,
    rotation: Arc<AtomicI8>,
    serial: u8,
    monochrome: bool,
    inverted: bool,
    dithered: bool,
}

impl VoyageFramebuffer {
    pub fn open() -> Result<Self, Error> {
        let file = OpenOptions::new().read(true).write(true).open("/dev/fb0")?;
        let var = var_screen_info(&file)?;
        let fix = fix_screen_info(&file)?;
        if (var.xres, var.yres, var.xoffset, var.yoffset) != (WIDTH, HEIGHT, 0, 0)
            || var.rotate != ROTATION as u32
            || var.bits_per_pixel != 8
            || var.grayscale != 1
            || fix.line_length as usize != STRIDE
            || (fix.smem_len as usize) < MAP_LEN
        {
            bail!("Voyage alpha requires the verified portrait grayscale framebuffer mode");
        }

        // The fixed info reports enough mapped storage for every visible row.
        let mapping = unsafe {
            libc::mmap(
                ptr::null_mut(),
                MAP_LEN,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if mapping == libc::MAP_FAILED {
            return Err(io::Error::last_os_error()).context("mapping Voyage framebuffer");
        }
        let pixels = NonNull::new(mapping.cast()).context("null framebuffer mapping")?;
        Ok(Self {
            file,
            pixels,
            image: Pixmap::new(WIDTH, HEIGHT, 1),
            rotation: Arc::new(AtomicI8::new(ROTATION)),
            serial: 0,
            monochrome: false,
            inverted: false,
            dithered: false,
        })
    }

    pub fn rotation_handle(&self) -> Arc<AtomicI8> {
        self.rotation.clone()
    }

    pub fn save_stock_screen(&self, path: &Path) -> Result<(), Error> {
        let mut image = Pixmap::new(WIDTH, HEIGHT, 1);
        for (row, dst) in image
            .data_mut()
            .chunks_exact_mut(WIDTH as usize)
            .enumerate()
        {
            unsafe {
                ptr::copy_nonoverlapping(
                    self.pixels.as_ptr().add(row * STRIDE),
                    dst.as_mut_ptr(),
                    dst.len(),
                );
            }
        }
        fs::write(path, image.data()).context("saving stock framebuffer")
    }

    pub fn restore_stock_screen(&mut self, path: &Path) -> Result<(), Error> {
        let original = fs::read(path)?;
        if original.len() != self.image.data().len() {
            bail!("stock framebuffer backup has the wrong size");
        }
        self.image.data_mut().copy_from_slice(&original);
        self.update(&self.rect(), UpdateMode::Full)?;
        Ok(())
    }

    fn refresh(&mut self, rect: &Rectangle, mode: UpdateMode) -> Result<u32, Error> {
        self.serial = self.serial.wrapping_add(1);
        let marker = 0x5000_0000 | ((std::process::id() & 0xffff) << 8) | u32::from(self.serial);
        let waveform = match mode {
            UpdateMode::Fast | UpdateMode::FastMono => DU,
            UpdateMode::Gui => GC16_FAST,
            UpdateMode::Full | UpdateMode::Partial => GC16,
        };
        let update = Update {
            top: rect.min.y as u32,
            left: rect.min.x as u32,
            width: rect.width() as u32,
            height: rect.height() as u32,
            waveform,
            mode: u32::from(mode == UpdateMode::Full),
            marker,
            hist_bw_waveform: DU,
            hist_gray_waveform: if waveform == GC16 { GC16 } else { GC16_FAST },
            temperature: 0x1000,
            flags: if waveform == DU { FORCE_MONOCHROME } else { 0 },
            alt_buffer: [0; 7],
        };
        if unsafe { libc::ioctl(self.file.as_raw_fd(), SEND_UPDATE, &update) } < 0 {
            return Err(io::Error::last_os_error()).context("submitting Voyage refresh");
        }
        // Wait here so the shadow image can be copied again without racing
        // the panel driver. DU keeps drag and selection previews fast.
        let mut completion = [marker, 0];
        if unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                WAIT_FOR_UPDATE,
                completion.as_mut_ptr(),
            )
        } < 0
        {
            return Err(io::Error::last_os_error()).context("waiting for Voyage refresh");
        }
        Ok(marker)
    }
}

impl Drop for VoyageFramebuffer {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.pixels.as_ptr().cast(), MAP_LEN) };
    }
}

impl Framebuffer for VoyageFramebuffer {
    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width() && y < self.height() {
            self.image.set_pixel(x, y, color);
        }
    }

    fn set_blended_pixel(&mut self, x: u32, y: u32, color: Color, alpha: f32) {
        if x < self.width() && y < self.height() {
            self.image.set_blended_pixel(x, y, color, alpha);
        }
    }

    fn invert_region(&mut self, rect: &Rectangle) {
        if let Some(rect) = rect.intersection(&self.rect()) {
            self.image.invert_region(&rect);
        }
    }

    fn shift_region(&mut self, rect: &Rectangle, drift: u8) {
        if let Some(rect) = rect.intersection(&self.rect()) {
            self.image.shift_region(&rect, drift);
        }
    }

    fn update(&mut self, rect: &Rectangle, mode: UpdateMode) -> Result<u32, Error> {
        let Some(rect) = rect.intersection(&self.rect()) else {
            return Ok(0);
        };
        let rotation = self.rotation();
        let turns = quarter_turns(rotation);
        let rect = panel_rect(rect, turns);
        let left = rect.min.x as usize;
        let right = rect.max.x as usize;
        let width = self.width() as usize;
        for y in rect.min.y as usize..rect.max.y as usize {
            let dst = unsafe { self.pixels.as_ptr().add(y * STRIDE + left) };
            if turns == 0 && !self.inverted && !self.monochrome && !self.dithered {
                let start = y * width;
                let row = &self.image.data()[start + left..start + right];
                unsafe { ptr::copy_nonoverlapping(row.as_ptr(), dst, row.len()) };
                continue;
            }
            for x in left..right {
                let source = panel_to_view(
                    Point {
                        x: x as i32,
                        y: y as i32,
                    },
                    rotation,
                );
                let raw = self.image.data()[source.y as usize * width + source.x as usize];
                let mut gray = if self.inverted { 255 - raw } else { raw };
                if self.monochrome {
                    gray = if gray < 128 { 0 } else { 255 };
                } else if self.dithered {
                    gray = transform_dither_g16(x as u32, y as u32, Color::Gray(gray)).gray();
                }
                unsafe { dst.add(x - left).write(gray) };
            }
        }
        self.refresh(&rect, mode)
    }

    fn wait(&self, _: u32) -> Result<i32, Error> {
        Ok(0)
    }
    fn save(&self, path: &str) -> Result<(), Error> {
        self.image.save(path)
    }
    fn set_rotation(&mut self, rotation: i8) -> Result<(u32, u32), Error> {
        if !(0..=3).contains(&rotation) {
            bail!("invalid Voyage rotation {rotation}");
        }
        if rotation != self.rotation() {
            let (width, height) = if quarter_turns(rotation) % 2 == 0 {
                (WIDTH, HEIGHT)
            } else {
                (HEIGHT, WIDTH)
            };
            self.image = Pixmap::new(width, height, 1);
            self.rotation.store(rotation, Ordering::Relaxed);
            eprintln!(
                "Voyage rotation: {} degrees ({width}x{height}).",
                i16::from(quarter_turns(rotation)) * 90
            );
        }
        Ok(self.dims())
    }
    fn set_monochrome(&mut self, enable: bool) {
        self.monochrome = enable;
    }
    fn set_dithered(&mut self, enable: bool) {
        self.dithered = enable;
    }
    fn set_inverted(&mut self, enable: bool) {
        self.inverted = enable;
    }
    fn monochrome(&self) -> bool {
        self.monochrome
    }
    fn dithered(&self) -> bool {
        self.dithered
    }
    fn inverted(&self) -> bool {
        self.inverted
    }
    fn width(&self) -> u32 {
        self.image.width
    }
    fn height(&self) -> u32 {
        self.image.height
    }
    fn rotation(&self) -> i8 {
        self.rotation.load(Ordering::Relaxed)
    }
}
