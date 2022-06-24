use crate::color::WHITE;
use crate::device::CURRENT_DEVICE;
use crate::document::BYTES_PER_PAGE;
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData, SliderId, THICKNESS_MEDIUM, Align};
use crate::view::filler::Filler;
use crate::view::slider::Slider;
use crate::view::icon::Icon;
use crate::view::label::Label;
use crate::gesture::GestureEvent;
use crate::input::DeviceEvent;
use crate::unit::scale_by_dpi;
use crate::geom::Rectangle;
use crate::font::Fonts;
use crate::color::SEPARATOR_NORMAL;
use crate::app::Context;

#[derive(Debug)]
pub struct Scrubber {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    original_loc: usize,
    original_page: f32,
    current_page: f32,
    precision: usize,
    synthetic: bool,
    back_enabled: bool,
}

impl Scrubber {
    pub fn new(rect: Rectangle, current_loc: usize, pages_count: usize, synthetic: bool) -> Scrubber {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let dpi = CURRENT_DEVICE.dpi;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let side = rect.height() as i32;
        let y = rect.min.y + thickness;

        let separator = Filler::new(rect![pt!(rect.min.x, rect.min.y), pt!(rect.max.x, y)],
                                    SEPARATOR_NORMAL);
        children.push(Box::new(separator) as Box<dyn View>);

        let label_width = 3 * side / 2;
        let label = Label::new(rect![rect.min.x, y,
                                     rect.min.x + label_width, rect.max.y],
                               "".to_string(), Align::Center);
        children.push(Box::new(label) as Box<dyn View>);

        let current_page = if synthetic {
                               current_loc as f32 / BYTES_PER_PAGE as f32
                           } else {
                               current_loc as f32
                           };

        let pcount = if synthetic { pages_count as f32 / BYTES_PER_PAGE as f32 } else { pages_count as f32 };

        let precision = if !synthetic || pcount > 10.0 { 0 }
                        else if pcount > 5.0 { 1 }
                        else { 2 };

        let slider = Slider::new(rect![rect.min.x + label_width, y,
                                       rect.max.x - side, rect.max.y],
                                 SliderId::Scrubber,
                                 current_page,
                                 0.0,
                                 pcount);
        children.push(Box::new(slider) as Box<dyn View>);

        let back_rect = rect![pt!(rect.max.x - side, y),
                              pt!(rect.max.x, rect.max.y)];
        let back_icon = Filler::new(back_rect, WHITE);
        children.push(Box::new(back_icon) as Box<dyn View>);

        Scrubber {
            id,
            rect,
            children,
            original_loc: current_loc,
            original_page: current_page,
            current_page,
            precision,
            synthetic,
            back_enabled: false,
        }
    }

    pub fn set_value(&mut self, loc: usize, rq: &mut RenderQueue) {
        let page = if self.synthetic {
                       loc as f32 / BYTES_PER_PAGE as f32
                   } else {
                       loc as f32
                   };
        self.update_value(page, rq);
        let slider = self.child_mut(2).downcast_mut::<Slider>().unwrap();
        slider.update(page, rq);
        self.current_page = page;
        self.update_back_icon(self.original_loc != loc, rq);
    }

    pub fn update_value(&mut self, page: f32, rq: &mut RenderQueue) {
        self.current_page = page;
        let diff = page - self.original_page;
        let precision = self.precision;
        let label = self.child_mut(1).downcast_mut::<Label>().unwrap();
        label.fast_update(&format!("{0}{1:.2$}p",
                                   if diff > 0.0 {"+"} else if diff < 0.0 {"-"} else {""},
                                   diff.abs(),
                                   precision),
                          rq);
    }

    pub fn update_back_icon(&mut self, enable: bool, rq: &mut RenderQueue) {
        if self.back_enabled != enable {
            let index = 3;
            let back_rect = *self.child(index).rect();
            if enable {
                let back_icon = Icon::new("back",
                                          back_rect,
                                          Event::GoTo(self.original_loc));
                self.children[index] = Box::new(back_icon) as Box<dyn View>;
            } else {
                let filler = Filler::new(back_rect, WHITE);
                self.children[index] = Box::new(filler) as Box<dyn View>;
            }
            self.back_enabled = enable;
            rq.add(RenderData::new(self.id, back_rect, UpdateMode::Gui));
        }
    }

}

impl View for Scrubber {

    fn handle_event(&mut self, evt: &Event, _hub: &Hub, _bus: &mut Bus, _rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Tap(center)) |
            Event::Gesture(GestureEvent::HoldFingerShort(center, ..)) if self.rect.includes(center) => true,
            Event::Gesture(GestureEvent::Swipe { start, .. }) if self.rect.includes(start) => true,
            Event::Device(DeviceEvent::Finger { position, .. }) if self.rect.includes(position) => true,
            _ => false,
        }
    }

    fn render(&self, _fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
    }

    fn resize(&mut self, rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let dpi = CURRENT_DEVICE.dpi;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let side = rect.height() as i32;
        let x_scrubber = rect.min.x + 3 * side / 2;
        let y_start = rect.min.y + thickness;
        self.children[0].resize(rect![pt!(rect.min.x, rect.min.y),
                                      pt!(rect.max.x, y_start)], hub, rq, context);
        self.children[1].resize(rect![pt!(rect.min.x, y_start),
                                      pt!(x_scrubber, rect.max.y)], hub, rq, context);
        self.children[2].resize(rect![pt!(x_scrubber, y_start),
                                      pt!(rect.max.x - side, rect.max.y)], hub, rq, context);
        self.children[3].resize(rect![pt!(rect.max.x - side, y_start),
                                      pt!(rect.max.x, rect.max.y)], hub, rq, context);
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
