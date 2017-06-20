#![feature(test)]
#![feature(const_fn, drop_types_in_const)]

extern crate test;
extern crate state;
#[macro_use] extern crate lazy_static;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::cell::Cell;

use test::Bencher;

lazy_static! {
    static ref ATOMIC: AtomicUsize = AtomicUsize::new(0);
}

#[bench]
fn container_get(b: &mut Bencher) {
    static STATE: state::Container = state::Container::new();
    STATE.set(AtomicUsize::new(0));

    b.iter(|| {
        STATE.get::<AtomicUsize>().load(Ordering::Relaxed)
    });
}

#[bench]
fn storage_get(b: &mut Bencher) {
    static STORAGE: state::Storage<AtomicUsize> = state::Storage::new();

    STORAGE.set(AtomicUsize::new(0));
    b.iter(|| {
        test::black_box(STORAGE.get().load(Ordering::Relaxed))
    });
}

#[bench]
fn local_storage_get(b: &mut Bencher) {
    static STORAGE: state::LocalStorage<Cell<usize>> = state::LocalStorage::new();

    STORAGE.set(|| Cell::new(0));
    b.iter(|| {
        test::black_box(STORAGE.get().get())
    });
}

#[bench]
fn container_local_get(b: &mut Bencher) {
    static STATE: state::Container = state::Container::new();
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
fn container_get_many_threads(b: &mut Bencher) {
    static STATE: state::Container = state::Container::new();
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
fn container_local_get_many_threads(b: &mut Bencher) {
    static STATE: state::Container = state::Container::new();
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
fn storage_get_many_threads(b: &mut Bencher) {
    static STORAGE: state::Storage<AtomicUsize> = state::Storage::new();

    STORAGE.set(AtomicUsize::new(0));
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                test::black_box(STORAGE.get().load(Ordering::Relaxed))
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

#[bench]
fn local_storage_get_many_threads(b: &mut Bencher) {
    static STORAGE: state::LocalStorage<Cell<usize>> = state::LocalStorage::new();

    STORAGE.set(|| Cell::new(0));
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                test::black_box(STORAGE.get().get())
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

