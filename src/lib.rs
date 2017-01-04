//! # state -- safe and effortless state management
//!
//! This crate allows you to safely and effortlessly manage global and/or
//! thread-local state through `set` and `get` methods. State is set and
//! retrieved in a type-directed fashion: `state` allows _one_ instance of a
//! given type to be stored in global storage as well as _n_ instances of a
//! given type to be stored in _n_ thread-local-storage slots. This makes
//! `state` ideal for singleton instances, global configuration, and
//! singly-initialized state.
//!
//! ## Usage
//!
//! Include `state` in your `Cargo.toml` `[dependencies]`:
//!
//! ```toml
//! [dependencies]
//! state = "0.1"
//! ```
//!
//! Thread-local state management is not enabled by default. You can enable it
//! via the "tls" feature:
//!
//! ```toml
//! [dependencies]
//! state = { version = "0.1", features = ["tls"] }
//! ```
//!
//! ## Global State
//!
//! Global state is set via the [set](fn.set.html) function and retrieved via
//! the [get](fn.get.html) function. The type of the value being set must be
//! thread-safe and transferrable across thread boundaries. In other words, it
//! must satisfy `Sync + Send + 'static`.
//!
//! ### Example
//!
//! Set and later retrieve a value of type T:
//!
//! ```rust
//! # struct T;
//! # impl T { fn new() -> T { T } }
//! state::set(T::new());
//! state::get::<T>();
//! ```
//!
//! ## Thread-Local State
//!
//! Thread-local state is set via the [set_local](fn.set_local.html) function
//! and retrieved via the [get_local](fn.get_local.html) function. The type of
//! the value being set must be transferrable across thread boundaries but need
//! not be thread-safe. In other words, it must satisfy `Send + 'static` but not
//! necessarily `Sync`. Values retrieved from thread-local state are exactly
//! that: local to the current thread. As such, you cannot use thread-local
//! state to synchronize across multiple threads.
//!
//! Thread-local state is initialized on an as-needed basis. The function used
//! to initialize the thread-local state is passed in as an argument to
//! `set_local`. When the state is retrieved from a thread for the first time,
//! the function is executed to generate the initial value. The function is
//! executed at most once per thread. The same function is used for
//! initialization across all threads.
//!
//! **Note:** Rust reuses thread IDs across multiple threads. This means that is
//! possible to set thread-local state in thread A, have that thread die, start
//! a new thread B, and access the state set in A in B.
//!
//! ### Example
//!
//! Set and later retrieve a value of type T:
//!
//! ```rust
//! # struct T;
//! # impl T { fn new() -> T { T } }
//! state::set_local(|| T::new());
//! state::get_local::<T>();
//! ```
//!
//! ## Use Cases
//!
//! `state` is an optimal solution in several scenarios.
//!
//! ### Singleton
//!
//! Suppose you have the following structure which is initialized in `main`
//! after receiving input from the user:
//!
//! ```rust
//! struct Configuration {
//!     name: String,
//!     number: isize,
//!     verbose: bool
//! }
//!
//! fn main() {
//!     let config = Configuration {
//!         /* fill in structure at run-time from user input */
//! #        name: "Sergio".to_string(),
//! #        number: 1,
//! #        verbose: true
//!     };
//! }
//! ```
//!
//! You'd like to access this structure later, at any point in the program.
//! Prior to `state`, assuming you needed to setup the structure after program
//! start, your options were:
//!
//!   1. Use a `static mut` and `unsafe` to set an `Option<Configuration>` to
//!      `Some`. Retrieve by checking for `Some`.
//!   2. Use `lazy_static` with a `RwLock` to set an `Option<Configuration>`
//!      to `Some`. Retrieve by `lock`ing and checking for `Some`.
//!
//! With `state`, you simply call `state::set` and `state::get`, as follows:
//!
//! ```rust
//! # struct Configuration { name: String, number: isize, verbose: bool }
//!
//! fn main() {
//!     let config = Configuration {
//!         /* fill in structure at run-time from user input */
//! #        name: "Sergio".to_string(),
//! #        number: 1,
//! #        verbose: true
//!     };
//!
//!     // Make the config avaiable globally.
//!     state::set(config);
//!
//!     /* at any point later in the program */
//!     let config = state::get::<Configuration>();
//! }
//! ```
//!
//! ### Mutable, thread-local data
//!
//! It is entirely safe to have an unsynchronized global object, as long as that
//! object is accessible to a single thread at a time: the standard library's
//! `thread_local!` macro allows this behavior to be encapsulated. `state`
//! provides another, arguably simpler solution.
//!
//! Say you want to count the number of invocations to a function per thread.
//! You store the invocations in a `struct InvokeCount(Cell<usize>)` and use
//! `invoke_count.0.set(invoke_count.0.get() + 1)` to increment the count. The
//! following implements this using `state`:
//!
//! ```rust
//! use std::cell::Cell;
//! use std::thread;
//!
//! struct InvokeCount(Cell<usize>);
//!
//! fn function_to_measure() {
//!     let count = state::get_local::<InvokeCount>();
//!     count.0.set(count.0.get() + 1);
//! }
//!
//! fn main() {
//!     // setup the initializer for thread-local state
//!     state::set_local(|| InvokeCount(Cell::new(0)));
//!
//!     // spin up many threads that call `function_to_measure`.
//!     let mut threads = vec![];
//!     for i in 0..10 {
//!         threads.push(thread::spawn(|| {
//!             function_to_measure();
//!             state::get_local::<InvokeCount>().0.get()
//!         }));
//!     }
//!
//!     // retrieve the thread-local counts
//!     let counts: Vec<usize> = threads.into_iter()
//!         .map(|t| t.join().unwrap())
//!         .collect();
//! }
//! ```
//!
//! ## Performance
//!
//! `state` is heavily tuned to perform near-optimally when there are many
//! threads. On average, `state` performs slightly worse than `lazy_static` when
//! only a _single_ thread is used to access a global variable, and slightly
//! better than `lazy_static` when _many_ threads are used to access a global
//! variable. Keep in mind that `state` allows global initialization at _any_
//! point in the program, while `lazy_static` initialization must be declared
//! apriori.
//!
mod state;
mod ident_hash;

use std::sync::{Once, ONCE_INIT};

use state::State;

static STATE_INIT: Once = ONCE_INIT;
static mut STATE: *const State = 0 as *const State;

// Initializes the `STATE` global variable. This _MUST_ be called before
// accessing the variable!
#[inline(always)]
unsafe fn ensure_state_initialized() {
    STATE_INIT.call_once(|| {
        STATE = Box::into_raw(Box::new(State::new()));
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
///
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
///
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
