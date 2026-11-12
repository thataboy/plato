// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only diagnostic for 32-bit little-endian Linux ARM.
mod display;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

unsafe extern "C" {
    fn ioctl(fd: i32, request: i32, ...) -> i32;
    fn poll(fds: *mut PollFd, nfds: usize, timeout_ms: i32) -> i32;
}

const O_NONBLOCK: i32 = 0x800;
const POLLIN: i16 = 1;
const FBIOGET_VSCREENINFO: i32 = 0x4600;
const FBIOGET_FSCREENINFO: i32 = 0x4602;
// Older ARM kernel input_event uses a pair of 32-bit timeval fields.
const EVENT_BYTES: usize = 16;
const RECORD_LIMIT: usize = 600;
const CAPTURE_TIME: Duration = Duration::from_secs(20);

fn read_ioctl<T>(file: &File, request: i32, value: &mut T) -> io::Result<()> {
    // Each caller provides an initialized, writable buffer sized for its ioctl.
    if unsafe { ioctl(file.as_raw_fd(), request, value as *mut T) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn evdev_read(size: usize, number: u8) -> i32 {
    (0x8000_4500u32 | ((size as u32) << 16) | u32::from(number)) as i32
}

fn runtime() -> io::Result<()> {
    let started = Instant::now();
    let (send, receive) = mpsc::channel();
    let worker = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(Duration::from_millis(25));
        send.send(()).ok();
    })?;
    receive
        .recv_timeout(Duration::from_secs(2))
        .map_err(io::Error::other)?;
    worker
        .join()
        .map_err(|_| io::Error::other("thread panicked"))?;
    println!("thread/channel/timer: OK ({:?})", started.elapsed());
    println!("wall clock: {:?}", SystemTime::now());
    let mut entropy = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut entropy)?;
    println!("entropy read: OK");
    let output = Command::new("/bin/sh")
        .args(["-c", "printf plato"])
        .output()?;
    if !output.status.success() || output.stdout != b"plato" {
        return Err(io::Error::other("unexpected subprocess result"));
    }
    println!("subprocess and pipe: OK");
    Ok(())
}

fn framebuffer() -> io::Result<()> {
    let fb = File::open("/dev/fb0")?;
    // linux/fb.h on ARM32: fb_var_screeninfo is 160 bytes and
    // fb_fix_screeninfo is 68 bytes, with 32-bit unsigned long fields.
    let mut var = [0u32; 40];
    let mut fix = [0u32; 17];
    read_ioctl(&fb, FBIOGET_VSCREENINFO, &mut var)?;
    read_ioctl(&fb, FBIOGET_FSCREENINFO, &mut fix)?;
    println!(
        "active={}x{} virtual={}x{} offset={},{} bpp={} grayscale={} rotate={}",
        var[0], var[1], var[2], var[3], var[4], var[5], var[6], var[7], var[34]
    );
    println!(
        "smem_len={} type={} visual={} stride={}",
        fix[5], fix[6], fix[8], fix[11]
    );
    Ok(())
}

struct Input {
    file: File,
    name: &'static str,
    records: usize,
}

fn open_inputs() -> io::Result<Vec<Input>> {
    let mut inputs = Vec::new();
    for entry in fs::read_dir("/dev/input")? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with("event") {
            continue;
        }
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(entry.path())
        {
            Ok(file) => file,
            Err(err) => {
                println!("{}: {err}", entry.path().display());
                continue;
            }
        };
        let mut name = [0u8; 128];
        if let Err(err) = read_ioctl(&file, evdev_read(name.len(), 0x06), &mut name) {
            println!("{}: EVIOCGNAME: {err}", entry.path().display());
            continue;
        }
        let name = match name.split(|&byte| byte == 0).next() {
            Some(b"cyttsp4_mt") => "cyttsp4_mt",
            Some(b"fsr_keypad") => "fsr_keypad",
            _ => continue,
        };
        println!("{name}: {}", entry.path().display());
        if name == "cyttsp4_mt" {
            for axis in [0x00, 0x01, 0x2f, 0x35, 0x36, 0x39] {
                let mut info = [0i32; 6];
                if read_ioctl(&file, evdev_read(24, 0x40 + axis), &mut info).is_ok() {
                    println!(
                        "  ABS {axis:#04x}: min={} max={} resolution={}",
                        info[1], info[2], info[5]
                    );
                }
            }
        }
        inputs.push(Input {
            file,
            name,
            records: 0,
        });
    }
    if inputs.is_empty() {
        return Err(io::Error::other("neither Voyage input device found"));
    }
    Ok(inputs)
}

fn capture() -> io::Result<()> {
    let mut inputs = open_inputs()?;
    let mut polls: Vec<_> = inputs
        .iter()
        .map(|input| PollFd {
            fd: input.file.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        })
        .collect();
    let mut buf = [0u8; EVENT_BYTES * 32];
    println!("Capturing for 20 seconds. Columns: device seconds.microseconds type code value");
    io::stdout().flush()?;
    let deadline = Instant::now() + CAPTURE_TIME;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || polls.iter().all(|p| p.fd < 0) {
            break;
        }
        // poll writes only within the initialized descriptor array.
        let ready = unsafe {
            poll(
                polls.as_mut_ptr(),
                polls.len(),
                left.as_millis().max(1) as i32,
            )
        };
        if ready < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        for (p, input) in polls.iter_mut().zip(&mut inputs) {
            if p.revents == 0 {
                continue;
            }
            if p.revents & POLLIN == 0 {
                println!("{}: poll flags {:#x}", input.name, p.revents);
                p.fd = -1;
                continue;
            }
            let n = match input.file.read(&mut buf) {
                Ok(n) => n,
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(err) => {
                    println!("{}: read error: {err}", input.name);
                    p.fd = -1;
                    continue;
                }
            };
            if n == 0 || n % EVENT_BYTES != 0 {
                println!("{}: unexpected event read size {n}", input.name);
                p.fd = -1;
                continue;
            }
            for event in buf[..n]
                .chunks_exact(EVENT_BYTES)
                .take(RECORD_LIMIT - input.records)
            {
                let sec = u32::from_le_bytes(event[0..4].try_into().unwrap());
                let usec = u32::from_le_bytes(event[4..8].try_into().unwrap());
                let kind = u16::from_le_bytes(event[8..10].try_into().unwrap());
                let code = u16::from_le_bytes(event[10..12].try_into().unwrap());
                let value = i32::from_le_bytes(event[12..16].try_into().unwrap());
                println!("{} {sec}.{usec:06} {kind} {code} {value}", input.name);
                input.records += 1;
            }
            if input.records == RECORD_LIMIT {
                p.fd = -1;
            }
        }
    }
    for input in inputs {
        println!("{}: {} records", input.name, input.records);
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let [action, backup] = args.as_slice() {
        let backup = std::path::Path::new(backup);
        let result = match action.to_str() {
            Some("--draw-patch") => display::draw(backup),
            Some("--restore-patch") => display::restore(backup),
            _ => Err(io::Error::other("unknown display probe action")),
        };
        if let Err(err) = result {
            eprintln!("ERROR: {err}");
            std::process::exit(1);
        }
        return;
    }
    if !args.is_empty() {
        eprintln!("Expected an action and a patch backup path.");
        std::process::exit(2);
    }
    println!("Plato Voyage ARM runtime and read-only hardware probe v1");
    let checks: [(&str, fn() -> io::Result<()>); 3] = [
        ("Rust runtime", runtime),
        ("Framebuffer ioctls", framebuffer),
        ("Input capture", capture),
    ];
    let mut failed = false;
    for (name, check) in checks {
        println!("\n{name}");
        if let Err(err) = check() {
            println!("ERROR: {err}");
            failed = true;
        }
    }
    println!("END OF ARM PROBE");
    std::process::exit(i32::from(failed));
}
