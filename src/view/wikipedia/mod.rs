mod bottom_bar;

use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode, Pixmap};
use crate::geom::{Rectangle, Dir, CycleDir, halves};
use crate::unit::scale_by_dpi;
use crate::font::Fonts;
use crate::view::{View, Event, Hub, Bus, RenderQueue, RenderData};
use crate::view::{ViewId, Id, ID_FEEDER};
use crate::view::{SMALL_BAR_HEIGHT, THICKNESS_MEDIUM};
use crate::document::{Document, Location};
use crate::document::html::HtmlDocument;
use crate::view::common::{toggle_main_menu, toggle_battery_menu, toggle_clock_menu};
use crate::gesture::GestureEvent;
use crate::input::{DeviceEvent, ButtonCode, ButtonStatus};
use crate::color::BLACK;
use crate::app::{Context, suppress_flash};
use crate::view::filler::Filler;
use crate::view::image::Image;
use crate::view::top_bar::TopBar;
use self::bottom_bar::BottomBar;
use crate::wikipedia;

const VIEWER_STYLESHEET: &str = "css/wikipedia.css";
const USER_STYLESHEET: &str = "css/wikipedia-user.css";

pub struct Wiki {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    doc: HtmlDocument,
    location: usize,
    query: String,
    titles: Vec<String>,
    locs: Vec<usize>,
    count: usize,
    current_chapter_hi: usize,
    active: bool,
    wifi: bool,
}

impl Wiki {
    pub fn new(rect: Rectangle, query: &str, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) -> Wiki {
        suppress_flash(hub, context);
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
                                              false, false);
        children.push(Box::new(bottom_bar) as Box<dyn View>);

        let wifi = context.settings.wifi;

        rq.add(RenderData::new(id, rect, UpdateMode::Full));
        hub.send(Event::Proceed).ok();

        Wiki {
            id,
            rect,
            children,
            doc,
            location: 0,
            query: query.to_string(),
            titles: Vec::new(),
            locs: Vec::new(),
            count: 0,
            current_chapter_hi: 0,
            active: false,
            wifi,
        }

    }

    fn wiki(&mut self, rq: &mut RenderQueue, context: &mut Context) {
        let res = wikipedia::wiki(&self.query, context);
        self.count = 0;
        match res {
            Ok((content, titles, cnt)) => {
                self.count = cnt;
                self.doc.update(&content);
                self.titles = titles;
                for i in 0..cnt {
                    let location = Location::Uri(format!("#{i}"));
                    if let Some((_, loc)) = self.doc.pixmap(location, 1.0) {
                        self.locs.push(loc);
                    }
                }
            }
            Err(e) => self.doc.update(&format!("<h2>Error</h2><p>{:?}</p>", e)),
        }
        self.active = false;
        self.go_to_location(Location::Exact(0), rq);
    }

    fn update_bottom_bar(&mut self, rq: &mut RenderQueue) {
        let cc = self.current_chapter_hi;
        let hpc = self.has_previous_chapter();
        let hnc = self.has_next_chapter();
        if let Some(bottom_bar) = self.children[4].downcast_mut::<BottomBar>() {
            bottom_bar.update_icons(hpc, hnc, rq);
            bottom_bar.update_label(&format!("{}/{}: {}",
                                             cc + 1,
                                             self.count,
                                             self.titles[cc]),
                                    rq);
        }
    }

    fn get_current_chapter_hi(&mut self) -> usize {
        if let Some(next) = self.doc.resolve_location(Location::Next(self.location)) {
            for (i, loc) in self.locs.iter().enumerate() {
                if *loc >= next {
                    return i.saturating_sub(1)
                }
            }
        }
        self.count.saturating_sub(1)
    }

    fn current_chapter_lo(&mut self) -> usize {
        for (i, loc) in self.locs.iter().enumerate() {
            if self.location <= *loc {
                return i.saturating_sub(1);
            }
        }
        self.count.saturating_sub(1)
    }

    fn has_next_chapter(&mut self) -> bool {
        (self.current_chapter_hi + 1) < self.count
    }

    fn has_previous_chapter(&mut self) -> bool {
        self.current_chapter_lo() > 0 || self.locs[0] < self.location
    }

    fn go_to_neighbor(&mut self, dir: CycleDir, rq: &mut RenderQueue) {
        let location = match dir {
            CycleDir::Previous => Location::Previous(self.location),
            CycleDir::Next => Location::Next(self.location),
        };
        self.go_to_location(location, rq);
    }

    fn go_to_location(&mut self, location: Location, rq: &mut RenderQueue) {
        if let Some(image) = self.children[2].downcast_mut::<Image>() {
            if let Some((pixmap, loc)) = self.doc.pixmap(location, 1.0) {
                if loc != self.location {
                    image.update(pixmap, rq);
                    self.location = loc;
                    self.current_chapter_hi = self.get_current_chapter_hi();
                }
            }
        }
        self.update_bottom_bar(rq);
    }

    fn jump_backward(&mut self, rq: &mut RenderQueue) {
        let cc = self.current_chapter_lo();
        let ch = if self.location > self.locs[cc] {cc} else {cc.saturating_sub(1)};
        self.go_to_location(Location::Exact(self.locs[ch]), rq);
    }

    fn jump_forward(&mut self, rq: &mut RenderQueue) {
        let ch = (self.current_chapter_hi + 1).min(self.count - 1);
        self.go_to_location(Location::Exact(self.locs[ch]), rq);
    }

    fn go(&mut self, dir: CycleDir,  hub: &Hub, rq: &mut RenderQueue) {
        match dir {
            CycleDir::Previous =>
                if self.doc.resolve_location(Location::Previous(self.location)).is_some() {
                    self.go_to_neighbor(CycleDir::Previous, rq);
                } else {
                    hub.send(Event::Back).ok();
                },
            CycleDir::Next =>
                if self.doc.resolve_location(Location::Next(self.location)).is_some() {
                    self.go_to_neighbor(CycleDir::Next, rq);
                } else {
                    hub.send(Event::Back).ok();
                },
        }
    }
}

impl View for Wiki {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, _bus: &mut Bus, rq: &mut RenderQueue, context: &mut Context) -> bool {
        match *evt {
            Event::Device(DeviceEvent::NetUp) => {
                if self.active {
                    self.wiki(rq, context);
                }
                true
            },
            Event::Proceed => {
                self.active = true;
                if context.online {
                    self.wiki(rq, context);
                } else {
                    // when not online but wifi is on, NetUp doesn't seem to get triggered
                    // switch off wifi to ensure view gets notified when NetUp
                    hub.send(Event::SetWifi(false)).ok();
                    hub.send(Event::SetWifi(true)).ok();
                    hub.send(Event::Notify("Waiting for network connection.".to_string())).ok();
                }
                true
            },
            Event::Page(dir) => {
                match dir {
                    CycleDir::Previous => self.jump_backward(rq),
                    CycleDir::Next => self.jump_forward(rq),
                }
                true
            },
            Event::Gesture(GestureEvent::Swipe { dir, start, .. }) if self.rect.includes(start) => {
                match dir {
                    Dir::West => self.go_to_neighbor(CycleDir::Next, rq),
                    Dir::East => self.go_to_neighbor(CycleDir::Previous, rq),
                    _ => (),
                }
                true
            },
            Event::Device(DeviceEvent::Button { code, status: ButtonStatus::Released, .. }) => {
                match code {
                    ButtonCode::Backward => self.go(CycleDir::Previous, hub, rq),
                    ButtonCode::Forward => self.go(CycleDir::Next, hub, rq),
                    _ => (),
                }
                true
            },
            Event::Gesture(GestureEvent::Tap(center)) if self.rect.includes(center) => {
                let half_width = self.rect.width() as i32 / 2;
                if center.x < half_width {
                    self.go(CycleDir::Previous, hub, rq);
                } else {
                    self.go(CycleDir::Next, hub, rq);
                }
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
