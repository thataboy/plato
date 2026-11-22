use plato_core::view::reader::search_results::{Hit, SearchResults};
use plato_core::view::View;
use plato_core::font::Fonts;
use plato_core::framebuffer::{Pixmap, Framebuffer};
use plato_core::geom::{Rectangle, Point};
#[test]
fn render_incremental_results_preview() {
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).unwrap();
    let mut fonts=Fonts::load().unwrap();
    let rect=Rectangle::new(Point::new(0,0),Point::new(1264,1680));
    let hits=(0..12).map(|i|Hit { location:i, chapter:if i<2 {"Chapter 1 · The beginning"} else {"Chapter 2 · A new journey"}.into(),position:format!("p. {}",12+i*4),before:"…she looked out across the quiet garden and saw ".into(),matched:"the morning light".into(),after:" falling softly across the path beyond the gate…".into() }).collect();
    let list=SearchResults::new(rect,"morning light".into(),hits,false,0);
    let mut fb=Pixmap::new(1264,1680,1);
    list.render(&mut fb,rect,&mut fonts);
    assert!(fb.data().iter().any(|&p|p==0));
    assert!(fb.data().iter().any(|&p|p==255));
    fb.save("/tmp/plato-search-results-preview.png").unwrap();
}
