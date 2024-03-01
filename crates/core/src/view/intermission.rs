use std::path::PathBuf;
use crate::device::CURRENT_DEVICE;
use crate::document::{Location, open};
use crate::geom::Rectangle;
use crate::font::{Fonts, font_from_style, DISPLAY_STYLE};
use super::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue};
use crate::framebuffer::Framebuffer;
use crate::settings::{IntermKind, LOGO_SPECIAL_PATH, COVER_SPECIAL_PATH};
use crate::metadata::{SortMethod, BookQuery, sort};
use crate::color::{TEXT_NORMAL, TEXT_INVERTED_HARD};
use crate::context::Context;
use globset::GlobBuilder;
use walkdir::{WalkDir, DirEntry};
use std::fs::metadata;
use chrono::Local;
use lazy_static::lazy_static;
use std::sync::Mutex;
use rand_core::{RngCore, SeedableRng};
use rand_xoshiro::Xoroshiro128Plus;

lazy_static! {
    // count of images in screensaver folder
    static ref IMG_COUNT: Mutex<usize> = Mutex::new(0);
    // shuffled vec of indices for screensaver images
    static ref SHUFFLE: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    // rng for shuffling
    static ref RNG: Mutex<Xoroshiro128Plus> = Mutex::new(Xoroshiro128Plus::seed_from_u64(Local::now().timestamp_nanos() as u64));
}

pub struct Intermission {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    message: Message,
    halt: bool,
}

pub enum Message {
    Text(String),
    Image(PathBuf),
    Cover(PathBuf),
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry.file_name()
         .to_str()
         .map(|s| s.starts_with("."))
         .unwrap_or(false)
}

impl Intermission {
    pub fn new(rect: Rectangle, kind: IntermKind, context: &Context) -> Intermission {
        let path = &context.settings.intermissions[kind];
        let message = match path.to_str() {
            Some(LOGO_SPECIAL_PATH) => Message::Text(kind.text().to_string()),
            Some(COVER_SPECIAL_PATH) => {
                let query = BookQuery {
                    reading: Some(true),
                    .. Default::default()
                };
                let (mut files, _) = context.library.list(&context.library.home, Some(&query), false);
                sort(&mut files, SortMethod::Opened, true);
                if !files.is_empty() {
                    Message::Cover(context.library.home.join(&files[0].file.path))
                } else {
                    Message::Text(kind.text().to_string())
                }
            },
            _ => {
                if let Ok(md) = metadata(path) {
                    if md.is_dir() {
                        let glob = GlobBuilder::new("**/*.{png,jpeg,jpg}")
                                                   .case_insensitive(true)
                                                   .build().unwrap().compile_matcher();
                        let mut images: Vec<PathBuf> = Vec::new();
                        for entry in WalkDir::new(&path).min_depth(1).into_iter().filter_map(|e| e.ok()) {
                            if is_hidden(&entry) { continue; }
                            let path = entry.path();
                            if glob.is_match(path) {
                                images.push(path.to_path_buf());
                            }
                        }
                        let n = images.len();
                        if n > 0 {
                            let mut count = IMG_COUNT.lock().unwrap();
                            let mut v = SHUFFLE.lock().unwrap();
                            let mut rng = RNG.lock().unwrap();
                            if *count != n || v.is_empty() {
                                *count = n;
                                *v = Vec::from_iter(0..n);
                                // https://en.wikipedia.org/wiki/Fisher%E2%80%93Yates_shuffle
                                for i in (1..n).rev() {
                                    let j = rng.next_u64() as usize % (i + 1);
                                    (v[j], v[i]) = (v[i], v[j]);
                                }
                            }
                            Message::Image(images[v.pop().unwrap()].clone())
                        } else {
                            Message::Text(kind.text().to_string())
                        }
                    } else {
                        Message::Image(path.clone())
                    }
                } else {
                    Message::Text(kind.text().to_string())
                }
            },
        };
        Intermission {
            id: ID_FEEDER.next(),
            rect,
            children: Vec::new(),
            message,
            halt: kind == IntermKind::PowerOff,
        }
    }
}

impl View for Intermission {
    fn handle_event(&mut self, _evt: &Event, _hub: &Hub, _bus: &mut Bus, _rq: &mut RenderQueue, _context: &mut Context) -> bool {
        true
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, fonts: &mut Fonts) {
        let scheme = if self.halt {
            TEXT_INVERTED_HARD
        } else {
            TEXT_NORMAL
        };

        fb.draw_rectangle(&self.rect, scheme[0]);

        match self.message {
            Message::Text(ref text) => {
                let dpi = CURRENT_DEVICE.dpi;

                let font = font_from_style(fonts, &DISPLAY_STYLE, dpi);
                let padding = font.em() as i32;
                let max_width = self.rect.width() as i32 - 3 * padding;
                let mut plan = font.plan(text, None, None);

                if plan.width > max_width {
                    let scale = max_width as f32 / plan.width as f32;
                    let size = (scale * DISPLAY_STYLE.size as f32) as u32;
                    font.set_size(size, dpi);
                    plan = font.plan(text, None, None);
                }

                let x_height = font.x_heights.0 as i32;

                let dx = (self.rect.width() as i32 - plan.width) / 2;
                let dy = (self.rect.height() as i32) / 3;

                font.render(fb, scheme[1], &plan, pt!(dx, dy));

                let mut doc = open("icons/dodecahedron.svg").unwrap();
                let (width, height) = doc.dims(0).unwrap();
                let scale = (plan.width as f32 / width.max(height) as f32) / 4.0;
                let (pixmap, _) = doc.pixmap(Location::Exact(0), scale).unwrap();
                let dx = (self.rect.width() as i32 - pixmap.width as i32) / 2;
                let dy = dy + 2 * x_height;
                let pt = self.rect.min + pt!(dx, dy);

                fb.draw_blended_pixmap(&pixmap, pt, scheme[1]);
            },
            Message::Image(ref path) => {
                if let Some(mut doc) = open(path) {
                    if let Some((width, height)) = doc.dims(0) {
                        let w_ratio = self.rect.width() as f32 / width;
                        let h_ratio = self.rect.height() as f32 / height;
                        let scale = w_ratio.min(h_ratio);
                        if let Some((pixmap, _)) = doc.pixmap(Location::Exact(0), scale) {
                            let dx = (self.rect.width() as i32 - pixmap.width as i32) / 2;
                            let dy = (self.rect.height() as i32 - pixmap.height as i32) / 2;
                            let pt = self.rect.min + pt!(dx, dy);
                            fb.draw_pixmap(&pixmap, pt);
                            if fb.inverted() {
                                let rect = pixmap.rect() + pt;
                                fb.invert_region(&rect);
                            }
                        }
                    }
                }
            },
            Message::Cover(ref path) => {
                if let Some(mut doc) = open(path) {
                    if let Some(pixmap) = doc.preview_pixmap(self.rect.width() as f32, self.rect.height() as f32) {
                        let dx = (self.rect.width() as i32 - pixmap.width as i32) / 2;
                        let dy = (self.rect.height() as i32 - pixmap.height as i32) / 2;
                        let pt = self.rect.min + pt!(dx, dy);
                        fb.draw_pixmap(&pixmap, pt);
                        if fb.inverted() {
                            let rect = pixmap.rect() + pt;
                            fb.invert_region(&rect);
                        }
                    }
                }
            },
        }
    }

    fn might_rotate(&self) -> bool {
        false
    }

    fn rect(&self) -> &Rectangle {
        &self.rect
    }

    fn rect_mut(&mut self) -> &mut Rectangle {
        &mut self.rect
    }

    fn children(&self) -> &Vec<Box<dyn View>> {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn View>> {
        &mut self.children
    }

    fn id(&self) -> Id {
        self.id
    }
}
