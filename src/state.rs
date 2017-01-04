#[cfg(feature = "tls")] extern crate thread_local;

use std::any::TypeId;
use std::cell::UnsafeCell;
use std::hash::BuildHasherDefault;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

use ident_hash::IdentHash;

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

pub struct State {
    map: UnsafeCell<HashMap<TypeId, *mut u8, BuildHasherDefault<IdentHash>>>,
    mutex: AtomicUsize,
}

impl State {
    pub fn new() -> State {
        State {
            map: UnsafeCell::new(HashMap::<_, _, _>::default()),
            mutex: AtomicUsize::new(0)
        }
    }

    #[inline(always)]
    fn lock(&self) {
        while self.mutex.compare_and_swap(0, 1, Ordering::SeqCst) != 0 {}
    }

    #[inline(always)]
    fn unlock(&self) {
        assert!(self.mutex.compare_and_swap(1, 0, Ordering::SeqCst) == 1);
    }

    #[inline(always)]
    pub fn set<T: Send + Sync + 'static>(&'static self, state: T) -> bool {
        let type_id = TypeId::of::<T>();

        unsafe {
            self.lock();
            let already_set = (*self.map.get()).contains_key(&type_id);
            if !already_set {
                let state_entry = Box::into_raw(Box::new(state));
                (*self.map.get()).insert(type_id, state_entry as *mut u8);
            }

            self.unlock();
            !already_set
        }
    }

    #[inline(always)]
    pub fn try_get<T: Send + Sync + 'static>(&'static self) -> Option<&'static T> {
        let type_id = TypeId::of::<T>();

        unsafe {
            self.lock();
            let item = (*self.map.get()).get(&type_id);
            self.unlock();
            item.map(|ptr| &*(*ptr as *mut T))
        }
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
