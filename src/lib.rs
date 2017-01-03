mod state;

use std::collections::HashMap;
use std::sync::{Once, ONCE_INIT};
use std::sync::RwLock;

use state::State;

static STATE_INIT: Once = ONCE_INIT;
static mut STATE: *const State = 0 as *const State;

// Initializes the `STATE` global variable. This _MUST_ be called before
// accessing the variable!
#[inline(always)]
unsafe fn ensure_state_initialized() {
    STATE_INIT.call_once(|| {
        STATE = Box::into_raw(Box::new(State {
            map: RwLock::new(HashMap::new()),
        }));
    });
}

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
pub fn set<T: Send + Sync + 'static>(state: T) -> bool {
    unsafe {
        ensure_state_initialized();
        (*STATE).set(state)
    }
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
    unsafe {
        ensure_state_initialized();
        (*STATE).try_get::<T>()
    }
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
pub fn get<T: Send + Sync + 'static>() -> &'static T {
    unsafe {
        ensure_state_initialized();
        (*STATE).try_get::<T>().expect("state:get(): absent type")
    }
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
pub fn set_local<T, F>(state_init: F) -> bool
    where T: Send + 'static,
          F: Fn() -> T + 'static
{
    unsafe {
        ensure_state_initialized();
        (*STATE).set_local::<T, F>(state_init)
    }
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
pub fn try_get_local<T: Send + 'static>() -> Option<&'static T> {
    unsafe {
        ensure_state_initialized();
        (*STATE).try_get_local::<T>()
    }
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
pub fn get_local<T: Send + 'static>() -> &'static T {
    unsafe {
        ensure_state_initialized();
        (*STATE).try_get_local::<T>() .expect("state::get_local(): absent type")
    }
}
