#![feature(test)]

extern crate test;
extern crate state;
#[macro_use] extern crate lazy_static;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::cell::Cell;

use test::Bencher;

type TypeMap = state::TypeMap![Send + Sync];

lazy_static! {
    static ref ATOMIC: AtomicUsize = AtomicUsize::new(0);
}

#[bench]
fn type_map_get(b: &mut Bencher) {
    static STATE: TypeMap = TypeMap::new();
    STATE.set(AtomicUsize::new(0));

    b.iter(|| {
        STATE.get::<AtomicUsize>().load(Ordering::Relaxed)
    });
}

#[bench]
fn cell_get(b: &mut Bencher) {
    static CELL: state::InitCell<AtomicUsize> = state::InitCell::new();

    CELL.set(AtomicUsize::new(0));
    b.iter(|| {
        test::black_box(CELL.get().load(Ordering::Relaxed))
    });
}

#[bench]
fn local_cell_get(b: &mut Bencher) {
    static CELL: state::LocalInitCell<Cell<usize>> = state::LocalInitCell::new();

    CELL.set(|| Cell::new(0));
    b.iter(|| {
        test::black_box(CELL.get().get())
    });
}

#[bench]
fn type_map_local_get(b: &mut Bencher) {
    static STATE: TypeMap = TypeMap::new();
    STATE.set_local(|| AtomicUsize::new(0));

    b.iter(|| {
        STATE.get_local::<AtomicUsize>().load(Ordering::Relaxed)
    });
}

#[bench]
fn lazy_static_get(b: &mut Bencher) {
    b.iter(|| {
        test::black_box((*ATOMIC).load(Ordering::Relaxed))
    });
}

#[bench]
fn type_map_get_many_threads(b: &mut Bencher) {
    static STATE: TypeMap = TypeMap::new();
    STATE.set(AtomicUsize::new(0));

    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                STATE.get::<AtomicUsize>().load(Ordering::Relaxed)
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

#[bench]
fn type_map_local_get_many_threads(b: &mut Bencher) {
    static STATE: TypeMap = TypeMap::new();
    STATE.set_local(|| AtomicUsize::new(0));

    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                STATE.get_local::<AtomicUsize>().load(Ordering::Relaxed)
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

#[bench]
fn type_map_freeze_many_threads(b: &mut Bencher) {
    use std::sync::Arc;

    let mut state: TypeMap = TypeMap::new();
    state.set(AtomicUsize::new(0));
    state.freeze();

    let state = Arc::new(state);
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            let state = state.clone();
            threads.push(thread::spawn(move || {
                state.get::<AtomicUsize>().load(Ordering::Relaxed)
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

#[bench]
fn lazy_static_get_many_threads(b: &mut Bencher) {
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                test::black_box((*ATOMIC).load(Ordering::Relaxed))
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

#[bench]
fn cell_get_many_threads(b: &mut Bencher) {
    static CELL: state::InitCell<AtomicUsize> = state::InitCell::new();

    CELL.set(AtomicUsize::new(0));
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                test::black_box(CELL.get().load(Ordering::Relaxed))
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

#[bench]
fn local_cell_get_many_threads(b: &mut Bencher) {
    static CELL: state::LocalInitCell<Cell<usize>> = state::LocalInitCell::new();

    CELL.set(|| Cell::new(0));
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                test::black_box(CELL.get().get())
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

