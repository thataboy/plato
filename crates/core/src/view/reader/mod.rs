mod tool_bar;
mod scrubber;
mod bottom_bar;
mod results_bar;
mod margin_cropper;
mod chapter_label;
mod results_label;

use std::thread;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering as AtomicOrdering;
use std::path::PathBuf;
use std::io::prelude::*;
use std::fs::OpenOptions;
use std::collections::{VecDeque, BTreeMap};
use std::cell::{RefCell, Ref};
use std::mem::drop;
use fxhash::{FxHashMap, FxHashSet};
use chrono::Local;
use regex::Regex;
use septem::prelude::*;
use septem::{Roman, Digit};
use rand_core::RngCore;
use crate::input::{DeviceEvent, FingerStatus, ButtonCode, ButtonStatus};
use crate::framebuffer::{Framebuffer, UpdateMode, Pixmap};
use crate::view::{View, Event, AppCmd, Hub, Bus, RenderQueue, RenderData};
use crate::view::{ViewId, Id, ID_FEEDER, EntryKind, EntryId, SliderId};
use crate::view::{SMALL_BAR_HEIGHT, BIG_BAR_HEIGHT, THICKNESS_MEDIUM};
use crate::unit::{scale_by_dpi, mm_to_px};
use crate::device::CURRENT_DEVICE;
use crate::helpers::{AsciiExtension, first_n_words, trim_non_alphanumeric, encode_entities, safe_slice};
use crate::font::{Fonts, font_from_style, SMALL_STYLE};
use crate::font::family_names;
use self::margin_cropper::{MarginCropper, BUTTON_DIAMETER};
use super::top_bar::TopBar;
use self::tool_bar::ToolBar;
use self::scrubber::Scrubber;
use self::bottom_bar::BottomBar;
use self::results_bar::ResultsBar;
use crate::view::common::{locate, rlocate, locate_by_id, get_save_path};
use crate::view::common::{toggle_main_menu, toggle_battery_menu, toggle_clock_menu};
use crate::view::icon::ICONS_PIXMAPS;
use crate::view::filler::Filler;
use crate::view::named_input::NamedInput;
use crate::view::search_bar::SearchBar;
use crate::view::keyboard::Keyboard;
use crate::view::menu::{Menu, MenuKind};
use crate::view::menu_entry::MenuEntry;
use crate::view::notification::Notification;
use crate::view::theme::{ThemeDialog, ThemeProp};
use crate::settings::{guess_frontlight, FinishedAction, SouthEastCornerAction, BottomRightGestureAction, SouthStripAction, WestStripAction, EastStripAction, ProgressBarSettings};
use crate::settings::{DEFAULT_FONT_FAMILY, DEFAULT_TEXT_ALIGN, DEFAULT_LINE_HEIGHT, DEFAULT_MARGIN_WIDTH, MIN_LINE_HEIGHT_GRADIENT, MAX_LINE_HEIGHT_GRADIENT};
use crate::settings::{HYPHEN_PENALTY, STRETCH_TOLERANCE};
use crate::settings::Theme;
use crate::frontlight::LightLevels;
use crate::gesture::GestureEvent;
use crate::document::{Document, open, Location, TextLocation, BoundedText, Neighbors, BYTES_PER_PAGE};
use crate::document::{TocEntry, SimpleTocEntry, TocLocation, toc_as_html, annotations_as_html, bookmarks_as_html};
use crate::document::html::HtmlDocument;
use crate::metadata::{Info, FileInfo, ReaderInfo, Annotation, TextAlign, ZoomMode, ScrollMode, PageScheme};
use crate::metadata::{Margin, CroppingMargins, make_query};
use crate::metadata::{DEFAULT_CONTRAST_EXPONENT, DEFAULT_CONTRAST_GRAY};
use crate::geom::{Point, Vec2, Rectangle, Boundary, CornerSpec, BorderSpec};
use crate::geom::{Dir, DiagDir, CycleDir, LinearDir, Axis, Region, halves};
use crate::color::{BLACK, WHITE, GRAY03, GRAY10};
use crate::context::Context;

const HISTORY_SIZE: usize = 32;
const RECT_DIST_JITTER: f32 = 24.0;
const ANNOTATION_DRIFT: u8 =  0x44;
const HIGHLIGHT_DRIFT: u8 =  0x22;
const MEM_SCHEME: &str = "mem:";
const ON_INVERTED: &str = "__inverted";
const ON_UNINVERTED: &str = "__uninverted";
const MAX_SEARCH_RESULTS: usize = 200;

enum ThemeStash {
    New(Theme),
    Existing(usize),
}

struct Chapter {
    pub title: String,
    pub page: usize,
    pub progress: f32,
    pub remain: f32,
}

impl Default for Chapter {
    fn default() -> Self {
        Chapter {
            title: String::default(),
            page: usize::MAX,
            progress: 0.0,
            remain: 0.0,
        }
    }
}

pub struct Reader {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    doc: Arc<Mutex<Box<dyn Document>>>,
    cache: BTreeMap<usize, Resource>,                // Cached page pixmaps.
    chunks: Vec<RenderChunk>,                        // Chunks of pages being rendered.
    text: FxHashMap<usize, Vec<BoundedText>>,        // Text of the current chunks.
    annotations: FxHashMap<usize, Vec<Annotation>>,  // Annotations for the current chunks.
    noninverted_regions: FxHashMap<usize, Vec<Boundary>>,
    focus: Option<ViewId>,
    search: Option<Search>,
    search_direction: LinearDir,
    held_buttons: FxHashSet<ButtonCode>,
    selection: Option<Selection>,
    target_annotation: Option<[TextLocation; 2]>,
    history: VecDeque<usize>,
    state: State,
    info: Info,
    current_page: usize,
    pages_count: usize,
    view_port: ViewPort,
    contrast: Contrast,
    synthetic: bool,
    page_turns: usize,
    reflowable: bool,
    ephemeral: bool,
    finished: bool,
    progress_bar: ProgressBarSettings,
    theme: Option<ThemeStash>, // temporarily store selection in theme dialog
    chapter: RefCell<Chapter>, // cache chapter info
    time_format: String,
    dirty_clock: RefCell<bool>,
    font_size: f32,
}

#[derive(Debug)]
struct ViewPort {
    zoom_mode: ZoomMode,
    scroll_mode: ScrollMode,
    page_offset: Point,   // Offset relative to the top left corner of a resource's frame.
    margin_width: i32,
}

impl Default for ViewPort {
    fn default() -> Self {
        ViewPort {
            zoom_mode: ZoomMode::FitToPage,
            scroll_mode: ScrollMode::Screen,
            page_offset: pt!(0, 0),
            margin_width: 0,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum State {
    Idle,
    Selection(i32),
    AdjustSelection,
}

#[derive(Debug)]
struct Selection {
    start: TextLocation,
    end: TextLocation,
    anchor: TextLocation,
}

#[derive(Debug)]
struct Resource {
    pixmap: Pixmap,
    frame: Rectangle,  // The pixmap's rectangle minus the cropping margins.
    scale: f32,
}

#[derive(Debug, Clone)]
struct RenderChunk {
    location: usize,
    frame: Rectangle,  // A subrectangle of the corresponding resource's frame.
    position: Point,
    scale: f32,
}

#[derive(Debug)]
struct Search {
    query: String,
    highlights: BTreeMap<usize, Vec<Vec<Boundary>>>,
    running: Arc<AtomicBool>,
    current_page: usize,
    results_count: usize,
}

impl Default for Search {
    fn default() -> Self {
        Search {
            query: String::new(),
            highlights: BTreeMap::new(),
            running: Arc::new(AtomicBool::new(true)),
            current_page: 0,
            results_count: 0,
        }
    }
}

#[derive(Debug)]
struct Contrast {
    exponent: f32,
    gray: f32,
}

impl Default for Contrast {
    fn default() -> Contrast {
        Contrast {
            exponent: DEFAULT_CONTRAST_EXPONENT,
            gray: DEFAULT_CONTRAST_GRAY,
        }
    }
}

macro_rules! set_extra_css {
    ($doc:expr, $css:expr, $settings:expr) => {
        $doc.set_extra_css(
            &$css.replace("%FONTSIZE%", &format!("{:.1}pt", $settings.reader.font_size))
                 .replace("%LINEHEIGHT%", &format!("{:.3}em", $settings.reader.line_height))
                 .replace("%TEXTALIGN%", &$settings.reader.text_align.to_string().to_lowercase())
        )
    }
}

fn scaling_factor(rect: &Rectangle, cropping_margin: &Margin, screen_margin_width: i32, dims: (f32, f32), zoom_mode: ZoomMode) -> f32 {
    if let ZoomMode::Custom(sf) = zoom_mode {
        return sf;
    }

    let (page_width, page_height) = dims;
    let surface_width = (rect.width() as i32 - 2 * screen_margin_width) as f32;
    let frame_width = (1.0 - (cropping_margin.left + cropping_margin.right)) * page_width;
    let width_ratio = surface_width / frame_width;
    match zoom_mode {
        ZoomMode::FitToPage => {
            let surface_height = (rect.height() as i32 - 2 * screen_margin_width) as f32;
            let frame_height = (1.0 - (cropping_margin.top + cropping_margin.bottom)) * page_height;
            let height_ratio = surface_height / frame_height;
            width_ratio.min(height_ratio)
        },
        ZoomMode::FitToWidth => width_ratio,
        ZoomMode::Custom(_) => unreachable!(),
    }
}

fn build_pixmap(rect: &Rectangle, doc: &mut dyn Document, location: usize) -> (Pixmap, usize) {
    let scale = scaling_factor(rect, &Margin::default(), 0, doc.dims(location).unwrap(), ZoomMode::FitToPage);
    doc.pixmap(Location::Exact(location), scale, CURRENT_DEVICE.color_samples()).unwrap()
}

fn find_cut(frame: &Rectangle, y_pos: i32, scale: f32, dir: LinearDir, lines: &[BoundedText]) -> Option<i32> {
    let y_pos_u = y_pos as f32 / scale;
    let frame_u = frame.to_boundary() / scale;
    let mut rect_a: Option<Boundary> = None;
    let max_line_height = frame_u.height() / 10.0;

    for line in lines {
        if frame_u.contains(&line.rect) && line.rect.height() <= max_line_height &&
           y_pos_u >= line.rect.min.y && y_pos_u < line.rect.max.y {
            rect_a = Some(line.rect);
            break;
        }
    }

    rect_a.map(|ra| {
        if dir == LinearDir::Backward {
            (scale * ra.min.y).floor() as i32
        } else {
            (scale * ra.max.y).ceil() as i32
        }
    })
}

impl Reader {
    pub fn new(rect: Rectangle, mut info: Info, hub: &Hub, context: &mut Context) -> Option<Reader> {
        let id = ID_FEEDER.next();
        let settings = &context.settings;
        let path = context.library.home.join(&info.file.path);
        let font_size = info.reader.as_ref().and_then(|r| r.font_size)
                            .unwrap_or(settings.reader.font_size);

        open(&path).and_then(|mut doc| {
            let (width, height) = context.display.dims;

            doc.layout(width, height, font_size, CURRENT_DEVICE.dpi);

            let synthetic = doc.has_synthetic_page_numbers();
            let reflowable = doc.is_reflowable();

            let mut progress_bar = settings.reader.progress_bar.clone();
            progress_bar.enabled = info.reader.as_ref().and_then(|r| r.show_progress_bar)
                                       .unwrap_or(progress_bar.enabled);

            let margin_width = info.reader.as_ref().and_then(|r| r.margin_width)
                                   .unwrap_or(settings.reader.margin_width);

            if margin_width != DEFAULT_MARGIN_WIDTH {
                doc.set_margin_width(margin_width, synthetic && progress_bar.enabled);
            }

            let font_family = info.reader.as_ref().and_then(|r| r.font_family.as_ref())
                                  .unwrap_or(&settings.reader.font_family);

            if font_family != DEFAULT_FONT_FAMILY {
                doc.set_font_family(font_family, &settings.reader.font_path);
            }

            let line_height = info.reader.as_ref().and_then(|r| r.line_height)
                                  .unwrap_or(settings.reader.line_height);

            if (line_height - DEFAULT_LINE_HEIGHT).abs() > f32::EPSILON {
                doc.set_line_height(line_height);
            }

            let text_align = info.reader.as_ref().and_then(|r| r.text_align)
                                 .unwrap_or(settings.reader.text_align);

            if text_align != DEFAULT_TEXT_ALIGN {
                doc.set_text_align(text_align);
            }

            let hyphen_penalty = settings.reader.paragraph_breaker.hyphen_penalty;

            if hyphen_penalty != HYPHEN_PENALTY {
                doc.set_hyphen_penalty(hyphen_penalty);
            }

            let stretch_tolerance = settings.reader.paragraph_breaker.stretch_tolerance;

            if stretch_tolerance != STRETCH_TOLERANCE {
                doc.set_stretch_tolerance(stretch_tolerance);
            }

            if settings.reader.ignore_document_css {
                doc.set_ignore_document_css(true);
            }

            let mut view_port = ViewPort::default();
            let mut contrast = Contrast::default();
            let pages_count = doc.pages_count();
            let current_page;

            // TODO: use get_or_insert_with?
            if let Some(ref mut r) = info.reader {
                r.opened = Local::now().naive_local();

                if r.finished {
                    r.finished = false;
                    r.current_page = 0;
                    r.page_offset = None;
                }

                // need to do this before resolving location
                if let Some(ref css) = r.extra_css {
                    set_extra_css!(doc, css, settings);
                }

                current_page = doc.resolve_location(Location::Exact(r.current_page))
                                  .unwrap_or_else(|| doc.resolve_location(Location::Exact(0)).unwrap());

                if let Some(zoom_mode) = r.zoom_mode {
                    view_port.zoom_mode = zoom_mode;
                }

                if let Some(scroll_mode) = r.scroll_mode {
                    view_port.scroll_mode = scroll_mode;
                } else {
                    view_port.scroll_mode = if settings.reader.continuous_fit_to_width {
                        ScrollMode::Screen
                    } else {
                        ScrollMode::Page
                    };
                }

                if let Some(page_offset) = r.page_offset {
                    view_port.page_offset = page_offset;
                }

                if !doc.is_reflowable() {
                    view_port.margin_width = mm_to_px(r.screen_margin_width.unwrap_or(0) as f32,
                                                      CURRENT_DEVICE.dpi) as i32;
                }

                if let Some(exponent) = r.contrast_exponent {
                    contrast.exponent = exponent;
                }

                if let Some(gray) = r.contrast_gray {
                    contrast.gray = gray;
                }

            } else {
                current_page = doc.resolve_location(Location::Exact(0))?;

                info.reader = Some(ReaderInfo {
                    current_page,
                    pages_count,
                    .. Default::default()
                });
            }

            println!("{}", info.file.path.display());

            hub.send(Event::Update(UpdateMode::Full)).ok();

            Some(Reader {
                id,
                rect,
                children: Vec::new(),
                doc: Arc::new(Mutex::new(doc)),
                cache: BTreeMap::new(),
                chunks: Vec::new(),
                text: FxHashMap::default(),
                annotations: FxHashMap::default(),
                noninverted_regions: FxHashMap::default(),
                focus: None,
                search: None,
                search_direction: LinearDir::Forward,
                held_buttons: FxHashSet::default(),
                selection: None,
                target_annotation: None,
                history: VecDeque::new(),
                state: State::Idle,
                info,
                current_page,
                pages_count,
                view_port,
                synthetic,
                page_turns: 0,
                contrast,
                ephemeral: false,
                reflowable,
                finished: false,
                progress_bar,
                theme: None,
                chapter: RefCell::new(Chapter::default()),
                time_format: context.settings.time_format.clone(),
                dirty_clock: RefCell::new(false),
                font_size,
            })
        })
    }

    pub fn from_html(rect: Rectangle, html: &str, link_uri: Option<&str>, hub: &Hub, context: &mut Context) -> Reader {
        let id = ID_FEEDER.next();

        let mut info = Info {
            file: FileInfo {
                path: PathBuf::from(MEM_SCHEME),
                kind: "html".to_string(),
                size: html.len() as u64,
            },
            .. Default::default()
        };

        let mut doc = HtmlDocument::new_from_memory(html);
        let (width, height) = context.display.dims;
        let font_size = context.settings.reader.font_size;
        doc.layout(width, height, font_size, CURRENT_DEVICE.dpi);
        let margin_width = context.settings.reader.margin_width.max(4);
        doc.set_margin_width(margin_width, false);
        let pages_count = doc.pages_count();
        info.title = doc.title().unwrap_or_default();
        let mut progress_bar = context.settings.reader.progress_bar.clone();
        progress_bar.enabled = false;

        let mut current_page = 0;
        if let Some(link_uri) = link_uri {
            let mut loc = Location::Exact(0);
            while let Some((links, offset)) = doc.links(loc) {
                if links.iter().any(|link| link.text == link_uri) {
                    current_page = offset;
                    break;
                }
                loc = Location::Next(offset);
            }
        }

        hub.send(Event::Update(UpdateMode::Partial)).ok();

        Reader {
            id,
            rect,
            children: Vec::new(),
            doc: Arc::new(Mutex::new(Box::new(doc))),
            cache: BTreeMap::new(),
            chunks: Vec::new(),
            text: FxHashMap::default(),
            annotations: FxHashMap::default(),
            noninverted_regions: FxHashMap::default(),
            focus: None,
            search: None,
            search_direction: LinearDir::Forward,
            held_buttons: FxHashSet::default(),
            selection: None,
            target_annotation: None,
            history: VecDeque::new(),
            state: State::Idle,
            info,
            current_page,
            pages_count,
            view_port: ViewPort::default(),
            synthetic: true,
            page_turns: 0,
            contrast: Contrast::default(),
            ephemeral: true,
            reflowable: true,
            finished: false,
            progress_bar,
            theme: None,
            chapter: RefCell::new(Chapter::default()),
            time_format: context.settings.time_format.clone(),
            dirty_clock: RefCell::new(false),
            font_size,
        }
    }

    fn load_pixmap(&mut self, location: usize) {
        if self.cache.contains_key(&location) {
            return;
        }

        let mut doc = self.doc.lock().unwrap();
        let cropping_margin = self.info.reader.as_ref()
                                  .and_then(|r| r.cropping_margins.as_ref()
                                                 .map(|c| c.margin(location)))
                                  .cloned().unwrap_or_default();
        let dims = doc.dims(location).unwrap_or((3.0, 4.0));
        let screen_margin_width = self.view_port.margin_width;
        let scale = scaling_factor(&self.rect, &cropping_margin, screen_margin_width, dims, self.view_port.zoom_mode);
        if let Some((pixmap, _)) = doc.pixmap(Location::Exact(location), scale, CURRENT_DEVICE.color_samples()) {
            let frame = rect![(cropping_margin.left * pixmap.width as f32).ceil() as i32,
                              (cropping_margin.top * pixmap.height as f32).ceil() as i32,
                              ((1.0 - cropping_margin.right) * pixmap.width as f32).floor() as i32,
                              ((1.0 - cropping_margin.bottom) * pixmap.height as f32).floor() as i32];
            self.cache.insert(location, Resource { pixmap, frame, scale });
        } else {
            let width = (dims.0 as f32 * scale).max(1.0) as u32;
            let height = (dims.1 as f32 * scale).max(1.0) as u32;
            let pixmap = Pixmap::empty(width, height, CURRENT_DEVICE.color_samples());
            let frame = pixmap.rect();
            self.cache.insert(location, Resource { pixmap, frame, scale });
        }
    }

    fn load_text(&mut self, location: usize) {
        if self.text.contains_key(&location) {
            return;
        }

        let mut doc = self.doc.lock().unwrap();
        let loc = Location::Exact(location);
        let words = doc.words(loc)
                       .map(|(words, _)| words)
                       .unwrap_or_default();
        self.text.insert(location, words);
    }

    fn chapter(&self) -> Ref<Chapter> {
        {
            let mut ch = self.chapter.borrow_mut();
            if self.current_page != ch.page {
                ch.page = self.current_page;
                let mut doc = self.doc.lock().unwrap();
                let rtoc = self.toc().or_else(|| doc.toc());
                let chapter = rtoc.as_ref().and_then(|toc| doc.chapter(self.current_page, toc));
                ch.title = chapter.map(|(c, _, _)| c.title.clone()).unwrap_or_default();
                ch.progress = chapter.map(|(_, p, _)| p).unwrap_or_default();
                ch.remain = chapter.map(|(_, _, r)| r).unwrap_or_default();
            }
        }
        self.chapter.borrow()
    }

    fn chapter_info(&self) -> (String, f32) {
        let (title, remain) = {
            let chapter = self.chapter();
            (chapter.title.clone(), chapter.remain)
        };
        if self.synthetic {
            // scale remain by font size
            (title, remain * self.font_size / 6.0)
        } else {
            (title, remain)
        }
    }

    fn go_to_page(&mut self, location: usize, record: bool, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        let loc = {
            let mut doc = self.doc.lock().unwrap();
            doc.resolve_location(Location::Exact(location))
        };

        if let Some(location) = loc {
            if record {
                self.history.push_back(self.current_page);
                if self.history.len() > HISTORY_SIZE {
                    self.history.pop_front();
                }
            }

            if let Some(ref mut s) = self.search {
                s.current_page = s.highlights.range(..=location).count().saturating_sub(1);
            }

            self.current_page = location;
            self.view_port.page_offset = pt!(0);
            self.current_page = location;
            let mode = self.get_update_mode(true, context);
            self.update(Some(mode), hub, rq, context);
            self.update_bottom_bar(rq);

            if self.search.is_some() {
                self.update_results_bar(rq);
            }
        }
    }

    fn go_to_chapter(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        let current_page = self.current_page;
        let loc = {
            let mut doc = self.doc.lock().unwrap();
            if let Some(toc) = self.toc()
                                   .or_else(|| doc.toc()) {
                let chap_offset = if dir == CycleDir::Previous {
                   doc.chapter(current_page, &toc)
                      .and_then(|(chap, _, _)| doc.resolve_location(chap.location.clone()))
                      .and_then(|chap_offset| if chap_offset < current_page { Some(chap_offset) } else { None })
                } else {
                    None
                };
                chap_offset.or_else(||
                    doc.chapter_relative(current_page, dir, &toc)
                       .and_then(|rel_chap| doc.resolve_location(rel_chap.location.clone())))
            } else {
                None
            }
        };
        if let Some(location) = loc {
            self.go_to_page(location, true, hub, rq, context);
        }
    }

    fn text_location_range(&self) -> Option<[TextLocation; 2]> {
        let mut min_loc = None;
        let mut max_loc = None;
        for chunk in &self.chunks {
            for word in &self.text[&chunk.location] {
                let rect = (word.rect * chunk.scale).to_rect();
                if rect.overlaps(&chunk.frame) {
                    if let Some(ref mut min) = min_loc {
                        if word.location < *min {
                            *min = word.location;
                        }
                    } else {
                        min_loc = Some(word.location);
                    }
                    if let Some(ref mut max) = max_loc {
                        if word.location > *max {
                            *max = word.location;
                        }
                    } else {
                        max_loc = Some(word.location);
                    }
                }
            }
        }

        min_loc.and_then(|min| max_loc.map(|max| [min, max]))
    }

    fn go_to_bookmark(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        let loc_bkm = self.info.reader.as_ref().and_then(|r| {
            match dir {
                CycleDir::Next => r.bookmarks.range(self.current_page+1 ..)
                                   .next().cloned(),
                CycleDir::Previous => r.bookmarks.range(.. self.current_page)
                                       .next_back().cloned(),
            }
        });

        if let Some(location) = loc_bkm {
            self.go_to_page(location, true, hub, rq, context);
        }
    }

    fn go_to_annotation(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        let loc_annot = self.info.reader.as_ref().and_then(|r| {
            match dir {
                CycleDir::Next => self.text_location_range().and_then(|[_, max]| {
                    r.annotations.iter()
                     .filter(|annot| annot.selection[0] > max)
                     .map(|annot| annot.selection[0]).min()
                     .map(|tl| tl.location())
                }),
                CycleDir::Previous => self.text_location_range().and_then(|[min, _]| {
                    r.annotations.iter()
                     .filter(|annot| annot.selection[1] < min)
                     .map(|annot| annot.selection[1]).max()
                     .map(|tl| tl.location())
                }),
            }
        });

        if let Some(location) = loc_annot {
            self.go_to_page(location, true, hub, rq, context);
        }
    }

    fn go_to_last_page(&mut self, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        if let Some(location) = self.history.pop_back() {
            self.go_to_page(location, false, hub, rq, context);
        }
    }

    fn vertical_scroll(&mut self, delta_y: i32, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if delta_y == 0 || self.view_port.zoom_mode == ZoomMode::FitToPage || self.cache.is_empty() {
            return;
        }

        let mut next_top_offset = self.view_port.page_offset.y + delta_y;
        let mut location = self.current_page;

        match self.view_port.scroll_mode {
            ScrollMode::Screen => {
                let max_top_offset = self.cache[&location].frame.height().saturating_sub(1) as i32;

                if next_top_offset < 0 {
                    let mut doc = self.doc.lock().unwrap();
                    if let Some(previous_location) = doc.resolve_location(Location::Previous(location)) {
                        if !self.cache.contains_key(&previous_location) {
                            return;
                        }
                        location = previous_location;
                        let frame = self.cache[&location].frame;
                        next_top_offset = (frame.height() as i32 + next_top_offset).max(0);
                    } else {
                        next_top_offset = 0;
                    }
                } else if next_top_offset > max_top_offset {
                    let mut doc = self.doc.lock().unwrap();
                    if let Some(next_location) = doc.resolve_location(Location::Next(location)) {
                        if !self.cache.contains_key(&next_location) {
                            return;
                        }
                        location = next_location;
                        let frame = self.cache[&location].frame;
                        let mto = frame.height().saturating_sub(1) as i32;
                        next_top_offset = (next_top_offset - max_top_offset - 1).min(mto);
                    } else {
                        next_top_offset = max_top_offset;
                    }
                }

                {
                    let Resource { frame, scale, .. } = *self.cache.get(&location).unwrap();
                    let mut doc = self.doc.lock().unwrap();
                    if let Some((lines, _)) = doc.lines(Location::Exact(location)) {
                        if let Some(mut y_pos) = find_cut(&frame, frame.min.y + next_top_offset,
                                                          scale, LinearDir::Forward, &lines) {
                            y_pos = y_pos.clamp(frame.min.y, frame.max.y - 1);
                            next_top_offset = y_pos - frame.min.y;
                        }
                    }
                }
            },
            ScrollMode::Page => {
                let frame_height = self.cache[&location].frame.height() as i32;
                let available_height = self.rect.height() as i32 - 2 * self.view_port.margin_width;
                if frame_height > available_height {
                    next_top_offset = next_top_offset.max(0).min(frame_height - available_height);
                } else {
                    next_top_offset = self.view_port.page_offset.y;
                }
            },
        }

        let location_changed = location != self.current_page;
        if !location_changed && next_top_offset == self.view_port.page_offset.y {
            return;
        }

        self.view_port.page_offset.y = next_top_offset;
        self.current_page = location;
        self.update(None, hub, rq, context);

        if location_changed {
            if let Some(ref mut s) = self.search {
                s.current_page = s.highlights.range(..=location).count().saturating_sub(1);
            }
            self.update_bottom_bar(rq);
            if self.search.is_some() {
                self.update_results_bar(rq);
            }
        }
    }

    fn directional_scroll(&mut self, delta: Point, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if delta == pt!(0) || self.cache.is_empty() {
            return;
        }

        let Resource { frame, .. } = self.cache[&self.current_page];
        let next_page_offset = self.view_port.page_offset + delta;
        let vpw = self.rect.width() as i32 - 2 * self.view_port.margin_width;
        let vph = self.rect.height() as i32 - 2 * self.view_port.margin_width;
        let vprect = rect![pt!(0), pt!(vpw, vph)] + next_page_offset + frame.min;

        if vprect.overlaps(&frame) {
            self.view_port.page_offset = next_page_offset;
            self.update(None, hub, rq, context);
        }
    }

    fn go_to_neighbor(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if self.chunks.is_empty() {
            return;
        }

        let current_page = self.current_page;
        let page_offset = self.view_port.page_offset;

        let loc = {
            let neighloc = match dir {
                CycleDir::Previous => {
                    match self.view_port.zoom_mode {
                        ZoomMode::FitToPage => Location::Previous(current_page),
                        ZoomMode::FitToWidth => match self.view_port.scroll_mode {
                            ScrollMode::Screen => {
                                let first_chunk = self.chunks.first().cloned().unwrap();
                                let mut location = first_chunk.location;
                                let available_height = self.rect.height() as i32 - 2 * self.view_port.margin_width;
                                let mut height = 0;

                                loop {
                                    self.load_pixmap(location);
                                    self.load_text(location);
                                    let Resource { mut frame, .. } = self.cache[&location];
                                    if location == first_chunk.location {
                                        frame.max.y = first_chunk.frame.min.y;
                                    }
                                    height += frame.height() as i32;
                                    if height >= available_height {
                                        break;
                                    }
                                    let mut doc = self.doc.lock().unwrap();
                                    if let Some(previous_location) = doc.resolve_location(Location::Previous(location)) {
                                        location = previous_location;
                                    } else {
                                        break;
                                    }
                                }

                                let mut next_top_offset = (height - available_height).max(0);
                                if height > available_height {
                                    let Resource { frame, scale, .. } = self.cache[&location];
                                    let mut doc = self.doc.lock().unwrap();
                                    if let Some((lines, _)) = doc.lines(Location::Exact(location)) {
                                        if let Some(mut y_pos) = find_cut(&frame, frame.min.y + next_top_offset,
                                            scale, LinearDir::Forward, &lines) {
                                            y_pos = y_pos.clamp(frame.min.y, frame.max.y - 1);
                                            next_top_offset = y_pos - frame.min.y;
                                        }
                                    }
                                }

                                self.view_port.page_offset.y = next_top_offset;
                                Location::Exact(location)
                            },
                            ScrollMode::Page => {
                                let available_height = self.rect.height() as i32 - 2 * self.view_port.margin_width;
                                if self.view_port.page_offset.y > 0 {
                                    self.view_port.page_offset.y = (self.view_port.page_offset.y - available_height).max(0);
                                    Location::Exact(current_page)
                                } else {
                                    let previous_location = self.doc.lock().unwrap()
                                                                .resolve_location(Location::Previous(current_page));
                                    if let Some(location) = previous_location {
                                        self.load_pixmap(location);
                                        let frame = self.cache[&location].frame;
                                        self.view_port.page_offset.y = (frame.height() as i32 - available_height).max(0);
                                    }
                                    Location::Previous(current_page)
                                }
                            },
                        },
                        ZoomMode::Custom(_) => {
                            self.view_port.page_offset = pt!(0);
                            Location::Previous(current_page)
                        },
                    }
                },
                CycleDir::Next => {
                    match self.view_port.zoom_mode {
                        ZoomMode::FitToPage => Location::Next(current_page),
                        ZoomMode::FitToWidth => match self.view_port.scroll_mode {
                            ScrollMode::Screen => {
                                let &RenderChunk { location, frame, .. } = self.chunks.last().unwrap();
                                self.load_pixmap(location);
                                self.load_text(location);
                                let pixmap_frame = self.cache[&location].frame;
                                let next_top_offset = frame.max.y - pixmap_frame.min.y;
                                if next_top_offset == pixmap_frame.height() as i32 {
                                    self.view_port.page_offset.y = 0;
                                    Location::Next(location)
                                } else {
                                    self.view_port.page_offset.y = next_top_offset;
                                    Location::Exact(location)
                                }
                            },
                            ScrollMode::Page => {
                                let available_height = self.rect.height() as i32 - 2 * self.view_port.margin_width;
                                let frame_height = self.cache[&current_page].frame.height() as i32;
                                let next_top_offset = self.view_port.page_offset.y + available_height;
                                if frame_height < available_height || next_top_offset == frame_height {
                                    self.view_port.page_offset.y = 0;
                                    Location::Next(current_page)
                                } else {
                                    self.view_port.page_offset.y = next_top_offset.min(frame_height - available_height);
                                    Location::Exact(current_page)
                                }
                            },
                        },
                        ZoomMode::Custom(_) => {
                            self.view_port.page_offset = pt!(0);
                            Location::Next(current_page)
                        },
                    }
                },
            };
            let mut doc = self.doc.lock().unwrap();
            doc.resolve_location(neighloc)
        };
        match loc {
            Some(location) if location != current_page || self.view_port.page_offset != page_offset => {
                if let Some(ref mut s) = self.search {
                    s.current_page = s.highlights.range(..=location).count().saturating_sub(1);
                }

                self.current_page = location;
                let mode = self.get_update_mode(true, context);
                self.update(Some(mode), hub, rq, context);
                self.update_bottom_bar(rq);

                if self.search.is_some() {
                    self.update_results_bar(rq);
                }
            },
            _ => {
                match dir {
                    CycleDir::Next => {
                        self.finished = true;
                        let action = if self.ephemeral {
                            FinishedAction::Close
                        } else {
                            context.settings.reader.finished
                        };
                        match action {
                            FinishedAction::Notify => {
                                let notif = Notification::new("No next page.".to_string(),
                                                              hub, rq, context);
                                self.children.push(Box::new(notif) as Box<dyn View>);
                            },
                            FinishedAction::Close => {
                                self.quit(context);
                                hub.send(Event::Back).ok();
                            },
                        }
                    },
                    CycleDir::Previous => {
                        if self.ephemeral {
                            self.quit(context);
                            hub.send(Event::Back).ok();
                        } else {
                            let notif = Notification::new("No previous page.".to_string(),
                                                          hub, rq, context);
                            self.children.push(Box::new(notif) as Box<dyn View>);
                        }
                    },
                }
            },
        }
    }

    fn go_to_results_page(&mut self, index: usize, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        let mut loc = None;
        if let Some(ref mut s) = self.search {
            if index < s.highlights.len() {
                s.current_page = index;
                loc = Some(*s.highlights.keys().nth(index).unwrap());
            }
        }
        if let Some(location) = loc {
            self.current_page = location;
            self.view_port.page_offset = pt!(0, 0);
            self.selection = None;
            self.state = State::Idle;
            self.update_results_bar(rq);
            self.update_bottom_bar(rq);
            self.update(None, hub, rq, context);
        }
    }

    fn go_to_results_neighbor(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        let loc = self.search.as_ref().and_then(|s| {
            match dir {
                CycleDir::Next => s.highlights.range(self.current_page+1..)
                                              .next().map(|e| *e.0),
                CycleDir::Previous => s.highlights.range(..self.current_page)
                                                  .next_back().map(|e| *e.0),
            }
        });
        if let Some(location) = loc {
            if let Some(ref mut s) = self.search {
                s.current_page = s.highlights.range(..=location).count().saturating_sub(1);
            }
            self.view_port.page_offset = pt!(0, 0);
            self.current_page = location;
            self.update_results_bar(rq);
            self.update_bottom_bar(rq);
            self.update(None, hub, rq, context);
        } else if let Some(ref s) = self.search {
            let msg = if s.running.load(AtomicOrdering::Relaxed) {
                "Still searching".to_string()
            } else {
                format!("Reached {} results page", if dir == CycleDir::Next {"last"} else {"first"} )
            };
            hub.send(Event::Notify(msg)).ok();
        }
    }

    fn update_bottom_bar(&mut self, rq: &mut RenderQueue) {
        let current_page = self.current_page;
        if let Some(index) = locate::<BottomBar>(self) {
            let (title, remain) = self.chapter_info();
            let mut doc = self.doc.lock().unwrap();
            let bottom_bar = self.children[index].as_mut().downcast_mut::<BottomBar>().unwrap();
            let neighbors = Neighbors {
                previous_page: doc.resolve_location(Location::Previous(current_page)),
                next_page: doc.resolve_location(Location::Next(current_page)),
            };
            bottom_bar.update_chapter_label(title, remain, rq);
            bottom_bar.update_page_label(current_page, self.pages_count, rq);
            bottom_bar.update_icons(&neighbors, rq);

        }
        self.set_scrubber(current_page, rq);
    }

    fn set_scrubber(&mut self, loc: usize, rq: &mut RenderQueue) {
        if let Some(index) = locate::<Scrubber>(self) {
            let scrubber = self.children[index].as_mut().downcast_mut::<Scrubber>().unwrap();
            scrubber.set_value(loc, rq);
        }
    }

    #[inline]
    fn update_scrubber(&mut self, page: f32, rq: &mut RenderQueue) {
        if let Some(index) = locate::<Scrubber>(self) {
            let scrubber = self.children[index].as_mut().downcast_mut::<Scrubber>().unwrap();
            scrubber.update_value(page, rq);
        }
    }

    fn update_tool_bar(&mut self, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate::<ToolBar>(self) {
            let tool_bar = self.children[index].as_mut().downcast_mut::<ToolBar>().unwrap();
            let settings = &context.settings;
            if self.reflowable {
                let font_family = self.info.reader.as_ref()
                                      .and_then(|r| r.font_family.clone())
                                      .unwrap_or_else(|| settings.reader.font_family.clone());
                tool_bar.update_font_family(font_family, rq);
                let font_size = self.info.reader.as_ref()
                                    .and_then(|r| r.font_size)
                                    .unwrap_or(settings.reader.font_size);
                tool_bar.update_font_size_slider(font_size, rq);
                let text_align = self.info.reader.as_ref()
                                    .and_then(|r| r.text_align)
                                    .unwrap_or(settings.reader.text_align);
                tool_bar.update_text_align_icon(text_align, rq);
                let line_height = self.info.reader.as_ref()
                                      .and_then(|r| r.line_height)
                                      .unwrap_or(settings.reader.line_height);
                tool_bar.update_line_height(line_height, rq);
            } else {
                tool_bar.update_contrast_exponent_slider(self.contrast.exponent, rq);
                tool_bar.update_contrast_gray_slider(self.contrast.gray, rq);
            }
            let reflowable = self.reflowable;
            let margin_width = self.info.reader.as_ref()
                                   .and_then(|r| if reflowable { r.margin_width } else { r.screen_margin_width })
                                   .unwrap_or_else(|| if reflowable { settings.reader.margin_width } else { 0 });
            tool_bar.update_margin_width(margin_width, rq);
        }
    }

    fn update_results_bar(&mut self, rq: &mut RenderQueue) {
        if self.search.is_none() {
            return;
        }
        let (count, current_page, pages_count) = {
            let s = self.search.as_ref().unwrap();
            (s.results_count, s.current_page, s.highlights.len())
        };
        if let Some(index) = locate::<ResultsBar>(self) {
            let results_bar = self.child_mut(index).downcast_mut::<ResultsBar>().unwrap();
            results_bar.update_results_label(count, rq);
            results_bar.update_page_label(current_page, pages_count, rq);
            results_bar.update_icons(current_page, pages_count, rq);
        }
    }

    #[inline]
    fn update_noninverted_regions(&mut self, inverted: bool) {
        self.noninverted_regions.clear();
        if inverted {
            for chunk in &self.chunks {
                if let Some((images, _)) = self.doc.lock().unwrap().images(Location::Exact(chunk.location)) {
                    let large_images: Vec<Boundary> = images
                        .iter()
                        .filter(|img| img.width() > 50.0 && img.height() > 50.0)
                        .cloned()
                        .collect();
                    self.noninverted_regions.insert(chunk.location, large_images);
                }
            }
        }
    }

    #[inline]
    fn update_annotations(&mut self) {
        self.annotations.clear();
        if let Some(annotations) = self.info.reader.as_ref().map(|r| &r.annotations).filter(|a| !a.is_empty()) {
            for chunk in &self.chunks {
                let words = &self.text[&chunk.location];
                if words.is_empty() {
                    continue;
                }
                for annot in annotations {
                    let [start, end] = annot.selection;
                    if (start >= words[0].location && start <= words[words.len()-1].location) ||
                       (end >= words[0].location && end <= words[words.len()-1].location) {
                        self.annotations.entry(chunk.location)
                            .or_insert_with(Vec::new)
                            .push(annot.clone());
                    }
                }
            }
        }
    }

    fn get_update_mode(&self, check_chapter_start: bool, context: &Context) -> UpdateMode {
        let pair = context.settings.reader.refresh_rate.by_kind
                                   .get(&self.info.file.kind)
                                   .unwrap_or_else(|| &context.settings.reader.refresh_rate.global);
        let refresh_rate = if context.fb.inverted() { pair.inverted } else { pair.regular };
        // if due for full refresh
        if refresh_rate > 0 && self.page_turns + 1 >= refresh_rate as usize
           ||
           // or start of chapter
           check_chapter_start && context.settings.reader.refresh_rate.chapter_start
           && self.page_turns > 1 // ignore recent refresh and very short chapters
           && self.chapter().progress == 0.0 {
            UpdateMode::Full
        } else {
            UpdateMode::Partial
        }
    }

    fn update(&mut self, update_mode: Option<UpdateMode>, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        let update_mode = update_mode.unwrap_or_else(|| self.get_update_mode(false, context));
        if update_mode == UpdateMode::Full {
            self.page_turns = 0;
        } else if update_mode == UpdateMode::Partial {
            self.page_turns += 1;
        }

        self.chunks.clear();
        let mut location = self.current_page;
        let smw = self.view_port.margin_width;

        match self.view_port.zoom_mode {
            ZoomMode::FitToPage => {
                self.load_pixmap(location);
                self.load_text(location);
                let Resource { frame, scale, .. } = self.cache[&location];
                let dx = smw + ((self.rect.width() - frame.width()) as i32 - 2 * smw) / 2;
                let dy = smw + ((self.rect.height() - frame.height()) as i32 - 2 * smw) / 2;
                self.chunks.push(RenderChunk { frame, location, position: pt!(dx, dy), scale });
            },
            ZoomMode::FitToWidth => match self.view_port.scroll_mode {
                ScrollMode::Screen => {
                    let available_height = self.rect.height() as i32 - 2 * smw;
                    let mut height = 0;
                    while height < available_height {
                        self.load_pixmap(location);
                        self.load_text(location);
                        let Resource { mut frame, scale, .. } = self.cache[&location];
                        if location == self.current_page {
                            frame.min.y += self.view_port.page_offset.y;
                        }
                        let position = pt!(smw, smw + height);
                        self.chunks.push(RenderChunk { frame, location, position, scale });
                        height += frame.height() as i32;
                        if let Ok(mut doc) = self.doc.lock() {
                            if let Some(next_location) = doc.resolve_location(Location::Next(location)) {
                                location = next_location;
                            } else {
                                break;
                            }
                        }
                    }
                    if height > available_height {
                        if let Some(last_chunk) = self.chunks.last_mut() {
                            last_chunk.frame.max.y -= height - available_height;
                            let mut doc = self.doc.lock().unwrap();
                            if let Some((lines, _)) = doc.lines(Location::Exact(last_chunk.location)) {
                                let pixmap_frame = self.cache[&last_chunk.location].frame;
                                if let Some(mut y_pos) = find_cut(&pixmap_frame, last_chunk.frame.max.y, last_chunk.scale, LinearDir::Backward, &lines) {
                                    y_pos = y_pos.clamp(pixmap_frame.min.y, pixmap_frame.max.y - 1);
                                    last_chunk.frame.max.y = y_pos;
                                }
                            }
                        }
                        let actual_height: i32 = self.chunks.iter().map(|c| c.frame.height() as i32).sum();
                        let dy = (available_height - actual_height) / 2;
                        for chunk in &mut self.chunks {
                            chunk.position.y += dy;
                        }
                    }
                },
                ScrollMode::Page => {
                    self.load_pixmap(location);
                    self.load_text(location);
                    let available_height = self.rect.height() as i32 - 2 * smw;
                    let Resource { mut frame, scale, .. } = self.cache[&location];
                    frame.min.y += self.view_port.page_offset.y;
                    frame.max.y = (frame.min.y + available_height).min(frame.max.y);
                    let position = pt!(smw, smw + (available_height - frame.height() as i32) / 2);
                    self.chunks.push(RenderChunk { frame, location, position, scale });
                },
            },
            ZoomMode::Custom(_) => {
                self.load_pixmap(location);
                self.load_text(location);
                let Resource { frame, scale, .. } = self.cache[&location];
                let vpw = self.rect.width() as i32 - 2 * smw;
                let vph = self.rect.height() as i32 - 2 * smw;
                let vpr = rect![pt!(0), pt!(vpw, vph)] + self.view_port.page_offset + frame.min;
                if let Some(rect) = frame.intersection(&vpr) {
                    let position = pt!(smw) + rect.min - vpr.min;
                    self.chunks.push(RenderChunk { frame: rect, location, position, scale });
                }
            },
        }

        rq.add(RenderData::new(self.id, self.rect, update_mode));
        let first_location = self.chunks.first().map(|c| c.location).unwrap();
        let last_location = self.chunks.last().map(|c| c.location).unwrap();

        while self.cache.len() > 3 {
            let left_count = self.cache.range(..first_location).count();
            let right_count = self.cache.range(last_location+1..).count();
            let extremum = if left_count >= right_count {
                self.cache.keys().next().cloned().unwrap()
            } else {
                self.cache.keys().next_back().cloned().unwrap()
            };
            self.cache.remove(&extremum);
        }

        self.update_annotations();
        self.update_noninverted_regions(context.fb.inverted());

        if self.view_port.zoom_mode == ZoomMode::FitToPage ||
           self.view_port.zoom_mode == ZoomMode::FitToWidth {
            let doc2 = self.doc.clone();
            let hub2 = hub.clone();
            thread::spawn(move || {
                let mut doc = doc2.lock().unwrap();
                if let Some(next_location) = doc.resolve_location(Location::Next(last_location)) {
                    hub2.send(Event::LoadPixmap(next_location)).ok();
                }
            });
            let doc3 = self.doc.clone();
            let hub3 = hub.clone();
            thread::spawn(move || {
                let mut doc = doc3.lock().unwrap();
                if let Some(previous_location) = doc.resolve_location(Location::Previous(first_location)) {
                    hub3.send(Event::LoadPixmap(previous_location)).ok();
                }
            });
        }
    }

    fn search(&mut self, text: &str, query: Regex, hub: &Hub, rq: &mut RenderQueue) {
        let s = Search {
            query: text.to_string(),
            .. Default::default()
        };

        // trigger draw stop button
        hub.send(Event::Update(UpdateMode::Gui)).ok();

        let hub2 = hub.clone();
        let doc2 = Arc::clone(&self.doc);
        let running = Arc::clone(&s.running);
        let search_direction = self.search_direction;
        let pages_count = self.pages_count;

        thread::spawn(move || {
            let mut results_count = 0;
            let mut loc = match search_direction {
                LinearDir::Forward => Location::Exact(0),
                LinearDir::Backward => Location::Exact(pages_count-1),
            };

            loop {
                if !running.load(AtomicOrdering::Relaxed) {
                    break;
                }

                let mut doc = doc2.lock().unwrap();
                let mut text = String::new();
                let mut rects = BTreeMap::new();

                if let Some(location) = doc.resolve_location(loc) {
                    if let Some((ref words, _)) = doc.words(Location::Exact(location)) {
                        if !words.is_empty() {
                            let mut end_offset = 0;
                            for word in words {
                                if !running.load(AtomicOrdering::Relaxed) {
                                    break;
                                }
                                let (is_dyn, offset) =
                                    if let TextLocation::Dynamic(offset) = word.location {
                                        (true, offset)
                                    } else {
                                        (false, 1)
                                    };
                                if text.ends_with('\u{00AD}') {
                                    text.pop();
                                } else if !text.ends_with('-') && !text.is_empty() && offset > end_offset {
                                    text.push(' ');
                                }
                                rects.insert(text.len(), word.rect);
                                text += &word.text;
                                if is_dyn {
                                    end_offset = offset + word.text.len();
                                }
                            }
                        }
                        for m in query.find_iter(&text) {
                            if let Some((first, _)) = rects.range(..= m.start()).next_back() {
                                let mut match_rects = Vec::new();
                                for (_, rect) in rects.range(*first .. m.end()) {
                                    if !running.load(AtomicOrdering::Relaxed) {
                                        break;
                                    }
                                    match_rects.push(*rect);
                                }
                                results_count += 1;
                                hub2.send(Event::SearchResult(location, match_rects)).ok();
                                if results_count >= MAX_SEARCH_RESULTS && running.load(AtomicOrdering::Relaxed) {
                                    hub2.send(Event::Notify(format!("Maximum {MAX_SEARCH_RESULTS} results reached. Search stopped."))).ok();
                                    running.store(false, AtomicOrdering::Relaxed);
                                    break;
                                }
                            }
                        }
                    }
                    loc = match search_direction {
                        LinearDir::Forward => Location::Next(location),
                        LinearDir::Backward => Location::Previous(location),
                    };
                } else {
                    break;
                }
            }

            running.store(false, AtomicOrdering::Relaxed);
            hub2.send(Event::EndOfSearch).ok();
        });

        if self.search.is_some() {
            self.render_results(rq);
        }

        self.search = Some(s);
    }

    /// stop search or exit search mode if search already stopped or only 1 page of results
    fn stop_search(&mut self, rq: &mut RenderQueue) {
        if let Some(ref mut s) = self.search {
            let was_running = s.running.swap(false, AtomicOrdering::Relaxed);
            let pages_count = s.highlights.len();
            self.render_results(rq);
            if !was_running || pages_count <= 1 {
                self.search = None;
            }
        }
    }

    fn toggle_keyboard(&mut self, enable: bool, id: Option<ViewId>, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate::<Keyboard>(self) {
            if enable {
                return;
            }

            let mut rect = *self.child(index).rect();
            rect.absorb(self.child(index-1).rect());

            if index == 1 {
                rect.absorb(self.child(index+1).rect());
                self.children.drain(index - 1 ..= index + 1);
                rq.add(RenderData::expose(rect, UpdateMode::Gui));
            } else {
                self.children.drain(index - 1 ..= index);

                let start_index = locate::<TopBar>(self).map(|index| index+2).unwrap_or(0);
                let y_min = self.child(start_index).rect().min.y;
                let delta_y = rect.height() as i32;

                for i in start_index..index-1 {
                    let shifted_rect = *self.child(i).rect() + pt!(0, delta_y);
                    self.child_mut(i).resize(shifted_rect, hub, rq, context);
                    rq.add(RenderData::new(self.child(i).id(), shifted_rect, UpdateMode::Gui));
                }

                let rect = rect![self.rect.min.x, y_min, self.rect.max.x, y_min + delta_y];
                rq.add(RenderData::expose(rect, UpdateMode::Gui));
            }

            context.kb_rect = Rectangle::default();
            hub.send(Event::Focus(None)).ok();
        } else {
            if !enable {
                return;
            }

            let dpi = CURRENT_DEVICE.dpi;
            let (small_height, big_height) = (scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32,
                                              scale_by_dpi(BIG_BAR_HEIGHT, dpi) as i32);
            let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
            let (small_thickness, big_thickness) = halves(thickness);

            let mut kb_rect = rect![self.rect.min.x,
                                    self.rect.max.y - (small_height + 3 * big_height) as i32 + big_thickness,
                                    self.rect.max.x,
                                    self.rect.max.y - small_height - small_thickness];

            let number = matches!(id, Some(ViewId::GoToPageInput) |
                                      Some(ViewId::GoToResultsPageInput) |
                                      Some(ViewId::NamePageInput));

            let index = rlocate::<Filler>(self).unwrap_or(0);

            if index == 0 {
                let separator = Filler::new(rect![self.rect.min.x, kb_rect.max.y,
                                                  self.rect.max.x, kb_rect.max.y + thickness],
                                            BLACK);
                self.children.insert(index, Box::new(separator) as Box<dyn View>);
            }

            let keyboard = Keyboard::new(&mut kb_rect, number, context);
            self.children.insert(index, Box::new(keyboard) as Box<dyn View>);

            let separator = Filler::new(rect![self.rect.min.x, kb_rect.min.y - thickness,
                                              self.rect.max.x, kb_rect.min.y],
                                        BLACK);
            self.children.insert(index, Box::new(separator) as Box<dyn View>);

            if index == 0 {
                for i in index..index+3 {
                    rq.add(RenderData::new(self.child(i).id(), *self.child(i).rect(), UpdateMode::Gui));
                }
            } else {
                for i in index..index+2 {
                    rq.add(RenderData::new(self.child(i).id(), *self.child(i).rect(), UpdateMode::Gui));
                }

                let delta_y = kb_rect.height() as i32 + thickness;
                let start_index = locate::<TopBar>(self).map(|index| index+2).unwrap_or(0);

                for i in start_index..index {
                    let shifted_rect = *self.child(i).rect() + pt!(0, -delta_y);
                    self.child_mut(i).resize(shifted_rect, hub, rq, context);
                    rq.add(RenderData::new(self.child(i).id(), shifted_rect, UpdateMode::Gui));
                }
            }
        }
    }

    fn remove_tool_bar(&mut self, rq: &mut RenderQueue) {
        if let Some(index) = locate::<ToolBar>(self) {
            let mut rect = *self.child(index).rect();
            rect.absorb(self.child(index + 1).rect());
            self.children.drain(index ..= index + 1);
            rq.add(RenderData::expose(rect, UpdateMode::Gui));
        }
    }

    fn toggle_results_bar(&mut self, enable: bool, rq: &mut RenderQueue, _context: &mut Context) {
        if let Some(index) = locate::<ResultsBar>(self) {
            if enable {
                return;
            }

            let mut rect = *self.child(index).rect();
            rect.absorb(self.child(index - 1).rect());
            self.children.drain(index - 1 ..= index);
            rq.add(RenderData::expose(rect, UpdateMode::Gui));
        } else {
            if !enable {
                return;
            }

            let dpi = CURRENT_DEVICE.dpi;
            let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
            let small_height = scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32;
            let index = locate::<TopBar>(self).map(|index| index+2).unwrap_or(0);

            let sp_rect = *self.child(index).rect() - pt!(0, small_height);
            let y_min = sp_rect.max.y;
            let mut rect = rect![self.rect.min.x, y_min,
                                 self.rect.max.x, y_min + small_height - thickness];

            if let Some(ref s) = self.search {
                let results_bar = ResultsBar::new(rect, s.current_page,
                                                  s.highlights.len(), s.results_count,
                                                  !s.running.load(AtomicOrdering::Relaxed));
                self.children.insert(index, Box::new(results_bar) as Box<dyn View>);
                let separator = Filler::new(sp_rect, BLACK);
                self.children.insert(index, Box::new(separator) as Box<dyn View>);
                rect.absorb(&sp_rect);
                rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
            }
        }
    }

    fn toggle_search_bar(&mut self, enable: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate::<SearchBar>(self) {
            if enable {
                return;
            }

            if let Some(ViewId::ReaderSearchInput) = self.focus {
                self.toggle_keyboard(false, None, hub, rq, context);
            }

            if self.child(0).is::<TopBar>() {
                self.toggle_bars(Some(false), hub, rq, context);
            } else {
                let mut rect = *self.child(index).rect();
                rect.absorb(self.child(index-1).rect());
                rect.absorb(self.child(index+1).rect());
                self.children.drain(index - 1 ..= index + 1);
                rq.add(RenderData::expose(rect, UpdateMode::Gui));
            }
        } else {
            if !enable {
                return;
            }

            self.remove_tool_bar(rq);
            self.remove_scrubber(rq);

            let dpi = CURRENT_DEVICE.dpi;
            let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
            let (small_thickness, big_thickness) = halves(thickness);
            let small_height = scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32;
            let index = locate::<TopBar>(self).map(|index| index+2).unwrap_or(0);

            if index == 0 {
                let sp_rect = rect![self.rect.min.x, self.rect.max.y - small_height - small_thickness,
                                    self.rect.max.x, self.rect.max.y - small_height + big_thickness];
                let separator = Filler::new(sp_rect, BLACK);
                self.children.insert(index, Box::new(separator) as Box<dyn View>);
            }

            let sp_rect = rect![self.rect.min.x, self.rect.max.y - 2 * small_height - small_thickness,
                                self.rect.max.x, self.rect.max.y - 2 * small_height + big_thickness];
            let y_min = sp_rect.max.y;
            let rect = rect![self.rect.min.x, y_min,
                             self.rect.max.x, y_min + small_height - thickness];
            let search_bar = SearchBar::new(rect, ViewId::ReaderSearchInput, "", "", true, context);
            self.children.insert(index, Box::new(search_bar) as Box<dyn View>);

            let separator = Filler::new(sp_rect, BLACK);
            self.children.insert(index, Box::new(separator) as Box<dyn View>);

            rq.add(RenderData::new(self.child(index).id(), *self.child(index).rect(), UpdateMode::Gui));
            rq.add(RenderData::new(self.child(index+1).id(), *self.child(index+1).rect(), UpdateMode::Gui));

            if index == 0 {
                rq.add(RenderData::new(self.child(index+2).id(), *self.child(index+2).rect(), UpdateMode::Gui));
            }

            self.toggle_keyboard(true, Some(ViewId::ReaderSearchInput), hub, rq, context);
            hub.send(Event::Focus(Some(ViewId::ReaderSearchInput))).ok();
        }
    }

    fn toggle_bars(&mut self, enable: Option<bool>, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(top_index) = locate::<TopBar>(self) {
            if let Some(true) = enable {
                return;
            }

            if let Some(bottom_index) = locate::<BottomBar>(self) {
                let mut top_rect = *self.child(top_index).rect();
                for i in top_index+1 ..= bottom_index {
                    top_rect.absorb(self.child(i).rect());
                }

                self.children.drain(top_index..=bottom_index);

                rq.add(RenderData::expose(top_rect, UpdateMode::Gui));
                hub.send(Event::Focus(None)).ok();
            }
        } else {
            if let Some(false) = enable {
                return;
            }

            let dpi = CURRENT_DEVICE.dpi;
            let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
            let (small_thickness, big_thickness) = halves(thickness);
            let (small_height, big_height) = (scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32,
                                              scale_by_dpi(BIG_BAR_HEIGHT, dpi) as i32);

            let mut doc = self.doc.lock().unwrap();
            let mut index = 0;

            let top_bar = TopBar::new(rect![self.rect.min.x,
                                            self.rect.min.y,
                                            self.rect.max.x,
                                            self.rect.min.y + small_height - small_thickness],
                                      Event::Back,
                                      self.info.title(),
                                      context);

            self.children.insert(index, Box::new(top_bar) as Box<dyn View>);
            index += 1;

            let separator = Filler::new(rect![self.rect.min.x,
                                              self.rect.min.y + small_height - small_thickness,
                                              self.rect.max.x,
                                              self.rect.min.y + small_height + big_thickness],
                                        BLACK);
            self.children.insert(index, Box::new(separator) as Box<dyn View>);
            index += 1;

            if let Some(ref s) = self.search {
                if let Some(sindex) = rlocate::<SearchBar>(self) {
                    index = sindex + 2;
                } else {
                    let separator = Filler::new(rect![self.rect.min.x,
                                                      self.rect.max.y - 3 * small_height - small_thickness,
                                                      self.rect.max.x,
                                                      self.rect.max.y - 3 * small_height + big_thickness],
                                                BLACK);
                    self.children.insert(index, Box::new(separator) as Box<dyn View>);
                    index += 1;

                    let results_bar = ResultsBar::new(rect![self.rect.min.x,
                                                            self.rect.max.y - 3 * small_height + big_thickness,
                                                            self.rect.max.x,
                                                            self.rect.max.y - 2 * small_height - small_thickness],
                                                      s.current_page, s.highlights.len(),
                                                      s.results_count, !s.running.load(AtomicOrdering::Relaxed));
                    self.children.insert(index, Box::new(results_bar) as Box<dyn View>);
                    index += 1;

                    let separator = Filler::new(rect![self.rect.min.x,
                                                      self.rect.max.y - 2 * small_height - small_thickness,
                                                      self.rect.max.x,
                                                      self.rect.max.y - 2 * small_height + big_thickness],
                                                BLACK);
                    self.children.insert(index, Box::new(separator) as Box<dyn View>);
                    index += 1;

                    let search_bar = SearchBar::new(rect![self.rect.min.x,
                                                          self.rect.max.y - 2 * small_height + big_thickness,
                                                          self.rect.max.x,
                                                          self.rect.max.y - small_height - small_thickness],
                                                    ViewId::ReaderSearchInput,
                                                    "", &s.query, true, context);
                    self.children.insert(index, Box::new(search_bar) as Box<dyn View>);
                    index += 1;
                }
            } else {
                let med_height = (small_height + big_height) / 2;
                let mut y_top = self.rect.max.y - (med_height + small_height) as i32 - big_thickness;
                let scrubber = Scrubber::new(rect![self.rect.min.x,
                                                  y_top,
                                                  self.rect.max.x,
                                                  y_top + med_height as i32],
                                             self.current_page, self.pages_count, self.synthetic);
                self.children.insert(index, Box::new(scrubber) as Box<dyn View>);
                index += 1;

                let tb_height = 2 * med_height;
                y_top -= tb_height as i32;
                let tool_bar = ToolBar::new(rect![self.rect.min.x,
                                                  y_top,
                                                  self.rect.max.x,
                                                  y_top + tb_height],
                                            self.reflowable,
                                            self.synthetic,
                                            self.info.reader.as_ref(),
                                            context);
                self.children.insert(index, Box::new(tool_bar) as Box<dyn View>);
                index += 1;

                y_top -= big_thickness + small_thickness;
                let separator = Filler::new(rect![self.rect.min.x,
                                                  y_top,
                                                  self.rect.max.x,
                                                  y_top + small_thickness + big_thickness],
                                            BLACK);
                self.children.insert(index, Box::new(separator) as Box<dyn View>);
                index += 1;
            }

            let separator = Filler::new(rect![self.rect.min.x,
                                              self.rect.max.y - small_height - small_thickness,
                                              self.rect.max.x,
                                              self.rect.max.y - small_height + big_thickness],
                                        BLACK);
            self.children.insert(index, Box::new(separator) as Box<dyn View>);
            index += 1;

            let neighbors = Neighbors {
                previous_page: doc.resolve_location(Location::Previous(self.current_page)),
                next_page: doc.resolve_location(Location::Next(self.current_page)),
            };

            drop(doc);

            let bottom_bar = {
                let (title, remain) = self.chapter_info();
                BottomBar::new(rect![self.rect.min.x,
                                     self.rect.max.y - small_height + big_thickness,
                                     self.rect.max.x,
                                     self.rect.max.y],
                               self.current_page,
                               self.pages_count,
                               title,
                               remain,
                               &neighbors,
                               self.synthetic)
            };
            self.children.insert(index, Box::new(bottom_bar) as Box<dyn View>);

            for i in 0..=index {
                rq.add(RenderData::new(self.child(i).id(), *self.child(i).rect(), UpdateMode::Gui));
            }
        }
    }

    fn toggle_margin_cropper(&mut self, enable: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate::<MarginCropper>(self) {
            if enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if !enable {
                return;
            }

            self.toggle_bars(Some(false), hub, rq, context);

            let dpi = CURRENT_DEVICE.dpi;
            let padding = scale_by_dpi(BUTTON_DIAMETER / 2.0, dpi) as i32;
            let pixmap_rect = rect![self.rect.min + pt!(padding),
                                    self.rect.max - pt!(padding)];

            let margin = self.info.reader.as_ref()
                             .and_then(|r| r.cropping_margins.as_ref()
                                            .map(|c| c.margin(self.current_page)))
                             .cloned().unwrap_or_default();

            let mut doc = self.doc.lock().unwrap();
            let (pixmap, _) = build_pixmap(&pixmap_rect, doc.as_mut(), self.current_page);

            let margin_cropper = MarginCropper::new(self.rect, pixmap, &margin, context);
            rq.add(RenderData::new(margin_cropper.id(), *margin_cropper.rect(), UpdateMode::Gui));
            self.children.push(Box::new(margin_cropper) as Box<dyn View>);
        }
    }

    fn toggle_edit_note(&mut self, text: Option<String>, enable: Option<bool>, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::EditNote) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);

            self.toggle_keyboard(false, None, hub, rq, context);
        } else {
            if let Some(false) = enable {
                return;
            }

            let mut edit_note = NamedInput::new("Note".to_string(), ViewId::EditNote, ViewId::EditNoteInput, 32, context);
            if let Some(text) = text.as_ref() {
                edit_note.set_text(text, &mut RenderQueue::new(), context);
            }

            rq.add(RenderData::new(edit_note.id(), *edit_note.rect(), UpdateMode::Gui));
            hub.send(Event::Focus(Some(ViewId::EditNoteInput))).ok();

            self.children.push(Box::new(edit_note) as Box<dyn View>);
        }
    }

    fn toggle_name_page(&mut self, enable: Option<bool>, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::NamePage) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);

            self.toggle_keyboard(false, None, hub, rq, context);
        } else {
            if let Some(false) = enable {
                return;
            }

            let name_page = NamedInput::new("Name page".to_string(), ViewId::NamePage, ViewId::NamePageInput, 4, context);
            rq.add(RenderData::new(name_page.id(), *name_page.rect(), UpdateMode::Gui));
            hub.send(Event::Focus(Some(ViewId::NamePageInput))).ok();

            self.children.push(Box::new(name_page) as Box<dyn View>);
        }
    }

    fn remove_scrubber(&mut self, rq: &mut RenderQueue) {
        if let Some(index) = locate::<Scrubber>(self) {
            let rect = *self.child(index).rect();
            self.children.drain(index..=index);
            rq.add(RenderData::expose(rect, UpdateMode::Gui));
        }
    }

    fn toggle_go_to_page(&mut self, enable: Option<bool>, id: ViewId, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let (text, input_id) = if id == ViewId::GoToPage {
            ("Go to page", ViewId::GoToPageInput)
        } else {
            ("Go to results page", ViewId::GoToResultsPageInput)
        };

        if let Some(index) = locate_by_id(self, id) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
            self.toggle_keyboard(false, None, hub, rq, context);
            self.toggle_bars(Some(false), hub, rq, context);

        } else {
            if let Some(false) = enable {
                return;
            }

            self.remove_tool_bar(rq);
            self.remove_scrubber(rq);
            let go_to_page = NamedInput::new(text.to_string(), id, input_id, 4, context);
            rq.add(RenderData::new(go_to_page.id(), *go_to_page.rect(), UpdateMode::Gui));
            hub.send(Event::Focus(Some(input_id))).ok();

            self.children.push(Box::new(go_to_page) as Box<dyn View>);
        }
    }

    pub fn toggle_annotation_menu(&mut self, annot: &Annotation, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::AnnotationMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let sel = annot.selection;
            let mut entries = Vec::new();

            if annot.note.is_empty() {
                entries.push(EntryKind::Command("Remove Highlight".to_string(), EntryId::RemoveAnnotation(sel)));
                entries.push(EntryKind::Separator);
                entries.push(EntryKind::Command("Add Note".to_string(), EntryId::EditAnnotationNote(sel)));
            } else {
                entries.push(EntryKind::Command("Remove Annotation".to_string(), EntryId::RemoveAnnotation(sel)));
                entries.push(EntryKind::Separator);
                entries.push(EntryKind::Command("Edit Note".to_string(), EntryId::EditAnnotationNote(sel)));
                entries.push(EntryKind::Command("Remove Note".to_string(), EntryId::RemoveAnnotationNote(sel)));
            }

            let selection_menu = Menu::new(rect, ViewId::AnnotationMenu, MenuKind::Contextual, entries, context);
            rq.add(RenderData::new(selection_menu.id(), *selection_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(selection_menu) as Box<dyn View>);
        }
    }

    pub fn toggle_selection_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::SelectionMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }
            let mut entries = vec![
                EntryKind::Command("Highlight".to_string(), EntryId::HighlightSelection),
                EntryKind::Command("Add Note".to_string(), EntryId::AnnotateSelection),
                EntryKind::Command("Adjust Selection".to_string(), EntryId::AdjustSelection),
            ];

            if self.info.file.kind == "epub" {
                let has_extra_css = self.info.reader.as_ref().map_or(false, |r| r.extra_css.is_some());
                if has_extra_css || !context.settings.css_styles.is_empty() {
                    let mut tweaks = context.settings.css_styles.iter()
                                     .enumerate()
                                     .filter(|(_, x)| !x.css.trim().is_empty())
                                     .map(|(i, x)| { EntryKind::Command(x.name.clone(),
                                                                        EntryId::SetCssTweak(i)) })
                                     .collect::<Vec<EntryKind>>();
                    if has_extra_css {
                        if !tweaks.is_empty() {
                            tweaks.push(EntryKind::Separator);
                        }
                        tweaks.push(EntryKind::Command("Undo last".to_string(), EntryId::UndoLastCssTweak));
                        tweaks.push(EntryKind::Command("Undo all".to_string(), EntryId::UndoAllCssTweaks));
                    }
                    if !tweaks.is_empty() {
                        entries.push(EntryKind::Separator);
                        entries.push(EntryKind::Command("Inspect".to_string(), EntryId::ShowCssTweaks));
                        entries.push(EntryKind::SubMenu("CSS tweaks".to_string(), tweaks));
                    }
                }
            }

            entries.push(EntryKind::Separator);
            entries.push(EntryKind::Command("Define".to_string(), EntryId::DefineSelection));
            entries.push(EntryKind::Command("Translate".to_string(), EntryId::TranslateSelection));
            entries.push(EntryKind::Command("Wikipedia".to_string(), EntryId::WikiSelection));
            entries.push(EntryKind::Command("Search".to_string(), EntryId::SearchForSelection));

            if self.info.reader.as_ref().map_or(false, |r| !r.page_names.is_empty()) {
                entries.push(EntryKind::Command("Go To".to_string(), EntryId::GoToSelectedPageName));
            }

            let selection_menu = Menu::new(rect, ViewId::SelectionMenu, MenuKind::Contextual, entries, context);
            rq.add(RenderData::new(selection_menu.id(), *selection_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(selection_menu) as Box<dyn View>);
        }
    }

    pub fn toggle_title_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::TitleMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let zoom_mode = self.view_port.zoom_mode;
            let scroll_mode = self.view_port.scroll_mode;
            let sf = if let ZoomMode::Custom(sf) = zoom_mode { sf } else { 1.0 };

            let mut entries = if self.reflowable {
                vec![EntryKind::SubMenu("Zoom Mode".to_string(), vec![
                     EntryKind::RadioButton("Fit to Page".to_string(),
                                            EntryId::SetZoomMode(ZoomMode::FitToPage),
                                            zoom_mode == ZoomMode::FitToPage),
                     EntryKind::RadioButton(format!("Custom ({:.1}%)", 100.0 * sf),
                                            EntryId::SetZoomMode(ZoomMode::Custom(sf)),
                                            zoom_mode == ZoomMode::Custom(sf))])]
            } else {
                vec![EntryKind::SubMenu("Zoom Mode".to_string(), vec![
                     EntryKind::RadioButton("Fit to Page".to_string(),
                                            EntryId::SetZoomMode(ZoomMode::FitToPage),
                                            zoom_mode == ZoomMode::FitToPage),
                     EntryKind::RadioButton("Fit to Width".to_string(),
                                            EntryId::SetZoomMode(ZoomMode::FitToWidth),
                                            zoom_mode == ZoomMode::FitToWidth),
                     EntryKind::RadioButton(format!("Custom ({:.1}%)", 100.0 * sf),
                                            EntryId::SetZoomMode(ZoomMode::Custom(sf)),
                                            zoom_mode == ZoomMode::Custom(sf))])]
            };

            entries.push(EntryKind::SubMenu("Scroll Mode".to_string(), vec![
                 EntryKind::RadioButton("Screen".to_string(),
                                        EntryId::SetScrollMode(ScrollMode::Screen),
                                        scroll_mode == ScrollMode::Screen),
                 EntryKind::RadioButton("Page".to_string(),
                                        EntryId::SetScrollMode(ScrollMode::Page),
                                        scroll_mode == ScrollMode::Page)]));

            if self.ephemeral {
                entries.push(EntryKind::Command("Save".to_string(), EntryId::Save));
            }

            if self.info.reader.as_ref().map_or(false, |r| !r.annotations.is_empty()) {
                entries.push(EntryKind::Command("Annotations".to_string(), EntryId::Annotations));
            }

            if self.info.reader.as_ref().map_or(false, |r| !r.bookmarks.is_empty()) {
                entries.push(EntryKind::Command("Bookmarks".to_string(), EntryId::Bookmarks));
            }

            if !entries.is_empty() {
                entries.push(EntryKind::Separator);
            }

            entries.push(EntryKind::CheckBox("Apply Dithering".to_string(),
                                             EntryId::ToggleDithered,
                                             context.fb.dithered()));

            if self.synthetic {
                if self.info.reader.as_ref().map_or(false,
                                                    |r| r.font_family.is_some()
                                                    || r.font_size.is_some()
                                                    || r.margin_width.is_some()
                                                    || r.text_align.is_some()
                                                    || r.line_height.is_some()) {
                    entries.push(EntryKind::Command("Use default settings".to_string(), EntryId::ResetToDefaults));
                }
                let mut themes = context.settings.themes.iter().enumerate()
                                    // .filter(|(_, x)| !x.name.trim_start().starts_with("__"))
                                    .map(|(i, x)| { EntryKind::CommandEx(x.name.clone(),
                                                                       EntryId::ApplyTheme(i),
                                                                       vec![EntryKind::Command("Rename".to_string(), EntryId::RenameTheme(i)),
                                                                            EntryKind::Command("Delete".to_string(), EntryId::DeleteTheme(i)),
                                                                            EntryKind::Command("Overwrite".to_string(), EntryId::OverwriteTheme(i)),
                                                                       ])
                }).collect::<Vec<EntryKind>>();
                if !themes.is_empty() {
                    themes.push(EntryKind::Separator);
                    themes.push(EntryKind::Command("New theme...".to_string(), EntryId::SaveTheme));
                    entries.push(EntryKind::SubMenu("Themes".to_string(), themes));
                } else {
                    entries.push(EntryKind::Command("Save settings as theme".to_string(), EntryId::SaveTheme));
                }

                if self.info.file.kind == "epub" {
                    if self.info.reader.as_ref().map_or(false, |r| r.extra_css.is_some()) {
                        let tweaks = vec![
                            EntryKind::Command("Show status".to_string(), EntryId::ShowCssTweaks),
                            EntryKind::Separator,
                            EntryKind::Command("Undo last".to_string(), EntryId::UndoLastCssTweak),
                            EntryKind::Command("Undo all".to_string(), EntryId::UndoAllCssTweaks),
                        ];
                        entries.push(EntryKind::SubMenu("CSS tweaks".to_string(), tweaks));
                    } else if !context.settings.css_styles.is_empty() {
                        entries.push(EntryKind::Command("CSS tweaks".to_string(), EntryId::ShowCssTweaks));
                    }
                }
            }

            let kind = if let Some(_) = locate::<TopBar>(self) {
                MenuKind::DropDown
            } else {
                MenuKind::Contextual
            };

            let mut title_menu = Menu::new(rect, ViewId::TitleMenu, kind, entries, context);
            title_menu.child_mut(1)
                      .downcast_mut::<MenuEntry>().unwrap()
                      .set_disabled(zoom_mode != ZoomMode::FitToWidth, rq);

            rq.add(RenderData::new(title_menu.id(), *title_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(title_menu) as Box<dyn View>);
        }
    }

    fn toggle_font_family_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::FontFamilyMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let mut families = family_names(&context.settings.reader.font_path)
                                           .map_err(|e| eprintln!("Can't get family names: {:#}.", e))
                                           .unwrap_or_default();
            let current_family = self.info.reader.as_ref()
                                     .and_then(|r| r.font_family.clone())
                                     .unwrap_or_else(|| context.settings.reader.font_family.clone());
            families.insert(DEFAULT_FONT_FAMILY.to_string());
            let entries = families.iter().map(|f| EntryKind::RadioButton(f.clone(),
                                                                         EntryId::SetFontFamily(f.clone()),
                                                                         *f == current_family)).collect();
            let font_family_menu = Menu::new(rect, ViewId::FontFamilyMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(font_family_menu.id(), *font_family_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(font_family_menu) as Box<dyn View>);
        }
    }

    fn toggle_font_size_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::FontSizeMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let font_size = self.info.reader.as_ref().and_then(|r| r.font_size)
                                .unwrap_or(context.settings.reader.font_size);
            let min_font_size = context.settings.reader.font_size / 2.0;
            let max_font_size = 3.0 * context.settings.reader.font_size / 2.0;
            let entries = (0..=20).filter_map(|v| {
                let fs = font_size - 1.0 + v as f32 / 10.0;
                if fs >= min_font_size && fs <= max_font_size {
                    Some(EntryKind::RadioButton(format!("{:.1}", fs),
                                                EntryId::SetFontSize(v),
                                                (fs - font_size).abs() < 0.05))
                } else {
                    None
                }
            }).collect();
            let font_size_menu = Menu::new(rect, ViewId::FontSizeMenu, MenuKind::Contextual, entries, context);
            rq.add(RenderData::new(font_size_menu.id(), *font_size_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(font_size_menu) as Box<dyn View>);
        }
    }

    fn toggle_text_align_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::TextAlignMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let text_align = self.info.reader.as_ref().and_then(|r| r.text_align)
                                .unwrap_or(context.settings.reader.text_align);
            let choices = [TextAlign::Justify, TextAlign::Left, TextAlign::Right, TextAlign::Center];
            let entries = choices.iter().map(|v| {
                EntryKind::RadioButton(v.to_string(),
                                       EntryId::SetTextAlign(*v),
                                       text_align == *v)
            }).collect();
            let text_align_menu = Menu::new(rect, ViewId::TextAlignMenu, MenuKind::Contextual, entries, context);
            rq.add(RenderData::new(text_align_menu.id(), *text_align_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(text_align_menu) as Box<dyn View>);
        }
    }

    fn toggle_line_height_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::LineHeightMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }
            let line_height = self.info.reader.as_ref()
                                  .and_then(|r| r.line_height).unwrap_or(context.settings.reader.line_height);
            let lh_gradient = context.settings.reader.line_height_gradient.clamp(MIN_LINE_HEIGHT_GRADIENT, MAX_LINE_HEIGHT_GRADIENT);
            let epsilon = lh_gradient / 2.0;
            // line heights go from 1.0 - 2.5
            // capped at 25 choices in case user chose a very fine line_height_gradient
            let cnt = ((1.5 / lh_gradient) as i32).min(25);
            let entries = (0..=cnt).map(|x| {
                let lh = 1.0 + x as f32 * lh_gradient;
                EntryKind::RadioButton(format!("{:.3}", lh),
                                       EntryId::SetLineHeight(x),
                                       (lh - line_height).abs() < epsilon)
            }).collect();
            let line_height_menu = Menu::new(rect, ViewId::LineHeightMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(line_height_menu.id(), *line_height_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(line_height_menu) as Box<dyn View>);
        }
    }

    fn toggle_contrast_exponent_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::ContrastExponentMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let entries = (0..=8).map(|x| {
                let e = 1.0 + x as f32 / 2.0;
                EntryKind::RadioButton(format!("{:.1}", e),
                                       EntryId::SetContrastExponent(x),
                                       (e - self.contrast.exponent).abs() < f32::EPSILON)
            }).collect();
            let contrast_exponent_menu = Menu::new(rect, ViewId::ContrastExponentMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(contrast_exponent_menu.id(), *contrast_exponent_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(contrast_exponent_menu) as Box<dyn View>);
        }
    }

    fn toggle_contrast_gray_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::ContrastGrayMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let entries = (1..=6).map(|x| {
                let g = ((1 << 8) - (1 << (8 - x))) as f32;
                EntryKind::RadioButton(format!("{:.1}", g),
                                       EntryId::SetContrastGray(x),
                                       (g - self.contrast.gray).abs() < f32::EPSILON)
            }).collect();
            let contrast_gray_menu = Menu::new(rect, ViewId::ContrastGrayMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(contrast_gray_menu.id(), *contrast_gray_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(contrast_gray_menu) as Box<dyn View>);
        }
    }

    fn toggle_theme_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::ThemeMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }
            let mut entries = context.settings.themes.iter().enumerate()
                                // .filter(|(_, x)| !x.name.trim_start().starts_with("__"))
                                .map(|(i, x)| { EntryKind::CommandEx(x.name.clone(),
                                                                     EntryId::ApplyTheme(i),
                                                                     vec![EntryKind::Command("Rename".to_string(), EntryId::RenameTheme(i)),
                                                                          EntryKind::Command("Delete".to_string(), EntryId::DeleteTheme(i)),
                                                                          EntryKind::Command("Overwrite".to_string(), EntryId::OverwriteTheme(i)),
                                                                     ])

            }).collect::<Vec<EntryKind>>();
            if !entries.is_empty() {
                entries.push(EntryKind::Separator);
            }
            entries.push(EntryKind::Command("New theme...".to_string(), EntryId::SaveTheme));
            let theme_menu = Menu::new(rect, ViewId::ThemeMenu, MenuKind::Contextual, entries, context);
            rq.add(RenderData::new(theme_menu.id(), *theme_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(theme_menu) as Box<dyn View>);
        }
    }

    fn toggle_theme_dialog(&mut self, enable: bool, idx: Option<usize>, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate::<ThemeDialog>(self) {
            if enable { return; }
            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if !enable { return; }
            self.toggle_bars(Some(false), hub, rq, context);
            let font_size = self.info.reader.as_ref().and_then(|r| r.font_size)
                                .unwrap_or(context.settings.reader.font_size);
            let has_relative_fs = (font_size - context.settings.reader.font_size).abs() > f32::EPSILON;
            let thd = ThemeDialog::new(has_relative_fs, idx, context);
            rq.add(RenderData::new(thd.id(), *thd.rect(), UpdateMode::Gui));
            self.children.push(Box::new(thd) as Box<dyn View>);
        }
    }

    fn toggle_name_theme(&mut self, enable: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::NameTheme) {
            if enable { return; }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);

            self.toggle_keyboard(false, None, hub, rq, context);
        } else {
            if !enable { return; }

            let mut name_theme = NamedInput::new("Name theme".to_string(),
                                             ViewId::NameTheme, ViewId::NameThemeInput, 21, context);
            if let Some(ThemeStash::Existing(idx)) = self.theme {
                let name = context.settings.themes[idx].name.clone();
                name_theme.set_text(&name, rq, context);
            }
            rq.add(RenderData::new(name_theme.id(), *name_theme.rect(), UpdateMode::Gui));
            hub.send(Event::Focus(Some(ViewId::NameThemeInput))).ok();

            self.children.push(Box::new(name_theme) as Box<dyn View>);
        }
    }

    /// stash newly created theme away while getting theme name
    fn stash_theme(&mut self, context: &mut Context) {
        if let Some(index) = locate::<ThemeDialog>(self) {
            if let Some(thd) = self.child(index).downcast_ref::<ThemeDialog>() {
                let mut theme = Theme::default();
                if thd.is_on(ThemeProp::FontFamily) {
                    theme.font_family = Some(self.info.reader.as_ref()
                        .and_then(|r| r.font_family.clone())
                        .unwrap_or(context.settings.reader.font_family.clone()));
                }
                if thd.is_on(ThemeProp::RelativeFontSize) {
                    theme.font_size = Some(self.info.reader.as_ref()
                            .and_then(|r| r.font_size)
                            .unwrap_or(context.settings.reader.font_size)
                        - context.settings.reader.font_size);
                    theme.font_size_relative = Some(true);
                } else if thd.is_on(ThemeProp::FontSize) {
                    theme.font_size = Some(self.info.reader.as_ref()
                        .and_then(|r| r.font_size)
                        .unwrap_or(context.settings.reader.font_size));
                }
                if thd.is_on(ThemeProp::MarginWidth) {
                    theme.margin_width = Some(self.info.reader.as_ref()
                        .and_then(|r| r.margin_width)
                        .unwrap_or(context.settings.reader.margin_width));
                }
                if thd.is_on(ThemeProp::LineSpacing) {
                    theme.line_height = Some(self.info.reader.as_ref()
                        .and_then(|r| r.line_height)
                        .unwrap_or(context.settings.reader.line_height));
                }
                if thd.is_on(ThemeProp::TextAlign) {
                    theme.text_align = Some(self.info.reader.as_ref()
                        .and_then(|r| r.text_align)
                        .unwrap_or(context.settings.reader.text_align));
                }
                if thd.is_on(ThemeProp::FrontLight) {
                    theme.frontlight = Some(context.settings.frontlight);
                    theme.frontlight_levels = Some(context.frontlight.levels());
                }
                if thd.is_on(ThemeProp::InvertedMode) {
                    theme.inverted = Some(context.fb.inverted());
                }
                // if thd.is_on(ThemeProp::IgnoreDocumentCss) {
                //     theme.ignore_document_css = Some(true);
                // }
                if thd.is_on(ThemeProp::KeepMenuOnScreen) {
                    theme.dismiss = Some(false);
                }
                self.theme = Some(ThemeStash::New(theme));
            }
        }
    }

    fn save_theme(&mut self, name: &str, hub: &Hub, context: &mut Context) {
        if let Some(ThemeStash::New(ref mut theme)) = self.theme {
            theme.name = name.trim().to_string();
            let mode = if let Some(index) = context.settings.themes
                                .iter()
                                .position(|x| x.name.to_lowercase() == theme.name.to_lowercase()) {
                context.settings.themes[index] = theme.clone();
                "Replaced"
            } else {
                context.settings.themes.push(theme.clone());
                "Created"
            };
            hub.send(Event::Notify(format!("{} theme {}", mode, theme.name))).ok();
        }
        self.theme = None;
    }

    fn toggle_margin_width_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::MarginWidthMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let reflowable = self.reflowable;
            let margin_width = self.info.reader.as_ref()
                                   .and_then(|r| if reflowable { r.margin_width } else { r.screen_margin_width })
                                   .unwrap_or_else(|| if reflowable { context.settings.reader.margin_width } else { 0 });
            let min_margin_width = context.settings.reader.min_margin_width;
            let max_margin_width = context.settings.reader.max_margin_width;
            let entries = (min_margin_width..=max_margin_width).map(|mw|
                EntryKind::RadioButton(format!("{}", mw),
                                       EntryId::SetMarginWidth(mw),
                                       mw == margin_width)
            ).collect();
            let margin_width_menu = Menu::new(rect, ViewId::MarginWidthMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(margin_width_menu.id(), *margin_width_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(margin_width_menu) as Box<dyn View>);
        }
    }

    fn toggle_page_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::PageMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let has_name = self.info.reader.as_ref()
                               .map_or(false, |r| r.page_names.contains_key(&self.current_page));

            let mut entries = vec![EntryKind::Command("Name".to_string(), EntryId::SetPageName)];
            if has_name {
                entries.push(EntryKind::Command("Remove Name".to_string(), EntryId::RemovePageName));
            }
            let names = self.info.reader.as_ref()
                            .map(|r| r.page_names.iter()
                                      .map(|(i, s)| EntryKind::Command(s.to_string(), EntryId::GoTo(*i)))
                                      .collect::<Vec<EntryKind>>())
                            .unwrap_or_default();
            if !names.is_empty() {
                entries.push(EntryKind::Separator);
                entries.push(EntryKind::SubMenu("Go To".to_string(), names));
            }

            let page_menu = Menu::new(rect, ViewId::PageMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(page_menu.id(), *page_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(page_menu) as Box<dyn View>);
        }
    }

    fn toggle_margin_cropper_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::MarginCropperMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let current_page = self.current_page;
            let is_split = self.info.reader.as_ref()
                               .and_then(|r| r.cropping_margins
                                              .as_ref().map(CroppingMargins::is_split));

            let mut entries = vec![EntryKind::RadioButton("Any".to_string(),
                                                          EntryId::ApplyCroppings(current_page, PageScheme::Any),
                                                          is_split.is_some() && !is_split.unwrap()),
                                   EntryKind::RadioButton("Even/Odd".to_string(),
                                                          EntryId::ApplyCroppings(current_page, PageScheme::EvenOdd),
                                                          is_split.is_some() && is_split.unwrap())];

            let is_applied = self.info.reader.as_ref()
                                 .map(|r| r.cropping_margins.is_some())
                                 .unwrap_or(false);
            if is_applied {
                entries.extend_from_slice(&[EntryKind::Separator,
                                            EntryKind::Command("Remove".to_string(), EntryId::RemoveCroppings)]);
            }

            let margin_cropper_menu = Menu::new(rect, ViewId::MarginCropperMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(margin_cropper_menu.id(), *margin_cropper_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(margin_cropper_menu) as Box<dyn View>);
        }
    }

    fn toggle_search_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate_by_id(self, ViewId::SearchMenu) {
            if let Some(true) = enable {
                return;
            }

            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }

            let entries = vec![EntryKind::RadioButton("Forward".to_string(),
                                                      EntryId::SearchDirection(LinearDir::Forward),
                                                      self.search_direction == LinearDir::Forward),
                               EntryKind::RadioButton("Backward".to_string(),
                                                      EntryId::SearchDirection(LinearDir::Backward),
                                                      self.search_direction == LinearDir::Backward)];

            let search_menu = Menu::new(rect, ViewId::SearchMenu, MenuKind::Contextual, entries, context);
            rq.add(RenderData::new(search_menu.id(), *search_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(search_menu) as Box<dyn View>);
        }
    }

    fn set_font_size(&mut self, font_size: f32, redraw: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }

        if let Some(ref mut r) = self.info.reader {
            r.font_size = Some(font_size);
        }

        let (width, height) = context.display.dims;
        {
            let mut doc = self.doc.lock().unwrap();

            doc.layout(width, height, font_size, CURRENT_DEVICE.dpi);

            if !redraw { return; }

            if self.synthetic {
                let current_page = self.current_page.min(doc.pages_count() - 1);
                if let Some(location) = doc.resolve_location(Location::Exact(current_page)) {
                    self.current_page = location;
                }
            } else {
                let ratio = doc.pages_count() / self.pages_count;
                self.pages_count = doc.pages_count();
                self.current_page = (ratio * self.current_page).min(self.pages_count - 1);
            }
        }
        self.font_size = font_size;
        self.cache.clear();
        self.text.clear();
        self.update(Some(UpdateMode::Partial), hub, rq, context);
        self.update_tool_bar(rq, context);
        self.update_bottom_bar(rq);
    }

    fn set_default(&mut self, prop: &ThemeProp, hub: &Hub, context: &mut Context) {
        let mut changed = false;
        if let Some(ref r) = self.info.reader {
            let defaults = &mut context.settings.reader;
            match *prop {
                ThemeProp::FontFamily => if let Some(ref font) = r.font_family {
                    let font_family = font.to_string();
                    if defaults.font_family != font_family {
                        defaults.font_family = font_family;
                        changed = true;
                    }
                },
                ThemeProp::FontSize => if let Some(font_size) = r.font_size {
                    if defaults.font_size != font_size {
                        defaults.font_size = font_size;
                        changed = true;
                    }
                },
                ThemeProp::MarginWidth => if let Some(margin_width) = r.margin_width {
                    if defaults.margin_width != margin_width {
                        defaults.margin_width = margin_width;
                        changed = true;
                    }
                },
                ThemeProp::TextAlign => if let Some(text_align) = r.text_align {
                    if defaults.text_align != text_align {
                        defaults.text_align = text_align;
                        changed = true;
                    }
                },
                ThemeProp::LineSpacing => if let Some(line_height) = r.line_height {
                    if defaults.line_height != line_height {
                        defaults.line_height = line_height;
                        changed = true;
                    }
                },
                _ => (),
            };
        }
        let msg = if changed {
            format!("Default {} set", *prop)
        } else {
            "Already the default".to_string()
        };
        hub.send(Event::Notify(msg)).ok();
    }

    fn reset_to_defaults(&mut self, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let r = self.info.reader.clone();
        let defaults = &context.settings.reader.clone();
        if let Some(ref r) = r {
            if self.reflowable {
                if let Some(ref font) = r.font_family {
                    if &defaults.font_family[..] != font {
                        self.set_font_family(&defaults.font_family[..], false, hub, rq, context);
                    }
                }
                if let Some(font_size) = r.font_size {
                    if defaults.font_size != font_size {
                        self.set_font_size(defaults.font_size, false, hub, rq, context);
                    }
                }
                if let Some(margin_width) = r.margin_width {
                    if defaults.margin_width != margin_width {
                        self.set_margin_width(defaults.margin_width, false, hub, rq, context);
                    }
                }
                if let Some(text_align) = r.text_align {
                    if defaults.text_align != text_align {
                        self.set_text_align(defaults.text_align, false, hub, rq, context);
                    }
                }
                if let Some(line_height) = r.line_height {
                    if defaults.line_height != line_height {
                        self.set_line_height(defaults.line_height, false, hub, rq, context);
                    }
                }
            }
        }
        if let Some(ref mut r) = self.info.reader {
            r.font_family = None;
            r.font_size = None;
            r.margin_width = None;
            r.text_align = None;
            r.line_height = None;
        }
        {
            let mut doc = self.doc.lock().unwrap();
            let current_page = self.current_page.min(doc.pages_count() - 1);
            if let Some(location) =  doc.resolve_location(Location::Exact(current_page)) {
                self.current_page = location;
            }
        }
        self.cache.clear();
        self.text.clear();
        self.update(Some(UpdateMode::Partial), hub, rq, context);
        self.update_tool_bar(rq, context);
        self.update_bottom_bar(rq);
    }

    fn apply_theme(&mut self, idx: usize, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }

        if let Some(theme) = context.settings.themes.get(idx) {
            let theme = theme.clone(); // make borrow checker happy
            if theme.dismiss.unwrap_or(true) {
                self.toggle_bars(Some(false), hub, rq, context);
            }
            let mut dirty = false;
            if let Some(ref v) = theme.font_family {
                self.set_font_family(v, false, hub, rq, context);
                dirty = true;
            }
            if let Some(v) = theme.font_size {
                let v = if v < 0.0 || theme.font_size_relative.unwrap_or(false) {
                    let font_size = self.info.reader.as_ref().and_then(|r| r.font_size)
                                        .unwrap_or(context.settings.reader.font_size);
                    v + font_size
                } else {
                    v
                };
                let min_font_size = context.settings.reader.font_size / 2.0;
                let max_font_size = 3.0 * context.settings.reader.font_size / 2.0;
                self.set_font_size(v.clamp(min_font_size, max_font_size), false, hub, rq, context);
                dirty = true;
            }
            if let Some(v) = theme.text_align {
                self.set_text_align(v, false, hub, rq, context);
                dirty = true;
            }
            if let Some(v) = theme.margin_width {
                let min_margin_width = context.settings.reader.min_margin_width;
                let max_margin_width = context.settings.reader.max_margin_width;
                let mw = v.clamp(min_margin_width, max_margin_width);
                self.set_margin_width(mw, false, hub, rq, context);
                dirty = true;
            }
            if let Some(v) = theme.line_height {
                self.set_line_height(v.clamp(0.5, 2.0), false, hub, rq, context);
                dirty = true;
            }
            if let Some(v) = theme.frontlight {
                if context.settings.frontlight != v {
                    hub.send(Event::ToggleFrontlight).ok();
                }
            }
            if let Some(ref v) = theme.frontlight_levels {
                context.frontlight.set_intensity(v.intensity);
                context.frontlight.set_warmth(v.warmth);
            }
            if let Some(v) = theme.inverted {
                if v != context.fb.inverted()
                   && theme.name.trim() != ON_INVERTED && theme.name.trim() != ON_UNINVERTED {
                    hub.send(Event::Select(EntryId::ToggleInverted)).ok();
                }
            }
            if let Some(v) = theme.ignore_document_css {
                {
                    let mut doc = self.doc.lock().unwrap();
                    doc.set_ignore_document_css(v);
                }
                dirty = true;
            }
            if dirty {
                {
                    let mut doc = self.doc.lock().unwrap();
                    let current_page = self.current_page.min(doc.pages_count() - 1);
                    if let Some(location) =  doc.resolve_location(Location::Exact(current_page)) {
                        self.current_page = location;
                    }
                }
                self.cache.clear();
                self.text.clear();
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                self.update_bottom_bar(rq);
            }
        }
    }

    fn apply_css_tweak(&mut self, index: usize, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }
        if let Some(Selection { anchor: TextLocation::Dynamic(offset), .. }) = self.selection {
            let (div_sel, span_sel);
            {
                let mut doc = self.doc.lock().unwrap();
                if let Some((dsel, ssel, _, _)) = doc.get_node_data_at(offset, 0) {
                    (div_sel, span_sel) = (dsel, ssel);
                } else {
                    hub.send(Event::Notify("Unable to determine CSS selector".to_string())).ok();
                    return;
                }
            }
            if span_sel.is_empty() {
                self.apply_css_tweak_aux(&div_sel, index, hub, context);
            } else {
                let entries = vec![div_sel.to_owned(),
                                   span_sel.to_owned(),
                                   format!("{} {}", div_sel, span_sel),
                                   format!("{}, {}", div_sel, span_sel),
                                   format!("{0}, {0} {1}", div_sel, span_sel)];
                let entries = entries.iter()
                    .map(|x| { EntryKind::Command(x.clone(),
                                                  EntryId::SetCssTweakEx(x.clone(), index))
                }).collect();
                let pt = pt!(self.rect().width() as i32 / 2, self.rect().height() as i32 / 3);
                let menu = Menu::new(rect![pt, pt], ViewId::CssSelectorMenu, MenuKind::Contextual, entries, context);
                rq.add(RenderData::new(menu.id(), *menu.rect(), UpdateMode::Gui));
                self.children.push(Box::new(menu) as Box<dyn View>);
            }
        }
    }

    fn apply_css_tweak_aux(&mut self, selector: &str, index: usize, hub: &Hub, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }
        let mut dirty = false;
        let mut doc = self.doc.lock().unwrap();
        if let Some(ref mut r) = self.info.reader {
            let mut css = context.settings.css_styles[index].css.trim().to_string();
            // \n used to separate rules
            css = format!("\n{} {}{}{}",
                          selector,
                          if css.starts_with('{') {""} else {"{"},
                          css,
                          if css.ends_with('}') {""} else {"}"});
            if let Some(ref old_css) = r.extra_css {
                css = str::replacen(old_css, &css, "", 1) + &css;
            }
            r.extra_css = Some(css.to_string());
            set_extra_css!(doc, css, &context.settings);
            dirty = true;
            hub.send(Event::Notify(format!("{} applied to {}",
                                           context.settings.css_styles[index].name,
                                           selector))).ok();
        }
        if dirty {
            self.cache.clear();
            self.text.clear();
        }
    }

    fn css_tweaks_as_html(&mut self, context: &mut Context) -> Option<String> {
        if Arc::strong_count(&self.doc) > 1 {
            return None;
        }

        if let Some(ref r) = self.info.reader {
            let mut buf =  "<html><head><title>CSS tweaks</title>\n\
                            <link rel=\"stylesheet\" type=\"text/css\" href=\"css/css-tweaks.css\"/>\n\
                            </head>\n<body>\n".to_string();
            if let Some(Selection { anchor: TextLocation::Dynamic(offset), .. }) = self.selection {
                let mut doc = self.doc.lock().unwrap();
                if let Some((div_sel, span_sel, txt, html)) = doc.get_node_data_at(offset, 700) {
                    let selector = format!("{}{}{}",
                                           div_sel,
                                           if span_sel.is_empty() {""} else {", "},
                                           span_sel);
                    buf.push_str(&format!("<p><strong>selector</strong>: {}<br />\n\
                                           <strong>text</strong>: {}{}<br />\n\
                                           <strong>html</strong>: <pre>... {} ...</pre></p>\n",
                                          encode_entities(&selector),
                                          encode_entities(safe_slice(&txt, 0, txt.len().min(200))),
                                          if txt.len() > 200 {" ..."} else {""},
                                          encode_entities(&html)));
                }
            }
            if let Some(ref css) = r.extra_css {
                buf.push_str("<h3>Applied styles</h3>\n");
                buf.push_str(&format!("<ul>\n<li><code>{}</code></li>\n</ul>\n",
                                      encode_entities(css).trim().replace("}", "}</code></li>\n<li><code>")));
            }
            if !context.settings.css_styles.is_empty() {
                buf.push_str("<h3>Available styles</h3>\n");
                let styles = context.settings.css_styles.iter()
                                    .filter(|x| !x.css.trim().is_empty())
                                    .map(|x| format!("<li><strong>{}</strong>: <code>{}</code></li>\n",
                                                     encode_entities(&x.name),
                                                     encode_entities(&x.css)))
                                    .collect::<String>();
                buf.push_str(&format!("<ul>\n{}\n</ul>\n", styles));
            }
            buf.push_str("\n</body></html>");
            Some(buf)
        } else {
            None
        }
    }

    fn undo_last_tweak(&mut self, hub: &Hub, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }

        let mut css = "".to_string();
        let mut changed = false;
        if let Some(ref mut r) = self.info.reader {
            let old_css = r.extra_css.as_ref().unwrap().trim().to_string();
            // locate the next to last } (the last } isn't followed by \n thanks to trim() )
            if let Some(i) = old_css.rfind("}\n") {
                css = old_css[..=i].to_string();
            }
            if css != old_css {
                r.extra_css = if !css.is_empty() {
                    Some(css.to_string())
                } else {
                    None
                };
                changed = true;
            }
        }
        if changed {
            {
                let mut doc = self.doc.lock().unwrap();
                set_extra_css!(doc, css, &context.settings);
            }
            hub.send(Event::Notify("Last tweak removed".to_string())).ok();
            self.cache.clear();
            self.text.clear();
        }
    }

    fn set_text_align(&mut self, text_align: TextAlign, redraw: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }

        if let Some(ref mut r) = self.info.reader {
            r.text_align = Some(text_align);
        }

        {
            let mut doc = self.doc.lock().unwrap();
            doc.set_text_align(text_align);

            if !redraw { return; }

            if self.synthetic {
                let current_page = self.current_page.min(doc.pages_count() - 1);
                if let Some(location) =  doc.resolve_location(Location::Exact(current_page)) {
                    self.current_page = location;
                }
            } else {
                self.pages_count = doc.pages_count();
                self.current_page = self.current_page.min(self.pages_count - 1);
            }
        }

        self.cache.clear();
        self.text.clear();
        self.update(Some(UpdateMode::Partial), hub, rq, context);
        self.update_tool_bar(rq, context);
        self.update_bottom_bar(rq);
    }

    fn set_font_family(&mut self, font_family: &str, redraw: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }

        if let Some(ref mut r) = self.info.reader {
            r.font_family = Some(font_family.to_string());
        }

        {
            let mut doc = self.doc.lock().unwrap();
            let font_path = if font_family == DEFAULT_FONT_FAMILY {
                "fonts"
            } else {
                &context.settings.reader.font_path
            };

            doc.set_font_family(font_family, font_path);

            if !redraw { return; }

            if self.synthetic {
                let current_page = self.current_page.min(doc.pages_count() - 1);
                if let Some(location) =  doc.resolve_location(Location::Exact(current_page)) {
                    self.current_page = location;
                }
            } else {
                self.pages_count = doc.pages_count();
                self.current_page = self.current_page.min(self.pages_count - 1);
            }
        }

        self.cache.clear();
        self.text.clear();
        self.update(Some(UpdateMode::Partial), hub, rq, context);
        self.update_tool_bar(rq, context);
        self.update_bottom_bar(rq);
    }

    fn set_line_height(&mut self, line_height: f32, redraw: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }

        if let Some(ref mut r) = self.info.reader {
            r.line_height = Some(line_height);
        }

        {
            let mut doc = self.doc.lock().unwrap();
            doc.set_line_height(line_height);

            if !redraw { return; }

            if self.synthetic {
                let current_page = self.current_page.min(doc.pages_count() - 1);
                if let Some(location) =  doc.resolve_location(Location::Exact(current_page)) {
                    self.current_page = location;
                }
            } else {
                self.pages_count = doc.pages_count();
                self.current_page = self.current_page.min(self.pages_count - 1);
            }
        }

        self.cache.clear();
        self.text.clear();
        self.update(Some(UpdateMode::Partial), hub, rq, context);
        self.update_tool_bar(rq, context);
        self.update_bottom_bar(rq);
    }

    fn set_margin_width(&mut self, width: i32, redraw: bool, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if Arc::strong_count(&self.doc) > 1 {
            return;
        }

        if let Some(ref mut r) = self.info.reader {
            if self.reflowable {
                r.margin_width = Some(width);
            } else {
                if width == 0 {
                    r.screen_margin_width = None;
                } else {
                    r.screen_margin_width = Some(width);
                }
            }
        }

        if self.reflowable {
            let mut doc = self.doc.lock().unwrap();
            doc.set_margin_width(width, self.synthetic & self.progress_bar.enabled);

            if self.synthetic {
                let current_page = self.current_page.min(doc.pages_count() - 1);
                if let Some(location) =  doc.resolve_location(Location::Exact(current_page)) {
                    self.current_page = location;
                }
            } else {
                self.pages_count = doc.pages_count();
                self.current_page = self.current_page.min(self.pages_count - 1);
            }
        } else {
            let next_margin_width = mm_to_px(width as f32, CURRENT_DEVICE.dpi) as i32;
            if self.view_port.zoom_mode == ZoomMode::FitToWidth {
                // Apply the scale change.
                let ratio = (self.rect.width() as i32 - 2 * next_margin_width) as f32 /
                            (self.rect.width() as i32 - 2 * self.view_port.margin_width) as f32;
                self.view_port.page_offset.y = (self.view_port.page_offset.y as f32 * ratio) as i32;
            } else {
                // Keep the center still.
                self.view_port.page_offset += pt!(next_margin_width - self.view_port.margin_width);
            }
            self.view_port.margin_width = next_margin_width;
        }

        if redraw {
            self.text.clear();
            self.cache.clear();
            self.update(Some(UpdateMode::Partial), hub, rq, context);
            self.update_tool_bar(rq, context);
            self.update_bottom_bar(rq);
        }
    }

    fn toggle_bookmark(&mut self, rq: &mut RenderQueue) {
        if let Some(ref mut r) = self.info.reader {
            if !r.bookmarks.insert(self.current_page) {
                r.bookmarks.remove(&self.current_page);
            }
        }
        let w = self.rect.width() as i32 / 25;
        let min = pt!(self.rect.max.x - w, self.rect.min.y);
        let max = pt!(self.rect.max.x, self.rect.min.y + w);
        rq.add(RenderData::new(self.id, rect![min, max], UpdateMode::Gui));
    }

    fn set_contrast_exponent(&mut self, exponent: f32, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(ref mut r) = self.info.reader {
            r.contrast_exponent = Some(exponent);
        }
        self.contrast.exponent = exponent;
        self.update(Some(UpdateMode::Partial), hub, rq, context);
        self.update_tool_bar(rq, context);
    }

    fn set_contrast_gray(&mut self, gray: f32, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(ref mut r) = self.info.reader {
            r.contrast_gray = Some(gray);
        }
        self.contrast.gray = gray;
        self.update(Some(UpdateMode::Partial), hub, rq, context);
        self.update_tool_bar(rq, context);
    }

    fn set_zoom_mode(&mut self, zoom_mode: ZoomMode, reset_page_offset: bool, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        if self.view_port.zoom_mode == zoom_mode {
            return;
        }

        if let Some(index) = locate_by_id(self, ViewId::TitleMenu) {
            self.child_mut(index)
                .child_mut(1)
                .downcast_mut::<MenuEntry>().unwrap()
                .set_disabled(zoom_mode != ZoomMode::FitToWidth, rq);
        }

        self.view_port.zoom_mode = zoom_mode;
        if reset_page_offset {
            self.view_port.page_offset = pt!(0, 0);
        }
        self.cache.clear();
        self.update(Some(UpdateMode::Partial), hub, rq, context);
    }

    fn toggle_inverted(&mut self, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let inverted = !context.fb.inverted();
        self.update_noninverted_regions(inverted);
        context.fb.toggle_inverted();
        context.settings.inverted = inverted;
        rq.add(RenderData::new(self.id(), context.fb.rect(), UpdateMode::Full));
        if let Some(idx) = context.settings.themes.iter()
                            .position(|x| x.name.trim()
                                          == if inverted { ON_INVERTED } else { ON_UNINVERTED }) {
            self.apply_theme(idx, hub, rq, context);
        }
    }

    fn set_scroll_mode(&mut self, scroll_mode: ScrollMode, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        if self.view_port.scroll_mode == scroll_mode || self.view_port.zoom_mode != ZoomMode::FitToWidth {
            return;
        }
        self.view_port.scroll_mode = scroll_mode;
        self.view_port.page_offset = pt!(0, 0);
        self.update(None, hub, rq, context);
    }

    fn crop_margins(&mut self, index: usize, margin: &Margin, hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        if self.view_port.zoom_mode != ZoomMode::FitToPage {
            let Resource { pixmap, frame, .. } = self.cache.get(&index).unwrap();
            let offset = frame.min + self.view_port.page_offset;
            let x_ratio = offset.x as f32 / pixmap.width as f32;
            let y_ratio = offset.y as f32 / pixmap.height as f32;
            let dims = {
                let doc = self.doc.lock().unwrap();
                doc.dims(index).unwrap()
            };
            let scale = scaling_factor(&self.rect, margin, self.view_port.margin_width, dims, self.view_port.zoom_mode);
            if x_ratio >= margin.left && x_ratio <= (1.0 - margin.right) {
                self.view_port.page_offset.x = (scale * (x_ratio - margin.left) * dims.0) as i32;
            } else {
                self.view_port.page_offset.x = 0;
            }
            if y_ratio >= margin.top && y_ratio <= (1.0 - margin.bottom) {
                self.view_port.page_offset.y = (scale * (y_ratio - margin.top) * dims.1) as i32;
            } else {
                self.view_port.page_offset.y = 0;
            }
        }
        if let Some(r) = self.info.reader.as_mut() {
            if r.cropping_margins.is_none() {
                r.cropping_margins = Some(CroppingMargins::Any(Margin::default()));
            }
            for c in r.cropping_margins.iter_mut() {
                *c.margin_mut(index) = margin.clone();
            }
        }
        self.cache.clear();
        self.update(Some(UpdateMode::Partial), hub, rq, context);
    }

    fn toc(&self) -> Option<Vec<TocEntry>> {
        let mut index = 0;
        self.info.toc.as_ref()
            .map(|simple_toc| self.toc_aux(simple_toc, &mut index))
    }

    fn toc_aux(&self, simple_toc: &[SimpleTocEntry], index: &mut usize) -> Vec<TocEntry> {
        let mut toc = Vec::new();
        for entry in simple_toc {
            *index += 1;
            match entry {
                SimpleTocEntry::Leaf(title, location) | SimpleTocEntry::Container(title, location, _) => {
                    let current_title = title.clone();
                    let current_location = match location {
                        TocLocation::Uri(uri) if uri.starts_with('\'') => {
                            self.find_page_by_name(&uri[1..])
                                .map(Location::Exact)
                                .unwrap_or_else(|| location.clone().into())
                        },
                        _ => location.clone().into(),
                    };
                    let current_index = *index;
                    let current_children = if let SimpleTocEntry::Container(_, _, children) = entry {
                        self.toc_aux(children, index)
                    } else {
                        Vec::new()
                    };
                    toc.push(TocEntry {
                        title: current_title,
                        location: current_location,
                        index: current_index,
                        children: current_children,
                    });
                },
            }
        }
        toc
    }

    fn find_page_by_name(&self, name: &str) -> Option<usize> {
        self.info.reader.as_ref().and_then(|r| {
            if let Ok(a) = name.parse::<u32>() {
                r.page_names
                 .iter().filter_map(|(i, s)| s.parse::<u32>().ok().map(|b| (b, i)))
                 .filter(|(b, _)| *b <= a)
                 .max_by(|x, y| x.0.cmp(&y.0))
                 .map(|(b, i)| *i + (a - b) as usize)
            } else if let Some(a) = name.chars().next().and_then(|c| c.to_alphabetic_digit()) {
                r.page_names
                 .iter().filter_map(|(i, s)| s.chars().next()
                                              .and_then(|c| c.to_alphabetic_digit())
                                              .map(|c| (c, i)))
                 .filter(|(b, _)| *b <= a)
                 .max_by(|x, y| x.0.cmp(&y.0))
                 .map(|(b, i)| *i + (a - b) as usize)
            } else if let Ok(a) = Roman::from_str(name) {
                r.page_names
                 .iter().filter_map(|(i, s)| Roman::from_str(s).ok().map(|b| (*b, i)))
                 .filter(|(b, _)| *b <= *a)
                 .max_by(|x, y| x.0.cmp(&y.0))
                 .map(|(b, i)| *i + (*a - b) as usize)
            } else {
                None
            }
        })
    }

    fn text_excerpt(&self, sel: [TextLocation; 2]) -> Option<String> {
        let [start, end] = sel;
        let parts = self.text.values().flatten()
                        .filter(|bnd| bnd.location >= start && bnd.location <= end)
                        .collect::<Vec<_>>();

        if parts.is_empty() {
            return None;
        }

        let mut text = String::new();
        let mut end_offset = 0;

        for p in &parts {
            let (is_dyn, offset) =
                if let TextLocation::Dynamic(offset) = p.location {
                    (true, offset)
                } else {
                    (false, 1)
                };
            if text.ends_with('\u{00AD}') {
                text.pop();
            } else if !text.ends_with('-') && !text.is_empty() && offset > end_offset {
                text.push(' ');
            }
            text += &p.text;
            if is_dyn {
                end_offset = offset + p.text.len();
            }
        }

        Some(text)
    }

    fn selected_text(&self) -> Option<String> {
        self.selection.as_ref().and_then(|sel| self.text_excerpt([sel.start, sel.end]))
    }

    fn text_rect(&self, sel: [TextLocation; 2]) -> Option<Rectangle> {
        let [start, end] = sel;
        let mut result: Option<Rectangle> = None;

        for chunk in &self.chunks {
            if let Some(words) = self.text.get(&chunk.location) {
                for word in words {
                    if word.location >= start && word.location <= end {
                        let rect = (word.rect * chunk.scale).to_rect() - chunk.frame.min + chunk.position;
                        if let Some(ref mut r) = result {
                            r.absorb(&rect);
                        } else {
                            result = Some(rect);
                        }
                    }
                }
            }
        }

        result
    }

    fn render_results(&self, rq: &mut RenderQueue) {
        for chunk in &self.chunks {
            if let Some(groups) = self.search.as_ref().and_then(|s| s.highlights.get(&chunk.location)) {
                for rects in groups {
                    let mut rect_opt: Option<Rectangle> = None;
                    for rect in rects {
                        let rect = (*rect * chunk.scale).to_rect() - chunk.frame.min + chunk.position;
                        if let Some(ref mut r) = rect_opt {
                            r.absorb(&rect);
                        } else {
                            rect_opt = Some(rect);
                        }
                    }
                    if let Some(rect) = rect_opt {
                        rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                    }
                }
            }
        }
    }

    fn selection_rect(&self) -> Option<Rectangle> {
        self.selection.as_ref().and_then(|sel| self.text_rect([sel.start, sel.end]))
    }

    fn find_annotation_ref(&mut self, sel: [TextLocation; 2]) -> Option<&Annotation> {
        self.info.reader.as_ref()
            .and_then(|r| r.annotations.iter()
                           .find(|a| a.selection[0] == sel[0] && a.selection[1] == sel[1]))
    }

    fn find_annotation_mut(&mut self, sel: [TextLocation; 2]) -> Option<&mut Annotation> {
        self.info.reader.as_mut()
            .and_then(|r| r.annotations.iter_mut()
                           .find(|a| a.selection[0] == sel[0] && a.selection[1] == sel[1]))
    }

    fn reseed(&mut self, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate::<TopBar>(self) {
            if let Some(top_bar) = self.child_mut(index).downcast_mut::<TopBar>() {
                top_bar.reseed(rq, context);
            }
        }

        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
    }

    fn quit(&mut self, context: &mut Context) {
        if let Some(ref mut s) = self.search {
            s.running.store(false, AtomicOrdering::Relaxed);
        }

        if self.ephemeral {
            return;
        }

        if let Some(ref mut r) = self.info.reader {
            r.current_page = self.current_page;
            r.pages_count = self.pages_count;
            r.finished = self.finished;
            r.dithered = context.fb.dithered();

            if self.view_port.zoom_mode == ZoomMode::FitToPage {
                r.zoom_mode = None;
                r.page_offset = None;
            } else {
                r.zoom_mode = Some(self.view_port.zoom_mode);
                r.page_offset = Some(self.view_port.page_offset);
            }

            if self.view_port.zoom_mode == ZoomMode::FitToWidth {
                r.scroll_mode = Some(self.view_port.scroll_mode);
            } else {
                r.scroll_mode = None;
            }

            r.rotation = Some(CURRENT_DEVICE.to_canonical(context.display.rotation));

            if (self.contrast.exponent - DEFAULT_CONTRAST_EXPONENT).abs() > f32::EPSILON {
                r.contrast_exponent = Some(self.contrast.exponent);
                if (self.contrast.gray - DEFAULT_CONTRAST_GRAY).abs() > f32::EPSILON {
                    r.contrast_gray = Some(self.contrast.gray);
                } else {
                    r.contrast_gray = None;
                }
            } else {
                r.contrast_exponent = None;
                r.contrast_gray = None;
            }

            context.library.sync_reader_info(&self.info.file.path, r);
        }
    }

    fn scale_page(&mut self, center: Point, factor: f32, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if self.cache.is_empty() {
            return;
        }

        let current_factor = if let ZoomMode::Custom(sf) = self.view_port.zoom_mode {
            sf
        } else {
            self.cache[&self.current_page].scale
        };

        if let Some(chunk) = self.chunks.iter().find(|chunk| {
            let chunk_rect = chunk.frame - chunk.frame.min + chunk.position;
            chunk_rect.includes(center)
        }) {
            let smw = self.view_port.margin_width;
            let frame = self.cache[&chunk.location].frame;
            self.current_page = chunk.location;
            self.view_port.page_offset = Point::from(factor * Vec2::from(center - chunk.position + chunk.frame.min - frame.min)) -
                                         pt!(self.rect.width() as i32 / 2 - smw,
                                             self.rect.height() as i32 / 2 - smw);

            self.set_zoom_mode(ZoomMode::Custom(current_factor * factor), false, hub, rq, context);
        }
    }

    fn has_progress_bar(&self) -> bool {
        self.synthetic && self.progress_bar.enabled && locate::<BottomBar>(self).is_none()
    }

    fn update_clock(&self, fb: &mut dyn Framebuffer, fonts: &mut Fonts) -> i32 {
        let pb = &self.progress_bar;
        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(fonts, &SMALL_STYLE, dpi);
        let margin = scale_by_dpi(pb.horz_margin as f32, dpi) as i32;
        let y_margin = scale_by_dpi(pb.vert_margin as f32, dpi) as i32;
        let x = self.rect.min.x as i32 + margin;
        let y = self.rect.max.y as i32 - y_margin;
        let clock_width = font.x_heights.0 as i32 * self.time_format.chars().count() as i32;
        let time = Local::now();
        let plan = font.plan(
            time.format(&self.time_format).to_string(),
            Some(clock_width + margin),
            None);
        let rect = rect![
            pt!(self.rect.min.x, y - font.x_heights.1 as i32 - 1),
            pt!(x + clock_width, self.rect.max.y)
        ];
        fb.draw_rectangle(&rect, WHITE);
        font.render(fb, BLACK, &plan, pt!(x + clock_width - plan.width, y));
        *self.dirty_clock.borrow_mut() = false;
        clock_width
    }

}

impl View for Reader {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, _bus: &mut Bus, rq: &mut RenderQueue, context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Rotate { quarter_turns, .. }) if quarter_turns != 0 => {
                let (_, dir) = CURRENT_DEVICE.mirroring_scheme();
                let n = (4 + (context.display.rotation - dir * quarter_turns)) % 4;
                hub.send(Event::Select(EntryId::Rotate(n))).ok();
                true
            },
            Event::Gesture(GestureEvent::Swipe { dir, start, end }) if self.rect.includes(start) => {
                match self.view_port.zoom_mode {
                    ZoomMode::FitToPage | ZoomMode::FitToWidth => {
                        match dir {
                            Dir::West => self.go_to_neighbor(CycleDir::Next, hub, rq, context),
                            Dir::East => self.go_to_neighbor(CycleDir::Previous, hub, rq, context),
                            Dir::South | Dir::North => self.vertical_scroll(start.y - end.y, hub, rq, context),
                        };
                    },
                    ZoomMode::Custom(_) => {
                        match dir {
                            Dir::West | Dir::East => self.directional_scroll(pt!(start.x - end.x, 0), hub, rq, context),
                            Dir::South | Dir::North => self.directional_scroll(pt!(0,start.y - end.y), hub, rq, context),
                        };
                    },
                }
                true
            },
            Event::Gesture(GestureEvent::SlantedSwipe { start, end, dir }) if self.rect.includes(start) => {
                if let ZoomMode::Custom(_) = self.view_port.zoom_mode {
                    self.directional_scroll(start - end, hub, rq, context);
                } else {
                    match dir {
                        DiagDir::NorthEast => self.toggle_inverted(hub, rq, context),
                        DiagDir::SouthWest => { self.quit(context); hub.send(Event::Back).ok(); },
                        DiagDir::NorthWest | DiagDir::SouthEast => {
                            let delta = if dir == DiagDir::NorthWest {-1} else {1};
                            let n = (4 + (context.display.rotation + delta)) % 4;
                            hub.send(Event::Select(EntryId::Rotate(n))).ok();
                        },
                    }
                }
                true
            },
            Event::Gesture(GestureEvent::Spread { axis: Axis::Horizontal, center, .. }) if self.rect.includes(center) => {
                if !self.reflowable {
                    self.set_zoom_mode(ZoomMode::FitToWidth, true, hub, rq, context);
                }
                true
            },
            Event::Gesture(GestureEvent::Pinch { axis: Axis::Horizontal, center, .. }) if self.rect.includes(center) => {
                self.set_zoom_mode(ZoomMode::FitToPage, true, hub, rq, context);
                true
            },
            Event::Gesture(GestureEvent::Spread { axis: Axis::Vertical, center, .. }) if self.rect.includes(center) => {
                if !self.reflowable {
                    self.set_scroll_mode(ScrollMode::Screen, hub, rq, context);
                }
                true

            },
            Event::Gesture(GestureEvent::Pinch { axis: Axis::Vertical, center, .. }) if self.rect.includes(center) => {
                if !self.reflowable {
                    self.set_scroll_mode(ScrollMode::Page, hub, rq, context);
                }
                true
            },
            Event::Gesture(GestureEvent::Spread { axis: Axis::Diagonal, center, factor }) |
            Event::Gesture(GestureEvent::Pinch { axis: Axis::Diagonal, center, factor }) if factor.is_finite() &&
                                                                                            self.rect.includes(center) => {
                self.scale_page(center, factor, hub, rq, context);
                true
            },
            Event::Gesture(GestureEvent::Arrow { dir, .. }) => {
                match dir {
                    Dir::West => {
                        if self.search.is_none() {
                            self.go_to_chapter(CycleDir::Previous, hub, rq, context);
                        } else {
                            self.go_to_results_page(0, hub, rq, context);
                        }
                    },
                    Dir::East => {
                        if self.search.is_none() {
                            self.go_to_chapter(CycleDir::Next, hub, rq, context);
                        } else {
                            let last_page = self.search.as_ref().unwrap().highlights.len() - 1;
                            self.go_to_results_page(last_page, hub, rq, context);
                        }
                    },
                    Dir::North => {
                        self.search_direction = LinearDir::Backward;
                        self.toggle_search_bar(true, hub, rq, context);
                    },
                    Dir::South => {
                        self.search_direction = LinearDir::Forward;
                        self.toggle_search_bar(true, hub, rq, context);
                    },
                }
                true
            },
            Event::Gesture(GestureEvent::Corner { dir, .. }) => {
                match dir {
                    DiagDir::NorthWest => self.go_to_bookmark(CycleDir::Previous, hub, rq, context),
                    DiagDir::NorthEast => self.go_to_bookmark(CycleDir::Next, hub, rq, context),
                    DiagDir::SouthEast => match context.settings.reader.bottom_right_gesture {
                        BottomRightGestureAction::ToggleDithered => {
                            hub.send(Event::Select(EntryId::ToggleDithered)).ok();
                        },
                        BottomRightGestureAction::ToggleInverted => {
                            self.toggle_inverted(hub, rq, context);
                        },
                    },
                    DiagDir::SouthWest => {
                        if context.settings.frontlight_presets.len() > 1 {
                            if context.settings.frontlight {
                                let lightsensor_level = if CURRENT_DEVICE.has_lightsensor() {
                                    context.lightsensor.level().ok()
                                } else {
                                    None
                                };
                                if let Some(ref frontlight_levels) = guess_frontlight(lightsensor_level, &context.settings.frontlight_presets) {
                                    let LightLevels { intensity, warmth } = *frontlight_levels;
                                    context.frontlight.set_intensity(intensity);
                                    context.frontlight.set_warmth(warmth);
                                }
                            }
                        } else {
                            hub.send(Event::ToggleFrontlight).ok();
                        }
                    },
                };
                true
            },
            Event::Gesture(GestureEvent::MultiCorner { dir, .. }) => {
                match dir {
                    DiagDir::NorthWest => self.go_to_annotation(CycleDir::Previous, hub, rq, context),
                    DiagDir::NorthEast => self.go_to_annotation(CycleDir::Next, hub, rq, context),
                    _ => (),
                }
                true
            },
            Event::Gesture(GestureEvent::Cross(_)) => {
                self.quit(context);
                hub.send(Event::Back).ok();
                true
            },
            Event::Gesture(GestureEvent::Diamond(_)) => {
                self.toggle_bars(None, hub, rq, context);
                true
            },
            Event::Gesture(GestureEvent::HoldButtonShort(code, ..)) => {
                match code {
                    ButtonCode::Backward => self.go_to_chapter(CycleDir::Previous, hub, rq, context),
                    ButtonCode::Forward => self.go_to_chapter(CycleDir::Next, hub, rq, context),
                    _ => (),
                }
                self.held_buttons.insert(code);
                true
            },
            Event::Device(DeviceEvent::Button { code, status: ButtonStatus::Released, .. }) => {
                if !self.held_buttons.remove(&code) {
                    match code {
                        ButtonCode::Backward => {
                            if self.search.is_none() {
                                self.go_to_neighbor(CycleDir::Previous, hub, rq, context);
                            } else {
                                self.go_to_results_neighbor(CycleDir::Previous, hub, rq, context);
                            }
                        },
                        ButtonCode::Forward => {
                            if self.search.is_none() {
                                self.go_to_neighbor(CycleDir::Next, hub, rq, context);
                            } else {
                                self.go_to_results_neighbor(CycleDir::Next, hub, rq, context);
                            }
                        },
                        _ => (),
                    }
                }
                true
            },
            Event::Device(DeviceEvent::Finger { position, status: FingerStatus::Motion, id, .. }) if self.state == State::Selection(id) => {
                let mut nearest_word = None;
                let mut dmin = u32::MAX;
                let dmax = (scale_by_dpi(RECT_DIST_JITTER, CURRENT_DEVICE.dpi) as i32).pow(2) as u32;
                let mut rects = Vec::new();

                for chunk in &self.chunks {
                    for word in &self.text[&chunk.location] {
                        let rect = (word.rect * chunk.scale).to_rect() - chunk.frame.min + chunk.position;
                        rects.push((rect, word.location));
                        let d = position.rdist2(&rect);
                        if d < dmax && d < dmin {
                            dmin = d;
                            nearest_word = Some(word.clone());
                        }
                    }
                }

                let selection = self.selection.as_mut().unwrap();

                if let Some(word) = nearest_word {
                    let old_start = selection.start;
                    let old_end = selection.end;
                    let (start, end) = word.location.min_max(selection.anchor);

                    if start == old_start && end == old_end {
                        return true;
                    }

                    let (start_low, start_high) = old_start.min_max(start);
                    let (end_low, end_high) = old_end.min_max(end);

                    if start_low != start_high {
                        if let Some(mut i) = rects.iter().position(|(_, loc)| *loc == start_low) {
                            let mut rect = rects[i].0;
                            while rects[i].1 < start_high {
                                let next_rect = rects[i+1].0;
                                if rect.max.y.min(next_rect.max.y) - rect.min.y.max(next_rect.min.y) >
                                   rect.height().min(next_rect.height()) as i32 / 2 {
                                    if rects[i+1].1 == start_high {
                                        if rect.min.x < next_rect.min.x {
                                            rect.max.x = next_rect.min.x;
                                        } else {
                                            rect.min.x = next_rect.max.x;
                                        }
                                        rect.min.y = rect.min.y.min(next_rect.min.y);
                                        rect.max.y = rect.max.y.max(next_rect.max.y);
                                    } else {
                                        rect.absorb(&next_rect);
                                    }
                                } else {
                                    rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                                    rect = next_rect;
                                }
                                i += 1;
                            }
                            rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                        }
                    }

                    if end_low != end_high {
                        if let Some(mut i) = rects.iter().rposition(|(_, loc)| *loc == end_high) {
                            let mut rect = rects[i].0;
                            while rects[i].1 > end_low {
                                let prev_rect = rects[i-1].0;
                                if rect.max.y.min(prev_rect.max.y) - rect.min.y.max(prev_rect.min.y) >
                                   rect.height().min(prev_rect.height()) as i32 / 2 {
                                    if rects[i-1].1 == end_low {
                                        if rect.min.x > prev_rect.min.x {
                                            rect.min.x = prev_rect.max.x;
                                        } else {
                                            rect.max.x = prev_rect.min.x;
                                        }
                                        rect.min.y = rect.min.y.min(prev_rect.min.y);
                                        rect.max.y = rect.max.y.max(prev_rect.max.y);
                                    } else {
                                        rect.absorb(&prev_rect);
                                    }
                                } else {
                                    rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                                    rect = prev_rect;
                                }
                                i -= 1;
                            }
                            rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                        }
                    }

                    selection.start = start;
                    selection.end = end;
                }
                true
            },
            Event::Device(DeviceEvent::Finger { status: FingerStatus::Up, position, id, .. }) if self.state == State::Selection(id) => {
                self.state = State::Idle;
                let radius = scale_by_dpi(24.0, CURRENT_DEVICE.dpi) as i32;
                self.toggle_selection_menu(Rectangle::from_disk(position, radius), Some(true), rq, context);
                true
            },
            Event::Gesture(GestureEvent::Tap(center)) if self.state == State::AdjustSelection && self.rect.includes(center) => {
                let mut found = None;
                let mut dmin = u32::MAX;
                let dmax = (scale_by_dpi(RECT_DIST_JITTER, CURRENT_DEVICE.dpi) as i32).pow(2) as u32;
                let mut rects = Vec::new();

                for chunk in &self.chunks {
                    for word in &self.text[&chunk.location] {
                        let rect = (word.rect * chunk.scale).to_rect() - chunk.frame.min + chunk.position;
                        rects.push((rect, word.location));
                        let d = center.rdist2(&rect);
                        if d < dmax && d < dmin {
                            dmin = d;
                            found = Some((word.clone(), rects.len() - 1));
                        }
                    }
                }

                let selection = self.selection.as_mut().unwrap();

                if let Some((word, index)) = found {
                    let old_start = selection.start;
                    let old_end = selection.end;

                    let (start, end) = if word.location <= old_start {
                        (word.location, old_end)
                    } else if word.location >= old_end {
                        (old_start, word.location)
                    } else {
                        let (start_index, end_index) = (rects.iter().position(|(_, loc)| *loc == old_start),
                                                        rects.iter().position(|(_, loc)| *loc == old_end));
                        match (start_index, end_index) {
                            (Some(s), Some(e)) => {
                                if index - s > e - index {
                                    (old_start, word.location)
                                } else {
                                    (word.location, old_end)
                                }
                            },
                            (Some(..), None) => (word.location, old_end),
                            (None, Some(..)) => (old_start, word.location),
                            (None, None) => (old_start, old_end)
                        }
                    };

                    if start == old_start && end == old_end {
                        return true;
                    }

                    let (start_low, start_high) = old_start.min_max(start);
                    let (end_low, end_high) = old_end.min_max(end);

                    if start_low != start_high {
                        if let Some(mut i) = rects.iter().position(|(_, loc)| *loc == start_low) {
                            let mut rect = rects[i].0;
                            while i < rects.len() - 1 && rects[i].1 < start_high {
                                let next_rect = rects[i+1].0;
                                if rect.min.y < next_rect.max.y && next_rect.min.y < rect.max.y {
                                    if rects[i+1].1 == start_high {
                                        if rect.min.x < next_rect.min.x {
                                            rect.max.x = next_rect.min.x;
                                        } else {
                                            rect.min.x = next_rect.max.x;
                                        }
                                        rect.min.y = rect.min.y.min(next_rect.min.y);
                                        rect.max.y = rect.max.y.max(next_rect.max.y);
                                    } else {
                                        rect.absorb(&next_rect);
                                    }
                                } else {
                                    rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                                    rect = next_rect;
                                }
                                i += 1;
                            }
                            rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                        }
                    }

                    if end_low != end_high {
                        if let Some(mut i) = rects.iter().rposition(|(_, loc)| *loc == end_high) {
                            let mut rect = rects[i].0;
                            while i > 0 && rects[i].1 > end_low {
                                let prev_rect = rects[i-1].0;
                                if rect.min.y < prev_rect.max.y && prev_rect.min.y < rect.max.y {
                                    if rects[i-1].1 == end_low {
                                        if rect.min.x > prev_rect.min.x {
                                            rect.min.x = prev_rect.max.x;
                                        } else {
                                            rect.max.x = prev_rect.min.x;
                                        }
                                        rect.min.y = rect.min.y.min(prev_rect.min.y);
                                        rect.max.y = rect.max.y.max(prev_rect.max.y);
                                    } else {
                                        rect.absorb(&prev_rect);
                                    }
                                } else {
                                    rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                                    rect = prev_rect;
                                }
                                i -= 1;
                            }
                            rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                        }
                    }

                    selection.start = start;
                    selection.end = end;
                }
                true
            },
            Event::Gesture(GestureEvent::Tap(center)) if self.rect.includes(center) => {
                if self.focus.is_some() {
                    return true;
                }

                let mut nearest_link = None;
                let mut dmin = u32::MAX;
                let dmax = (scale_by_dpi(RECT_DIST_JITTER, CURRENT_DEVICE.dpi) as i32).pow(2) as u32;

                for chunk in &self.chunks {
                    let (links, _) = self.doc.lock().ok()
                                         .and_then(|mut doc| doc.links(Location::Exact(chunk.location)))
                                         .unwrap_or((Vec::new(), 0));
                    for link in links {
                        let rect = (link.rect * chunk.scale).to_rect() - chunk.frame.min + chunk.position;
                        let d = center.rdist2(&rect);
                        if d < dmax && d < dmin {
                            dmin = d;
                            nearest_link = Some(link.clone());
                        }
                    }
                }

                if let Some(link) = nearest_link.take() {
                    let pdf_page = Regex::new(r"^#page=(\d+).*$").unwrap();
                    let djvu_page = Regex::new(r"^#([+-])?(\d+)$").unwrap();
                    let toc_page = Regex::new(r"^@(.+)$").unwrap();
                    if let Some(caps) = toc_page.captures(&link.text) {
                        let loc_opt = if caps[1].chars().all(|c| c.is_digit(10)) {
                            caps[1].parse::<usize>()
                                   .map(Location::Exact)
                                   .ok()
                        } else {
                            Some(Location::Uri(caps[1].to_string()))
                        };
                        if let Some(location) = loc_opt {
                            self.quit(context);
                            hub.send(Event::Back).ok();
                            hub.send(Event::GoToLocation(location)).ok();
                        }
                    } else if let Some(caps) = pdf_page.captures(&link.text) {
                        if let Ok(index) = caps[1].parse::<usize>() {
                            self.go_to_page(index.saturating_sub(1), true, hub, rq, context);
                        }
                    } else if let Some(caps) = djvu_page.captures(&link.text) {
                        if let Ok(mut index) = caps[2].parse::<usize>() {
                            let prefix = caps.get(1).map(|m| m.as_str());
                            match prefix {
                                Some("-") => index = self.current_page.saturating_sub(index),
                                Some("+") => index += self.current_page,
                                _ => index = index.saturating_sub(1),
                            }
                            self.go_to_page(index, true, hub, rq, context);
                        }
                    } else {
                        let mut doc = self.doc.lock().unwrap();
                        let loc = Location::LocalUri(self.current_page, link.text.clone());
                        if let Some(location) = doc.resolve_location(loc) {
                            hub.send(Event::GoTo(location)).ok();
                        } else {
                            if link.text.starts_with("https:") || link.text.starts_with("http:") {
                                if let Some(path) = context.settings.external_urls_queue.as_ref() {
                                    if let Ok(mut file) = OpenOptions::new().create(true)
                                                                            .append(true)
                                                                            .open(path) {
                                        if let Err(e) = writeln!(file, "{}", link.text) {
                                            eprintln!("Couldn't write to {}: {:#}.", path.display(), e);
                                        } else {
                                            let message = format!("Queued {}.", link.text);
                                            let notif = Notification::new(message, hub, rq, context);
                                            self.children.push(Box::new(notif) as Box<dyn View>);
                                        }
                                    }
                                }
                            } else {
                                eprintln!("Can't resolve URI: {}.", link.text);
                            }
                        }
                    }
                    return true;
                }

                if let ZoomMode::Custom(_) = self.view_port.zoom_mode {
                    let dx = self.rect.width() as i32 - 2 * self.view_port.margin_width;
                    let dy = self.rect.height() as i32 - 2 * self.view_port.margin_width;
                    match Region::from_point(center, self.rect,
                                             context.settings.reader.strip_width,
                                             context.settings.reader.corner_width) {
                        Region::Corner(diag_dir) => {
                            match diag_dir {
                                DiagDir::NorthEast => self.directional_scroll(pt!(dx, -dy), hub, rq, context),
                                DiagDir::SouthEast => self.directional_scroll(pt!(dx, dy), hub, rq, context),
                                DiagDir::SouthWest => self.directional_scroll(pt!(-dx, dy), hub, rq, context),
                                DiagDir::NorthWest => self.directional_scroll(pt!(-dx, -dy), hub, rq, context),
                            }
                        },
                        Region::Strip(dir) => {
                            match dir {
                                Dir::North => self.directional_scroll(pt!(0, -dy), hub, rq, context),
                                Dir::East => self.directional_scroll(pt!(dx, 0), hub, rq, context),
                                Dir::South => self.directional_scroll(pt!(0, dy), hub, rq, context),
                                Dir::West => self.directional_scroll(pt!(-dx, 0), hub, rq, context),
                            }
                        },
                        Region::Center => self.toggle_bars(None, hub, rq, context),
                    }

                    return true;
                }

                match Region::from_point(center, self.rect,
                                         context.settings.reader.strip_width,
                                         context.settings.reader.corner_width) {
                    Region::Corner(diag_dir) => {
                        match diag_dir {
                            DiagDir::NorthWest => self.go_to_last_page(hub, rq, context),
                            DiagDir::NorthEast =>
                                if self.search.is_some() {
                                    self.stop_search(rq);
                                    self.update(Some(UpdateMode::Partial), hub, rq, context);
                                } else if self.ephemeral {
                                    self.quit(context);
                                    hub.send(Event::Back).ok();
                                } else {
                                    self.toggle_bookmark(rq);
                                },
                            DiagDir::SouthEast =>
                                if self.search.is_none() {
                                    match context.settings.reader.south_east_corner {
                                        SouthEastCornerAction::GoToPage => {
                                            hub.send(Event::Toggle(ViewId::GoToPage)).ok();
                                        },
                                        SouthEastCornerAction::NextPage => {
                                            self.go_to_neighbor(CycleDir::Next, hub, rq, context);
                                        },
                                    }
                                } else {
                                    self.go_to_neighbor(CycleDir::Next, hub, rq, context);
                                },
                            DiagDir::SouthWest =>
                                if self.search.is_none() {
                                    if self.ephemeral {
                                        self.quit(context);
                                        hub.send(Event::Back).ok();
                                    } else {
                                        hub.send(Event::Show(ViewId::TableOfContents)).ok();
                                    }
                                } else {
                                    self.go_to_neighbor(CycleDir::Previous, hub, rq, context);
                                },
                        }
                    },
                    Region::Strip(dir) => {
                        match dir {
                            Dir::West => {
                                if self.search.is_none() {
                                    match context.settings.reader.west_strip {
                                        WestStripAction::PreviousPage => {
                                            self.go_to_neighbor(CycleDir::Previous, hub, rq, context);
                                        }
                                        WestStripAction::NextPage => {
                                            self.go_to_neighbor(CycleDir::Next, hub, rq, context);
                                        }
                                        WestStripAction::None => (),
                                    }
                                } else {
                                    self.go_to_results_neighbor(CycleDir::Previous, hub, rq, context);
                                }
                            },
                            Dir::East => {
                                if self.search.is_none() {
                                    match context.settings.reader.east_strip {
                                        EastStripAction::PreviousPage => {
                                            self.go_to_neighbor(CycleDir::Previous, hub, rq, context);
                                        }
                                        EastStripAction::NextPage => {
                                            self.go_to_neighbor(CycleDir::Next, hub, rq, context);
                                        }
                                        EastStripAction::None => (),
                                    }
                                } else {
                                    self.go_to_results_neighbor(CycleDir::Next, hub, rq, context);
                                }
                            },
                            Dir::South => if self.synthetic
                                             && center.y > self.rect.max.y
                                                           - scale_by_dpi(70.0, CURRENT_DEVICE.dpi) as i32 {
                                self.progress_bar.enabled = !self.progress_bar.enabled;
                                let mut margin_width = context.settings.reader.margin_width;
                                if let Some(ref mut r) = self.info.reader {
                                    r.show_progress_bar = Some(self.progress_bar.enabled);
                                    if let Some(mw) = r.margin_width {
                                        margin_width = mw;
                                    }
                                }
                                self.set_margin_width(margin_width, true, hub, rq, context);
                            } else {
                                match context.settings.reader.south_strip {
                                    SouthStripAction::ToggleBars => {
                                        self.toggle_bars(None, hub, rq, context);
                                    },
                                    SouthStripAction::NextPage => {
                                        self.go_to_neighbor(CycleDir::Next, hub, rq, context);
                                    }
                                }
                            },
                            Dir::North => if let Some(_) = locate::<TopBar>(self) {
                                self.toggle_bars(None, hub, rq, context);
                            } else {
                                self.toggle_title_menu(rect![center, center], Some(true), rq, context);
                            }
                        }
                    },
                    Region::Center => self.toggle_bars(None, hub, rq, context),
                }

                true
            },
            Event::Gesture(GestureEvent::HoldFingerShort(center, id)) if self.rect.includes(center) => {
                if self.focus.is_some() {
                    return true;
                }

                let mut found = None;
                let mut dmin = u32::MAX;
                let dmax = (scale_by_dpi(RECT_DIST_JITTER, CURRENT_DEVICE.dpi) as i32).pow(2) as u32;

                if let Some(rect) = self.selection_rect() {
                    let d = center.rdist2(&rect);
                    if d < dmax {
                        self.state = State::Idle;
                        let radius = scale_by_dpi(24.0, CURRENT_DEVICE.dpi) as i32;
                        self.toggle_selection_menu(Rectangle::from_disk(center, radius), Some(true), rq, context);
                    }
                    return true;
                }

                for chunk in &self.chunks {
                    for word in &self.text[&chunk.location] {
                        let rect = (word.rect * chunk.scale).to_rect() - chunk.frame.min + chunk.position;
                        let d = center.rdist2(&rect);
                        if d < dmax && d < dmin {
                            dmin = d;
                            found = Some((word.clone(), rect));
                        }
                    }
                }

                if let Some((nearest_word, rect)) = found {
                    let anchor = nearest_word.location;
                    if let Some(annot) = self.annotations.values().flatten()
                                             .find(|annot| anchor >= annot.selection[0] && anchor <= annot.selection[1]).cloned() {
                        let radius = scale_by_dpi(24.0, CURRENT_DEVICE.dpi) as i32;
                        self.toggle_annotation_menu(&annot, Rectangle::from_disk(center, radius), Some(true), rq, context);
                    } else {
                        self.selection = Some(Selection {
                            start: anchor,
                            end: anchor,
                            anchor,
                        });
                        self.state = State::Selection(id);
                        rq.add(RenderData::new(self.id, rect, UpdateMode::Fast));
                    }
                }

                true
            },
            Event::Gesture(GestureEvent::HoldFingerLong(center, _)) if self.rect.includes(center) => {
                if let Some(text) = self.selected_text() {
                    let query = trim_non_alphanumeric(&text);
                    let language = self.info.language.clone();
                    hub.send(Event::Select(EntryId::Launch(AppCmd::Dictionary { query, language }))).ok();
                }
                self.selection = None;
                self.state = State::Idle;
                true
            },
            Event::Update(mode) => {
                self.update(Some(mode), hub, rq, context);
                true
            },
            Event::LoadPixmap(location) => {
                self.load_pixmap(location);
                true
            },
            Event::Submit(ViewId::GoToPageInput, ref text) => {
                let re = Regex::new(r#"^([-+'])?(.+)$"#).unwrap();
                if let Some(caps) = re.captures(text) {
                    let prefix = caps.get(1).map(|m| m.as_str());
                    if prefix == Some("'") {
                        if let Some(location) = self.find_page_by_name(&caps[2]) {
                            self.go_to_page(location, true, hub, rq, context);
                        }
                    } else {
                        if text == "_" {
                            let location = (context.rng.next_u64() % self.pages_count as u64) as usize;
                            self.go_to_page(location, true, hub, rq, context);
                        } else if text == "(" {
                            self.go_to_page(0, true, hub, rq, context);
                        } else if text == ")" {
                            self.go_to_page(self.pages_count.saturating_sub(1), true, hub, rq, context);
                        } else if let Some(percent) = text.strip_suffix('%') {
                            if let Ok(number) = percent.parse::<f64>() {
                                let location = (number.max(0.0).min(100.0) / 100.0 * self.pages_count as f64).round() as usize;
                                self.go_to_page(location, true, hub, rq, context);
                            }
                        } else if let Ok(number) = caps[2].parse::<f64>() {
                            let location = {
                                let bpp = if self.synthetic { BYTES_PER_PAGE } else { 1.0 };
                                let mut index = (number * bpp).max(0.0).round() as usize;
                                match prefix {
                                    Some("-") => index = self.current_page.saturating_sub(index),
                                    Some("+") => index += self.current_page,
                                    _ => index = index.saturating_sub(1/(bpp as usize)),
                                }
                                index
                            };
                            self.go_to_page(location, true, hub, rq, context);
                        }
                    }
                }
                true
            },
            Event::Submit(ViewId::GoToResultsPageInput, ref text) => {
                if let Ok(index) = text.parse::<usize>() {
                    self.go_to_results_page(index.saturating_sub(1), hub, rq, context);
                }
                true
            },
            Event::Submit(ViewId::NamePageInput, ref text) => {
                if !text.is_empty() {
                    if let Some(ref mut r) = self.info.reader {
                        r.page_names.insert(self.current_page, text.to_string());
                    }
                }
                self.toggle_keyboard(false, None, hub, rq, context);
                true
            },
            Event::Submit(ViewId::EditNoteInput, ref note) => {
                let selection = self.selection.take().map(|sel| [sel.start, sel.end]);

                if let Some(sel) = selection {
                    let text = self.text_excerpt(sel).unwrap();
                    if let Some(r) = self.info.reader.as_mut() {
                        r.annotations.push(Annotation {
                            selection: sel,
                            note: note.to_string(),
                            text,
                            modified: Local::now().naive_local(),
                        });
                    }
                    if let Some(rect) = self.text_rect(sel) {
                        rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                    }
                } else {
                    if let Some(sel) = self.target_annotation.take() {
                        if let Some(annot) = self.find_annotation_mut(sel) {
                            annot.note = note.to_string();
                            annot.modified = Local::now().naive_local();
                        }
                        if let Some(rect) = self.text_rect(sel) {
                            rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                        }
                    }
                }

                self.update_annotations();
                self.toggle_keyboard(false, None, hub, rq, context);
                true
            },
            Event::Submit(ViewId::ReaderSearchInput, ref text) => {
                match make_query(text) {
                    Some(query) => {
                        self.search(text, query, hub, rq);
                        self.toggle_keyboard(false, None, hub, rq, context);
                        self.toggle_results_bar(true, rq, context);
                    },
                    None => {
                        let notif = Notification::new("Invalid search query.".to_string(),
                                                      hub, rq, context);
                        self.children.push(Box::new(notif) as Box<dyn View>);
                    },
                }
                true
            },
            Event::Page(dir) => {
                self.go_to_neighbor(dir, hub, rq, context);
                true
            },
            Event::GoTo(location) | Event::Select(EntryId::GoTo(location)) => {
                self.go_to_page(location, true, hub, rq, context);
                true
            },
            Event::GoToLocation(ref location) => {
                let offset_opt = {
                    let mut doc = self.doc.lock().unwrap();
                    doc.resolve_location(location.clone())
                };
                if let Some(offset) = offset_opt {
                    self.go_to_page(offset, true, hub, rq, context);
                }
                true
            },
            Event::Chapter(dir) => {
                self.go_to_chapter(dir, hub, rq, context);
                true
            },
            Event::ResultsPage(dir) => {
                self.go_to_results_neighbor(dir, hub, rq, context);
                true
            },
            Event::CropMargins(ref margin) => {
                let current_page = self.current_page;
                self.crop_margins(current_page, margin.as_ref(), hub, rq, context);
                true
            },
            Event::Toggle(ViewId::TopBottomBars) => {
                self.toggle_bars(None, hub, rq, context);
                true
            },
            Event::Toggle(ViewId::GoToPage) => {
                self.toggle_go_to_page(None, ViewId::GoToPage, hub, rq, context);
                true
            },
            Event::Toggle(ViewId::GoToResultsPage) => {
                self.toggle_go_to_page(None, ViewId::GoToResultsPage, hub, rq, context);
                true
            },
            Event::Slider(SliderId::FontSize, font_size, FingerStatus::Up) => {
                self.set_font_size(font_size, true, hub, rq, context);
                true
            },
            Event::Slider(SliderId::ContrastExponent, exponent, FingerStatus::Up) => {
                self.set_contrast_exponent(exponent, hub, rq, context);
                true
            },
            Event::Slider(SliderId::ContrastGray, gray, FingerStatus::Up) => {
                self.set_contrast_gray(gray, hub, rq, context);
                true
            },
            Event::Slider(SliderId::Scrubber, _page, FingerStatus::Down) => {
                self.remove_tool_bar(rq);
                true
            },
            Event::Slider(SliderId::Scrubber, page, FingerStatus::Up) => {
                let loc = if self.synthetic {
                    (page * BYTES_PER_PAGE as f32) as usize
                } else {
                    (page as usize).saturating_sub(1)
                }.min(self.pages_count.saturating_sub(1));
                self.go_to_page(loc, true, hub, rq, context);
                true
            },
            Event::Slider(SliderId::Scrubber, page, FingerStatus::Motion) => {
                self.update_scrubber(page, rq);
                true
            },
            Event::ToggleNear(ViewId::TitleMenu, rect) => {
                self.toggle_title_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::MainMenu, rect) => {
                toggle_main_menu(self, rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::BatteryMenu, rect) => {
                toggle_battery_menu(self, rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::ClockMenu, rect) => {
                toggle_clock_menu(self, rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::MarginCropperMenu, rect) => {
                self.toggle_margin_cropper_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::SearchMenu, rect) => {
                self.toggle_search_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::FontFamilyMenu, rect) => {
                self.toggle_font_family_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::FontSizeMenu, rect) => {
                self.toggle_font_size_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::TextAlignMenu, rect) => {
                self.toggle_text_align_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::MarginWidthMenu, rect) => {
                self.toggle_margin_width_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::LineHeightMenu, rect) => {
                self.toggle_line_height_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::ContrastExponentMenu, rect) => {
                self.toggle_contrast_exponent_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::ContrastGrayMenu, rect) => {
                self.toggle_contrast_gray_menu(rect, None, rq, context);
                true
            },
            Event::SetDefault(ref prop) => {
                self.set_default(prop, hub, context);
                true
            },
            Event::Select(EntryId::ResetToDefaults) => {
                self.reset_to_defaults(hub, rq, context);
                true
            },
            Event::ToggleNear(ViewId::ThemeMenu, rect) => {
                self.toggle_theme_menu(rect, None, rq, context);
                true
            },
            Event::Show(ViewId::ThemeDialog) | Event::Select(EntryId::SaveTheme) => {
                self.toggle_theme_dialog(true, None, hub, rq, context);
                true
            },
            Event::Close(ViewId::ThemeDialog) => {
                self.toggle_theme_dialog(false, None, hub, rq, context);
                true
            },
            Event::SaveTheme => {
                self.stash_theme(context);
                self.toggle_theme_dialog(false, None, hub, rq, context);
                self.toggle_name_theme(true, hub, rq, context);
                true
            },
            Event::OverwriteTheme(idx) => {
                let name = context.settings.themes.get(idx).unwrap().name.clone();
                self.stash_theme(context);
                self.toggle_theme_dialog(false, None, hub, rq, context);
                self.save_theme(&name, hub, context);
                true
            }
            Event::Select(EntryId::RenameTheme(idx)) => {
                self.toggle_bars(Some(false), hub, rq, context);
                self.theme = Some(ThemeStash::Existing(idx));
                self.toggle_name_theme(true, hub, rq, context);
                true
            }
            Event::Select(EntryId::DeleteTheme(idx)) => {
                self.toggle_bars(Some(false), hub, rq, context);
                if let Some(ref theme) = context.settings.themes.get(idx) {
                    hub.send(Event::Notify(format!("Deleted theme {}", theme.name))).ok();
                    context.settings.themes.remove(idx);
                }
                true
            }
            Event::Select(EntryId::OverwriteTheme(idx)) => {
                self.toggle_theme_dialog(true, Some(idx), hub, rq, context);
                true
            },
            Event::Submit(ViewId::NameThemeInput, ref text) => {
                let text = text.trim();
                if !text.is_empty() {
                    match self.theme {
                        Some(ThemeStash::New(_)) => self.save_theme(text, hub, context),
                        Some(ThemeStash::Existing(idx)) => if idx < context.settings.themes.len() {
                            context.settings.themes[idx].name = text.to_string();
                            hub.send(Event::Notify(format!("Theme renamed to {}", text))).ok();
                        },
                        _ => (),
                    }
                }
                self.toggle_name_theme(false, hub, rq, context);
                true
            },
            Event::ToggleNear(ViewId::PageMenu, rect) => {
                self.toggle_page_menu(rect, None, rq, context);
                true
            },
            Event::Close(ViewId::MainMenu) => {
                toggle_main_menu(self, Rectangle::default(), Some(false), rq, context);
                true
            },
            Event::Close(ViewId::SearchBar) => {
                self.stop_search(rq);
                if self.search.is_none() {
                    self.toggle_results_bar(false, rq, context);
                    self.toggle_search_bar(false, hub, rq, context);
                }
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                true
            },
            Event::Close(ViewId::GoToPage) => {
                self.toggle_go_to_page(Some(false), ViewId::GoToPage, hub, rq, context);
                true
            },
            Event::Close(ViewId::GoToResultsPage) => {
                self.toggle_go_to_page(Some(false), ViewId::GoToResultsPage, hub, rq, context);
                true
            },
            Event::Close(ViewId::SelectionMenu) => {
                if self.state == State::Idle && self.target_annotation.is_none() {
                    if let Some(rect) = self.selection_rect() {
                        self.selection = None;
                        rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                    }
                }
                false
            },
            Event::Close(ViewId::EditNote) => {
                self.toggle_edit_note(None, Some(false), hub, rq, context);
                if let Some(rect) = self.selection_rect() {
                    self.selection = None;
                    rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                }
                self.target_annotation = None;
                false
            },
            Event::Close(ViewId::NamePage) => {
                self.toggle_keyboard(false, None, hub, rq, context);
                false
            },
            Event::Show(ViewId::TableOfContents) => {
                {
                    self.toggle_bars(Some(false), hub, rq, context);
                }
                let mut doc = self.doc.lock().unwrap();
                if let Some(toc) = self.toc()
                                       .or_else(|| doc.toc())
                                       .filter(|toc| !toc.is_empty()) {
                    let chap = doc.chapter(self.current_page, &toc)
                                  .map(|(c, _, _)| c);
                    let chap_index = chap.map_or(usize::MAX, |chap| chap.index);
                    let html = toc_as_html(&toc, chap_index);
                    let link_uri = chap.and_then(|chap| {
                        match chap.location {
                            Location::Uri(ref uri) => Some(format!("@{}", uri)),
                            Location::Exact(offset) => Some(format!("@{}", offset)),
                            _ => None,
                        }
                    });
                    hub.send(Event::OpenHtml(html, link_uri)).ok();
                }
                true
            },
            Event::Select(EntryId::Annotations) => {
                self.toggle_bars(Some(false), hub, rq, context);
                let mut starts = self.annotations.values().flatten()
                                     .map(|annot| annot.selection[0]).collect::<Vec<TextLocation>>();
                starts.sort();
                let active_range = starts.first().cloned().zip(starts.last().cloned());
                if let Some(mut annotations) = self.info.reader.as_ref().map(|r| &r.annotations).cloned() {
                    annotations.sort_by(|a, b| a.selection[0].cmp(&b.selection[0]));
                    let html = annotations_as_html(&annotations, active_range);
                    let link_uri = annotations.iter()
                                              .filter(|annot| annot.selection[0].location() <= self.current_page)
                                              .max_by_key(|annot| annot.selection[0])
                                              .map(|annot| format!("@{}", annot.selection[0].location()));
                    hub.send(Event::OpenHtml(html, link_uri)).ok();
                }
                true
            },
            Event::Select(EntryId::Bookmarks) => {
                self.toggle_bars(Some(false), hub, rq, context);
                if let Some(bookmarks) = self.info.reader.as_ref().map(|r| &r.bookmarks) {
                    let html = bookmarks_as_html(bookmarks, self.current_page, self.synthetic);
                    let link_uri = bookmarks.range(..= self.current_page).next_back()
                                            .map(|index| format!("@{}", index));
                    hub.send(Event::OpenHtml(html, link_uri)).ok();
                }
                true
            },
            Event::Show(ViewId::SearchBar) => {
                self.toggle_search_bar(true, hub, rq, context);
                true
            },
            Event::Show(ViewId::MarginCropper) => {
                self.toggle_margin_cropper(true, hub, rq, context);
                true
            },
            Event::Close(ViewId::MarginCropper) => {
                self.toggle_margin_cropper(false, hub, rq, context);
                true
            },
            Event::SearchResult(location, ref rects) => {
                if let Some(ref mut s) = self.search {
                    let pages_count = s.highlights.len();
                    s.highlights.entry(location).or_insert_with(Vec::new).push(rects.clone());
                    s.results_count += 1;
                    let results_count = s.results_count;
                    if results_count > 1 && location <= self.current_page && s.highlights.len() > pages_count {
                        s.current_page += 1;
                    }

                    self.update_results_bar(rq);

                    if results_count == 1 {
                        self.toggle_results_bar(false, rq, context);
                        self.toggle_search_bar(false, hub, rq, context);
                        self.go_to_page(location, true, hub, rq, context);
                    } else if location == self.current_page {
                        self.update(Some(UpdateMode::Partial), hub, rq, context);
                    }
                }
                true
            },
            Event::EndOfSearch => {
                if self.search.is_none() {
                    return true;
                }
                let (results_count, pages_count) =
                     self.search.as_ref().map(|s| (s.results_count, s.highlights.len())).unwrap();
                if results_count == 0 {
                    self.toggle_search_bar(true, hub, rq, context);
                    hub.send(Event::Focus(Some(ViewId::ReaderSearchInput))).ok();
                }
                let mut msg = if results_count > 0 {
                    results_count.to_string()
                } else {
                    "No".to_string()
                } + " search result" + if results_count != 1 {"s"} else {""};
                if pages_count > 0 {
                    msg += &format!(" in {} page{}", pages_count, if pages_count > 1 {"s"} else {""});
                }
                let notif = Notification::new(msg, hub, rq, context);
                self.children.push(Box::new(notif) as Box<dyn View>);
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                true
            },
            Event::Select(EntryId::AnnotateSelection) => {
                self.toggle_edit_note(None, Some(true), hub, rq, context);
                true
            },
            Event::Select(EntryId::HighlightSelection) => {
                if let Some(sel) = self.selection.take() {
                    let text = self.text_excerpt([sel.start, sel.end]).unwrap();
                    if let Some(r) = self.info.reader.as_mut() {
                        r.annotations.push(Annotation {
                            selection: [sel.start, sel.end],
                            note: String::new(),
                            text,
                            modified: Local::now().naive_local(),
                        });
                    }
                    if let Some(rect) = self.text_rect([sel.start, sel.end]) {
                        rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                    }
                    self.update_annotations();
                }

                true
            },
            Event::Select(EntryId::DefineSelection) => {
                if let Some(text) = self.selected_text() {
                    let query = trim_non_alphanumeric(&first_n_words(&text, 5));
                    let language = self.info.language.clone();
                    hub.send(Event::Select(EntryId::Launch(AppCmd::Dictionary { query, language }))).ok();
                }
                self.selection = None;
                true
            },
            Event::Select(EntryId::TranslateSelection) => {
                if let Some(text) = self.selected_text() {
                    let query = text.trim().to_string();
                    let source = "auto".to_string();
                    let target = context.settings.languages[0].clone();
                    hub.send(Event::Select(EntryId::Launch(AppCmd::Translate { query, source, target }))).ok();
                }
                self.selection = None;
                true
            },
            Event::Select(EntryId::WikiSelection) => {
                if let Some(text) = self.selected_text() {
                    let query = trim_non_alphanumeric(&first_n_words(&text, 8));
                    hub.send(Event::Select(EntryId::Launch(AppCmd::Wiki { query }))).ok();
                }
                self.selection = None;
                true
            },
            Event::Select(EntryId::SetCssTweak(index)) => {
                self.apply_css_tweak(index, hub, rq, context);
                self.selection = None;
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                true
            },
            Event::Select(EntryId::SetCssTweakEx(ref selector, index)) => {
                self.apply_css_tweak_aux(selector, index, hub, context);
                self.selection = None;
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                true
            },
            Event::Select(EntryId::ShowCssTweaks) => {
                self.toggle_bars(Some(false), hub, rq, context);
                if let Some(html) = self.css_tweaks_as_html(context) {
                    hub.send(Event::OpenHtml(html, None)).ok();
                }
                self.selection = None;
                true
            },
            Event::Select(EntryId::UndoLastCssTweak) => {
                self.undo_last_tweak(hub, context);
                self.selection = None;
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                true
            },
            Event::Select(EntryId::UndoAllCssTweaks) => {
                {
                    let mut doc = self.doc.lock().unwrap();
                    doc.set_extra_css("");
                }
                if let Some(ref mut r) = self.info.reader {
                    r.extra_css = None;
                }
                hub.send(Event::Notify("All tweaks removed".to_string())).ok();
                self.selection = None;
                self.cache.clear();
                self.text.clear();
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                true
            },
            Event::Select(EntryId::SearchForSelection) => {
                if let Some(text) = self.selected_text() {
                    let text = &trim_non_alphanumeric(&first_n_words(&text, 5));
                    match make_query(text) {
                        Some(query) => {
                            self.search(text, query, hub, rq);
                        },
                        None => {
                            let notif = Notification::new("Invalid search query.".to_string(),
                                                          hub, rq, context);
                            self.children.push(Box::new(notif) as Box<dyn View>);
                        },
                    }
                }
                if let Some(rect) = self.selection_rect() {
                    rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                }
                self.selection = None;
                true
            },
            Event::Select(EntryId::GoToSelectedPageName) => {
                if let Some(loc) = self.selected_text().and_then(|text| {
                    let end = text.find(|c: char| !c.is_ascii_digit() &&
                                                  Digit::from_char(c).is_err() &&
                                                  !c.is_ascii_uppercase())
                                  .unwrap_or_else(|| text.len());
                    self.find_page_by_name(&text[..end])
                }) {
                    self.go_to_page(loc, true, hub, rq, context);
                }
                if let Some(rect) = self.selection_rect() {
                    rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                }
                self.selection = None;
                true
            },
            Event::Select(EntryId::AdjustSelection) => {
                self.state = State::AdjustSelection;
                true
            },
            Event::Select(EntryId::EditAnnotationNote(sel)) => {
                let text = self.find_annotation_ref(sel).map(|annot| annot.note.clone());
                self.toggle_edit_note(text, Some(true), hub, rq, context);
                self.target_annotation = Some(sel);
                true
            },
            Event::Select(EntryId::RemoveAnnotationNote(sel)) => {
                if let Some(annot) = self.find_annotation_mut(sel) {
                    annot.note.clear();
                    annot.modified = Local::now().naive_local();
                    self.update_annotations();
                }
                if let Some(rect) = self.text_rect(sel) {
                    rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                }
                true
            },
            Event::Select(EntryId::RemoveAnnotation(sel)) => {
                if let Some(annotations) = self.info.reader.as_mut().map(|r| &mut r.annotations) {
                    annotations.retain(|annot| annot.selection[0] != sel[0] || annot.selection[1] != sel[1]);
                    self.update_annotations();
                }
                if let Some(rect) = self.text_rect(sel) {
                    rq.add(RenderData::new(self.id, rect, UpdateMode::Gui));
                }
                true
            },
            Event::Select(EntryId::SetZoomMode(zoom_mode)) => {
                self.set_zoom_mode(zoom_mode, true, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetScrollMode(scroll_mode)) => {
                self.set_scroll_mode(scroll_mode, hub, rq, context);
                true
            },
            Event::Select(EntryId::Save) => {
                let doc = self.doc.lock().unwrap();
                let (path, library_index) = get_save_path(&self.info.title, &self.info.file.kind, context);
                let msg = match doc.save(&path) {
                    Err(e) => format!("{}", e),
                    Ok(()) => {
                        if let Some(index) = library_index {
                            context.reimport(index);
                        }
                        format!("Saved {}.", path)
                    },
                };
                let notif = Notification::new(msg, hub, rq, context);
                self.children.push(Box::new(notif) as Box<dyn View>);
                true
            },
            Event::Select(EntryId::ApplyCroppings(index, scheme)) => {
                self.info.reader.as_mut().map(|r| {
                    if r.cropping_margins.is_none() {
                        r.cropping_margins = Some(CroppingMargins::Any(Margin::default()));
                    }
                    r.cropping_margins.as_mut().map(|c| c.apply(index, scheme))
                });
                true
            },
            Event::Select(EntryId::RemoveCroppings) => {
                if let Some(r) = self.info.reader.as_mut() {
                    r.cropping_margins = None;
                }
                self.cache.clear();
                self.update(Some(UpdateMode::Partial), hub, rq, context);
                true
            },
            Event::Select(EntryId::SearchDirection(dir)) => {
                self.search_direction = dir;
                true
            },
            Event::Select(EntryId::SetFontFamily(ref font_family)) => {
                self.set_font_family(font_family, true, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetTextAlign(text_align)) => {
                self.set_text_align(text_align, true, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetFontSize(v)) => {
                let font_size = self.info.reader.as_ref()
                                    .and_then(|r| r.font_size)
                                    .unwrap_or(context.settings.reader.font_size);
                let font_size = font_size - 1.0 + v as f32 / 10.0;
                self.set_font_size(font_size, true, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetMarginWidth(width)) => {
                self.set_margin_width(width, true, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetLineHeight(v)) => {
                let lh_gradient = context.settings.reader.line_height_gradient.clamp(MIN_LINE_HEIGHT_GRADIENT, MAX_LINE_HEIGHT_GRADIENT);
                let line_height = 1.0 + v as f32 * lh_gradient;
                self.set_line_height(line_height, true, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetContrastExponent(v)) => {
                let exponent = 1.0 + v as f32 / 2.0;
                self.set_contrast_exponent(exponent, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetContrastGray(v)) => {
                let gray = ((1 << 8) - (1 << (8 - v))) as f32;
                self.set_contrast_gray(gray, hub, rq, context);
                true
            },
            Event::Select(EntryId::SetPageName) => {
                self.toggle_name_page(None, hub, rq, context);
                true
            },
            Event::Select(EntryId::RemovePageName) => {
                if let Some(ref mut r) = self.info.reader {
                    r.page_names.remove(&self.current_page);
                }
                true
            },
            Event::Select(EntryId::ToggleInverted) => {
                self.toggle_inverted(hub, rq, context);
                true
            },
            Event::Select(EntryId::ApplyTheme(idx)) => {
                self.apply_theme(idx, hub, rq, context);
                true
            },
            Event::Reseed => {
                self.reseed(rq, context);
                true
            },
            Event::ToggleFrontlight => {
                if let Some(index) = locate::<TopBar>(self) {
                    self.child_mut(index).downcast_mut::<TopBar>().unwrap()
                        .update_frontlight_icon(rq, context);
                }
                true
            },
            Event::Device(DeviceEvent::Button { code: ButtonCode::Home, status: ButtonStatus::Pressed, .. }) => {
                self.quit(context);
                hub.send(Event::Back).ok();
                true
            },
            Event::Select(EntryId::Quit) |
            Event::Select(EntryId::Reboot) |
            Event::Back |
            Event::Suspend => {
                self.quit(context);
                false
            },
            Event::Focus(v) => {
                if self.focus != v {
                    if let Some(ViewId::ReaderSearchInput) = v {
                        self.toggle_results_bar(false, rq, context);
                        if let Some(ref mut s) = self.search {
                            s.running.store(false, AtomicOrdering::Relaxed);
                        }
                        self.render_results(rq);
                        self.search = None;
                    }
                    self.focus = v;
                    if v.is_some() {
                        self.toggle_keyboard(true, v, hub, rq, context);
                    }
                }
                true
            },
            Event::ClockTick => {
                if self.has_progress_bar() && self.progress_bar.show_clock {
                    *self.dirty_clock.borrow_mut() = false;
                    self.update(Some(UpdateMode::Gui), hub, rq, context);
                }
                true
            },
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, rect: Rectangle, fonts: &mut Fonts) {
        if *self.dirty_clock.borrow() {
            self.update_clock(fb, fonts);
            return;
        }

        fb.draw_rectangle(&rect, WHITE);

        for chunk in &self.chunks {
            let Resource { ref pixmap, scale, .. } = self.cache[&chunk.location];
            let chunk_rect = chunk.frame - chunk.frame.min + chunk.position;

            if let Some(region_rect) = rect.intersection(&chunk_rect) {
                let chunk_frame = region_rect - chunk.position + chunk.frame.min;
                let chunk_position = region_rect.min;
                fb.draw_framed_pixmap_contrast(pixmap, &chunk_frame, chunk_position, self.contrast.exponent, self.contrast.gray);

                if let Some(rects) = self.noninverted_regions.get(&chunk.location) {
                    for r in rects {
                        let rect = (*r * scale).to_rect() - chunk.frame.min + chunk.position;
                        if let Some(ref image_rect) = rect.intersection(&region_rect) {
                            fb.invert_region(image_rect);
                        }
                    }
                }

                if let Some(groups) = self.search.as_ref().and_then(|s| s.highlights.get(&chunk.location)) {
                    for rects in groups {
                        let mut last_rect: Option<Rectangle> = None;
                        for r in rects {
                            let rect = (*r * scale).to_rect() - chunk.frame.min + chunk.position;
                            if let Some(ref search_rect) = rect.intersection(&region_rect) {
                                fb.invert_region(search_rect);
                            }
                            if let Some(last) = last_rect {
                                if rect.max.y.min(last.max.y) - rect.min.y.max(last.min.y) > rect.height().min(last.height()) as i32 / 2 &&
                                   (last.max.x < rect.min.x || rect.max.x < last.min.x) {
                                    let space = if last.max.x < rect.min.x {
                                        rect![last.max.x, (last.min.y + rect.min.y) / 2,
                                              rect.min.x, (last.max.y + rect.max.y) / 2]
                                    } else {
                                        rect![rect.max.x, (last.min.y + rect.min.y) / 2,
                                              last.min.x, (last.max.y + rect.max.y) / 2]
                                    };
                                    if let Some(ref res_rect) = space.intersection(&region_rect) {
                                        fb.invert_region(res_rect);
                                    }
                                }
                            }
                            last_rect = Some(rect);
                        }
                    }
                }

                if let Some(annotations) = self.annotations.get(&chunk.location) {
                    for annot in annotations {
                        let drift = if annot.note.is_empty() { HIGHLIGHT_DRIFT } else { ANNOTATION_DRIFT };
                        let [start, end] = annot.selection;
                        if let Some(text) = self.text.get(&chunk.location) {
                            let mut last_rect: Option<Rectangle> = None;
                            for word in text.iter().filter(|w| w.location >= start && w.location <= end) {
                                let rect = (word.rect * scale).to_rect() - chunk.frame.min + chunk.position;
                                if let Some(ref sel_rect) = rect.intersection(&region_rect) {
                                    fb.shift_region(sel_rect, drift);
                                }
                                if let Some(last) = last_rect {
                                    // Are `rect` and `last` on the same line?
                                    if rect.max.y.min(last.max.y) - rect.min.y.max(last.min.y) > rect.height().min(last.height()) as i32 / 2 &&
                                       (last.max.x < rect.min.x || rect.max.x < last.min.x) {
                                        let space = if last.max.x < rect.min.x {
                                            rect![last.max.x, (last.min.y + rect.min.y) / 2,
                                                  rect.min.x, (last.max.y + rect.max.y) / 2]
                                        } else {
                                            rect![rect.max.x, (last.min.y + rect.min.y) / 2,
                                                  last.min.x, (last.max.y + rect.max.y) / 2]
                                        };
                                        if let Some(ref sel_rect) = space.intersection(&region_rect) {
                                            fb.shift_region(sel_rect, drift);
                                        }
                                    }
                                }
                                last_rect = Some(rect);
                            }
                        }
                    }
                }

                if let Some(sel) = self.selection.as_ref() {
                    if let Some(text) = self.text.get(&chunk.location) {
                        let mut last_rect: Option<Rectangle> = None;
                        for word in text.iter().filter(|w| w.location >= sel.start && w.location <= sel.end) {
                            let rect = (word.rect * scale).to_rect() - chunk.frame.min + chunk.position;
                            if let Some(ref sel_rect) = rect.intersection(&region_rect) {
                                fb.invert_region(sel_rect);
                            }
                            if let Some(last) = last_rect {
                                if rect.max.y.min(last.max.y) - rect.min.y.max(last.min.y) > rect.height().min(last.height()) as i32 / 2 &&
                                   (last.max.x < rect.min.x || rect.max.x < last.min.x) {
                                    let space = if last.max.x < rect.min.x {
                                        rect![last.max.x, (last.min.y + rect.min.y) / 2,
                                              rect.min.x, (last.max.y + rect.max.y) / 2]
                                    } else {
                                        rect![rect.max.x, (last.min.y + rect.min.y) / 2,
                                              last.min.x, (last.max.y + rect.max.y) / 2]
                                    };
                                    if let Some(ref sel_rect) = space.intersection(&region_rect) {
                                        fb.invert_region(sel_rect);
                                    }
                                }
                            }
                            last_rect = Some(rect);
                        }
                    }
                }
            }
        }

        // stop / close button
        if self.ephemeral || self.search.is_some() && locate::<SearchBar>(self).is_none() {
            let dpi = CURRENT_DEVICE.dpi;
            let margin = scale_by_dpi(30.0, dpi) as i32;
            let icon = if let Some(ref s) = self.search {
                if s.running.load(AtomicOrdering::Relaxed) {
                    "stop"
                } else {
                    "close2"
                }
            } else {
                "close2"
            };
            let pixmap = ICONS_PIXMAPS.get(icon).unwrap();
            let pw = pixmap.width as i32;
            let background = rect![pt!(self.rect.max.x - 2 * margin - pw,
                                       self.rect.min.y),
                                   pt!(self.rect.max.x,
                                       self.rect.min.y + 2 * margin + pw)];
            fb.draw_rectangle(&background, WHITE);
            fb.draw_pixmap(pixmap, pt!(self.rect.max.x - margin - pw,
                                       self.rect.min.y + margin));
        } else

        if self.info.reader.as_ref().map_or(false, |r| r.bookmarks.contains(&self.current_page)) {
            let w = self.rect.width() as i32 / 25;
            let a = pt!(self.rect.max.x - w, self.rect.min.y);
            let b = pt!(self.rect.max.x, self.rect.min.y);
            let c = pt!(self.rect.max.x, self.rect.min.y + w);
            fb.draw_triangle(&[a, b, c], GRAY03);
        }

        if self.has_progress_bar() {
            let pb = &self.progress_bar;
            let dpi = CURRENT_DEVICE.dpi;
            let margin = scale_by_dpi(pb.horz_margin as f32, dpi) as i32;
            let y_margin = scale_by_dpi(pb.vert_margin as f32, dpi) as i32;
            let gap = scale_by_dpi(15.0 as f32, dpi) as i32;
            let available_width = self.rect.width() as i32 - 2 * margin;
            let bar_height = scale_by_dpi(pb.height as f32, dpi) as i32;
            let mut bar_width = available_width;
            let mut x = self.rect.min.x as i32 + margin;
            let y = self.rect.max.y as i32 - y_margin;  // bottom of progress bar
            if pb.show_clock {
                let clock_space = self.update_clock(fb, fonts) + gap;
                x += clock_space;
                bar_width -= clock_space;
            }
            let font = font_from_style(fonts, &SMALL_STYLE, dpi);
            let label_width = font.x_heights.0 as i32 * 7;
            bar_width -= label_width + gap;
            let page_size = x + self.current_page as i32 * bar_width / self.pages_count as i32;
            fb.draw_rounded_rectangle_with_border(
                    &rect![pt!(x, y - bar_height), pt!(x + bar_width, y)],
                    &CornerSpec::Uniform(bar_height / 2),
                    &BorderSpec { thickness: 0, color: GRAY10 },
                    &|x, _| if x < page_size { GRAY03 } else { GRAY10 });
            let (_, remain) = self.chapter_info();
            let plan = font.plan(&format!("{:.1} ➤", remain),
                                          Some(label_width + margin), // allow text to exceed margin
                                          None);
            x += bar_width + gap;
            font.render(fb, BLACK, &plan, pt!(x, y));
            *self.dirty_clock.borrow_mut() = false;
        }
    }

    fn render_rect(&self, rect: &Rectangle) -> Rectangle {
        rect.intersection(&self.rect)
            .unwrap_or(self.rect)
    }

    fn resize(&mut self, rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        self.toggle_bars(Some(false), hub, rq, context);

        match self.view_port.zoom_mode {
            ZoomMode::FitToWidth => {
                // Apply the scale change.
                let ratio = (rect.width() as i32 - 2 * self.view_port.margin_width) as f32 /
                            (self.rect.width() as i32 - 2 * self.view_port.margin_width) as f32;
                self.view_port.page_offset.y = (self.view_port.page_offset.y as f32 * ratio) as i32;
            },
            ZoomMode::Custom(_) => {
                // Keep the center still.
                self.view_port.page_offset += pt!(self.rect.width() as i32 - rect.width() as i32,
                                                  self.rect.height() as i32 - rect.height() as i32) / 2;
            },
            _ => (),
        }

        self.rect = rect;

        if self.reflowable {
            let font_size = self.info.reader.as_ref()
                                .and_then(|r| r.font_size)
                                .unwrap_or(context.settings.reader.font_size);
            let mut doc = self.doc.lock().unwrap();
            doc.layout(rect.width(), rect.height(), font_size, CURRENT_DEVICE.dpi);
            let current_page = self.current_page.min(doc.pages_count() - 1);
            if let Some(location) = doc.resolve_location(Location::Exact(current_page)) {
                self.current_page = location;
            }
            self.text.clear();
        }

        self.cache.clear();
        self.update(Some(UpdateMode::Full), hub, rq, context);
    }

    fn might_rotate(&self) -> bool {
        self.search.is_none() && locate::<ThemeDialog>(self).is_none()
    }

    fn is_background(&self) -> bool {
        true
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
