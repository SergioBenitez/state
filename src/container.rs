use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::any::{Any, TypeId};

use init::Init;
use ident_hash::IdentHash;

#[cfg(feature = "tls")]
use tls::LocalValue;

/// A container for global type-based state.
///
/// A container stores at most _one_ global instance of given type as well as
/// _n_ thread-local instances of a given type.
///
/// ## Global State
///
/// Global state is set via the [set](#method.set) method and retrieved via the
/// [get](#method.get) method. The type of the value being set must be
/// thread-safe and transferable across thread boundaries. In other words, it
/// must satisfy `Sync + Send + 'static`.
///
/// ### Example
///
/// Set and later retrieve a value of type T:
///
/// ```rust
/// # #![feature(const_fn)]
/// # struct T;
/// # impl T { fn new() -> T { T } }
/// static CONTAINER: state::Container = state::Container::new();
///
/// CONTAINER.set(T::new());
/// CONTAINER.get::<T>();
/// ```
///
/// ## Freezing
///
/// By default, all `get`, `set`, `get_local`, and `set_local` calls result in
/// synchronization overhead for safety. However, if calling `set` or
/// `set_local` is no longer required, the overhead can be eliminated by
/// _freezing_ the `Container`. A frozen container can only be read and never
/// written to. Attempts to write to a frozen container will fail.
///
/// To freeze a `Container`, call [`freeze()`](Container::freeze()). A frozen
/// container can never be thawed. To check if a container is frozen, call
/// [`is_frozen()`](Container::is_frozen()).
///
/// ## Thread-Local State
///
/// Thread-local state is set via the [set_local](#method.set_local) method and
/// retrieved via the [get_local](#method.get_local) method. The type of the
/// value being set must be transferable across thread boundaries but need not
/// be thread-safe. In other words, it must satisfy `Send + 'static` but not
/// necessarily `Sync`. Values retrieved from thread-local state are exactly
/// that: local to the current thread. As such, you cannot use thread-local
/// state to synchronize across multiple threads.
///
/// Thread-local state is initialized on an as-needed basis. The function used
/// to initialize the thread-local state is passed in as an argument to
/// `set_local`. When the state is retrieved from a given thread for the first
/// time, the function is executed to generate the initial value. The function
/// is executed at most once per thread. The same function is used for
/// initialization across all threads.
///
/// **Note:** Rust reuses thread IDs across multiple threads. This means that is
/// possible to set thread-local state in thread A, have that thread die, start
/// a new thread B, and access the state set in A in B.
///
/// ### Example
///
/// Set and later retrieve a value of type T:
///
/// ```rust
/// # #![feature(const_fn)]
/// # struct T;
/// # impl T { fn new() -> T { T } }
/// # #[cfg(not(feature = "tls"))] fn test() { }
/// # #[cfg(feature = "tls")] fn test() {
/// static CONTAINER: state::Container = state::Container::new();
///
/// CONTAINER.set_local(|| T::new());
/// CONTAINER.get_local::<T>();
/// # }
/// # fn main() { test() }
/// ```
pub struct Container {
    init: Init,
    map: UnsafeCell<*mut HashMap<TypeId, *mut Any, BuildHasherDefault<IdentHash>>>,
    mutex: AtomicUsize,
    frozen: AtomicBool
}

impl Container {
    const_if_enabled! {
        /// Creates a new container with no stored values.
        ///
        /// ## Example
        ///
        /// Create a globally available state container:
        ///
        /// ```rust
        /// # #![feature(const_fn)]
        /// static CONTAINER: state::Container = state::Container::new();
        /// ```
        pub fn new() -> Container {
            Container {
                init: Init::new(),
                map: UnsafeCell::new(0 as *mut _),
                mutex: AtomicUsize::new(0),
                frozen: AtomicBool::new(false),
            }
        }
    }

    // Initializes the `STATE` global variable. This _MUST_ be called before
    // accessing the variable!
    #[inline(always)]
    fn ensure_map_initialized(&self) {
        if self.init.needed() {
            unsafe {
                // TODO: Don't have an extra layer of indirection. HashMap needs
                // to expose a `const fn` to accomplish that, unfortunately.
                *self.map.get() = Box::into_raw(Box::new(HashMap::<_, _, _>::default()));
            }

            self.init.mark_complete();
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

    /// Freezes the container. A frozen container disallows writes allowing for
    /// synchronization-free reads.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::Container;
    ///
    /// // A new container starts unfrozen and can be written to.
    /// let mut container = Container::new();
    /// assert_eq!(container.set(1usize), true);
    ///
    /// // While unfrozen, `get`s require synchronization.
    /// assert_eq!(container.get::<usize>(), &1);
    ///
    /// // After freezing, calls to `set` or `set_local `will fail.
    /// container.freeze();
    /// assert_eq!(container.set(1u8), false);
    /// assert_eq!(container.set("hello"), false);
    ///
    /// // Calls to `get` or `get_local` are synchronization-free when frozen.
    /// assert_eq!(container.try_get::<u8>(), None);
    /// assert_eq!(container.get::<usize>(), &1);
    /// ```
    #[inline(always)]
    pub fn freeze(&mut self) {
        self.frozen.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if the container is frozen and `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::Container;
    ///
    /// // A new container starts unfrozen and is frozen using `freeze`.
    /// let mut container = Container::new();
    /// assert_eq!(container.is_frozen(), false);
    ///
    /// container.freeze();
    /// assert_eq!(container.is_frozen(), true);
    /// ```
    #[inline(always)]
    pub fn is_frozen(&self) -> bool {
        self.frozen.load(Ordering::Relaxed)
    }

    /// Sets the global state for type `T` if it has not been set before and
    /// `self` is not frozen.
    ///
    /// If the state for `T` has previously been set or `self` is frozen, the
    /// state is unchanged and `false` is returned. Otherwise `true` is
    /// returned.
    ///
    /// # Example
    ///
    /// Set the state for `AtomicUsize`. The first `set` is succesful while the
    /// second fails.
    ///
    /// ```rust
    /// # #![feature(const_fn)]
    /// # use std::sync::atomic::AtomicUsize;
    /// static CONTAINER: state::Container = state::Container::new();
    ///
    /// assert_eq!(CONTAINER.set(AtomicUsize::new(0)), true);
    /// assert_eq!(CONTAINER.set(AtomicUsize::new(1)), false);
    /// ```
    #[inline]
    pub fn set<T: Send + Sync + 'static>(&self, state: T) -> bool {
        if self.is_frozen() {
            return false;
        }

        self.ensure_map_initialized();
        let type_id = TypeId::of::<T>();

        unsafe {
            self.lock();
            let already_set = (**self.map.get()).contains_key(&type_id);
            if !already_set {
                let state_entry = Box::into_raw(Box::new(state) as Box<Any>);
                (**self.map.get()).insert(type_id, state_entry);
            }

            self.unlock();
            !already_set
        }
    }

    /// Attempts to retrieve the global state for type `T`.
    ///
    /// Returns `Some` if the state has previously been [set](#method.set).
    /// Otherwise returns `None`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # #![feature(const_fn)]
    /// # use std::sync::atomic::{AtomicUsize, Ordering};
    /// struct MyState(AtomicUsize);
    ///
    /// static CONTAINER: state::Container = state::Container::new();
    ///
    /// // State for `T` is initially unset.
    /// assert!(CONTAINER.try_get::<MyState>().is_none());
    ///
    /// CONTAINER.set(MyState(AtomicUsize::new(0)));
    ///
    /// let my_state = CONTAINER.try_get::<MyState>().expect("MyState");
    /// assert_eq!(my_state.0.load(Ordering::Relaxed), 0);
    /// ```
    #[inline]
    pub fn try_get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.ensure_map_initialized();
        let type_id = TypeId::of::<T>();

        unsafe {
            let unsync_get_item = || (**self.map.get()).get(&type_id);

            // If we're frozen, there can't be any concurrent writers, so we're
            // free to read this safely without taking a lock.
            let item = if self.is_frozen() {
                unsync_get_item()
            } else {
                self.lock();
                let item = unsync_get_item();
                self.unlock();
                item
            };

            item.map(|ptr| &*(*ptr as *const Any as *const T))
        }
    }

    /// Retrieves the global state for type `T`.
    ///
    /// # Panics
    ///
    /// Panics if the state for type `T` has not previously been
    /// [set](#method.set). Use [try_get](#method.try_get) for a non-panicking
    /// version.
    ///
    /// # Example
    ///
    /// ```rust
    /// # #![feature(const_fn)]
    /// # use std::sync::atomic::{AtomicUsize, Ordering};
    /// struct MyState(AtomicUsize);
    ///
    /// static CONTAINER: state::Container = state::Container::new();
    ///
    /// CONTAINER.set(MyState(AtomicUsize::new(0)));
    ///
    /// let my_state = CONTAINER.get::<MyState>();
    /// assert_eq!(my_state.0.load(Ordering::Relaxed), 0);
    /// ```
    #[inline]
    pub fn get<T: Send + Sync + 'static>(&self) -> &T {
        self.try_get()
            .expect("container::get(): get() called before set() for given type")
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
    /// # #![feature(const_fn)]
    /// # use std::cell::Cell;
    /// struct MyState(Cell<usize>);
    ///
    /// static CONTAINER: state::Container = state::Container::new();
    ///
    /// assert_eq!(CONTAINER.set_local(|| MyState(Cell::new(1))), true);
    /// assert_eq!(CONTAINER.set_local(|| MyState(Cell::new(2))), false);
    /// ```
    #[cfg(feature = "tls")]
    #[inline]
    pub fn set_local<T, F>(&self, state_init: F) -> bool
        where T: Send + 'static, F: Fn() -> T + 'static
    {
        self.set::<LocalValue<T>>(LocalValue::new(state_init))
    }

    /// Attempts to retrieve the thread-local state for type `T`.
    ///
    /// Returns `Some` if the state has previously been set via
    /// [set_local](#method.set_local). Otherwise returns `None`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # #![feature(const_fn)]
    /// # use std::cell::Cell;
    /// struct MyState(Cell<usize>);
    ///
    /// static CONTAINER: state::Container = state::Container::new();
    ///
    /// CONTAINER.set_local(|| MyState(Cell::new(10)));
    ///
    /// let my_state = CONTAINER.try_get_local::<MyState>().expect("MyState");
    /// assert_eq!(my_state.0.get(), 10);
    /// ```
    #[cfg(feature = "tls")]
    #[inline]
    pub fn try_get_local<T: Send + 'static>(&self) -> Option<&T> {
        // TODO: This will take a lock on the HashMap unnecessarily. Ideally
        // we'd have a `HashMap` per thread mapping from TypeId to (T, F).
        self.try_get::<LocalValue<T>>().map(|value| value.get())
    }

    /// Retrieves the thread-local state for type `T`.
    ///
    /// # Panics
    ///
    /// Panics if the thread-local state for type `T` has not previously been set
    /// via [set_local](#method.set_local). Use
    /// [try_get_local](#method.try_get_local) for a non-panicking version.
    ///
    /// # Example
    ///
    /// ```rust
    /// # #![feature(const_fn)]
    /// # use std::cell::Cell;
    /// struct MyState(Cell<usize>);
    ///
    /// static CONTAINER: state::Container = state::Container::new();
    ///
    /// CONTAINER.set_local(|| MyState(Cell::new(10)));
    ///
    /// let my_state = CONTAINER.get_local::<MyState>();
    /// assert_eq!(my_state.0.get(), 10);
    /// ```
    #[cfg(feature = "tls")]
    #[inline]
    pub fn get_local<T: Send + 'static>(&self) -> &T {
        self.try_get_local::<T>()
            .expect("container::get_local(): get_local() called before set_local()")
    }
}

unsafe impl Sync for Container {  }
unsafe impl Send for Container {  }

impl Drop for Container {
    fn drop(&mut self) {
        if !self.init.has_completed() {
            return
        }

        unsafe {
            let map = &mut **self.map.get();
            for value in map.values_mut() {
                let mut boxed_any: Box<Any> = Box::from_raw(*value);
                drop(&mut boxed_any);
            }

            let mut boxed_map: Box<HashMap<_, _, _>> = Box::from_raw(map);
            drop(&mut boxed_map);
        }
    }
}
