// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded framebuffer test for the geometry observed on the Voyage.
use super::{FBIOGET_FSCREENINFO, FBIOGET_VSCREENINFO, read_ioctl};
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

const LEFT: usize = 48;
const TOP: usize = 24;
const WIDTH: usize = 720;
const HEIGHT: usize = 160;
const STRIDE: usize = 1088;
const MAP_LEN: usize = (TOP + HEIGHT - 1) * STRIDE + LEFT + WIDTH;
const PATCH_BYTES: usize = WIDTH * HEIGHT;

// Older Kindle ABI. See KOReader's pinned ffi/mxcfb_kindle_h.lua.
const MXCFB_SEND_UPDATE: i32 = 0x4048_462e;
const MXCFB_WAIT_FOR_UPDATE_COMPLETE: i32 = 0xc008_462f_u32 as i32;
const GC16: u32 = 2;
const DU: u32 = 1;
const FULL: u32 = 1;
const TEMP_USE_AMBIENT: i32 = 0x1000;

#[repr(C)]
struct Rect {
    top: u32,
    left: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct Update {
    region: Rect,
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

unsafe extern "C" {
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> i32;
}

struct Framebuffer {
    file: File,
    pixels: *mut u8,
}

impl Framebuffer {
    fn open() -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open("/dev/fb0")?;
        let mut var = [0u32; 40];
        let mut fix = [0u32; 17];
        read_ioctl(&file, FBIOGET_VSCREENINFO, &mut var)?;
        read_ioctl(&file, FBIOGET_FSCREENINFO, &mut fix)?;
        println!(
            "active={}x{} virtual={}x{} offset={},{} bpp={} grayscale={} rotate={} stride={} memory={}",
            var[0],
            var[1],
            var[2],
            var[3],
            var[4],
            var[5],
            var[6],
            var[7],
            var[34],
            fix[11],
            fix[5]
        );
        if var[..8] != [1072, 1448, 1088, 6144, 0, 0, 8, 1]
            || var[34] != 3
            || fix[11] as usize != STRIDE
            || (fix[5] as usize) < MAP_LEN
        {
            return Err(io::Error::other(
                "unexpected framebuffer mode; patch untouched",
            ));
        }
        // MAP_SHARED, PROT_READ | PROT_WRITE. The checked fixed info covers
        // the mapping and every copied row is within that mapped region.
        let addr = unsafe { mmap(ptr::null_mut(), MAP_LEN, 3, 1, file.as_raw_fd(), 0) };
        if addr as usize == usize::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            file,
            pixels: addr.cast(),
        })
    }

    fn snapshot(&self) -> Vec<u8> {
        let mut patch = vec![0; PATCH_BYTES];
        for (row, dst) in patch.chunks_exact_mut(WIDTH).enumerate() {
            let offset = (TOP + row) * STRIDE + LEFT;
            // The heap buffer and framebuffer mapping cannot overlap.
            unsafe { ptr::copy_nonoverlapping(self.pixels.add(offset), dst.as_mut_ptr(), WIDTH) };
        }
        patch
    }

    fn paint(&self, patch: &[u8]) {
        assert_eq!(patch.len(), PATCH_BYTES);
        for (row, src) in patch.chunks_exact(WIDTH).enumerate() {
            let offset = (TOP + row) * STRIDE + LEFT;
            unsafe { ptr::copy_nonoverlapping(src.as_ptr(), self.pixels.add(offset), WIDTH) };
        }
    }

    fn refresh(&self) -> io::Result<()> {
        let marker = 0x504c_0000 | (std::process::id() & 0xffff);
        let mut update = Update {
            region: Rect {
                top: TOP as u32,
                left: LEFT as u32,
                width: WIDTH as u32,
                height: HEIGHT as u32,
            },
            waveform: GC16,
            mode: FULL,
            marker,
            hist_bw_waveform: DU,
            hist_gray_waveform: GC16,
            temperature: TEMP_USE_AMBIENT,
            flags: 0,
            alt_buffer: [0; 7],
        };
        println!("Submitting GC16 marker {marker:#x} for {WIDTH}x{HEIGHT} at {LEFT},{TOP}");
        let started = Instant::now();
        read_ioctl(&self.file, MXCFB_SEND_UPDATE, &mut update)
            .map_err(|err| io::Error::other(format!("MXCFB_SEND_UPDATE: {err}")))?;
        println!("Update accepted in {:?}", started.elapsed());
        let mut completion = [marker, 0];
        let started = Instant::now();
        read_ioctl(&self.file, MXCFB_WAIT_FOR_UPDATE_COMPLETE, &mut completion)
            .map_err(|err| io::Error::other(format!("MXCFB_WAIT_FOR_UPDATE_COMPLETE: {err}")))?;
        println!(
            "Update completed in {:?}; collision={}",
            started.elapsed(),
            completion[1]
        );
        Ok(())
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe { munmap(self.pixels.cast(), MAP_LEN) };
    }
}

pub fn draw(backup: &Path) -> io::Result<()> {
    let fb = Framebuffer::open()?;
    if backup.exists() {
        return Err(io::Error::other("a previous patch backup still exists"));
    }
    let original = fb.snapshot();
    let dark = original.iter().filter(|&&pixel| pixel <= 80).count();
    let light = original.iter().filter(|&&pixel| pixel >= 175).count();
    println!("Original patch at {LEFT},{TOP}: {dark} dark pixels, {light} light pixels.");
    if dark < 32 || light < 32 {
        return Err(io::Error::other(
            "top patch has insufficient contrast for a visual restoration check",
        ));
    }
    let pending = backup.with_extension("partial");
    let mut file = File::create(&pending)?;
    file.write_all(&original)?;
    file.sync_all()?;
    fs::rename(pending, backup)?;
    println!("Saved {PATCH_BYTES} bytes before drawing.");
    let mut pattern = vec![0; PATCH_BYTES];
    for row in pattern.chunks_exact_mut(WIDTH) {
        for (bar, pixels) in row.chunks_exact_mut(WIDTH / 6).enumerate() {
            pixels.fill((bar * 51) as u8);
        }
    }
    fb.paint(&pattern);
    fb.refresh()?;
    println!("Holding pattern for five seconds.");
    std::thread::sleep(Duration::from_secs(5));
    Ok(())
}

pub fn restore(backup: &Path) -> io::Result<()> {
    let fb = Framebuffer::open()?;
    let original = fs::read(backup)?;
    if original.len() != PATCH_BYTES {
        return Err(io::Error::other("patch backup has the wrong length"));
    }
    fb.paint(&original);
    if fb.snapshot() != original {
        return Err(io::Error::other(
            "framebuffer readback did not match restored pixels",
        ));
    }
    println!("Restored pixels match saved patch in framebuffer memory.");
    fb.refresh()?;
    fs::remove_file(backup)?;
    Ok(())
}
