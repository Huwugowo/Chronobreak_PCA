//! Optional stage markers for the recorder's accelerated acquisition benchmark.
use super::Endpoint;
use std::cell::Cell;

type Marker = fn(Option<(&'static str, Option<Endpoint>)>);
thread_local! {
    static MARKER: Cell<Option<Marker>> = const { Cell::new(None) };
}

pub fn install(marker: Option<Marker>) {
    MARKER.set(marker);
}

pub struct Scope(Option<Marker>);

pub fn enter(stage: &'static str, endpoint: Option<Endpoint>) -> Scope {
    let marker = MARKER.get();
    if let Some(mark) = marker {
        mark(Some((stage, endpoint)));
    }
    Scope(marker)
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(mark) = self.0 {
            mark(None);
        }
    }
}
