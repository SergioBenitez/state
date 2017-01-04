#![feature(test)]

extern crate test;
extern crate state;
#[macro_use] extern crate lazy_static;

use std::sync::atomic::{AtomicUsize, Ordering};
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
