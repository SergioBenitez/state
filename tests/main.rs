extern crate state;

use std::sync::{Arc, RwLock};

// Tiny structures to test that dropping works as expected.
struct DroppingStruct(Arc<RwLock<bool>>);
struct DroppingStructWrap(DroppingStruct);

impl Drop for DroppingStruct {
    fn drop(&mut self) {
        *self.0.write().unwrap() = true;
    }
}

// Ensure our DroppingStruct works as intended.
#[test]
fn test_dropping_struct() {
    let drop_flag = Arc::new(RwLock::new(false));
    let dropping_struct = DroppingStruct(drop_flag.clone());
    drop(dropping_struct);
    assert_eq!(*drop_flag.read().unwrap(), true);

    let drop_flag = Arc::new(RwLock::new(false));
    let dropping_struct = DroppingStruct(drop_flag.clone());
    let wrapper = DroppingStructWrap(dropping_struct);
    drop(wrapper);
    assert_eq!(*drop_flag.read().unwrap(), true);
}

mod container_tests {
    use super::{DroppingStruct, DroppingStructWrap};
    use super::state::Container;
    use std::sync::{Arc, RwLock};
    use std::thread;

    // We use one `CONTAINER` to get an implicit test since each `test` runs in
    // a different thread. This means we have to `set` different types in each
    // test if we want the `set` to succeed.
    static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();

    #[test]
    fn simple_set_get() {
        assert!(CONTAINER.set(1u32));
        assert_eq!(*CONTAINER.get::<u32>(), 1);
    }

    #[test]
    fn dst_set_get() {
        assert!(CONTAINER.set::<[u32; 4]>([1, 2, 3, 4u32]));
        assert_eq!(*CONTAINER.get::<[u32; 4]>(), [1, 2, 3, 4]);
    }

    #[test]
    fn set_get_remote() {
        thread::spawn(|| {
            CONTAINER.set(10isize);
        }).join().unwrap();

        assert_eq!(*CONTAINER.get::<isize>(), 10);
    }

    #[test]
    fn two_put_get() {
        assert!(CONTAINER.set("Hello, world!".to_string()));

        let s_old = CONTAINER.get::<String>();
        assert_eq!(s_old, "Hello, world!");

        assert!(!CONTAINER.set::<String>("Bye bye!".into()));
        assert_eq!(CONTAINER.get::<String>(), "Hello, world!");
        assert_eq!(CONTAINER.get::<String>(), s_old);
    }

    #[test]
    fn many_puts_only_one_succeeds() {
        let mut threads = vec![];
        for _ in 0..1000 {
            threads.push(thread::spawn(|| {
                CONTAINER.set(10i64)
            }))
        }

        let results: Vec<bool> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.into_iter().filter(|&b| b).count(), 1);
        assert_eq!(*CONTAINER.get::<i64>(), 10);
    }

    // Ensure setting when already set doesn't cause a drop.
    #[test]
    fn test_no_drop_on_set() {
        let drop_flag = Arc::new(RwLock::new(false));
        let dropping_struct = DroppingStruct(drop_flag.clone());

        let _drop_flag_ignore = Arc::new(RwLock::new(false));
        let _dropping_struct_ignore = DroppingStruct(_drop_flag_ignore.clone());

        CONTAINER.set::<DroppingStruct>(dropping_struct);
        assert!(!CONTAINER.set::<DroppingStruct>(_dropping_struct_ignore));
        assert_eq!(*drop_flag.read().unwrap(), false);
    }

    // Ensure dropping a container drops its contents.
    #[test]
    fn drop_inners_on_drop() {
        let drop_flag_a = Arc::new(RwLock::new(false));
        let dropping_struct_a = DroppingStruct(drop_flag_a.clone());

        let drop_flag_b = Arc::new(RwLock::new(false));
        let dropping_struct_b = DroppingStructWrap(DroppingStruct(drop_flag_b.clone()));

        {
            let container = <Container![Send + Sync]>::new();
            container.set(dropping_struct_a);
            assert_eq!(*drop_flag_a.read().unwrap(), false);

            container.set(dropping_struct_b);
            assert_eq!(*drop_flag_a.read().unwrap(), false);
            assert_eq!(*drop_flag_b.read().unwrap(), false);
        }

        assert_eq!(*drop_flag_a.read().unwrap(), true);
        assert_eq!(*drop_flag_b.read().unwrap(), true);
    }
}

#[cfg(feature = "tls")]
mod container_tests_tls {
    use std::sync::{Arc, Barrier};
    use super::state::Container;
    use std::cell::Cell;
    use std::thread;

    // We use one `CONTAINER` to get an implicit test since each `test` runs in
    // a different thread. This means we have to `set` different types in each
    // test if we want the `set` to succeed.
    static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();

    #[test]
    fn test_simple() {
        assert!(CONTAINER.try_get_local::<u32>().is_none());
        assert!(CONTAINER.set_local(|| 1u32));
        assert_eq!(*CONTAINER.get_local::<u32>(), 1);
    }

    #[test]
    fn test_double_put() {
        assert!(CONTAINER.set_local(|| 1i32));
        assert!(!CONTAINER.set_local(|| 1i32));
    }

    #[test]
    fn not_unique_when_sent() {
        assert!(CONTAINER.set_local(|| 1i64));
        let value = CONTAINER.get_local::<i64>();

        thread::spawn(move || {
            assert_eq!(*value, 1i64);
        }).join().expect("Panic.");
    }

    #[test]
    fn container_tls_really_is_tls() {
        const THREADS: usize = 50;

        let barriers = Arc::new((Barrier::new(THREADS), Barrier::new(THREADS)));
        assert!(CONTAINER.set_local(|| Cell::new(0u8)));

        let mut threads = vec![];
        for i in 1..THREADS {
            let barriers = barriers.clone();
            threads.push(thread::spawn(move || {
                barriers.0.wait();
                assert_eq!(CONTAINER.get_local::<Cell<u8>>().get(), 0);
                CONTAINER.get_local::<Cell<u8>>().set(i as u8);
                let v = CONTAINER.get_local::<Cell<u8>>().get();
                barriers.1.wait();
                v
            }));
        }

        barriers.0.wait();
        CONTAINER.get_local::<Cell<u8>>().set(0);
        barriers.1.wait();

        let vals = threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>();
        for (i, val) in vals.into_iter().enumerate() {
            assert_eq!((i + 1) as u8, val);
        }

        assert_eq!(CONTAINER.get_local::<Cell<u8>>().get(), 0);
    }

    #[test]
    fn container_tls_really_is_tls_take_2() {
        thread::spawn(|| {
            assert!(CONTAINER.set_local(|| Cell::new(1i8)));
            CONTAINER.get_local::<Cell<i8>>().set(2);

            thread::spawn(|| {
                assert_eq!(CONTAINER.get_local::<Cell<i8>>().get(), 1);
            }).join().expect("inner join");
        }).join().expect("outer join");
    }
}

mod cell_tests {
    use super::DroppingStruct;
    use super::state::InitCell;
    use std::sync::{Arc, RwLock};
    use std::thread;

    #[test]
    fn simple_put_get() {
        static CELL: InitCell<u32> = InitCell::new();

        assert!(CELL.set(10));
        assert_eq!(*CELL.get(), 10);
    }

    #[test]
    fn no_double_put() {
        static CELL: InitCell<u32> = InitCell::new();

        assert!(CELL.set(1));
        assert!(!CELL.set(5));
        assert_eq!(*CELL.get(), 1);
    }

    #[test]
    fn many_puts_only_one_succeeds() {
        static CELL: InitCell<u32> = InitCell::new();

        let mut threads = vec![];
        for _ in 0..1000 {
            threads.push(thread::spawn(|| {
                let was_set = CELL.set(10);
                assert_eq!(*CELL.get(), 10);
                was_set
            }))
        }

        let results: Vec<bool> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.into_iter().filter(|&b| b).count(), 1);
        assert_eq!(*CELL.get(), 10);
    }

    #[test]
    fn dst_set_get() {
        static CELL: InitCell<[u32; 4]> = InitCell::new();

        assert!(CELL.set([1, 2, 3, 4]));
        assert_eq!(*CELL.get(), [1, 2, 3, 4]);
    }

    // Ensure dropping a `InitCell<T>` drops `T`.
    #[test]
    fn drop_inners_on_drop() {
        let drop_flag = Arc::new(RwLock::new(false));
        let dropping_struct = DroppingStruct(drop_flag.clone());

        {
            let cell = InitCell::new();
            assert!(cell.set(dropping_struct));
            assert_eq!(*drop_flag.read().unwrap(), false);
        }

        assert_eq!(*drop_flag.read().unwrap(), true);
    }

    #[test]
    fn clone() {
        let cell: InitCell<u32> = InitCell::new();
        assert!(cell.try_get().is_none());

        let cell_clone = cell.clone();
        assert!(cell_clone.try_get().is_none());

        assert!(cell.set(10));
        let cell_clone = cell.clone();
        assert_eq!(*cell_clone.get(), 10);
    }
}

#[cfg(feature = "tls")]
mod cell_tests_tls {
    use super::state::LocalInitCell;

    use std::thread;
    use std::cell::Cell;
    use std::sync::{Arc, Barrier};

    #[test]
    fn simple_put_get() {
        static CELL: LocalInitCell<u32> = LocalInitCell::new();

        assert!(CELL.set(|| 10));
        assert_eq!(*CELL.get(), 10);
    }

    #[test]
    fn no_double_put() {
        static CELL: LocalInitCell<u32> = LocalInitCell::new();

        assert!(CELL.set(|| 1));
        assert!(!CELL.set(|| 5));
        assert_eq!(*CELL.get(), 1);
    }

    #[test]
    fn many_puts_only_one_succeeds() {
        static CELL: LocalInitCell<u32> = LocalInitCell::new();

        let mut threads = vec![];
        for _ in 0..1000 {
            threads.push(thread::spawn(|| {
                let was_set = CELL.set(|| 10);
                assert_eq!(*CELL.get(), 10);
                was_set
            }))
        }

        let results: Vec<bool> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.into_iter().filter(|&b| b).count(), 1);
        assert_eq!(*CELL.get(), 10);
    }

    #[test]
    fn cell_tls_really_is_tls() {
        const THREADS: usize = 50;
        static CELL: LocalInitCell<Cell<u8>> = LocalInitCell::new();

        let barriers = Arc::new((Barrier::new(THREADS), Barrier::new(THREADS)));
        assert!(CELL.set(|| Cell::new(0)));

        let mut threads = vec![];
        for i in 1..50 {
            let barriers = barriers.clone();
            threads.push(thread::spawn(move || {
                barriers.0.wait();
                CELL.get().set(i);
                let val = CELL.get().get();
                barriers.1.wait();
                val
            }));
        }

        barriers.0.wait();
        CELL.get().set(0);
        barriers.1.wait();

        let vals = threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>();
        for (i, val) in vals.into_iter().enumerate() {
            assert_eq!((i + 1) as u8, val);
        }

        assert_eq!(CELL.get().get(), 0);
    }

    #[test]
    fn cell_tls_really_is_tls_take_2() {
        static CELL: LocalInitCell<Cell<u8>> = LocalInitCell::new();

        thread::spawn(|| {
            assert!(CELL.set(|| Cell::new(1)));
            CELL.get().set(2);

            thread::spawn(|| {
                assert_eq!(CELL.get().get(), 1);
            }).join().expect("inner join");
        }).join().expect("outer join");
    }
}
