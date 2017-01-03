#![feature(const_fn)]
// FIXME: ^

#[cfg(feature = "tls")]
extern crate thread_local;

#[cfg(feature = "tls")]
use thread_local::ThreadLocal;

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Once, ONCE_INIT};
use std::cell::UnsafeCell;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, ATOMIC_BOOL_INIT, Ordering};

#[cfg(feature = "tls")]
struct LocalValue<T: Send + 'static> {
    tls: ThreadLocal<T>,
    init_fn: Box<Fn() -> T>
}

#[cfg(feature = "tls")]
impl<T: Send + 'static> LocalValue<T> {
    pub fn new<F: Fn() -> T + 'static>(init_fn: F) -> LocalValue<T> {
        LocalValue {
            tls: ThreadLocal::new(),
            init_fn: Box::new(init_fn)
        }
    }

    pub fn get<'r>(&'r self) -> &'r T {
        self.tls.get_or(|| Box::new((self.init_fn)()))
    }
}

#[cfg(feature = "tls")]
unsafe impl<T: Send + 'static> Sync for LocalValue<T> {  }

#[cfg(feature = "tls")]
unsafe impl<T: Send + 'static> Send for LocalValue<T> {  }

static STATE_INIT: Once = ONCE_INIT;

struct State {
    initialized: AtomicBool,
    map: UnsafeCell<*const RwLock<HashMap<TypeId, *mut u8>>>
}

impl State {
    #[inline(always)]
    pub fn ensure_initialized(&'static self) {
        // This _should_ be a _faster_ fast path.
        if self.initialized.load(Ordering::Relaxed) { return; }

        unsafe {
            STATE_INIT.call_once(|| {
                *self.map.get() = Box::into_raw(Box::new(RwLock::new(HashMap::new())));
            });
        }

        self.initialized.store(true, Ordering::Relaxed);
    }

    pub fn set<T: Send + Sync + 'static>(&'static self, state: T) -> bool {
        self.ensure_initialized();
        let type_id = TypeId::of::<T>();

        unsafe {
            if (**self.map.get()).read().unwrap().contains_key(&type_id) {
                return false;
            }

            let state_entry = Box::into_raw(Box::new(state));
            (**self.map.get()).write().unwrap().insert(type_id, state_entry as *mut u8);
        }

        true
    }

    pub fn try_get<T: Send + Sync + 'static>(&'static self) -> Option<&'static T> {
        self.ensure_initialized();
        let type_id = TypeId::of::<T>();

        unsafe {
            (**self.map.get()).read().unwrap().get(&type_id) .map(|ptr| &*(*ptr as *mut T))
        }
    }

    #[cfg(feature = "tls")]
    pub fn set_local<T, F>(&'static self, state_init: F) -> bool
        where T: Send + 'static, F: Fn() -> T + 'static
    {
        self.set::<LocalValue<T>>(LocalValue::new(state_init))
    }

    #[cfg(feature = "tls")]
    pub fn try_get_local<T: Send + 'static>(&'static self) -> Option<&'static T> {
        // TODO: This will take a lock on the HashMap unnecessarily. Ideally
        // we'd have a `HashMap` per thread mapping from TypeId to (T, F).
        self.try_get::<LocalValue<T>>().map(|value| value.get())
    }
}

static mut STATE: State = State {
    initialized: ATOMIC_BOOL_INIT,
    map: UnsafeCell::new(0 as *mut RwLock<_>)
};

/// Sets the global state for type `T` if it has not been set before.
///
/// If the state for `T` has previously been set, the state is unchanged and
/// `false` is returned. Returns `true` if `state` is successfully set as the
/// state for `T`.
///
/// # Example
///
/// ```rust
/// use std::sync::atomic::AtomicUsize;
///
/// struct MyState(AtomicUsize);
///
/// assert_eq!(state::set(MyState(AtomicUsize::new(0))), true);
/// assert_eq!(state::set(MyState(AtomicUsize::new(1))), false);
/// ```
#[inline(always)]
pub fn set<T: Send + Sync + 'static>(state: T) -> bool {
    unsafe { STATE.set(state) }
}

/// Attempts to retrieve the global state for type `T`.
///
/// Returns `Some` if the state has previously been [set](fn.set.html).
/// Otherwise returns `None`.
///
/// # Example
///
/// ```rust
/// use std::sync::atomic::{AtomicUsize, Ordering};
///
/// struct MyState(AtomicUsize);
///
/// // State for `T` is initially unset.
/// assert!(state::try_get::<MyState>().is_none());
///
/// state::set(MyState(AtomicUsize::new(0)));
/// let my_state = state::try_get::<MyState>().expect("MyState");
/// assert_eq!(my_state.0.load(Ordering::Relaxed), 0);
/// ```
#[inline(always)]
pub fn try_get<T: Send + Sync + 'static>() -> Option<&'static T> {
    unsafe { STATE.try_get::<T>() }
}

/// Retrieves the global state for type `T`.
///
/// # Panics
///
/// Panics if the state for type `T` has not previously been [set](fn.set.html).
/// Use [try_get](fn.try_get.html) for a non-panicking version.
///
/// # Example
///
/// ```rust
/// use std::sync::atomic::{AtomicUsize, Ordering};
///
/// struct MyState(AtomicUsize);
///
/// state::set(MyState(AtomicUsize::new(0)));
/// let my_state = state::get::<MyState>();
/// assert_eq!(my_state.0.load(Ordering::Relaxed), 0);
/// ```
#[inline(always)]
pub fn get<T: Send + Sync + 'static>() -> &'static T {
    unsafe { STATE.try_get::<T>().expect("state:get(): value is not present") }
}

/// Sets the thread-local state for type `T` if it has not been set before.
///
/// The state for type `T` will be initialized via the `state_init` function as
/// needed. If the state for `T` has previously been set, the state is unchanged
/// and `false` is returned. Returns `true` if the thread-local state is
/// successfully set to be initialized with `state_init`.
///
/// # Example
///
/// ```rust
/// use std::cell::Cell;
///
/// struct MyState(Cell<usize>);
///
/// assert_eq!(state::set_local(|| MyState(Cell::new(1))), true);
/// assert_eq!(state::set_local(|| MyState(Cell::new(2))), false);
/// ```
#[cfg(feature = "tls")]
#[inline(always)]
pub fn set_local<T, F>(state_init: F) -> bool
    where T: Send + 'static, F: Fn() -> T + 'static
{
    unsafe { STATE.set_local::<T, F>(state_init) }
}

/// Attempts to retrieve the thread-local state for type `T`.
///
/// Returns `Some` if the state has previously been set via
/// [set_local](fn.set_local.html). Otherwise returns `None`.
///
/// # Example
///
/// ```rust
/// use std::cell::Cell;
///
/// struct MyState(Cell<usize>);
///
/// state::set_local(|| MyState(Cell::new(10)));
///
/// let my_state = state::try_get_local::<MyState>().expect("MyState");
/// assert_eq!(my_state.0.get(), 10);
/// ```
#[cfg(feature = "tls")]
#[inline(always)]
pub fn try_get_local<T: Send + 'static>() -> Option<&'static T> {
    unsafe { STATE.try_get_local::<T>() }
}

/// Retrieves the thread-local state for type `T`.
///
/// # Panics
///
/// Panics if the thread-local state for type `T` has not previously been set
/// via [set_local](fn.set_local.html). Use
/// [try_get_local](fn.try_get_local.html) for a non-panicking version.
///
/// # Example
///
/// ```rust
/// use std::cell::Cell;
///
/// struct MyState(Cell<usize>);
///
/// state::set_local(|| MyState(Cell::new(10)));
///
/// let my_state = state::get_local::<MyState>();
/// assert_eq!(my_state.0.get(), 10);
/// ```
#[cfg(feature = "tls")]
#[inline(always)]
pub fn get_local<T: Send + 'static>() -> &'static T {
    unsafe { STATE.try_get_local::<T>().expect("state::get_local(): value is not present") }
}
