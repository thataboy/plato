mod bottom_bar;

use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode, Pixmap};
use crate::geom::{Rectangle, Dir, CycleDir, halves};
use crate::unit::scale_by_dpi;
use crate::font::Fonts;
use crate::view::{View, Event, Hub, Bus, RenderQueue, RenderData};
use crate::view::{ViewId, Id, ID_FEEDER, EntryId, EntryKind};
use crate::view::{SMALL_BAR_HEIGHT, THICKNESS_MEDIUM};
use crate::document::{Document, Location};
use crate::document::html::HtmlDocument;
use crate::view::common::{locate_by_id, toggle_main_menu, toggle_battery_menu, toggle_clock_menu};
use crate::gesture::GestureEvent;
use crate::input::{DeviceEvent, ButtonCode, ButtonStatus};
use crate::color::BLACK;
use crate::app::{Context, suppress_flash};
use crate::view::filler::Filler;
use crate::view::image::Image;
use crate::view::menu::{Menu, MenuKind};
use crate::view::top_bar::TopBar;
use self::bottom_bar::BottomBar;
use crate::wikipedia::{search, fetch, Page};

const VIEWER_STYLESHEET: &str = "css/wikipedia.css";
const USER_STYLESHEET: &str = "css/wikipedia-user.css";

#[derive(PartialEq)]
enum Mode {
    Search,
    Fetch,
    Idle,
}

pub struct Wiki {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    doc: HtmlDocument,
    location: usize,
    query: String,
    pages: Vec<Page>,
    count: usize,
    current_chapter: Option<usize>,
    mode: Mode,
    wifi: bool,
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
        doc.set_margin_width(context.settings.dictionary.margin_width);
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

        suppress_flash(hub, context);
        rq.add(RenderData::new(id, rect, UpdateMode::Full));
        hub.send(Event::Proceed).ok();

        Wiki {
            id,
            rect,
            children,
            doc,
            location: 0,
            query: query.to_string(),
            pages: Vec::new(),
            count: 0,
            current_chapter: None,
            mode: Mode::Search,
            wifi,
        }

    }

    fn search(&mut self, _hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        // hub.send(Event::Notify("Searching...".to_string())).ok();
        let res = search(&self.query, context);
        match res {
            Ok(results) => {
                self.pages = results;
                self.count = self.pages.len();
                self.go_to_chapter(0, rq);
            }
            Err(e) => self.doc.update(&format!("<h2>Error</h2><p>{:?}</p>", e)),
        }
        self.mode = Mode::Idle;
        self.go_to_location(Location::Exact(0), rq);
    }

    fn fetch(&mut self, hub: &Hub, _rq: &mut RenderQueue, context: &mut Context) {
        // hub.send(Event::Notify("Fetching full article...".to_string())).ok();
        if let Some(cc) = self.current_chapter {
            let res = fetch(&self.pages[cc].pageid, context);
            match res {
                Ok(text) => { hub.send(Event::OpenHtml(text, None)).ok(); },
                Err(e) => { hub.send(Event::Notify((&e).to_string())).ok(); },
            }
            self.mode = Mode::Idle;
        }
    }

    fn update_bottom_bar(&mut self, rq: &mut RenderQueue) {
        if let Some(cc) = self.current_chapter {
            if let Some(bottom_bar) = self.children[4].downcast_mut::<BottomBar>() {
                bottom_bar.update_icons(cc > 0, cc < self.count - 1, self.count > 0, rq);
                bottom_bar.update_label(&format!("{}/{}: {}",
                                                 cc + 1,
                                                 self.count,
                                                 self.pages[cc].title),
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
            self.go_to_next_chapter(dir, hub, rq);
        }
    }

    fn go_to_next_chapter(&mut self, dir: CycleDir, hub: &Hub, rq: &mut RenderQueue) {
        if let Some(cc) = self.current_chapter {
            match dir {
                CycleDir::Previous =>
                    if cc > 0 {
                        self.go_to_chapter(cc - 1, rq);
                    } else {
                        hub.send(Event::Back).ok();
                    },
                CycleDir::Next =>
                    if (cc + 1) < self.count {
                        self.go_to_chapter(cc + 1, rq);
                    } else {
                        hub.send(Event::Back).ok();
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
        self.doc.update(&self.pages[chapter].extract);
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
            let entries = self.pages.iter().enumerate()
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

}

impl View for Wiki {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, _bus: &mut Bus, rq: &mut RenderQueue, context: &mut Context) -> bool {
        match *evt {
            Event::Device(DeviceEvent::NetUp) => {
                match self.mode {
                    Mode::Search => self.search(hub, rq, context),
                    Mode::Fetch => self.fetch(hub, rq, context),
                    _ => (),
                }
                true
            },
            Event::Proceed => {
                if context.online {
                    match self.mode {
                        Mode::Search => self.search(hub, rq, context),
                        Mode::Fetch => self.fetch(hub, rq, context),
                        _ => (),
                    }
                } else if self.mode != Mode::Idle {
                    // when not online but wifi is on, NetUp doesn't seem to get triggered
                    // switch off wifi to ensure view gets notified when NetUp
                    hub.send(Event::SetWifi(false)).ok();
                    hub.send(Event::SetWifi(true)).ok();
                    hub.send(Event::Notify("Waiting for network connection.".to_string())).ok();
                }
                true
            },
            Event::Page(dir) => {
                self.go_to_next_chapter(dir, hub, rq);
                true
            },
            Event::Download => {
                self.mode = Mode::Fetch;
                hub.send(Event::Proceed).ok();
                true
            },
            Event::Gesture(GestureEvent::Swipe { dir, start, .. }) if self.rect.includes(start) => {
                match dir {
                    Dir::West => self.go_to_neighbor(CycleDir::Previous, hub, rq),
                    Dir::East => self.go_to_neighbor(CycleDir::Next, hub, rq),
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
                let half_width = self.rect.width() as i32 / 2;
                if center.x < half_width {
                    self.go_to_neighbor(CycleDir::Previous, hub, rq);
                } else {
                    self.go_to_neighbor(CycleDir::Next, hub, rq);
                }
                true
            },
            Event::Select(EntryId::GoTo(chapter)) => {
                self.go_to_chapter(chapter, rq);
                true
            },
            Event::ToggleNear(ViewId::ChapterMenu, rect) => {
                self.toggle_chapter_menu(rect, None, rq, context);
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
                hub.send(Event::Notify("closing...".to_string())).ok();
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
