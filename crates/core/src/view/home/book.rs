use std::path::PathBuf;
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData, THICKNESS_SMALL};
use crate::font::{MD_AUTHOR, MD_AUTHOR_SMALL, MD_KIND, MD_SIZE, MD_TITLE, MD_TITLE_SMALL, MD_YEAR};
use crate::color::{BLACK, GRAY02, GRAY08, GRAY10};
use crate::color::{TEXT_NORMAL, TEXT_INVERTED_HARD};
use crate::gesture::GestureEvent;
use crate::metadata::{Info, Status};
use crate::settings::FirstColumn;
use crate::unit::scale_by_dpi;
use crate::document::{HumanSize, Location, Document};
use crate::document::pdf::PdfOpener;
use crate::font::{Fonts, font_from_style};
use crate::geom::{Rectangle, CornerSpec, BorderSpec, halves};
use crate::context::Context;
use crate::document::BYTES_PER_PAGE;

const PROGRESS_HEIGHT: f32 = 10.0; // size of reading progress bars
const LARGEST_BOOK: i32 = 1500;   // page count of largest book, arbitrarily
const LARGEST_ARTICLE: i32 = 75;

pub struct Book {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    info: Info,
    index: usize,
    first_column: FirstColumn,
    cover_view: bool,
    preview_path: Option<PathBuf>,
    active: bool,
}

impl Book {
    pub fn new(rect: Rectangle, info: Info, index: usize,
               first_column: FirstColumn, cover_view: bool, preview_path: Option<PathBuf>) -> Book {
        Book {
            id: ID_FEEDER.next(),
            rect,
            children: Vec::new(),
            info,
            index,
            first_column,
            cover_view,
            preview_path,
            active: false,
        }
    }
}

impl View for Book {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, bus: &mut Bus, rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Tap(center)) if self.rect.includes(center) => {
                self.active = true;
                rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                hub.send(Event::Open(Box::new(self.info.clone()))).ok();
                true
            },
            Event::Gesture(GestureEvent::HoldFingerShort(center, ..)) if self.rect.includes(center) => {
                let pt = pt!(center.x, self.rect.center().y);
                bus.push_back(Event::ToggleBookMenu(Rectangle::from_point(pt), self.index));
                true
            },
            Event::RefreshBookPreview(ref path, ref preview_path) => {
                if self.info.file.path == *path {
                    self.preview_path = preview_path.clone();
                    rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                    true
                } else {
                    false
                }
            },
            Event::Invalid(ref path) => {
                if self.info.file.path == *path {
                    self.active = false;
                    rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                    true
                } else {
                    false
                }
            },
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, fonts: &mut Fonts) {
        let dpi = CURRENT_DEVICE.dpi;

        let scheme = if self.active {
            TEXT_INVERTED_HARD
        } else {
            TEXT_NORMAL
        };

        fb.draw_rectangle(&self.rect, scheme[0]);

        let file_info = &self.info.file;

        let (title, author) = if self.first_column == FirstColumn::TitleAndAuthor {
            (self.info.title(), self.info.author.as_str())
        } else {
            let filename = (if self.cover_view {
                file_info.path.file_name()
            } else {
                file_info.path.file_stem()
            }).map(|v| v.to_string_lossy().into_owned()).unwrap_or_default();
            (filename, "")
        };

        let kind = file_info.kind.to_uppercase();

        if self.cover_view {
            let (x_height, line_height, padding) = {
                let font = font_from_style(fonts, &MD_TITLE_SMALL, dpi);
                let x_height = font.x_heights.1 as i32;
                let line_height = font.line_height();
                (x_height, line_height, font.em() as i32)
            };

            let cover_height = 3 * self.rect.height() as i32 / 4;

            if let Some(preview_path) = self.preview_path.as_ref() {
                let cover_width = 3 * cover_height / 4;
                if preview_path.exists() {
                    if let Some((pixmap, _)) = PdfOpener::new().and_then(|opener| {
                        opener.open(preview_path)
                    }).and_then(|mut doc| {
                        doc.dims(0).and_then(|dims| {
                            let scale = (cover_width as f32 / dims.0).min(cover_height as f32 / dims.1);
                            doc.pixmap(Location::Exact(0), scale, CURRENT_DEVICE.color_samples())
                        })
                    }) {
                        let dx = (self.rect.width() as i32 - pixmap.width as i32) / 2;
                        let dy = (cover_height - pixmap.height as i32) / 2;
                        let pt = pt!(self.rect.min.x + dx,
                                     self.rect.min.y + dy + x_height / 2);
                        fb.draw_pixmap(&pixmap, pt);
                        if fb.inverted() {
                            let rect = pixmap.rect() + pt;
                            fb.invert_region(&rect);
                        }
                    }
                }
            }

            match self.info.status() {
                Status::New => {
                    let x = self.rect.min.x;
                    let size = cover_height / 8;
                    let points = [
                        pt!(x, self.rect.min.y),
                        pt!(x + size, self.rect.min.y),
                        pt!(x, self.rect.min.y + size),
                    ];
                    fb.draw_triangle(&points, BLACK);
                },
                Status::Finished => {
                    let rect = rect![
                        self.rect.max.x - (self.rect.width() as i32) / 4,
                        self.rect.min.y,
                        self.rect.max.x - (self.rect.width() as i32) / 4 + cover_height / 10,
                        self.rect.min.y + cover_height + padding
                    ];
                    fb.draw_rectangle(&rect, GRAY10);
                },
                _ => {}
            }

            // Title and author area (bottom 1/4)
            let text_width = self.rect.width() as i32 - padding / 2;
            let text_x = self.rect.min.x;
            let text_y = self.rect.min.y + cover_height + padding + line_height;
            let text_y2 = text_y + line_height + x_height / 2;

            // Title
            let font = font_from_style(fonts, &MD_TITLE_SMALL, dpi);
            let mut plan = font.plan(&title, None, None);

            // If author is empty and title doesn't fit on one line
            if author.is_empty() && plan.width > text_width {
                // Split title into two lines
                let (index, usable_width) = font.cut_point(&plan, text_width);
                let mut plan2 = plan.split_off(index, usable_width);
                font.crop_right(&mut plan, text_width);

                // Render first line
                let dx = (self.rect.width() as i32 - plan.width) / 2;
                let pt = pt!(text_x + dx, text_y);
                font.render(fb, scheme[1], &plan, pt);

                // Crop and render second line
                font.trim_left(&mut plan2);
                font.crop_right(&mut plan2, text_width);
                let dx = (self.rect.width() as i32 - plan2.width) / 2;
                let pt = pt!(text_x + dx, text_y2);
                font.render(fb, scheme[1], &plan2, pt);
            } else {
                // single-line
                font.crop_right(&mut plan, text_width);
                let dx = (self.rect.width() as i32 - plan.width) / 2;
                let pt = pt!(text_x + dx, text_y);
                font.render(fb, scheme[1], &plan, pt);
            }

            let progress_height = scale_by_dpi(PROGRESS_HEIGHT, dpi) as i32;
            let progress_y = self.rect.max.y - progress_height - x_height;

            // Author
            if !author.is_empty() {
                let font = font_from_style(fonts, &MD_AUTHOR_SMALL, dpi);
                let mut plan = font.plan(author, Some(text_width), None);
                font.crop_right(&mut plan, text_width);
                let dx = (self.rect.width() as i32 - plan.width) / 2;
                let pt = pt!(text_x + dx, text_y2);
                font.render(fb, scheme[1], &plan, pt);
            }

            if text_y2 + x_height >= progress_y {
                // no room for progress bar
                return;
            }

            // Progress bar
            if let Status::Reading(progress) = self.info.status() {
                if let Some(ref reader) = &self.info.reader {
                    let progress_width = self.rect.width() as i32 - padding - 2 * line_height;
                    let progress_x = self.rect.min.x + padding;
                    let largest_size = if self.info.identifier.is_empty() {
                        LARGEST_BOOK
                    } else {
                        LARGEST_ARTICLE
                    };
                    let pages_size = (reader.pages_count as i32 /
                          if matches!(&kind[..], "EPUB" | "HTML" | "HTM") {BYTES_PER_PAGE as i32} else {1}
                          * progress_width / largest_size).clamp(progress_width / 25, progress_width);
                    let curr_size = ((progress * pages_size as f32) as i32).max(2);

                    fb.draw_rounded_rectangle_with_border(
                        &rect![pt!(progress_x, progress_y),
                               pt!(progress_x + pages_size, progress_y + progress_height)],
                        &CornerSpec::Uniform(2),
                        &BorderSpec { thickness: 0, color: GRAY10 },
                        &|x, _| if x < progress_x + curr_size { GRAY02 } else { GRAY10 });

                    // Progress percentage
                    let font = font_from_style(fonts, &MD_SIZE, dpi);
                    let plan = font.plan(&format!("{:.0}%", progress * 100.0), None, None);
                    let pt = pt!(progress_x + pages_size + scale_by_dpi(7.0, dpi) as i32,
                                 progress_y + x_height / 2);
                    font.render(fb, scheme[1], &plan, pt);
                }
            }
            return;
        }

        // list view

        let (x_height, padding, baseline) = {
            let font = font_from_style(fonts, &MD_TITLE, dpi);
            let x_height = font.x_heights.0 as i32;
            (x_height, font.em() as i32, (self.rect.height() as i32 - 2 * x_height) / 3
                + scale_by_dpi(6.0, dpi) as i32)
        };

        let (small_half_padding, _big_half_padding) = halves(padding);
        let third_width = 6 * x_height;
        let second_width = scale_by_dpi(25.0, dpi) as i32; // x_height / 3;
        let first_width = self.rect.width() as i32 - second_width - third_width;
        let mut width = first_width - padding - small_half_padding;
        let mut start_x = self.rect.min.x + padding;

        // Preview

        if let Some(preview_path) = self.preview_path.as_ref() {
            let th = self.rect.height() as i32 - x_height;
            let tw = 3 * th / 4;
            if preview_path.exists() {
                if let Some((pixmap, _)) = PdfOpener::new().and_then(|opener| {
                    opener.open(preview_path)
                }).and_then(|mut doc| {
                    doc.dims(0).and_then(|dims| {
                        let scale = (tw as f32 / dims.0).min(th as f32 / dims.1);
                        doc.pixmap(Location::Exact(0), scale, CURRENT_DEVICE.color_samples())
                    })
                }) {
                    let dx = (tw - pixmap.width as i32) / 2;
                    let dy = (th - pixmap.height as i32) / 2;
                    let pt = pt!(self.rect.min.x + padding + dx,
                                 self.rect.min.y + x_height / 2 + dy);
                    fb.draw_pixmap(&pixmap, pt);
                    if fb.inverted() {
                        let rect = pixmap.rect() + pt;
                        fb.invert_region(&rect);
                    }
                }
            }

            width -= tw + padding;
            start_x += tw + padding;
        }

        let author_width = {
            let font = font_from_style(fonts, &MD_AUTHOR, dpi);
            let plan = font.plan(author, Some(width), None);
            plan.width
        };
        let mut author_x = start_x;
        let mut title_lines = 1;

        // Title
        {
            let font = font_from_style(fonts, &MD_TITLE, dpi);
            let mut plan = font.plan(&title, None, None);

            if plan.width > width {
                let available = width - author_width;
                if available > 3 * padding {
                    let (index, usable_width) = font.cut_point(&plan, width);
                    let leftover = plan.width - usable_width;
                    if leftover > 2 * padding {
                        let mut plan2 = plan.split_off(index, usable_width);
                        let max_width = available - if author_width > 0 { padding } else { 0 };
                        font.trim_left(&mut plan2);
                        font.crop_right(&mut plan2, max_width);
                        author_x += plan2.width + padding;
                        let pt = pt!(start_x,
                                     self.rect.max.y - baseline - x_height / 2);
                        font.render(fb, scheme[1], &plan2, pt);
                        title_lines += 1;
                    } else {
                        font.crop_right(&mut plan, width);
                    }
                } else {
                    font.crop_right(&mut plan, width);
                }
            }

            let mut dy = if author_width == 0 && title_lines == 1 {
                (self.rect.height() as i32 - x_height) / 2 + x_height
            } else {
                baseline + x_height
            };

            if title_lines == 1 {
                dy -= scale_by_dpi(2.5, dpi) as i32
            }
            let pt = pt!(start_x, self.rect.min.y + dy - x_height / 2);
            font.render(fb, scheme[1], &plan, pt);
        }

        // Author
        {
            let font = font_from_style(fonts, &MD_AUTHOR, dpi);
            let plan = font.plan(author, Some(width), None);
            let dy = if title_lines == 1 {
                scale_by_dpi(3.5, dpi) as i32
            } else {
                - scale_by_dpi(1.0, dpi) as i32
            };
            let pt = pt!(author_x, self.rect.max.y - baseline - x_height / 2 + dy);
            font.render(fb, scheme[1], &plan, pt);
        }

        match self.info.status() {
            // draw circle to indicate new or finished status
            Status::New | Status::Finished => {
                let circle_height = scale_by_dpi(17.0, dpi) as i32;
                let thickness = scale_by_dpi(THICKNESS_SMALL, dpi) as u16;
                let (small_radius, big_radius) = halves(circle_height);
                let center_x;
                let color;
                if self.info.reader.is_none() {
                    center_x = start_x - padding / 2;
                    color = BLACK;
                } else {
                    center_x = self.rect.min.x + first_width + second_width / 2;
                    color = GRAY08;
                };
                let center = pt!(center_x, self.rect.min.y + self.rect.height() as i32 / 2);
                fb.draw_rounded_rectangle_with_border(&rect![center - pt!(small_radius, small_radius),
                                                             center + pt!(big_radius, big_radius)],
                                                      &CornerSpec::Uniform(small_radius),
                                                      &BorderSpec { thickness, color },
                                                      &color);
            },
            // draw progress bar and percentage
            Status::Reading(progress) => {
                if let Some(ref reader) = &self.info.reader {
                    let progress_height = scale_by_dpi(PROGRESS_HEIGHT, dpi) as i32;
                    let largest_size = if self.info.identifier.is_empty() {
                        LARGEST_BOOK
                    } else {
                        LARGEST_ARTICLE
                    };
                    let pages_size = (reader.pages_count as i32 /
                          if matches!(&kind[..], "EPUB" | "HTML" | "HTM") {BYTES_PER_PAGE as i32} else {1}
                          * width / largest_size).clamp(width / 25, width);
                    let curr_size = start_x + ((progress * pages_size as f32) as i32).max(2);
                    let start_y = self.rect.max.y - progress_height - x_height / 2;
                    fb.draw_rounded_rectangle_with_border(
                            &rect![pt!(start_x, start_y),
                                   pt!(start_x + pages_size, start_y + progress_height)],
                            &CornerSpec::Uniform(2),
                            &BorderSpec { thickness: 0, color: GRAY10 },
                            &|x, _| if x < curr_size { GRAY02 } else { GRAY10 });

                    let font = font_from_style(fonts, &MD_SIZE, dpi);
                    let plan = font.plan(&format!("{:.0}%", progress * 100.0), None, None);
                    let pt = pt!(start_x + pages_size.min(width) + scale_by_dpi(7.0, dpi) as i32, //self.rect.max.x - padding - plan.width,
                                 start_y + x_height / 2);
                    font.render(fb, scheme[1], &plan, pt);
                }
            },
        }

        // year

        // some books set year as 0101 when undefined
        let year_is_blank = self.info.year.is_empty() || self.info.year == "0101";
        if !year_is_blank {
            let font = font_from_style(fonts, &MD_YEAR, dpi);
            let plan = font.plan(&self.info.year, None, None);
            let pt = pt!(self.rect.max.x - padding - plan.width,
                         self.rect.min.y + 6 * x_height / 3);
            font.render(fb, scheme[1], &plan, pt);
        }
        // File kind
        {
            let font = font_from_style(fonts, &MD_KIND, dpi);
            let mut plan = font.plan(&kind, None, None);
            let letter_spacing = scale_by_dpi(3.0, dpi) as i32;
            plan.space_out(letter_spacing);
            let pt = pt!(self.rect.max.x - padding - plan.width,
                         self.rect.min.y + (if year_is_blank {8} else {12}) * x_height / 3);
            font.render(fb, scheme[1], &plan, pt);
        }

        // File size
        {
            let size = file_info.size.human_size();
            let font = font_from_style(fonts, &MD_SIZE, dpi);
            let plan = font.plan(&size, None, None);
            let pt = pt!(self.rect.max.x - padding - plan.width,
                         self.rect.min.y + (if year_is_blank {13} else {17}) * x_height / 3);
            font.render(fb, scheme[1], &plan, pt);
        }
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
