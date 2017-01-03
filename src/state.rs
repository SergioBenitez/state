#[cfg(feature = "tls")]
extern crate thread_local;

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::RwLock;

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
    pub map: RwLock<HashMap<TypeId, *mut u8>>,
}

impl State {
    #[inline(always)]
    pub fn set<T: Send + Sync + 'static>(&'static self, state: T) -> bool {
        let type_id = TypeId::of::<T>();

        if self.map.read().unwrap().contains_key(&type_id) {
            return false;
        }

        let state_entry = Box::into_raw(Box::new(state));
        self.map.write().unwrap().insert(type_id, state_entry as *mut u8);

        true
    }

    #[inline(always)]
    pub fn try_get<T: Send + Sync + 'static>(&'static self) -> Option<&'static T> {
        let type_id = TypeId::of::<T>();

        unsafe {
            self.map.read().unwrap().get(&type_id).map(|ptr| &*(*ptr as *mut T))
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
