use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::view::{View, ViewId, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData, Align};
use crate::view::icon::Icon;
use crate::view::filler::Filler;
use crate::view::label::Label;
use crate::gesture::GestureEvent;
use crate::input::DeviceEvent;
use crate::geom::{Rectangle, CycleDir};
use crate::color::WHITE;
use crate::font::Fonts;
use crate::context::Context;

#[derive(Debug)]
pub struct BottomBar {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    has_prev: bool,
    has_next: bool,
    has_article: bool,
}

impl BottomBar {
    pub fn new(rect: Rectangle, text: &str, has_prev: bool, has_next: bool, has_article: bool) -> BottomBar {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let side = rect.height() as i32;

        let prev_rect = rect![rect.min, rect.min + side];
        if has_prev {
            let prev_icon = Icon::new("double_angle-left",
                                      prev_rect,
                                      Event::Page(CycleDir::Previous));
            children.push(Box::new(prev_icon) as Box<dyn View>);
        } else {
            let filler = Filler::new(prev_rect, WHITE);
            children.push(Box::new(filler) as Box<dyn View>);
        }

        let label_rect = rect![pt!(rect.min.x + side, rect.min.y),
                               pt!(rect.max.x - 4 * side, rect.max.y)];
        let label = Label::new(label_rect, text.to_string(), Align::Center)
                              .event(Some(Event::ToggleNear(ViewId::ChapterMenu, label_rect)));
        children.push(Box::new(label) as Box<dyn View>);

        let read_rect = rect![pt!(rect.max.x - 4 * side, rect.min.y),
                              pt!(rect.max.x - 3 * side, rect.max.y)];
        if has_article {
            let read_icon = Icon::new("read",
                                      read_rect,
                                      Event::Read);
            children.push(Box::new(read_icon) as Box<dyn View>);
        } else {
            let filler = Filler::new(read_rect, WHITE);
            children.push(Box::new(filler) as Box<dyn View>);
        }

        let download_rect = rect![pt!(rect.max.x - 3 * side, rect.min.y),
                                  pt!(rect.max.x - 2 * side, rect.max.y)];
        if has_article {
            let download_icon = Icon::new("download",
                                          download_rect,
                                          Event::Download);
            children.push(Box::new(download_icon) as Box<dyn View>);
        } else {
            let filler = Filler::new(download_rect, WHITE);
            children.push(Box::new(filler) as Box<dyn View>);
        }

        let search_rect = rect![pt!(rect.max.x - 2 * side, rect.min.y),
                                pt!(rect.max.x - side, rect.max.y)];
        let search_icon = Icon::new("search",
                                    search_rect,
                                    Event::Show(ViewId::SearchBar));
        children.push(Box::new(search_icon) as Box<dyn View>);

        let next_rect = rect![rect.max - side, rect.max];
        if has_next {
            let next_icon = Icon::new("double_angle-right",
                                      rect![rect.max - side, rect.max],
                                      Event::Page(CycleDir::Next));
            children.push(Box::new(next_icon) as Box<dyn View>);
        } else {
            let filler = Filler::new(next_rect, WHITE);
            children.push(Box::new(filler) as Box<dyn View>);
        }

        BottomBar {
            id,
            rect,
            children,
            has_prev,
            has_next,
            has_article,
        }
    }

    pub fn update_icons(&mut self, has_prev: bool, has_next: bool, has_article: bool, rq: &mut RenderQueue) {
        if self.has_prev != has_prev {
            let index = 0;
            let prev_rect = *self.child(index).rect();
            if has_prev {
                let prev_icon = Icon::new("double_angle-left",
                                          prev_rect,
                                          Event::Page(CycleDir::Previous));
                self.children[index] = Box::new(prev_icon) as Box<dyn View>;
            } else {
                let filler = Filler::new(prev_rect, WHITE);
                self.children[index] = Box::new(filler) as Box<dyn View>;
            }
            self.has_prev = has_prev;
            rq.add(RenderData::new(self.id, prev_rect, UpdateMode::Gui));
        }

        if self.has_article != has_article {
            let index = 2;
            let read_rect = *self.child(index).rect();
            let download_rect = *self.child(index+1).rect();
            if has_article {
                let read_icon = Icon::new("read",
                                          read_rect,
                                          Event::Read);
                self.children[index] = Box::new(read_icon) as Box<dyn View>;
                let download_icon = Icon::new("download",
                                          download_rect,
                                          Event::Download);
                self.children[index+1] = Box::new(download_icon) as Box<dyn View>;
            } else {
                let filler = Filler::new(read_rect, WHITE);
                self.children[index] = Box::new(filler) as Box<dyn View>;
                let filler = Filler::new(download_rect, WHITE);
                self.children[index+1] = Box::new(filler) as Box<dyn View>;
            }
            self.has_article = has_article;
            rq.add(RenderData::new(self.id, read_rect, UpdateMode::Gui));
            rq.add(RenderData::new(self.id, download_rect, UpdateMode::Gui));
        }

        if self.has_next != has_next {
            let index = self.len() - 1;
            let next_rect = *self.child(index).rect();
            if has_next {
                let next_icon = Icon::new("double_angle-right",
                                          next_rect,
                                          Event::Page(CycleDir::Next));
                self.children[index] = Box::new(next_icon) as Box<dyn View>;
            } else {
                let filler = Filler::new(next_rect, WHITE);
                self.children[index] = Box::new(filler) as Box<dyn View>;
            }
            self.has_next = has_next;
            rq.add(RenderData::new(self.id, next_rect, UpdateMode::Gui));
        }
    }

    pub fn update_label(&mut self, text: &str, rq: &mut RenderQueue) {
        let label = self.child_mut(1).downcast_mut::<Label>().unwrap();
        label.update(text, rq);
    }
}

impl View for BottomBar {
    fn handle_event(&mut self, evt: &Event, _hub: &Hub, _bus: &mut Bus, _rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Tap(center)) |
            Event::Gesture(GestureEvent::HoldFingerShort(center, ..)) if self.rect.includes(center) => true,
            Event::Device(DeviceEvent::Finger { position, .. }) if self.rect.includes(position) => true,
            _ => false,
        }
    }

    fn render(&self, _fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
    }

    fn resize(&mut self, rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let side = rect.height() as i32;
        let prev_rect = rect![rect.min, rect.min + side];
        self.children[0].resize(prev_rect, hub, rq, context);
        let label_rect = rect![pt!(rect.min.x + side, rect.min.y),
                               pt!(rect.max.x - 4 * side, rect.max.y)];
        self.children[1].resize(label_rect, hub, rq, context);
        let read_rect = rect![pt!(rect.max.x - 4 * side, rect.min.y),
                              pt!(rect.max.x - 3 * side, rect.max.y)];
        self.children[2].resize(read_rect, hub, rq, context);
        let download_rect = rect![pt!(rect.max.x - 3 * side, rect.min.y),
                                  pt!(rect.max.x - 2 * side, rect.max.y)];
        self.children[3].resize(download_rect, hub, rq, context);
        let search_rect = rect![pt!(rect.max.x - 2 * side, rect.min.y),
                                pt!(rect.max.x - side, rect.max.y)];
        self.children[4].resize(search_rect, hub, rq, context);
        let next_rect = rect![rect.max - side, rect.max];
        self.children[5].resize(next_rect, hub, rq, context);
        self.rect = rect;
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
