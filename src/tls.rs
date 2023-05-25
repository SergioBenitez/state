use std::fmt;

use crate::thread_local::ThreadLocal;
use crate::cell::InitCell;

pub struct LocalValue<T> {
    tls: ThreadLocal<T>,
    init_fn: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T: Send + 'static> LocalValue<T> {
    pub fn new<F: Fn() -> T>(init_fn: F) -> LocalValue<T>
        where F: Send + Sync + 'static
    {
        LocalValue {
            tls: ThreadLocal::new(),
            init_fn: Box::new(init_fn),
        }
    }

    pub fn get(&self) -> &T {
        self.tls.get_or(|| (self.init_fn)())
    }

    pub fn try_get(&self) -> Option<&T> {
        self.tls.get_or_try(|| Ok::<T, ()>((self.init_fn)()))
            .and_then(|res| res.ok())
    }
}

/// A thread-local init-once-per-thread cell for thread-local values.
///
/// A `LocalInitCell` instance allows global access to a `n` per-thread values,
/// all of which are initialized in the same manner when the value is first
/// retrieved from a thread.
//
/// The initialization function for values in `LocalInitCell` is specified via
/// the [set](#method.set) method. The initialization function must be set
/// before a value is attempted to be retrieved via the [get](#method.get)
/// method. The [try_get](#method.try_get) can be used to determine whether the
/// `LocalInitCell` has been initialized before attempting to retrieve a value.
///
/// For safety reasons, values stored in `LocalInitCell` must be `Send +
/// 'static`.
///
/// # Comparison with `InitCell`
///
/// When the use-case allows, there are two primary advantages to using a
/// `LocalInitCell` instance over a `InitCell` instance:
///
///   * Values stored in `LocalInitCell` do not need to implement `Sync`.
///   * There is no synchronization overhead when setting a value.
///
/// The primary disadvantages are:
///
///   * Values are recomputed once per thread on `get()` where `InitCell` never
///     recomputes values.
///   * Values need to be `'static` where `InitCell` imposes no such restriction.
///
/// Values `LocalInitCell` are _not_ the same across different threads. Any
/// modifications made to the stored value in one thread are _not_ visible in
/// another. Furthermore, because Rust reuses thread IDs, a new thread is _not_
/// guaranteed to receive a newly initialized value on its first call to `get`.
///
/// # Usage
///
/// **This type is only available when the `"tls"` feature is enabled.** To
/// enable the feature, include the `state` dependency in your `Cargo.toml` as
/// follows:
///
/// ```toml
/// [dependencies]
/// state = { version = "0.6.0", features = ["tls"] }
/// ```
///
/// # Example
///
/// The following example uses `LocalInitCell` to store a per-thread count:
///
/// ```rust
/// # extern crate state;
/// # use std::cell::Cell;
/// # use std::thread;
/// # use state::LocalInitCell;
/// static COUNT: LocalInitCell<Cell<usize>> = LocalInitCell::new();
///
/// fn check_count() {
///     let count = COUNT.get();
///
///     // initialize the state, in case we reuse thread IDs
///     count.set(0);
///
///     // increment it, non-atomically
///     count.set(count.get() + 1);
///
///     // The count should always be 1 since the state is thread-local.
///     assert_eq!(count.get(), 1);
/// }
///
/// fn main() {
///     // setup the initializer for thread-local state
///     COUNT.set(|| Cell::new(0));
///
///     // spin up many threads that call `check_count`.
///     let mut threads = vec![];
///     for i in 0..10 {
///         threads.push(thread::spawn(|| check_count()));
///     }
///
///     // Wait for all of the thread to finish.
///     for thread in threads {
///         thread.join().expect("correct count");
///     }
/// }
/// ```
pub struct LocalInitCell<T> {
    cell: InitCell<LocalValue<T>>
}

impl<T> LocalInitCell<T> {
    /// Create a new, uninitialized cell.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::LocalInitCell;
    ///
    /// static MY_GLOBAL: LocalInitCell<String> = LocalInitCell::new();
    /// ```
    pub const fn new() -> LocalInitCell<T> {
        LocalInitCell { cell: InitCell::new() }
    }
}

impl<T: Send + 'static> LocalInitCell<T> {
    /// Sets the initialization function for this local cell to
    /// `state_init` if it has not already been set before. The function will be
    /// used to initialize values on the first access from a thread with a new
    /// thread ID.
    ///
    /// If a value has previously been set, `self` is unchanged and `false` is
    /// returned. Otherwise `true` is returned.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use state::LocalInitCell;
    /// static MY_GLOBAL: LocalInitCell<&'static str> = LocalInitCell::new();
    ///
    /// assert_eq!(MY_GLOBAL.set(|| "Hello, world!"), true);
    /// assert_eq!(MY_GLOBAL.set(|| "Goodbye, world!"), false);
    /// ```
    #[inline]
    pub fn set<F: Fn() -> T>(&self, state_init: F) -> bool
        where F: Send + Sync + 'static
    {
        self.cell.set(LocalValue::new(state_init))
    }

    /// Attempts to borrow the value in this cell. If this is the
    /// first time a thread with the current thread ID has called `get` or
    /// `try_get` for `self`, the value will be initialized using the
    /// initialization function.
    ///
    /// Returns `Some` if the state has previously been [set](#method.set).
    /// Otherwise returns `None`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use state::LocalInitCell;
    /// static MY_GLOBAL: LocalInitCell<&'static str> = LocalInitCell::new();
    ///
    /// assert_eq!(MY_GLOBAL.try_get(), None);
    ///
    /// MY_GLOBAL.set(|| "Hello, world!");
    ///
    /// assert_eq!(MY_GLOBAL.try_get(), Some(&"Hello, world!"));
    /// ```
    #[inline]
    pub fn try_get(&self) -> Option<&T> {
        self.cell.try_get().and_then(|v| v.try_get())
    }

    /// If this is the first time a thread with the current thread ID has called
    /// `get` or `try_get` for `self`, the value will be initialized using the
    /// initialization function.
    ///
    /// # Panics
    ///
    /// Panics if an initialization function has not previously been
    /// [set](#method.set). Use [try_get](#method.try_get) for a non-panicking
    /// version.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use state::LocalInitCell;
    /// static MY_GLOBAL: LocalInitCell<&'static str> = LocalInitCell::new();
    ///
    /// MY_GLOBAL.set(|| "Hello, world!");
    /// assert_eq!(*MY_GLOBAL.get(), "Hello, world!");
    /// ```
    #[inline]
    pub fn get(&self) -> &T {
        self.try_get().expect("localcell::get(): called get() before set()")
    }
}

#[cfg(test)] static_assertions::assert_impl_all!(LocalValue<u8>: Send, Sync);
#[cfg(test)] static_assertions::assert_impl_all!(LocalInitCell<u8>: Send, Sync);

impl<T: fmt::Debug + Send + 'static> fmt::Debug for LocalInitCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.try_get() {
            Some(object) => object.fmt(f),
            None => write!(f, "[uninitialized local cell]")
        }
    }
}
