use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

thread_local! {
    static TAP: Cell<Option<(usize, Instant)>> = const { Cell::new(None) };
    static PAGE_TURN: Cell<bool> = const { Cell::new(false) };
}
static NEXT_TAP: AtomicUsize = AtomicUsize::new(1);

// Preserve the existing device-specific trace labels with one shared profiler.
#[cfg(all(target_arch = "arm", target_env = "musl"))]
pub const LABEL: &str = "Voyage";
#[cfg(not(all(target_arch = "arm", target_env = "musl")))]
pub const LABEL: &str = "Kobo profile";

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
        eprintln!("{LABEL} tap {id} {name} elapsed_ms={:.2}", released.elapsed().as_secs_f64() * 1000.0);
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
            eprintln!("{LABEL} tap {id} {} duration_ms={:.2} elapsed_ms={:.2}",
                      self.name, self.started.elapsed().as_secs_f64() * 1000.0,
                      released.elapsed().as_secs_f64() * 1000.0);
        } else {
            eprintln!("{LABEL} background {:?} {} duration_ms={:.2}",
                      std::thread::current().id(), self.name,
                      self.started.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_turn_is_scoped_to_a_tap() {
        page_turn();
        assert!(!is_page_turn());
        {
            let _tap = Tap::begin(Instant::now());
            page_turn();
            assert!(is_page_turn());
        }
        assert!(!is_page_turn());
        assert!(TAP.get().is_none());
        let _next_tap = Tap::begin(Instant::now());
        assert!(!is_page_turn());
    }

    #[test]
    fn prefetch_thread_does_not_inherit_page_turn() {
        let _tap = Tap::begin(Instant::now());
        page_turn();
        std::thread::spawn(|| {
            assert!(TAP.get().is_none());
            assert!(!is_page_turn());
            page_turn();
            assert!(!is_page_turn());
        }).join().unwrap();
        assert!(is_page_turn());
    }
}
