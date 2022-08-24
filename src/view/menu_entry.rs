use std::mem;
use std::sync::Mutex;
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::geom::{Rectangle, CornerSpec};
use super::menu::MenuKind;
use super::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData, EntryKind};
use super::icon::ICONS_PIXMAPS;
use crate::input::{DeviceEvent, FingerStatus};
use crate::gesture::GestureEvent;
use crate::font::{Fonts, font_from_style, NORMAL_STYLE, SPECIAL_STYLE};
use crate::color::{TEXT_NORMAL, TEXT_INVERTED_HARD};
use crate::app::Context;

static DOT_MENU_WIDTH: Mutex<i32> = Mutex::new(0);

pub struct MenuEntry {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    kind: EntryKind,
    corner_spec: Option<CornerSpec>,
    anchor: Rectangle,
    active: bool,
    dot_menu_active: Option<bool>,
}

impl MenuEntry {
    pub fn new(rect: Rectangle, kind: EntryKind, anchor: Rectangle, corner_spec: Option<CornerSpec>) -> MenuEntry {
        MenuEntry {
            id: ID_FEEDER.next(),
            rect,
            children: Vec::new(),
            kind,
            corner_spec,
            anchor,
            active: false,
            dot_menu_active: None,
        }
    }

    pub fn update(&mut self, value: bool, rq: &mut RenderQueue) {
        if let Some(v) = self.kind.get() {
            if v != value {
                self.kind.set(value);
                rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
            }
        }
    }

}

impl View for MenuEntry {
    fn handle_event(&mut self, evt: &Event, _hub: &Hub, bus: &mut Bus, rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Device(DeviceEvent::Finger { status, position, .. }) => {
                match status {
                    FingerStatus::Down if self.rect.includes(position) => {
                        self.active = true;
                        self.dot_menu_active = if let EntryKind::CommandEx(..) = self.kind {
                            let dot_menu_x = self.rect.max.x - *DOT_MENU_WIDTH.lock().unwrap();
                            Some(position.x >= dot_menu_x)
                        } else {
                            None
                        };
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Fast));
                        true
                    },
                    FingerStatus::Up if self.active => {
                        self.active = false;
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                        true
                    },
                    _ => false,
                }
            },
            Event::Gesture(GestureEvent::Tap(center)) if self.rect.includes(center) => {
                match self.kind {
                    EntryKind::CheckBox(_, _, ref mut value) => {
                        *value = !*value;
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                    },
                    EntryKind::RadioButton(_, _, ref mut value) if !*value => {
                        *value = true;
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                    },
                    _ => (),
                };
                match self.kind {
                    EntryKind::Command(_, ref id) |
                    EntryKind::CheckBox(_, ref id, _) |
                    EntryKind::RadioButton(_, ref id, _) => {
                        bus.push_back(Event::Select(id.clone()));
                        bus.push_back(Event::Validate);
                    },
                    EntryKind::SubMenu(_, ref entries) | EntryKind::More(ref entries) => {
                        bus.push_back(Event::SubMenu(self.anchor, entries.clone(), MenuKind::SubMenu));
                    },
                    EntryKind::CommandEx(_, ref id, ref entries) => {
                        let dot_menu_x = self.rect.max.x - *DOT_MENU_WIDTH.lock().unwrap();
                        if center.x < dot_menu_x {
                            bus.push_back(Event::Select(id.clone()));
                            bus.push_back(Event::Validate);
                        } else {
                            let rect = rect![dot_menu_x, self.rect.min.y,
                                             self.rect.max.x, self.rect.max.y];
                            bus.push_back(Event::SubMenu(rect, entries.clone(), MenuKind::Contextual));
                        }
                    },
                    EntryKind::Message(..) => {
                        bus.push_back(Event::Validate);
                    },
                    _ => (),
                };
                true
            },
            Event::PropagateSelect(ref other_id) => {
                match self.kind {
                    EntryKind::RadioButton(_, ref id, ref mut value) if *value => {
                        if mem::discriminant(id) == mem::discriminant(other_id) && id != other_id {
                            *value = false;
                            rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                            true
                        } else {
                            false
                        }
                    },
                    _ => false,
                }
            },
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, fonts: &mut Fonts) {
        let dpi = CURRENT_DEVICE.dpi;
        let style = if matches!(self.kind, EntryKind::More(..)) {
            SPECIAL_STYLE
        } else {
            NORMAL_STYLE
        };
        let font = font_from_style(fonts, &style, dpi);
        let x_height = font.x_heights.0 as i32;
        let padding = 4 * font.em() as i32;

        let scheme = if self.active {
            TEXT_INVERTED_HARD
        } else {
            TEXT_NORMAL
        };

        let arect = match self.dot_menu_active {
            None => self.rect.clone(),
            Some(active) => {
                let dot_menu_x = self.rect.max.x - *DOT_MENU_WIDTH.lock().unwrap();
                if active {
                    rect![dot_menu_x, self.rect.min.y,
                          self.rect.max.x, self.rect.max.y]
                } else {
                    rect![self.rect.min.x, self.rect.min.y,
                          dot_menu_x, self.rect.max.y]
                }
            },
        };

        if let Some(ref cs) = self.corner_spec {
            fb.draw_rounded_rectangle(&arect, cs, scheme[0]);
        } else {
            fb.draw_rectangle(&arect, scheme[0]);
        }

        let max_width = self.rect.width() as i32 - padding;
        let plan = font.plan(self.kind.text(), Some(max_width), None);
        let dy = (self.rect.height() as i32 - x_height) / 2;
        let pt = pt!(self.rect.min.x + padding / 2,
                     self.rect.max.y - dy);

        font.render(fb,
                    if !self.dot_menu_active.unwrap_or(false) { scheme[1] } else { TEXT_NORMAL[1] }
                    , &plan, pt);

        let (icon_name, x_offset) = match self.kind {
            EntryKind::CheckBox(_, _, value) if value => ("check_mark", 0),
            EntryKind::RadioButton(_, _, value) if value => ("bullet", 0),
            EntryKind::Message(_, Some(ref name)) => (name.as_str(), 0),
            EntryKind::SubMenu(..) |
            EntryKind::More(..) => ("angle-right-small",
                                    self.rect.width() as i32 - padding / 2),
            EntryKind::CommandEx(..) => ("vertical-dots",
                                         self.rect.width() as i32 - padding / 2),
            _ => ("", 0),
        };

        if let Some(pixmap) = ICONS_PIXMAPS.get(icon_name) {
            if let EntryKind::CommandEx(..) = self.kind {
                *DOT_MENU_WIDTH.lock().unwrap() = padding / 2;
            }
            let dx = x_offset + (padding / 2 - pixmap.width as i32) / 2;
            let dy = (self.rect.height() as i32 - pixmap.height as i32) / 2;
            let pt = self.rect.min + pt!(dx, dy);

            fb.draw_blended_pixmap(pixmap, pt, scheme[1]);
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
