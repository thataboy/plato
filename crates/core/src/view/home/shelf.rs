use std::thread;
use std::sync::Mutex;
use std::path::PathBuf;
use lazy_static::lazy_static;
use super::book::Book;
use crate::device::CURRENT_DEVICE;
use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData};
use crate::view::BIG_BAR_HEIGHT;
use crate::view::filler::Filler;
use crate::document::open;
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::settings::{FirstColumn, LibraryView};
use crate::geom::{Rectangle, Dir, CycleDir};
use crate::color::WHITE;
use crate::gesture::GestureEvent;
use crate::unit::scale_by_dpi;
use crate::metadata::Info;
use crate::geom::divide;
use crate::font::Fonts;
use crate::context::Context;

lazy_static! {
    static ref EXCLUSIVE_ACCESS: Mutex<u8> = Mutex::new(0);
}

pub struct Shelf {
    id: Id,
    pub rect: Rectangle,
    children: Vec<Box<dyn View>>,
    // maximum number of rows per page in list mode
    max_rows: usize,
    // maximum number of cols per page in cover mode
    max_cols: usize,
    first_column: FirstColumn,
    library_view: LibraryView,
}

impl Shelf {
    pub fn new(rect: Rectangle, first_column: FirstColumn, library_view: LibraryView) -> Shelf {
        let dpi = CURRENT_DEVICE.dpi;
        let big_height = scale_by_dpi(BIG_BAR_HEIGHT, dpi) as u32;
        // each cover image is 3x list row tall, with title and author taking 1 list row
        // thus in cover view, each book entry takes 4x list rows
        let cover_height = 3 * big_height;
        let cover_width = 3 * cover_height / 4;
        let max_rows = (rect.height() / big_height) as usize;
        let max_cols = (rect.width() / cover_width) as usize;
        Shelf {
            id: ID_FEEDER.next(),
            rect,
            children: Vec::new(),
            max_rows,
            max_cols,
            first_column,
            library_view,
        }
    }

    pub fn set_first_column(&mut self, first_column: FirstColumn) {
        self.first_column = first_column;
    }

    pub fn set_library_view(&mut self, library_view: LibraryView) {
        self.library_view = library_view;
    }

    pub fn max_items(&self) -> usize {
        if self.library_view == LibraryView::Cover {
            (self.max_rows / 4).max(2) * self.max_cols
        } else {
            self.max_rows
        }
    }

    pub fn resize(&mut self) {
        let dpi = CURRENT_DEVICE.dpi;
        let big_height = scale_by_dpi(BIG_BAR_HEIGHT, dpi) as i32;
        let cover_height = 3 * big_height;
        let cover_width = 3 * cover_height / 4;
        self.max_rows = (self.rect.height() as i32 / big_height) as usize;
        self.max_cols = (self.rect.width() as i32 / cover_width) as usize;
    }

    pub fn preview_path(&mut self, path: &PathBuf, hub: &Hub, context: &Context) -> Option<PathBuf> {
        if self.library_view != LibraryView::TextOnly {
            let dpi = CURRENT_DEVICE.dpi;
            let big_height = scale_by_dpi(BIG_BAR_HEIGHT, dpi) as i32;
            let th = 3 * big_height;
            let tw = 3 * th / 4;
            let thumb_path = context.library.thumbnail_preview(path);
            if !thumb_path.exists() {
                let hub2 = hub.clone();
                let thumb_path2 = thumb_path.to_string_lossy().into_owned();
                let full_path = context.library.home.join(path);
                let path = path.clone();
                thread::spawn(move || {
                    // This is a hack to circumvent a segfault (EXC_BAD_ACCESS)
                    // triggered by loading multiple jp2 pixmaps in parallel.
                    let _guard = EXCLUSIVE_ACCESS.lock().unwrap();
                    open(full_path).and_then(|mut doc| {
                        doc.preview_pixmap(tw as f32, th as f32)
                    }).map(|pixmap| {
                        if pixmap.save(&thumb_path2).is_ok() {
                            hub2.send(Event::RefreshBookPreview(path, Some(PathBuf::from(thumb_path2)))).ok();
                        }
                    })
                });
                Some(PathBuf::default())
            } else {
                Some(thumb_path)
            }
        } else {
            None
        }
    }

    pub fn update(&mut self, metadata: &[Info], hub: &Hub, rq: &mut RenderQueue, context: &Context) {
        self.children.clear();
        let max_items = self.max_items();
        // clear screen if not all slots are filled
        if metadata.len() < max_items {
            let filler = Filler::new(rect![self.rect.min.x,
                                           self.rect.min.y,
                                           self.rect.max.x,
                                           self.rect.max.y],
                                     WHITE);
            self.children.push(Box::new(filler) as Box<dyn View>);
        }

        if self.library_view == LibraryView::Cover {
            // cover view
            let row_height = self.rect.height() as i32 / (self.max_rows as i32 / 4).max(2);
            let col_width = self.rect.width() as i32 / self.max_cols as i32;

            for (index, info) in metadata.iter().enumerate() {
                let row = index / self.max_cols;
                let col = index % self.max_cols;

                let x_min = self.rect.min.x + (col as i32) * col_width;
                let y_min = self.rect.min.y + (row as i32) * row_height;

                let preview_path = self.preview_path(&info.file.path, hub, context);
                let book = Book::new(rect![x_min, y_min,
                                           x_min + col_width, y_min + row_height],
                                     info.clone(),
                                     index,
                                     self.first_column,
                                     true,
                                     preview_path);
                self.children.push(Box::new(book) as Box<dyn View>);
            }
        } else {
            // List view
            let book_heights = divide(self.rect.height() as i32, max_items as i32);
            let mut y_pos = self.rect.min.y;

            for (index, info) in metadata.iter().enumerate() {
                let y_min = y_pos;
                let y_max = y_pos + book_heights[index];

                let preview_path = self.preview_path(&info.file.path, hub, context);
                let book = Book::new(rect![self.rect.min.x, y_min,
                                           self.rect.max.x, y_max],
                                     info.clone(),
                                     index,
                                     self.first_column,
                                     false,
                                     preview_path);
                self.children.push(Box::new(book) as Box<dyn View>);

                y_pos += book_heights[index];
            }
        }

        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Partial));
    }
}

impl View for Shelf {
    fn handle_event(&mut self, evt: &Event, _hub: &Hub, bus: &mut Bus, _rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Swipe { dir, start, .. }) if self.rect.includes(start) => {
                match dir {
                    Dir::West => {
                        bus.push_back(Event::Page(CycleDir::Next));
                        true
                    },
                    Dir::East => {
                        bus.push_back(Event::Page(CycleDir::Previous));
                        true
                    },
                    _ => false,
                }
            },
            _ => false,
        }
    }

    fn render(&self, _fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
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
