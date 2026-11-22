use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData};
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::font::{Fonts, font_from_style, NORMAL_STYLE, SMALL_STYLE, MD_KIND};
use crate::geom::Rectangle;
use crate::color::{BLACK, WHITE};
use crate::context::Context;
use crate::gesture::GestureEvent;
use crate::device::CURRENT_DEVICE;

#[derive(Debug, Clone)]
pub struct Hit {
    pub location: usize,
    pub chapter: String,
    pub position: String,
    pub before: String,
    pub matched: String,
    pub after: String,
}

pub struct SearchResults {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    pub hits: Vec<Hit>,
    pub query: String,
    pub complete: bool,
    pub page: usize,
    last_refresh: std::time::Instant,
}
impl SearchResults {
    pub fn new(rect: Rectangle, query: String, hits: Vec<Hit>, complete: bool, page: usize) -> Self {
        Self { id: ID_FEEDER.next(), rect, children: Vec::new(), hits, query, complete, page, last_refresh: std::time::Instant::now() }
    }
    fn unit(&self) -> i32 { (CURRENT_DEVICE.dpi as i32 / 3).max(48) }
    fn content_top(&self) -> i32 { self.rect.min.y + self.unit() }
    fn close_rect(&self) -> Rectangle { let side=self.unit(); rect![self.rect.max.x-side,self.rect.min.y,self.rect.max.x,self.rect.min.y+side] }
    fn capacity(&self) -> usize { ((self.rect.max.y - self.unit() - self.content_top()) / (self.unit() * 12 / 5)).max(1) as usize }
    fn row_height(&self) -> i32 { (self.rect.max.y-self.unit()-self.content_top()) / self.capacity() as i32 }
    pub fn append(&mut self, hit: Hit, rq: &mut RenderQueue) {
        self.hits.push(hit);
        if self.hits.len() == 1 || self.last_refresh.elapsed() >= std::time::Duration::from_millis(800) {
            self.last_refresh = std::time::Instant::now();
            self.refresh(rq);
        }
    }
    fn turn(&mut self, next: bool, rq: &mut RenderQueue) {
        if next { if (self.page+1)*self.capacity() < self.hits.len() { self.page+=1; } }
        else { self.page=self.page.saturating_sub(1); }
        self.refresh(rq);
    }
    pub fn refresh(&self, rq: &mut RenderQueue) { rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui)); }
}
impl View for SearchResults {
    fn handle_event(&mut self, evt: &Event, _hub: &Hub, bus: &mut Bus, rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match evt {
            Event::Gesture(GestureEvent::Tap(p)) if self.rect.includes(*p) => {
                let u = self.unit();
                if self.close_rect().includes(*p) { bus.push_back(if self.complete { Event::CloseListSearch } else { Event::StopListSearch }); }
                else if p.y >= self.rect.max.y - u {
                    if p.x < self.rect.min.x + self.rect.width() as i32 * 15 / 100 { self.turn(false, rq); }
                    else if p.x >= self.rect.min.x + self.rect.width() as i32 * 45 / 100 && p.x < self.rect.min.x + self.rect.width() as i32 * 58 / 100 { self.turn(true, rq); }
                    else if p.x >= self.rect.min.x + self.rect.width() as i32 * 58 / 100 { bus.push_back(Event::HideSearchResults); }
                } else if p.y >= self.content_top() {
                    let index = self.page * self.capacity() + ((p.y - self.content_top()) / self.row_height()) as usize;
                    if index < self.hits.len() { bus.push_back(Event::OpenSearchHit(index)); }
                }
                true
            },
            Event::Gesture(GestureEvent::Swipe { dir, .. }) => {
                match dir {
                    crate::geom::Dir::West | crate::geom::Dir::North if (self.page+1)*self.capacity() < self.hits.len() => self.page += 1,
                    crate::geom::Dir::East | crate::geom::Dir::South => self.page = self.page.saturating_sub(1),
                    _ => (),
                }
                self.refresh(rq);
                true
            },
            Event::Gesture(..) => true,
            Event::Device(crate::input::DeviceEvent::Button { code, status: crate::input::ButtonStatus::Released, .. }) => {
                match code {
                    crate::input::ButtonCode::Forward => self.turn(true, rq),
                    crate::input::ButtonCode::Backward => self.turn(false, rq),
                    _ => return false,
                }
                true
            },
            Event::Device(crate::input::DeviceEvent::Finger { .. }) => true,
            _ => false,
        }
    }
    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, fonts: &mut Fonts) {
        fb.draw_rectangle(&self.rect, WHITE);
        let u=self.unit(); let pad=u/3;
        let width=self.rect.width() as i32-2*pad;
        let draw = |fb: &mut dyn Framebuffer, fonts: &mut Fonts, text: &str, x: i32, y: i32, w: i32, small: bool| {
            let font=font_from_style(fonts, if small { &SMALL_STYLE } else { &NORMAL_STYLE }, CURRENT_DEVICE.dpi);
            let plan=font.plan(text,Some(w),None);
            font.render(fb,BLACK,&plan,pt!(x,y));
        };
        let left=self.rect.min.x+pad;
        draw(fb,fonts,&format!("Search: {}",self.query),left,self.rect.min.y+u*2/3,self.rect.width() as i32-u-pad*2,false);
        lazy_static::lazy_static! {
            static ref CLOSE: Option<crate::framebuffer::Pixmap> = crate::document::pdf::PdfOpener::new()
                .and_then(|opener| opener.open("icons/close2.svg"))
                .and_then(|doc| doc.page(0).and_then(|p| p.pixmap(crate::unit::scale_by_dpi_raw(1.5/32.0,CURRENT_DEVICE.dpi),1)));
        }
        lazy_static::lazy_static! {
            static ref STOP: Option<crate::framebuffer::Pixmap> = crate::document::pdf::PdfOpener::new()
                .and_then(|opener| opener.open("icons/stop.svg"))
                .and_then(|doc| doc.page(0).and_then(|p| p.pixmap(crate::unit::scale_by_dpi_raw(1.5/32.0,CURRENT_DEVICE.dpi),1)));
        }
        let close=self.close_rect();
        if let Some(image) = if self.complete { CLOSE.as_ref() } else { STOP.as_ref() } {
            fb.draw_pixmap(image,pt!(close.center().x-image.width as i32/2,close.center().y-image.height as i32/2));
        } else {
            // Preview fallback when the host has no compatible MuPDF runtime.
            let side=u*3/5; let center=close.center();
            fb.draw_rectangle(&rect![center.x-side/2,center.y-side/2,center.x+side/2,center.y+side/2],BLACK);
            let d=side/4;
            if self.complete {
            fb.draw_segment(center+pt!(-d,-d),center+pt!(d,d),2.0,2.0,WHITE);
            fb.draw_segment(center+pt!(-d,d),center+pt!(d,-d),2.0,2.0,WHITE);
            }
        }
        let start=self.page*self.capacity();
        let mut previous="";
        for (i,hit) in self.hits.iter().skip(start).take(self.capacity()).enumerate() {
            let y=self.content_top()+i as i32*self.row_height();
            let chapter_start = hit.chapter != previous || i == 0;
            let mut heading_style = MD_KIND;
            heading_style.size = NORMAL_STYLE.size + NORMAL_STYLE.size / 12;
            let font = font_from_style(fonts, &heading_style, CURRENT_DEVICE.dpi);
            let position = font.plan(&hit.position, None, None);
            let position_x = self.rect.max.x - pad - position.width;
            font.render(fb, BLACK, &position, pt!(position_x, y+u/3));
            if chapter_start {
                let heading = font.plan(&hit.chapter, Some((position_x-left-pad).max(0)), None);
                font.render(fb, BLACK, &heading, pt!(left, y+u/3));
            }
            let text=format!("{}{}{}",hit.before,hit.matched,hit.after);
            let match_start=hit.before.len(); let match_end=match_start+hit.matched.len();
            let font=font_from_style(fonts,&NORMAL_STYLE,CURRENT_DEVICE.dpi);
            let mut offset=0; let line_height=u/2;
            for row in 0..3 {
                if offset>=text.len() {break;}
                // Binary-search character boundaries instead of shaping every
                // growing prefix (hundreds of plans per snippet on the UI thread).
                let boundaries: Vec<_> = text[offset..].char_indices().map(|(n,_)| offset+n).chain(std::iter::once(text.len())).collect();
                let (mut lo, mut hi) = (0, boundaries.len());
                while lo+1 < hi {
                    let mid=(lo+hi)/2;
                    if font.plan(&text[offset..boundaries[mid]],None,None).width <= width { lo=mid; }
                    else { hi=mid; }
                }
                let mut end=boundaries[lo];
                if end==offset {break;}
                if end<text.len() {
                    if let Some(space)=text[offset..end].rfind(char::is_whitespace) { if space>0 {end=offset+space+1;} }
                }
                let baseline=y+u*5/6+if chapter_start {10} else {5}+row*line_height;
                let plan=font.plan(&text[offset..end],None,None);
                font.render(fb,BLACK,&plan,pt!(left,baseline));
                let a=offset.max(match_start);let b=end.min(match_end);
                if a<b {
                    let x=left+font.plan(&text[offset..a],None,None).width;
                    let w=font.plan(&text[a..b],None,None).width;
                    fb.invert_region(&rect![x,baseline-font.ascender(),x+w,baseline-font.descender()]);
                }
                offset=end;
            }
            previous=&hit.chapter;
        }
        if self.hits.is_empty() {draw(fb,fonts,&if self.complete {"No matches found"} else {""},left,self.rect.min.y+2*u,width,false);}
        let screen_width=self.rect.width() as i32;
        let y=self.rect.max.y-u/3;
        draw(fb,fonts,"‹ Prev",left,y,screen_width*15/100-pad,false);
        draw(fb,fonts,&format!("{}{}   {} / {}",self.hits.len(),if self.complete {" results"} else {" found …"},self.page+1,((self.hits.len()+self.capacity()-1)/self.capacity()).max(1)),self.rect.min.x+screen_width*15/100,y,screen_width*30/100,false);
        draw(fb,fonts,"Next ›",self.rect.min.x+screen_width*50/100,y,screen_width*13/100,false);
        let font=font_from_style(fonts,&NORMAL_STYLE,CURRENT_DEVICE.dpi);
        let query=font.plan("Book »",Some(screen_width*42/100-pad),None);
        font.render(fb,BLACK,&query,pt!(self.rect.max.x-pad-query.width,y));
    }
    fn resize(&mut self, rect: Rectangle, _hub: &Hub, rq: &mut RenderQueue, _context: &mut Context) { self.rect=rect; self.page=0; self.refresh(rq); }
    fn rect(&self)->&Rectangle {&self.rect}
    fn rect_mut(&mut self)->&mut Rectangle {&mut self.rect}
    fn children(&self)->&Vec<Box<dyn View>> {&self.children}
    fn children_mut(&mut self)->&mut Vec<Box<dyn View>> {&mut self.children}
    fn id(&self)->Id {self.id}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paging_tracks_new_results_and_stays_in_bounds() {
        let mut list=SearchResults::new(rect![0,0,1264,1680],"test".into(),Vec::new(),false,0);
        let mut rq=RenderQueue::new();
        list.turn(false,&mut rq); list.turn(true,&mut rq); assert_eq!(list.page,0);
        let capacity=list.capacity();
        for i in 0..capacity+1 { list.append(Hit {location:i,chapter:String::new(),position:String::new(),before:String::new(),matched:"test".into(),after:String::new()},&mut rq); }
        list.turn(true,&mut rq); assert_eq!(list.page,1);
        list.turn(true,&mut rq); assert_eq!(list.page,1);
        list.turn(false,&mut rq); assert_eq!(list.page,0);
        assert!(list.row_height()*capacity as i32 <= list.rect.max.y-list.unit()-list.content_top());
    }
}
