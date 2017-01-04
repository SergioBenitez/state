#[cfg(feature = "tls")] extern crate thread_local;
extern crate seahash;

use std::any::TypeId;
use std::cell::UnsafeCell;
use std::hash::BuildHasherDefault;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

use self::seahash::SeaHasher;

#[cfg(feature = "tls")]
use self::thread_local::ThreadLocal;

#[cfg(feature = "tls")]
struct LocalValue<T: Send + 'static> {
    tls: ThreadLocal<T>,
    init_fn: Box<Fn() -> T>,
}

#[cfg(feature = "tls")]
impl<T: Send + 'static> LocalValue<T> {
    pub fn new<F: Fn() -> T + 'static>(init_fn: F) -> LocalValue<T> {
        LocalValue {
            tls: ThreadLocal::new(),
            init_fn: Box::new(init_fn),
        }
    }

    pub fn get<'r>(&'r self) -> &'r T {
        self.tls.get_or(|| Box::new((self.init_fn)()))
    }
}

#[cfg(feature = "tls")]
unsafe impl<T: Send + 'static> Sync for LocalValue<T> {}

#[cfg(feature = "tls")]
unsafe impl<T: Send + 'static> Send for LocalValue<T> {}

const IDLE: usize = 0;
const SETTING: usize = 1;
const GETTING: usize = 2;

pub struct State {
    pub map: UnsafeCell<HashMap<TypeId, *mut u8, BuildHasherDefault<SeaHasher>>>,
    pub state: AtomicUsize,
}

impl State {
    pub fn new() -> State {
        State {
            map: UnsafeCell::new(HashMap::<_, _, _>::default()),
            state: AtomicUsize::new(IDLE)
        }
    }

    #[inline(always)]
    fn change_state(&'static self, from: usize, to: usize) -> bool {
        self.state.compare_and_swap(from, to, Ordering::SeqCst) == from
    }

    #[inline(always)]
    pub fn set<T: Send + Sync + 'static>(&'static self, state: T) -> bool {
        // Wait until we're idle.
        while !self.change_state(IDLE, SETTING) {  }

        let type_id = TypeId::of::<T>();
        let result = unsafe {
            if (*self.map.get()).contains_key(&type_id) {
                false
            } else {
                let state_entry = Box::into_raw(Box::new(state));
                (*self.map.get()).insert(type_id, state_entry as *mut u8);
                true
            }
        };

        assert!(self.change_state(SETTING, IDLE));
        result
    }

    #[inline(always)]
    pub fn try_get<T: Send + Sync + 'static>(&'static self) -> Option<&'static T> {
        // Wait until we're idle.
        // FIXME: Allow concurrent gets!
        while !self.change_state(IDLE, GETTING) {  }

        let type_id = TypeId::of::<T>();
        let res = unsafe {
            (*self.map.get()).get(&type_id).map(|ptr| &*(*ptr as *mut T))
        };

        assert!(self.change_state(GETTING, IDLE));
        res
    }

    #[cfg(feature = "tls")]
    #[inline(always)]
    pub fn set_local<T, F>(&'static self, state_init: F) -> bool
        where T: Send + 'static,
              F: Fn() -> T + 'static
    {
        self.set::<LocalValue<T>>(LocalValue::new(state_init))
    }

    #[cfg(feature = "tls")]
    #[inline(always)]
    pub fn try_get_local<T: Send + 'static>(&'static self) -> Option<&'static T> {
        // TODO: This will take a lock on the HashMap unnecessarily. Ideally
        // we'd have a `HashMap` per thread mapping from TypeId to (T, F).
        self.try_get::<LocalValue<T>>().map(|value| value.get())
    }
}
