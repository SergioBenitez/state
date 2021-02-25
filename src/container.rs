use std::cell::UnsafeCell;
use std::marker::PhantomData;
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
/// A container can store at most _one_ instance of given type as well as _n_
/// thread-local instances of a given type.
///
/// ## Type Bounds
///
/// A `Container` can store values that are both `Send + Sync`, just `Send`, or
/// neither. The [`Container!`] macro is used to specify the type of container:
///
/// ```rust
/// use state::Container;
///
/// // Values must implement `Send + Sync`. The container itself is `Send + Sync`.
/// let container: Container![Send + Sync] = <Container![Send + Sync]>::new();
/// let container: Container![Sync + Send] = <Container![Sync + Send]>::new();
///
/// // Values must implement `Send`. The container itself is `Send`, `!Sync`.
/// let container: Container![Send] = <Container![Send]>::new();
///
/// // Values needn't implement `Send` nor `Sync`. `Container` is `!Send`, `!Sync`.
/// let container: Container![] = <Container![]>::new();
/// ```
///
/// ## Setting State
///
/// Global state is set via the [`set()`](Container::set()) method and retrieved
/// via the [`get()`](Container::get()) method. The type of the value being set
/// must meet the bounds of the `Container`.
///
/// ```rust
/// use state::Container;
///
/// fn f_send_sync<T: Send + Sync + Clone + 'static>(value: T) {
///     let container = <Container![Send + Sync]>::new();
///     container.set(value.clone());
///
///     let container = <Container![Send]>::new();
///     container.set(value.clone());
///
///     let container = <Container![]>::new();
///     container.set(value.clone());
/// }
///
/// fn f_send<T: Send + Clone + 'static>(value: T) {
///     // This would fail to compile since `T` may not be `Sync`.
///     // let container = <Container![Send + Sync]>::new();
///     // container.set(value.clone());
///
///     let container = <Container![Send]>::new();
///     container.set(value.clone());
///
///     let container = <Container![]>::new();
///     container.set(value.clone());
/// }
///
/// fn f<T: 'static>(value: T) {
///     // This would fail to compile since `T` may not be `Sync` or `Send`.
///     // let container = <Container![Send + Sync]>::new();
///     // container.set(value.clone());
///
///     // This would fail to compile since `T` may not be `Send`.
///     // let container = <Container![Send]>::new();
///     // container.set(value.clone());
///
///     let container = <Container![]>::new();
///     container.set(value);
/// }
///
/// // If `Container` is `Send + Sync`, it can be `const`-constructed.
/// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
///
/// CONTAINER.set(String::new());
/// CONTAINER.get::<String>();
/// ```
///
/// ## Freezing
///
/// By default, all `get`, `set`, `get_local`, and `set_local` calls result in
/// synchronization overhead for safety. However, if calling `set` or
/// `set_local` is no longer required, the overhead can be eliminated by
/// _freezing_ the `Container`. A frozen container can only be read and never
/// written to. Attempts to write to a frozen container will be ignored.
///
/// To freeze a `Container`, call [`freeze()`](Container::freeze()). A frozen
/// container can never be thawed. To check if a container is frozen, call
/// [`is_frozen()`](Container::is_frozen()).
///
/// ## Thread-Local State
///
/// Thread-local state on a `Send + Sync` container is set via the
/// [`set_local()`](Container::set_local()) method and retrieved via the
/// [`get_local()`](Container::get_local()) method. The type of the value being
/// set must be transferable across thread boundaries but need not be
/// thread-safe. In other words, it must satisfy `Send + 'static` but not
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
/// a new thread B, and access the state set in tread A in thread B.
///
/// ### Example
///
/// Set and later retrieve a value of type T:
///
/// ```rust
/// # struct T;
/// # impl T { fn new() -> T { T } }
/// # #[cfg(not(feature = "tls"))] fn test() { }
/// # #[cfg(feature = "tls")] fn test() {
/// use state::Container;
///
/// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
///
/// CONTAINER.set_local(|| T::new());
/// CONTAINER.get_local::<T>();
/// # }
/// # fn main() { test() }
/// ```
pub struct Container<K: kind::Kind> {
    init: Init,
    map: UnsafeCell<Option<TypeMap>>,
    mutex: AtomicUsize,
    frozen: AtomicBool,
    _kind: PhantomData<K>
}

mod kind {
    pub trait Kind { }

    pub struct Send;
    impl Kind for Send {}

    pub struct SendSync;
    impl Kind for SendSync {}

    pub struct Neither;
    impl Kind for Neither {}
}

pub type ContainerSend = Container<kind::Send>;
pub type ContainerSendSync = Container<kind::SendSync>;
pub type ContainerNeither = Container<kind::Neither>;

/// Type constructor for [`Container`](struct@Container) variants.
#[macro_export]
macro_rules! Container {
    () => ($crate::container::ContainerNeither);
    (Send) => ($crate::container::ContainerSend);
    (Send + Sync) => ($crate::container::ContainerSendSync);
    (Sync + Send) => ($crate::container::ContainerSendSync);
}

macro_rules! new {
    () => (
        Container {
            init: Init::new(),
            map: UnsafeCell::new(None),
            mutex: AtomicUsize::new(0),
            frozen: AtomicBool::new(false),
            _kind: PhantomData,
        }
    )
}

type TypeMap = HashMap<TypeId, AnyObject, BuildHasherDefault<IdentHash>>;

/// FIXME: This is a hack so we can create a *mut dyn Any with a `const`. It
/// depends on trait objects being represented in this way.
#[repr(C)]
struct AnyObject {
    data: *mut (),
    vtable: *mut (),
}

impl AnyObject {
    fn anonymize<T: 'static>(value: T) -> AnyObject {
        let any: Box<dyn Any> = Box::new(value) as Box<dyn Any>;
        let any: *mut dyn Any = Box::into_raw(any);
        unsafe { std::mem::transmute(any) }
    }

    fn deanonymize<T: 'static>(&self) -> &T {
        unsafe {
            let any: *const *const dyn Any = std::mem::transmute(self);
            &*(*any as *const dyn Any as *const T)
        }
    }
}

impl Drop for AnyObject {
    fn drop(&mut self) {
        unsafe {
            let any: *mut *mut dyn Any = std::mem::transmute(self);
            let any: *mut dyn Any = *any;
            let any: Box<dyn Any> = Box::from_raw(any);
            drop(any)
        }
    }
}

impl Container<kind::SendSync> {
    /// Creates a new container with no stored values.
    ///
    /// ## Example
    ///
    /// Create a globally available state container:
    ///
    /// ```rust
    /// use state::Container;
    ///
    /// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
    /// ```
    pub const fn new() -> Self {
        new!()
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
    /// # use std::sync::atomic::AtomicUsize;
    /// use state::Container;
    ///
    /// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
    ///
    /// assert_eq!(CONTAINER.set(AtomicUsize::new(0)), true);
    /// assert_eq!(CONTAINER.set(AtomicUsize::new(1)), false);
    /// ```
    #[inline]
    pub fn set<T: Send + Sync + 'static>(&self, state: T) -> bool {
        unsafe { self._set(state) }
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
    /// # use std::cell::Cell;
    /// use state::Container;
    ///
    /// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
    ///
    /// struct MyState(Cell<usize>);
    ///
    /// assert_eq!(CONTAINER.set_local(|| MyState(Cell::new(1))), true);
    /// assert_eq!(CONTAINER.set_local(|| MyState(Cell::new(2))), false);
    /// ```
    #[inline]
    #[cfg(feature = "tls")]
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
    /// # use std::cell::Cell;
    /// use state::Container;
    ///
    /// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
    ///
    /// struct MyState(Cell<usize>);
    ///
    /// CONTAINER.set_local(|| MyState(Cell::new(10)));
    ///
    /// let my_state = CONTAINER.try_get_local::<MyState>().expect("MyState");
    /// assert_eq!(my_state.0.get(), 10);
    /// ```
    #[inline]
    #[cfg(feature = "tls")]
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
    /// # use std::cell::Cell;
    /// use state::Container;
    ///
    /// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
    ///
    /// struct MyState(Cell<usize>);
    ///
    /// CONTAINER.set_local(|| MyState(Cell::new(10)));
    ///
    /// let my_state = CONTAINER.get_local::<MyState>();
    /// assert_eq!(my_state.0.get(), 10);
    /// ```
    #[inline]
    #[cfg(feature = "tls")]
    pub fn get_local<T: Send + 'static>(&self) -> &T {
        self.try_get_local::<T>()
            .expect("container::get_local(): get_local() called before set_local()")
    }
}

unsafe impl Send for Container<kind::SendSync> {  }
unsafe impl Sync for Container<kind::SendSync> {  }

#[cfg(test)] static_assertions::assert_impl_all!(Container![Send + Sync]: Send, Sync);
#[cfg(test)] static_assertions::assert_impl_all!(Container![Sync + Send]: Send, Sync);

impl Container<kind::Send> {
    /// Creates a new container with no stored values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::cell::Cell;
    ///
    /// use state::Container;
    ///
    /// let container = <Container![Send]>::new();
    ///
    /// let value: Cell<u8> = Cell::new(10);
    /// container.set(value);
    /// assert_eq!(container.get::<Cell<u8>>().get(), 10);
    ///
    /// container.get::<Cell<u8>>().set(99);
    /// assert_eq!(container.get::<Cell<u8>>().get(), 99);
    /// ```
    pub fn new() -> Self {
        // SAFETY: this can't be `const` or we violate `Sync`.
        new!()
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
    /// Set the state. The first `set` is succesful while the second fails.
    ///
    /// ```rust
    /// # use std::sync::atomic::AtomicUsize;
    /// use state::Container;
    ///
    /// let container = <Container![Send]>::new();
    /// assert!(container.set(AtomicUsize::new(0)));
    /// assert!(!container.set(AtomicUsize::new(1)));
    /// ```
    #[inline]
    pub fn set<T: Send + 'static>(&self, state: T) -> bool {
        unsafe { self._set(state) }
    }
}

unsafe impl Send for Container<kind::Send> {  }

#[cfg(test)] static_assertions::assert_impl_all!(Container![Send]: Send);
#[cfg(test)] static_assertions::assert_not_impl_any!(Container![Send]: Sync);
#[cfg(test)] static_assertions::assert_not_impl_any!(Container<kind::Send>: Sync);

impl Container<kind::Neither> {
    /// Creates a new container with no stored values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::cell::Cell;
    /// use state::Container;
    ///
    /// let container = <Container![]>::new();
    ///
    /// let value: Cell<u8> = Cell::new(10);
    /// container.set(value);
    /// assert_eq!(container.get::<Cell<u8>>().get(), 10);
    ///
    /// container.get::<Cell<u8>>().set(99);
    /// assert_eq!(container.get::<Cell<u8>>().get(), 99);
    /// ```
    pub fn new() -> Self {
        // SAFETY: this can't be `const` or we violate `Sync`.
        new!()
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
    /// Set the state. The first `set` is succesful while the second fails.
    ///
    /// ```rust
    /// use std::cell::Cell;
    /// use state::Container;
    ///
    /// let container = <Container![]>::new();
    /// assert!(container.set(Cell::new(10)));
    /// assert!(!container.set(Cell::new(17)));
    /// ```
    #[inline]
    pub fn set<T: 'static>(&self, state: T) -> bool {
        unsafe { self._set(state) }
    }
}

#[cfg(test)] static_assertions::assert_not_impl_any!(Container![]: Send, Sync);
#[cfg(test)] static_assertions::assert_not_impl_any!(Container<kind::Neither>: Send, Sync);

impl<K: kind::Kind> Container<K> {
    /// SAFETY: The caller needs to ensure that `T` has the required bounds
    /// `Sync` or `Send` bounds.
    unsafe fn _set<T: 'static>(&self, state: T) -> bool {
        if self.is_frozen() {
            return false;
        }

        self.lock();
        let map = self.map_mut();
        let type_id = TypeId::of::<T>();
        let already_set = map.contains_key(&type_id);
        if !already_set {
            map.insert(type_id, AnyObject::anonymize(state));
        }

        self.unlock();
        !already_set
    }

    /// Attempts to retrieve the global state for type `T`.
    ///
    /// Returns `Some` if the state has previously been [set](#method.set).
    /// Otherwise returns `None`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::atomic::{AtomicUsize, Ordering};
    /// use state::Container;
    ///
    /// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
    ///
    /// struct MyState(AtomicUsize);
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
    pub fn try_get<T: 'static>(&self) -> Option<&T> {
        unsafe {
            // If we're frozen, there can't be any concurrent writers, so we're
            // free to read this safely without taking a lock.
            let type_id = TypeId::of::<T>();
            let item = if self.is_frozen() {
                let map = self.map_ref();
                map.get(&type_id)
            } else {
                self.lock();
                let map = self.map_ref();
                let item = map.get(&type_id);
                self.unlock();
                item
            };

            item.map(|ptr| ptr.deanonymize())
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
    /// # use std::sync::atomic::{AtomicUsize, Ordering};
    /// use state::Container;
    ///
    /// static CONTAINER: Container![Send + Sync] = <Container![Send + Sync]>::new();
    ///
    /// struct MyState(AtomicUsize);
    ///
    /// CONTAINER.set(MyState(AtomicUsize::new(0)));
    ///
    /// let my_state = CONTAINER.get::<MyState>();
    /// assert_eq!(my_state.0.load(Ordering::Relaxed), 0);
    /// ```
    #[inline]
    pub fn get<T: 'static>(&self) -> &T {
        self.try_get()
            .expect("container::get(): get() called before set() for given type")
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
    /// let mut container = <Container![Send + Sync]>::new();
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
    /// let mut container = <Container![Send]>::new();
    /// assert_eq!(container.is_frozen(), false);
    ///
    /// container.freeze();
    /// assert_eq!(container.is_frozen(), true);
    /// ```
    #[inline(always)]
    pub fn is_frozen(&self) -> bool {
        self.frozen.load(Ordering::Relaxed)
    }

    // Initializes the `map` if needed.
    unsafe fn init_map_if_needed(&self) {
        if self.init.needed() {
            *self.map.get() = Some(HashMap::<_, _, _>::default());
            self.init.mark_complete();
        }
    }

    // Initializes the `map` if needed and returns it.
    //
    // SAFETY: Caller must ensure mutual exclusion of calls to this function
    // and/or calls to `map_ref`.
    #[inline(always)]
    unsafe fn map_mut(&self) -> &mut TypeMap {
        self.init_map_if_needed();
        (*self.map.get()).as_mut().unwrap()
    }

    // Initializes the `map` if needed and returns it.
    //
    // SAFETY: Caller must ensure mutual exclusion of calls to this function
    // and/or calls to `map_mut`.
    #[inline(always)]
    unsafe fn map_ref(&self) -> &TypeMap {
        self.init_map_if_needed();
        (*self.map.get()).as_ref().unwrap()
    }

    #[inline(always)]
    fn lock(&self) {
        while self.mutex.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed).is_err() {}
    }

    #[inline(always)]
    fn unlock(&self) {
        assert!(self.mutex.compare_exchange(1, 0, Ordering::AcqRel, Ordering::Relaxed).is_ok());
    }
}
