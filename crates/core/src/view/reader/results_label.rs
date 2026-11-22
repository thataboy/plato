use crate::device::CURRENT_DEVICE;
use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::color::TEXT_NORMAL;
use crate::geom::{Rectangle};
use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData};
use crate::context::Context;

pub struct ResultsLabel {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    count: usize,
    completed: bool,
}

impl ResultsLabel {
    pub fn new(rect: Rectangle, count: usize, completed: bool) -> ResultsLabel {
        ResultsLabel {
            id: ID_FEEDER.next(),
            rect,
            children: Vec::new(),
            count,
            completed,
        }
    }

    pub fn update(&mut self, count: usize, completed: bool, rq: &mut RenderQueue) {
        self.count = count;
        self.completed = completed;
        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
    }

    fn text(&self) -> String {
        if self.completed { format!("« {} results", self.count) }
        else { format!("« {} found...", self.count) }
    }

}


impl View for ResultsLabel {
    fn handle_event(&mut self, evt: &Event, _hub: &Hub, _bus: &mut Bus, rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(crate::gesture::GestureEvent::Tap(p)) if self.rect.includes(p) => {
                _bus.push_back(Event::ShowSearchResults);
                true
            },
            Event::EndOfSearch => {
                self.completed = true;
                rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                false
            },
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, fonts: &mut Fonts) {
        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(fonts, &NORMAL_STYLE, dpi);
        let padding = font.em() as i32 / 2;
        let max_width = self.rect.width().saturating_sub(2 * padding as u32) as i32;
        let plan = font.plan(&self.text(), Some(max_width), None);
        let dx = padding + (max_width - plan.width) / 2;
        let dy = (self.rect.height() as i32 - font.x_heights.0 as i32) / 2;
        let pt = pt!(self.rect.min.x + dx, self.rect.max.y - dy);
        fb.draw_rectangle(&self.rect, TEXT_NORMAL[0]);
        font.render(fb, TEXT_NORMAL[1], &plan, pt);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn count_text_tracks_search_completion() {
        let mut label=ResultsLabel::new(rect![0,0,400,80],0,false);
        assert_eq!(label.text(),"« 0 found...");
        let mut rq=RenderQueue::new();
        label.update(12,false,&mut rq);
        assert_eq!(label.text(),"« 12 found...");
        label.update(12,true,&mut rq);
        assert_eq!(label.text(),"« 12 results");
    }
}
