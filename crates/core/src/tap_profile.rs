use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

thread_local! {
    static TAP: Cell<Option<(usize, Instant)>> = const { Cell::new(None) };
    static PAGE_TURN: Cell<bool> = const { Cell::new(false) };
}
static NEXT_TAP: AtomicUsize = AtomicUsize::new(1);

pub struct Tap;

impl Tap {
    pub fn begin(released: Instant) -> Self {
        TAP.set(Some((NEXT_TAP.fetch_add(1, Ordering::Relaxed), released)));
        point("gesture dispatched");
        Self
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        TAP.set(None);
        PAGE_TURN.set(false);
    }
}

pub fn page_turn() {
    if TAP.get().is_some() {
        PAGE_TURN.set(true);
        point("page turn");
    }
}

pub fn is_page_turn() -> bool {
    PAGE_TURN.get()
}

pub fn point(name: &str) {
    if let Some((id, released)) = TAP.get() {
        eprintln!("Kobo profile tap {id} {name} elapsed_ms={:.2}", released.elapsed().as_secs_f64() * 1000.0);
    }
}

pub struct Stage {
    name: &'static str,
    started: Instant,
}

pub fn stage(name: &'static str) -> Option<Stage> {
    Some(Stage { name, started: Instant::now() })
}

impl Drop for Stage {
    fn drop(&mut self) {
        if let Some((id, released)) = TAP.get() {
            eprintln!("Kobo profile tap {id} {} duration_ms={:.2} elapsed_ms={:.2}",
                      self.name, self.started.elapsed().as_secs_f64() * 1000.0,
                      released.elapsed().as_secs_f64() * 1000.0);
        } else {
            eprintln!("Kobo profile background {:?} {} duration_ms={:.2}",
                      std::thread::current().id(), self.name,
                      self.started.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
