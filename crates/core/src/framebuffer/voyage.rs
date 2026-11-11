// SPDX-License-Identifier: AGPL-3.0-or-later
use super::linuxfb_sys::{fix_screen_info, var_screen_info};
use super::transform::transform_dither_g16;
use super::{Framebuffer, Pixmap, UpdateMode};
use crate::color::Color;
use crate::geom::{Point, Rectangle};
use anyhow::{bail, Context, Error};
use std::fs::{self, File, OpenOptions};
use std::cell::RefCell;
#[cfg(feature = "page-profiling")]
use std::time::Instant;
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
const WAIT_FOR_SUBMISSION: libc::c_int = 0x4004_4637;
const GL16_FAST: u32 = 6;
const REAGL: u32 = 8;
const GL16_INV: u32 = 11;
const ENABLE_INVERSION: u32 = 1;
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
    serial: u32,
    pending: RefCell<Vec<u32>>,
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
            pending: RefCell::new(Vec::new()),
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

    fn copy_to_panel(&self, rect: &Rectangle, inverted: bool) {
        let rotation = self.rotation();
        let turns = quarter_turns(rotation);
        let left = rect.min.x as usize;
        let right = rect.max.x as usize;
        let width = self.width() as usize;
        for y in rect.min.y as usize..rect.max.y as usize {
            let dst = unsafe { self.pixels.as_ptr().add(y * STRIDE + left) };
            if turns == 0 && !self.monochrome && !self.dithered {
                let start = y * width;
                let row = &self.image.data()[start + left..start + right];
                let destination = unsafe { std::slice::from_raw_parts_mut(dst, row.len()) };
                if inverted {
                    for (pixel, &gray) in destination.iter_mut().zip(row) {
                        *pixel = !gray;
                    }
                } else {
                    destination.copy_from_slice(row);
                }
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
                let mut gray = if inverted { 255 - raw } else { raw };
                if self.monochrome {
                    gray = if gray < 128 { 0 } else { 255 };
                } else if self.dithered {
                    gray = transform_dither_g16(x as u32, y as u32, Color::Gray(gray)).gray();
                }
                unsafe { dst.add(x - left).write(gray) };
            }
        }
    }

    fn refresh(&mut self, rect: &Rectangle, mode: UpdateMode, hardware_inversion: bool) -> Result<u32, Error> {
        self.serial = (self.serial + 1) & 0x0fff_ffff;
        let marker = 0x5000_0000 | self.serial;
        let waveform = match mode {
            UpdateMode::Fast | UpdateMode::FastMono => DU,
            UpdateMode::Gui => GC16_FAST,
            UpdateMode::Full => GC16,
            UpdateMode::Partial if hardware_inversion => GL16_INV,
            UpdateMode::Partial if self.inverted => GC16_FAST,
            UpdateMode::Partial => GL16_FAST,
        };
        let mut update = Update {
            top: rect.min.y as u32,
            left: rect.min.x as u32,
            width: rect.width() as u32,
            height: rect.height() as u32,
            waveform,
            mode: u32::from(mode == UpdateMode::Full),
            marker,
            hist_bw_waveform: if hardware_inversion { REAGL } else { DU },
            hist_gray_waveform: match waveform {
                GL16_INV => REAGL,
                GC16 => GC16,
                _ => GC16_FAST,
            },
            temperature: 0x1000,
            flags: match waveform {
                GL16_INV => ENABLE_INVERSION,
                DU => FORCE_MONOCHROME,
                _ => 0,
            },
            alt_buffer: [0; 7],
        };
        #[cfg(feature = "page-profiling")]
        crate::voyage_profile::point("panel submit");
        #[cfg(feature = "page-profiling")]
        let started = Instant::now();
        if unsafe { libc::ioctl(self.file.as_raw_fd(), SEND_UPDATE, &mut update) } < 0 {
            let error = io::Error::last_os_error();
            if !matches!(waveform, GL16_FAST | GL16_INV) || error.raw_os_error() != Some(libc::EINVAL) {
                return Err(error).context("submitting Voyage refresh");
            }
            if hardware_inversion {
                self.copy_to_panel(rect, true);
                update.flags = 0;
                update.hist_bw_waveform = DU;
                update.hist_gray_waveform = GC16_FAST;
            }
            update.waveform = GC16_FAST;
            if unsafe { libc::ioctl(self.file.as_raw_fd(), SEND_UPDATE, &mut update) } < 0 {
                return Err(io::Error::last_os_error()).context("submitting fallback Voyage refresh");
            }
        }
        #[cfg(feature = "page-profiling")]
        let submit_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.pending.borrow_mut().push(marker);
        // Full updates must complete before the frontlight is restored.
        #[cfg(feature = "page-profiling")]
        let profile_page = crate::voyage_profile::is_page_turn()
            && matches!(mode, UpdateMode::Partial | UpdateMode::Full);
        #[cfg(not(feature = "page-profiling"))]
        let profile_page = false;
        if mode == UpdateMode::Full || update.mode == 1 || profile_page {
            self.wait(marker)?;
            #[cfg(feature = "page-profiling")]
            if profile_page {
                crate::voyage_profile::point("page display complete");
            }
        }
        #[cfg(feature = "page-profiling")]
        eprintln!("Voyage refresh: marker={marker} mode={mode:?} waveform={} requested={waveform} panel_mode={} flags={} rect={rect:?} submit_ms={submit_ms:.2} total_ms={:.2}",
                  update.waveform, update.mode, update.flags, started.elapsed().as_secs_f64() * 1000.0);
        Ok(marker)
    }
}

impl Drop for VoyageFramebuffer {
    fn drop(&mut self) {
        let pending = self.pending.borrow().clone();
        for marker in pending {
            let _ = self.wait(marker);
        }
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

    fn draw_rectangle(&mut self, rect: &Rectangle, color: Color) {
        let Some(rect) = rect.intersection(&self.rect()) else {
            return;
        };
        let stride = self.width() as usize;
        let gray = color.gray();
        for y in rect.min.y as usize..rect.max.y as usize {
            let row = y * stride;
            self.image.data_mut()[row + rect.min.x as usize..row + rect.max.x as usize].fill(gray);
        }
    }

    fn draw_framed_pixmap(&mut self, pixmap: &Pixmap, rect: &Rectangle, pt: Point) {
        let offset = pt - rect.min;
        let Some(source) = rect.intersection(&pixmap.rect())
            .and_then(|source| source.intersection(&(self.rect() - offset))) else {
            return;
        };
        let dest = source + offset;
        let width = source.width() as usize;
        let stride = self.width() as usize;
        for (src_y, dst_y) in (source.min.y as usize..source.max.y as usize)
            .zip(dest.min.y as usize..dest.max.y as usize)
        {
            let src_start = src_y * pixmap.width as usize + source.min.x as usize;
            let dst_start = dst_y * stride + dest.min.x as usize;
            let row = &mut self.image.data_mut()[dst_start..dst_start + width];
            if pixmap.samples == 1 {
                row.copy_from_slice(&pixmap.data()[src_start..src_start + width]);
            } else {
                for (x, pixel) in row.iter_mut().enumerate() {
                    *pixel = pixmap.get_pixel(source.min.x as u32 + x as u32, src_y as u32).gray();
                }
            }
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
        // The kernel must finish capturing the previous framebuffer contents
        // before we overwrite the mapped memory. Panel completion is separate.
        #[cfg(feature = "page-profiling")]
        let profile = crate::voyage_profile::stage("framebuffer fence and copy");
        #[cfg(feature = "page-profiling")]
        let fence = Instant::now();
        if mode == UpdateMode::Full {
            self.wait_for_pending_updates()?;
        } else {
            let pending = self.pending.borrow().clone();
            // Bound the queue without waiting on every recent update at once.
            if pending.len() >= 64 {
                self.wait(pending[0])?;
            }
            if let Some(&previous) = pending.last() {
                let mut marker = previous;
                if unsafe { libc::ioctl(self.file.as_raw_fd(), WAIT_FOR_SUBMISSION, &mut marker) } < 0 {
                    // Fall back to completion if the firmware rejects submission waits.
                    self.wait(previous)?;
                }
            }
        }
        #[cfg(feature = "page-profiling")]
        let fence_ms = fence.elapsed().as_secs_f64() * 1000.0;
        #[cfg(feature = "page-profiling")]
        let copy = Instant::now();
        let rect = panel_rect(rect, quarter_turns(self.rotation()));
        let hardware_inversion = mode == UpdateMode::Partial
            && self.inverted && !self.monochrome && !self.dithered;
        self.copy_to_panel(&rect, self.inverted && !hardware_inversion);
        #[cfg(feature = "page-profiling")]
        eprintln!("Voyage framebuffer: fence_ms={fence_ms:.2} copy_ms={:.2}", copy.elapsed().as_secs_f64() * 1000.0);
        #[cfg(feature = "page-profiling")]
        drop(profile);
        self.refresh(&rect, mode, hardware_inversion)
    }

    fn wait_for_pending_updates(&self) -> Result<(), Error> {
        let pending = self.pending.borrow().clone();
        #[cfg(feature = "page-profiling")]
        let started = Instant::now();
        for marker in pending {
            self.wait(marker)?;
        }
        #[cfg(feature = "page-profiling")]
        eprintln!("Voyage pending drain: wait_ms={:.2}", started.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    }

    fn wait(&self, marker: u32) -> Result<i32, Error> {
        if !self.pending.borrow().contains(&marker) {
            return Ok(0);
        }
        #[cfg(feature = "page-profiling")]
        let started = Instant::now();
        let mut completion = [marker, 0];
        let result = unsafe {
            libc::ioctl(self.file.as_raw_fd(), WAIT_FOR_UPDATE, completion.as_mut_ptr())
        };
        if result < 0 {
            return Err(io::Error::last_os_error()).context("waiting for Voyage refresh");
        }
        self.pending.borrow_mut().retain(|value| *value != marker);
        #[cfg(feature = "page-profiling")]
        eprintln!("Voyage completion: marker={marker} wait_ms={:.2}", started.elapsed().as_secs_f64() * 1000.0);
        Ok(result)
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
