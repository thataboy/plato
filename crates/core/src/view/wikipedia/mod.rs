mod bottom_bar;

use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode, Pixmap};
use crate::geom::{Rectangle, Dir, CycleDir, halves};
use crate::unit::scale_by_dpi;
use crate::font::Fonts;
use crate::document::{Document, Location};
use crate::document::html::HtmlDocument;
use crate::gesture::GestureEvent;
use crate::helpers::{save_text, first_n_words};
use crate::input::{DeviceEvent, ButtonCode, ButtonStatus};
use crate::color::BLACK;
use crate::context::Context;
use crate::view::{View, Event, Hub, Bus, RenderQueue, RenderData};
use crate::view::{ViewId, Id, ID_FEEDER, EntryId, EntryKind};
use crate::view::{SMALL_BAR_HEIGHT, BIG_BAR_HEIGHT, THICKNESS_MEDIUM};
use crate::view::common::{locate, locate_by_id, toggle_main_menu, toggle_battery_menu, toggle_clock_menu, get_save_path};
use crate::view::filler::Filler;
use crate::view::image::Image;
use crate::view::keyboard::Keyboard;
use crate::view::menu::{Menu, MenuKind};
use crate::view::search_bar::SearchBar;
use crate::view::top_bar::TopBar;
use self::bottom_bar::BottomBar;
use crate::wikipedia::{search, fetch, WikiPage};

const VIEWER_STYLESHEET: &str = "css/wikipedia.css";
const USER_STYLESHEET: &str = "css/wikipedia-user.css";

#[derive(PartialEq)]
enum Mode {
    Search,
    Read,
    Download,
    Idle,
}

pub struct Wiki {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    doc: HtmlDocument,
    location: usize,
    query: String,
    lang: String,
    results: Vec<WikiPage>,
    count: usize,
    current_chapter: Option<usize>,
    mode: Mode,
    wifi: bool,
    is_stand_alone: bool,
    focus: Option<ViewId>,
}

impl Wiki {
    pub fn new(rect: Rectangle, query: &str, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) -> Wiki {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let dpi = CURRENT_DEVICE.dpi;
        let small_height = scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let (small_thickness, big_thickness) = halves(thickness);

        let top_bar = TopBar::new(rect![rect.min.x, rect.min.y,
                                        rect.max.x, rect.min.y + small_height - small_thickness],
                                  Event::Back,
                                  "Wikipedia".to_string(),
                                  context);
        children.push(Box::new(top_bar) as Box<dyn View>);

        let separator = Filler::new(rect![rect.min.x, rect.min.y + small_height - small_thickness,
                                          rect.max.x, rect.min.y + small_height + big_thickness],
                                    BLACK);
        children.push(Box::new(separator) as Box<dyn View>);

        let image_rect = rect![rect.min.x, rect.min.y + small_height + big_thickness,
                               rect.max.x, rect.max.y - small_height - small_thickness];

        let image = Image::new(image_rect, Pixmap::new(1, 1));
        children.push(Box::new(image) as Box<dyn View>);

        let mut doc = HtmlDocument::new_from_memory("");
        doc.layout(image_rect.width(), image_rect.height(), context.settings.dictionary.font_size, dpi);
        doc.set_margin_width(context.settings.dictionary.margin_width, false);
        doc.set_viewer_stylesheet(VIEWER_STYLESHEET);
        doc.set_user_stylesheet(USER_STYLESHEET);

        let separator = Filler::new(rect![rect.min.x, rect.max.y - small_height - small_thickness,
                                          rect.max.x, rect.max.y - small_height + big_thickness],
                                    BLACK);
        children.push(Box::new(separator) as Box<dyn View>);

        let bottom_bar = BottomBar::new(rect![rect.min.x, rect.max.y - small_height + big_thickness,
                                              rect.max.x, rect.max.y],
                                              "",
                                              false, false, false);
        children.push(Box::new(bottom_bar) as Box<dyn View>);

        let wifi = context.settings.wifi;
        let is_stand_alone = query.is_empty();
        let lang = context.settings.wikipedia_languages[0].to_owned();

        rq.add(RenderData::new(id, rect, UpdateMode::Full));

        if is_stand_alone {
            hub.send(Event::Show(ViewId::SearchBar)).ok();
        } else {
            hub.send(Event::Proceed).ok();
        }

        Wiki {
            id,
            rect,
            children,
            doc,
            location: 0,
            query: query.to_string(),
            lang,
            results: Vec::new(),
            count: 0,
            current_chapter: None,
            mode: Mode::Search,
            wifi,
            is_stand_alone,
            focus: None,
        }

    }

    fn search(&mut self, rq: &mut RenderQueue) {
        let res = search(&self.query, &self.lang);
        match res {
            Ok(results) => {
                self.results = results;
                self.count = self.results.len();
                self.current_chapter = None;
                self.go_to_chapter(0, rq);
            }
            Err(e) => self.doc.update(&format!("<h2>Error</h2><p>{:?}</p>", e)),
        }
        self.mode = Mode::Idle;
        self.go_to_location(Location::Exact(0), rq);
    }

    fn fetch(&mut self, hub: &Hub) {
        if let Some(cc) = self.current_chapter {
            let res = fetch(&self.results[cc].pageid, &self.lang);
            match res {
                Err(e) => { hub.send(Event::Notify((&e).to_string())).ok(); },
                Ok(text) => { hub.send(Event::OpenHtml(text, None)).ok(); }
            }
            self.mode = Mode::Idle;
        }
    }

    fn save(&mut self, hub: &Hub, context: &mut Context) {
        if let Some(cc) = self.current_chapter {
            let res = fetch(&self.results[cc].pageid, &self.lang);
            match res {
                Err(e) => { hub.send(Event::Notify((&e).to_string())).ok(); },
                Ok(text) => {
                    let (path, library_index) = get_save_path(&self.results[cc].title,
                                                             "html",
                                                             context);
                    let msg = match save_text(&text, &path) {
                        Err(e) => format!("{}", e),
                        Ok(()) => {
                            if let Some(index) = library_index {
                                context.reimport(index);
                            }
                            format!("Saved {}.", path)
                        },
                    };
                    hub.send(Event::Notify(msg)).ok();
                }
            }
            self.mode = Mode::Idle;
        }
    }

    fn update_bottom_bar(&mut self, rq: &mut RenderQueue) {
        if let Some(cc) = self.current_chapter {
            if let Some(index) = locate::<BottomBar>(self) {
                let bottom_bar = self.children[index].downcast_mut::<BottomBar>().unwrap();
                bottom_bar.update_icons(cc > 0, cc < self.count.saturating_sub(1), self.count > 0, rq);
                bottom_bar.update_label(&format!("{}/{}: {}",
                                                 cc + 1,
                                                 self.count,
                                                 self.results[cc].title),
                                        rq);
            }
        }
    }

    fn go_to_neighbor(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue) {
        let location = match dir {
            CycleDir::Previous => self.doc.resolve_location(Location::Previous(self.location)),
            CycleDir::Next => self.doc.resolve_location(Location::Next(self.location)),
        };
        if let Some(loc) = location {
            self.go_to_location(Location::Exact(loc), rq);
        } else {
            self.go_to_neighbor_chapter(dir, hub, rq);
        }
    }

    fn go_to_neighbor_chapter(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue) {
        if let Some(cc) = self.current_chapter {
            match dir {
                CycleDir::Previous =>
                    if cc > 0 {
                        self.go_to_chapter(cc - 1, rq);
                    } else {
                        if self.is_stand_alone {
                            self.go_to_chapter(self.count.saturating_sub(1), rq);
                        } else {
                            hub.send(Event::Back).ok();
                        }
                    },
                CycleDir::Next =>
                    if (cc + 1) < self.count {
                        self.go_to_chapter(cc + 1, rq);
                    } else {
                        if self.is_stand_alone {
                            self.go_to_chapter(0, rq);
                        } else {
                            hub.send(Event::Back).ok();
                        }
                    },
            }
        }
    }

    fn go_to_location(&mut self, location: Location, rq: &mut RenderQueue) {
        if let Some(image) = self.children[2].downcast_mut::<Image>() {
            if let Some((pixmap, loc)) = self.doc.pixmap(location, 1.0) {
                image.update(pixmap, rq);
                self.location = loc;
            }
        }
        self.update_bottom_bar(rq);
    }

    fn go_to_chapter(&mut self, chapter: usize, rq: &mut RenderQueue) {
        if let Some(cc) = self.current_chapter {
            if cc == chapter { return; }
        }
        self.current_chapter = Some(chapter);
        self.doc.update(&self.results[chapter].extract);
        self.go_to_location(Location::Exact(0), rq);
    }

    fn toggle_chapter_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
        if self.count == 0 {
            return;
        }
        if let Some(index) = locate_by_id(self, ViewId::ChapterMenu) {
            if let Some(true) = enable {
                return;
            }
            rq.add(RenderData::expose(*self.child(index).rect(), UpdateMode::Gui));
            self.children.remove(index);
        } else {
            if let Some(false) = enable {
                return;
            }
            let cc = self.current_chapter.unwrap_or(std::usize::MAX);
            let entries = self.results.iter().enumerate()
                                    .map(|(i, x)|
                                         EntryKind::RadioButton(format!("{}. {}", i+1, x.title),
                                                                EntryId::GoTo(i),
                                                                i == cc))
                                    .collect::<Vec<EntryKind>>();
            let chapter_menu = Menu::new(rect, ViewId::ChapterMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(chapter_menu.id(), *chapter_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(chapter_menu) as Box<dyn View>);
        }
    }

    fn reseed(&mut self, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(top_bar) = self.child_mut(0).downcast_mut::<TopBar>() {
            top_bar.reseed(rq, context);
        }
        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
    }

    fn toggle_search_bar(&mut self, enable: Option<bool>, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(index) = locate::<SearchBar>(self) {
            if let Some(true) = enable {
                return;
            }

            let mut rect = *self.child(index).rect();
            rect.absorb(self.child(index-1).rect()); // top sep
            rect.absorb(self.child(index+1).rect()); // kbd's sep
            rect.absorb(self.child(index+2).rect()); // kbd
            self.children.drain(index - 1 ..= index + 2);
            rq.add(RenderData::expose(rect, UpdateMode::Gui));
            hub.send(Event::Focus(None)).ok();
        } else {
            if let Some(false) = enable {
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

            let index = locate::<BottomBar>(self).unwrap();

            let keyboard = Keyboard::new(&mut kb_rect, false, context);
            self.children.insert(index, Box::new(keyboard) as Box<dyn View>);

            let separator = Filler::new(rect![self.rect.min.x, kb_rect.min.y - thickness,
                                              self.rect.max.x, kb_rect.min.y],
                                        BLACK);
            self.children.insert(index, Box::new(separator) as Box<dyn View>);


            let sp_rect = rect![self.rect.min.x, kb_rect.min.y - small_height - small_thickness,
                                self.rect.max.x, kb_rect.min.y - small_height + big_thickness];
            let y_min = sp_rect.max.y;
            let rect = rect![self.rect.min.x, y_min,
                             self.rect.max.x, y_min + small_height - thickness];
            let search_bar = SearchBar::new(rect,
                                            ViewId::WikiSearchInput,
                                            "",
                                            &first_n_words(&self.query, 5),
                                            true,
                                            context);
            self.children.insert(index, Box::new(search_bar) as Box<dyn View>);

            let separator = Filler::new(sp_rect, BLACK);
            self.children.insert(index, Box::new(separator) as Box<dyn View>);

            for i in index..index+4 {  // 4 items added
                rq.add(RenderData::new(self.child(i).id(), *self.child(i).rect(), UpdateMode::Gui));
            }
            hub.send(Event::Focus(Some(ViewId::WikiSearchInput))).ok();
        }
    }

    fn toggle_lang_menu(&mut self, rect: Rectangle, enable: Option<bool>, rq: &mut RenderQueue, context: &mut Context) {
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
            let langs = &context.settings.wikipedia_languages;
            let mut entries = langs.iter().rev()
                                   .map(|x| EntryKind::RadioButton(x.to_string(),
                                                                   EntryId::SetWikiLang(x.to_string()),
                                                                   self.lang == x.to_string()))
                                   .collect::<Vec<EntryKind>>();
            entries.push(EntryKind::Separator);
            let lang_menu = Menu::new(rect, ViewId::WikiLangMenu, MenuKind::DropDown, entries, context);
            rq.add(RenderData::new(lang_menu.id(), *lang_menu.rect(), UpdateMode::Gui));
            self.children.push(Box::new(lang_menu) as Box<dyn View>);
        }
    }

}

impl View for Wiki {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, _bus: &mut Bus, rq: &mut RenderQueue, context: &mut Context) -> bool {
        match *evt {
            Event::Device(DeviceEvent::NetUp) => {
                match self.mode {
                    Mode::Search => self.search(rq),
                    Mode::Read => self.fetch(hub),
                    Mode::Download => self.save(hub, context),
                    _ => (),
                }
                true
            },
            Event::Proceed => {
                if context.online {
                    match self.mode {
                        Mode::Search => self.search(rq),
                        Mode::Read => self.fetch(hub),
                        Mode::Download => self.save(hub, context),
                        _ => (),
                    }
                } else if self.mode != Mode::Idle {
                    if !context.settings.wifi {
                        hub.send(Event::SetWifi(true)).ok();
                    }
                    hub.send(Event::Notify("Waiting for network connection.".to_string())).ok();
                }
                true
            },
            Event::Submit(ViewId::WikiSearchInput, ref text) => {
                if !text.trim().is_empty() {
                    self.toggle_search_bar(Some(false), hub, rq, context);
                    self.query = text.trim().to_string();
                    self.mode = Mode::Search;
                    hub.send(Event::Proceed).ok();
                }
                true
            },
            Event::Page(dir) => {
                self.go_to_neighbor_chapter(dir, hub, rq);
                true
            },
            Event::Gesture(GestureEvent::Arrow { dir, .. }) => {
                match dir {
                    Dir::West => self.go_to_neighbor_chapter(CycleDir::Previous, hub, rq),
                    Dir::East => self.go_to_neighbor_chapter(CycleDir::Next, hub, rq),
                    _ => (),
                }
                true
            },
            Event::Read => {
                self.mode = Mode::Read;
                hub.send(Event::Proceed).ok();
                true
            },
            Event::Download => {
                self.mode = Mode::Download;
                hub.send(Event::Proceed).ok();
                true
            },
            Event::Gesture(GestureEvent::Swipe { dir, start, .. }) if self.rect.includes(start) => {
                match dir {
                    Dir::East => self.go_to_neighbor(CycleDir::Previous, hub, rq),
                    Dir::West => self.go_to_neighbor(CycleDir::Next, hub, rq),
                    _ => (),
                }
                true
            },
            Event::Device(DeviceEvent::Button { code, status: ButtonStatus::Released, .. }) => {
                match code {
                    ButtonCode::Backward => self.go_to_neighbor(CycleDir::Previous, hub, rq),
                    ButtonCode::Forward => self.go_to_neighbor(CycleDir::Next, hub, rq),
                    _ => (),
                }
                true
            },
            Event::Gesture(GestureEvent::Tap(center)) if self.rect.includes(center) => {
                if self.focus.is_some() {
                    self.toggle_search_bar(Some(false), hub, rq, context);
                } else {
                    let fifth_width = self.rect.width() as i32 / 5;
                    if center.x < 2 * fifth_width {
                        self.go_to_neighbor(CycleDir::Previous, hub, rq);
                    } else if center.x > 3 * fifth_width {
                        self.go_to_neighbor(CycleDir::Next, hub, rq);
                    }
                }
                true
            },
            Event::Select(EntryId::GoTo(chapter)) => {
                self.go_to_chapter(chapter, rq);
                true
            },
            Event::Select(EntryId::SetWikiLang(ref lang)) => {
                if *lang != self.lang {
                    self.lang = lang.clone();
                }
                true
            },
            Event::ToggleNear(ViewId::ChapterMenu, rect) => {
                self.toggle_chapter_menu(rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::SearchMenu, rect) => {
                self.toggle_lang_menu(rect, None, rq, context);
                true
            },
            Event::Show(ViewId::SearchBar) => {
                self.toggle_search_bar(None, hub, rq, context);
                true
            }
            Event::Close(ViewId::SearchBar) => {
                self.toggle_search_bar(Some(false), hub, rq, context);
                true
            }
            Event::Focus(v) => {
                self.focus = v;
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
            Event::Gesture(GestureEvent::Cross(_)) => {
                hub.send(Event::Back).ok();
                true
            },
            Event::Reseed => {
                self.reseed(rq, context);
                true
            },
            Event::Back => {
                if !self.wifi {
                    hub.send(Event::SetWifi(false)).ok();
                }
                false
            },
            _ => false,
        }
    }

    fn render(&self, _fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
    }

    fn resize(&mut self, rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {

        self.toggle_search_bar(Some(false), hub, rq, context);

        let dpi = CURRENT_DEVICE.dpi;
        let small_height = scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let (small_thickness, big_thickness) = halves(thickness);

        self.children[0].resize(rect![rect.min.x, rect.min.y,
                                      rect.max.x, rect.min.y + small_height - small_thickness],
                                hub, rq, context);

        self.children[1].resize(rect![rect.min.x, rect.min.y + small_height - small_thickness,
                                      rect.max.x, rect.min.y + small_height + big_thickness],
                                hub, rq, context);

        let image_rect = rect![rect.min.x, rect.min.y + small_height + big_thickness,
                               rect.max.x, rect.max.y - small_height - small_thickness];

        self.doc.layout(image_rect.width(), image_rect.height(), context.settings.dictionary.font_size, dpi);

        if let Some(image) = self.children[2].downcast_mut::<Image>() {
            if let Some((pixmap, loc)) = self.doc.pixmap(Location::Exact(self.location), 1.0) {
                image.update(pixmap, &mut RenderQueue::new());
                self.location = loc;
            }
        }
        self.children[2].resize(image_rect, hub, rq, context);

        self.children[3].resize(rect![rect.min.x, rect.max.y - small_height - small_thickness,
                                      rect.max.x, rect.max.y - small_height + big_thickness],
                                hub, rq, context);

        self.children[4].resize(rect![rect.min.x, rect.max.y - small_height + big_thickness,
                                      rect.max.x, rect.max.y],
                                hub, rq, context);
        self.rect = rect;
        self.go_to_location(Location::Exact(self.location), rq);

        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Full));

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
