extern crate state;

use std::thread;
use std::cell::Cell;

#[test]
fn simple_set_get() {
    state::set(1u32);
    assert_eq!(*state::get::<u32>(), 1);
}

#[test]
fn dst_set_get() {
    state::set::<[u32; 4]>([1, 2, 3, 4u32]);
    assert_eq!(*state::get::<[u32; 4]>(), [1, 2, 3, 4]);
}

#[test]
fn set_get_remote() {
    thread::spawn(|| {
        state::set(10isize);
    }).join().unwrap();

    assert_eq!(*state::get::<isize>(), 10);
}

#[test]
fn two_put_get() {
    let s = "Hello, world!".to_string();
    state::set::<String>(s);

    let s_old = state::get::<String>();
    assert_eq!(s_old, "Hello, world!");

    assert!(!state::set::<String>("Bye bye!".into()));
    assert_eq!(state::get::<String>(), "Hello, world!");
    assert_eq!(state::get::<String>(), s_old);
}


use std::sync::{Arc, RwLock};

struct DroppingStruct(Arc<RwLock<bool>>);

impl Drop for DroppingStruct {
    fn drop(&mut self) {
        *self.0.write().unwrap() = true;
    }
}

// Ensure out DroppingStruct works as intended.
#[test]
fn test_dropping_struct() {
    let drop_flag = Arc::new(RwLock::new(false));
    let dropping_struct = DroppingStruct(drop_flag.clone());
    drop(dropping_struct);
    assert_eq!(*drop_flag.read().unwrap(), true);
}

// Ensure setting when already set doesn't cause a drop.
#[test]
fn test_drop_on_replace() {
    let drop_flag = Arc::new(RwLock::new(false));
    let dropping_struct = DroppingStruct(drop_flag.clone());

    let _drop_flag_ignore = Arc::new(RwLock::new(false));
    let _dropping_struct_ignore = DroppingStruct(_drop_flag_ignore.clone());

    state::set::<DroppingStruct>(dropping_struct);
    assert!(!state::set::<DroppingStruct>(_dropping_struct_ignore));
    assert_eq!(*drop_flag.read().unwrap(), false);
}

#[test]
#[cfg(feature = "tls")]
fn test_simple_tls() {
    assert!(state::try_get_local::<u32>().is_none());
    state::set_local(|| 1u32);
    assert_eq!(*state::get_local::<u32>(), 1);
}


#[test]
#[cfg(feature = "tls")]
fn test_double_put_tls() {
    state::set_local(|| 1i32);
    assert!(!state::set_local(|| 1i32));
}

#[test]
#[cfg(feature = "tls")]
fn test_tls_really_is_tls() {
    state::set_local(|| Cell::new(0u32));

    let mut threads = vec![];
    for i in 1..50 {
        threads.push(thread::spawn(move || {
            state::get_local::<Cell<u32>>().set(i);
            assert_eq!(state::get_local::<Cell<u32>>().get(), i);
        }));
    }

    threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>();
    assert_eq!(state::get_local::<Cell<u32>>().get(), 0);
}

#[test]
#[cfg(feature = "tls")]
fn test_tls_really_is_tls_take_2() {
    thread::spawn(|| {
        state::set_local(|| Cell::new(1i8));
        state::get_local::<Cell<i8>>().set(2);

        thread::spawn(|| {
            assert_eq!(state::get_local::<Cell<i8>>().get(), 1);
        }).join().expect("inner join");
    }).join().expect("outer join");
}
