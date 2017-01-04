#![feature(test)]

extern crate test;
extern crate state;
#[macro_use] extern crate lazy_static;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use test::Bencher;

lazy_static! {
    static ref ATOMIC: AtomicUsize = AtomicUsize::new(0);
}

#[bench]
fn state_get(b: &mut Bencher) {
    state::set(AtomicUsize::new(0));
    b.iter(|| {
        state::get::<AtomicUsize>().load(Ordering::Relaxed)
    });
}

#[bench]
fn state_local_get(b: &mut Bencher) {
    state::set_local(|| AtomicUsize::new(0));
    b.iter(|| {
        state::get_local::<AtomicUsize>().load(Ordering::Relaxed)
    });
}

#[bench]
fn lazy_static_get(b: &mut Bencher) {
    b.iter(|| {
        test::black_box((*ATOMIC).load(Ordering::Relaxed))
    });
}

#[bench]
fn state_get_many_threads(b: &mut Bencher) {
    state::set(AtomicUsize::new(0));
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                state::get::<AtomicUsize>().load(Ordering::Relaxed)
            }));
        }

        threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>()
    });
}

#[bench]
fn state_local_get_many_threads(b: &mut Bencher) {
    state::set_local(|| AtomicUsize::new(0));
    b.iter(|| {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(thread::spawn(|| {
                state::get_local::<AtomicUsize>().load(Ordering::Relaxed)
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

