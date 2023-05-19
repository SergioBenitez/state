#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use loom::thread;

#[test]
fn init_exclusive() {
    use state::private::init::Init;

    const THREADS: usize = 2;

    loom::model(|| {
        let init = Arc::new(Init::new());
        let value = Arc::new(AtomicUsize::new(0));

        let mut threads = vec![];
        for _ in 1..(THREADS + 1) {
            let init = init.clone();
            let value = value.clone();
            let thread = thread::spawn(move || {
                if init.needed() {
                    value.fetch_add(1, Relaxed);
                    init.mark_complete();
                }
            });

            threads.push(thread);
        }

        for thread in threads {
            thread.join().unwrap();
        }

        assert_eq!(value.load(Relaxed), 1);
    });
}

#[test]
fn init_completed() {
    use state::private::init::Init;

    const THREADS: usize = 2;

    loom::model(|| {
        let init = Arc::new(Init::new());
        let value = Arc::new(AtomicUsize::new(0));

        let mut threads = vec![];
        for _ in 1..(THREADS + 1) {
            let init = init.clone();
            let value = value.clone();
            let thread = thread::spawn(move || {
                if init.has_completed() {
                    assert_eq!(value.load(Relaxed), 1);
                }

                if init.needed() {
                    value.fetch_add(1, Relaxed);
                    init.mark_complete();
                }
            });

            threads.push(thread);
        }

        for thread in threads {
            thread.join().unwrap();
        }

        assert_eq!(value.load(Relaxed), 1);
    });
}

#[test]
fn cell() {
    use state::InitCell;

    const THREADS: usize = 2;

    loom::model(|| {
        let cell = Arc::new(InitCell::<u8>::new());

        let mut threads = vec![];
        for _ in 1..(THREADS + 1) {
            let cell = cell.clone();
            let thread = thread::spawn(move || {
                cell.set(10);
                assert_eq!(cell.try_get(), Some(&10));
                assert!(!cell.set(20));
                assert_eq!(cell.try_get(), Some(&10));
            });

            threads.push(thread);
        }

        for thread in threads {
            thread.join().unwrap();
        }
    });
}

#[test]
fn type_map1() {
    use state::TypeMap;

    const THREADS: usize = 2;

    loom::model(|| {
        let type_map = Arc::new(<TypeMap![Send + Sync]>::new());

        let mut threads = vec![];
        for _ in 1..(THREADS + 1) {
            let type_map = type_map.clone();
            let thread = thread::spawn(move || {
                assert_eq!(type_map.try_get::<isize>(), None);
                type_map.set::<usize>(10);
                assert_eq!(type_map.try_get::<usize>(), Some(&10));
                assert!(!type_map.set::<usize>(11));
                assert_eq!(type_map.try_get::<usize>(), Some(&10));
            });

            threads.push(thread);
        }

        for thread in threads {
            thread.join().unwrap();
        }

        assert_eq!(type_map.len(), 1);
    });
}

#[test]
fn type_map2() {
    use state::TypeMap;

    const THREADS: usize = 2;

    loom::model(|| {
        let type_map = Arc::new(<TypeMap![Send + Sync]>::new());

        let mut threads = vec![];
        for _ in 1..(THREADS + 1) {
            let type_map = type_map.clone();
            let thread = thread::spawn(move || {
                if type_map.try_get::<Box<usize>>().is_some() {
                    assert!(!type_map.set::<Box<usize>>(Box::new(10)));
                }

                type_map.set::<Box<usize>>(Box::new(10));
                assert_eq!(**type_map.try_get::<Box<usize>>().unwrap(), 10);
            });

            threads.push(thread);
        }

        for thread in threads {
            thread.join().unwrap();
        }

        assert_eq!(type_map.len(), 1);
    });
}
