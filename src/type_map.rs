use std::marker::PhantomData;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::any::{Any, TypeId};

use crate::init::Init;
use crate::ident_hash::IdentHash;
use crate::shim::cell::UnsafeCell;
use crate::shim::sync::atomic::{AtomicUsize, Ordering};
use crate::shim::thread::yield_now;

#[cfg(feature = "tls")]
use crate::tls::LocalValue;

/// A type map storing values based on types.
///
/// A type map stores at most _one_ instance of given type as well as _n_
/// thread-local instances of a given type.
///
/// ## Type Bounds
///
/// A `TypeMap` can store values that are both `Send + Sync`, just `Send`, or
/// neither. The [`TypeMap!`](macro.TypeMap.html) macro is used to specify the
/// kind of type map:
///
/// ```rust
/// use state::TypeMap;
///
/// // Values must implement `Send + Sync`. The type_map itself is `Send + Sync`.
/// let type_map: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
/// let type_map: TypeMap![Sync + Send] = <TypeMap![Sync + Send]>::new();
///
/// // Values must implement `Send`. The type_map itself is `Send`, `!Sync`.
/// let type_map: TypeMap![Send] = <TypeMap![Send]>::new();
///
/// // Values needn't implement `Send` nor `Sync`. `TypeMap` is `!Send`, `!Sync`.
/// let type_map: TypeMap![] = <TypeMap![]>::new();
/// ```
///
/// ## Setting State
///
/// Global state is set via the [`set()`](TypeMap::set()) method and retrieved
/// via the [`get()`](TypeMap::get()) method. The type of the value being set
/// must meet the bounds of the `TypeMap`.
///
/// ```rust
/// use state::TypeMap;
///
/// fn f_send_sync<T: Send + Sync + Clone + 'static>(value: T) {
///     let type_map = <TypeMap![Send + Sync]>::new();
///     type_map.set(value.clone());
///
///     let type_map = <TypeMap![Send]>::new();
///     type_map.set(value.clone());
///
///     let type_map = <TypeMap![]>::new();
///     type_map.set(value.clone());
/// }
///
/// fn f_send<T: Send + Clone + 'static>(value: T) {
///     // This would fail to compile since `T` may not be `Sync`.
///     // let type_map = <TypeMap![Send + Sync]>::new();
///     // type_map.set(value.clone());
///
///     let type_map = <TypeMap![Send]>::new();
///     type_map.set(value.clone());
///
///     let type_map = <TypeMap![]>::new();
///     type_map.set(value.clone());
/// }
///
/// fn f<T: 'static>(value: T) {
///     // This would fail to compile since `T` may not be `Sync` or `Send`.
///     // let type_map = <TypeMap![Send + Sync]>::new();
///     // type_map.set(value.clone());
///
///     // This would fail to compile since `T` may not be `Send`.
///     // let type_map = <TypeMap![Send]>::new();
///     // type_map.set(value.clone());
///
///     let type_map = <TypeMap![]>::new();
///     type_map.set(value);
/// }
///
/// // If `TypeMap` is `Send + Sync`, it can be `const`-constructed.
/// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
///
/// TYPE_MAP.set(String::new());
/// TYPE_MAP.get::<String>();
/// ```
///
/// ## Freezing
///
/// By default, all `get`, `set`, `get_local`, and `set_local` calls result in
/// synchronization overhead for safety. However, if calling `set` or
/// `set_local` is no longer required, the overhead can be eliminated by
/// _freezing_ the `TypeMap`. A frozen type map can only be read and never
/// written to. Attempts to write to a frozen type map will be ignored.
///
/// To freeze a `TypeMap`, call [`freeze()`](TypeMap::freeze()). A frozen map
/// can never be thawed. To check if a type map is frozen, call
/// [`is_frozen()`](TypeMap::is_frozen()).
///
/// ## Thread-Local State
///
/// Thread-local state on a `Send + Sync` type map is set via the
/// [`set_local()`](TypeMap::set_local()) method and retrieved via the
/// [`get_local()`](TypeMap::get_local()) method. The type of the value being
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
/// use state::TypeMap;
///
/// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
///
/// TYPE_MAP.set_local(|| T::new());
/// TYPE_MAP.get_local::<T>();
/// # }
/// # fn main() { test() }
/// ```
pub struct TypeMap<K: kind::Kind> {
    init: Init,
    map: UnsafeCell<Option<TypeIdMap>>,
    mutex: AtomicUsize,
    frozen: bool,
    /// Force !Send (and carry the type).
    _kind: PhantomData<*mut K>
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

pub type TypeMapSend = TypeMap<kind::Send>;
pub type TypeMapSendSync = TypeMap<kind::SendSync>;
pub type TypeMapNeither = TypeMap<kind::Neither>;

/// Type constructor for [`TypeMap`](struct@TypeMap) variants.
#[macro_export]
macro_rules! TypeMap {
    () => ($crate::type_map::TypeMapNeither);
    (Send) => ($crate::type_map::TypeMapSend);
    (Send + Sync) => ($crate::type_map::TypeMapSendSync);
    (Sync + Send) => ($crate::type_map::TypeMapSendSync);
}

macro_rules! new {
    () => (
        TypeMap {
            init: Init::new(),
            map: UnsafeCell::new(None),
            mutex: AtomicUsize::new(0),
            frozen: false,
            _kind: PhantomData,
        }
    )
}

type TypeIdMap = HashMap<TypeId, Box<dyn Any>, BuildHasherDefault<IdentHash>>;

impl TypeMap<kind::SendSync> {
    /// Creates a new type map with no stored values.
    ///
    /// ## Example
    ///
    /// Create a globally available type map:
    ///
    /// ```rust
    /// use state::TypeMap;
    ///
    /// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
    /// ```
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        new!()
    }

    #[cfg(loom)]
    pub fn new() -> Self {
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
    /// use state::TypeMap;
    ///
    /// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
    ///
    /// assert_eq!(TYPE_MAP.set(AtomicUsize::new(0)), true);
    /// assert_eq!(TYPE_MAP.set(AtomicUsize::new(1)), false);
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
    /// use state::TypeMap;
    ///
    /// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
    ///
    /// struct MyState(Cell<usize>);
    ///
    /// assert_eq!(TYPE_MAP.set_local(|| MyState(Cell::new(1))), true);
    /// assert_eq!(TYPE_MAP.set_local(|| MyState(Cell::new(2))), false);
    /// ```
    #[inline]
    #[cfg(feature = "tls")]
    pub fn set_local<T, F>(&self, state_init: F) -> bool
        where T: Send + 'static, F: Fn() -> T + Send + Sync + 'static
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
    /// use state::TypeMap;
    ///
    /// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
    ///
    /// struct MyState(Cell<usize>);
    ///
    /// TYPE_MAP.set_local(|| MyState(Cell::new(10)));
    ///
    /// let my_state = TYPE_MAP.try_get_local::<MyState>().expect("MyState");
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
    /// use state::TypeMap;
    ///
    /// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
    ///
    /// struct MyState(Cell<usize>);
    ///
    /// TYPE_MAP.set_local(|| MyState(Cell::new(10)));
    ///
    /// let my_state = TYPE_MAP.get_local::<MyState>();
    /// assert_eq!(my_state.0.get(), 10);
    /// ```
    #[inline]
    #[cfg(feature = "tls")]
    pub fn get_local<T: Send + 'static>(&self) -> &T {
        self.try_get_local::<T>()
            .expect("type_map::get_local(): get_local() called before set_local()")
    }
}

unsafe impl Send for TypeMap<kind::SendSync> {  }
unsafe impl Sync for TypeMap<kind::SendSync> {  }

#[cfg(test)] static_assertions::assert_impl_all!(TypeMap![Send + Sync]: Send, Sync);
#[cfg(test)] static_assertions::assert_impl_all!(TypeMap![Sync + Send]: Send, Sync);

impl TypeMap<kind::Send> {
    /// Creates a new type map with no stored values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::cell::Cell;
    ///
    /// use state::TypeMap;
    ///
    /// let type_map = <TypeMap![Send]>::new();
    ///
    /// let value: Cell<u8> = Cell::new(10);
    /// type_map.set(value);
    /// assert_eq!(type_map.get::<Cell<u8>>().get(), 10);
    ///
    /// type_map.get::<Cell<u8>>().set(99);
    /// assert_eq!(type_map.get::<Cell<u8>>().get(), 99);
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
    /// use state::TypeMap;
    ///
    /// let type_map = <TypeMap![Send]>::new();
    /// assert!(type_map.set(AtomicUsize::new(0)));
    /// assert!(!type_map.set(AtomicUsize::new(1)));
    /// ```
    #[inline]
    pub fn set<T: Send + 'static>(&self, state: T) -> bool {
        unsafe { self._set(state) }
    }
}

unsafe impl Send for TypeMap<kind::Send> {  }

#[cfg(test)] static_assertions::assert_impl_all!(TypeMap![Send]: Send);
#[cfg(test)] static_assertions::assert_not_impl_any!(TypeMap![Send]: Sync);
#[cfg(test)] static_assertions::assert_not_impl_any!(TypeMap<kind::Send>: Sync);

impl TypeMap<kind::Neither> {
    /// Creates a new type_map with no stored values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::cell::Cell;
    /// use state::TypeMap;
    ///
    /// let type_map = <TypeMap![]>::new();
    ///
    /// let value: Cell<u8> = Cell::new(10);
    /// type_map.set(value);
    /// assert_eq!(type_map.get::<Cell<u8>>().get(), 10);
    ///
    /// type_map.get::<Cell<u8>>().set(99);
    /// assert_eq!(type_map.get::<Cell<u8>>().get(), 99);
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
    /// use state::TypeMap;
    ///
    /// let type_map = <TypeMap![]>::new();
    /// assert!(type_map.set(Cell::new(10)));
    /// assert!(!type_map.set(Cell::new(17)));
    /// ```
    #[inline]
    pub fn set<T: 'static>(&self, state: T) -> bool {
        unsafe { self._set(state) }
    }
}

#[cfg(test)] static_assertions::assert_not_impl_any!(TypeMap![]: Send, Sync);
#[cfg(test)] static_assertions::assert_not_impl_any!(TypeMap<kind::Neither>: Send, Sync);

impl<K: kind::Kind> TypeMap<K> {
    // Initializes the `map` if needed.
    unsafe fn init_map_if_needed(&self) {
        if self.init.needed() {
            self.map.with_mut(|ptr| *ptr = Some(HashMap::<_, _, _>::default()));
            self.init.mark_complete();
        }
    }

    // Initializes the `map` if needed and returns a mutable ref to it.
    //
    // SAFETY: Caller must ensure mutual exclusion of calls to this function
    // and/or calls to `map_ref`.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn map_mut(&self) -> &mut TypeIdMap {
        self.init_map_if_needed();
        self.map.with_mut(|ptr| (*ptr).as_mut().unwrap())
    }

    // Initializes the `map` if needed and returns an immutable ref to it.
    //
    // SAFETY: Caller must ensure mutual exclusion of calls to this function
    // and/or calls to `map_mut`.
    #[inline(always)]
    unsafe fn map_ref(&self) -> &TypeIdMap {
        self.init_map_if_needed();
        self.map.with(|ptr| (*ptr).as_ref().unwrap())
    }

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
            map.insert(type_id, Box::new(state) as Box<dyn Any>);
        }

        self.unlock();
        !already_set
    }

    /// SAFETY: The caller needs to ensure that the `T` returned from the `f` is
    /// not dependent on the stability of memory slots in the map. It also needs
    /// to ensure that `f` does not panic if liveness is desired.
    unsafe fn with_map_ref<'a, F, T: 'a>(&'a self, f: F) -> T
        where F: FnOnce(&'a TypeIdMap) -> T
    {
        // If we're frozen, there can't be any concurrent writers, so we're
        // free to read this safely without taking a lock.
        if self.is_frozen() {
            f(self.map_ref())
        } else {
            self.lock();
            let result = f(self.map_ref());
            self.unlock();
            result
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
    /// # use std::sync::atomic::{AtomicUsize, Ordering};
    /// use state::TypeMap;
    ///
    /// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
    ///
    /// struct MyState(AtomicUsize);
    ///
    /// // State for `T` is initially unset.
    /// assert!(TYPE_MAP.try_get::<MyState>().is_none());
    ///
    /// TYPE_MAP.set(MyState(AtomicUsize::new(0)));
    ///
    /// let my_state = TYPE_MAP.try_get::<MyState>().expect("MyState");
    /// assert_eq!(my_state.0.load(Ordering::Relaxed), 0);
    /// ```
    #[inline]
    pub fn try_get<T: 'static>(&self) -> Option<&T> {
        // SAFETY: `deanonymization` takes a potentially unstable refrence to an
        // `AnyObject` and converts it into a stable address: it is converting
        // an `&Box<dyn Any>` into the inner `&T`. The inner item is never
        // dropped until `self` is dropped: it is never replaced.
        unsafe {
            self.with_map_ref(|map| {
                map.get(&TypeId::of::<T>()).and_then(|ptr| ptr.downcast_ref())
            })
        }
    }

    /// Retrieves the global state for type `T`.
    ///
    /// # Panics
    ///
    /// Panics if the state for type `T` has not previously been
    /// [`set()`](Self::set()). Use [`try_get()`](Self::try_get()) for a
    /// non-panicking version.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::sync::atomic::{AtomicUsize, Ordering};
    /// use state::TypeMap;
    ///
    /// static TYPE_MAP: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();
    ///
    /// struct MyState(AtomicUsize);
    ///
    /// TYPE_MAP.set(MyState(AtomicUsize::new(0)));
    ///
    /// let my_state = TYPE_MAP.get::<MyState>();
    /// assert_eq!(my_state.0.load(Ordering::Relaxed), 0);
    /// ```
    #[inline]
    pub fn get<T: 'static>(&self) -> &T {
        self.try_get()
            .expect("type_map::get(): get() called before set() for given type")
    }

    /// Freezes the type_map. A frozen type_map disallows writes allowing for
    /// synchronization-free reads.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::TypeMap;
    ///
    /// // A new type_map starts unfrozen and can be written to.
    /// let mut type_map = <TypeMap![Send + Sync]>::new();
    /// assert_eq!(type_map.set(1usize), true);
    ///
    /// // While unfrozen, `get`s require synchronization.
    /// assert_eq!(type_map.get::<usize>(), &1);
    ///
    /// // After freezing, calls to `set` or `set_local `will fail.
    /// type_map.freeze();
    /// assert_eq!(type_map.set(1u8), false);
    /// assert_eq!(type_map.set("hello"), false);
    ///
    /// // Calls to `get` or `get_local` are synchronization-free when frozen.
    /// assert_eq!(type_map.try_get::<u8>(), None);
    /// assert_eq!(type_map.get::<usize>(), &1);
    /// ```
    #[inline(always)]
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Returns `true` if the type_map is frozen and `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::TypeMap;
    ///
    /// // A new type_map starts unfrozen and is frozen using `freeze`.
    /// let mut type_map = <TypeMap![Send]>::new();
    /// assert_eq!(type_map.is_frozen(), false);
    ///
    /// type_map.freeze();
    /// assert_eq!(type_map.is_frozen(), true);
    /// ```
    #[inline(always)]
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Returns the number of distinctly typed values in `self`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::TypeMap;
    ///
    /// let type_map = <TypeMap![Send + Sync]>::new();
    /// assert_eq!(type_map.len(), 0);
    ///
    /// assert_eq!(type_map.set(1usize), true);
    /// assert_eq!(type_map.len(), 1);
    ///
    /// assert_eq!(type_map.set(2usize), false);
    /// assert_eq!(type_map.len(), 1);
    ///
    /// assert_eq!(type_map.set(1u8), true);
    /// assert_eq!(type_map.len(), 2);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        // SAFETY: We retrieve a `usize`, which is clearly stable.
        unsafe { self.with_map_ref(|map| map.len()) }
    }

    /// Returns `true` if `self` contains zero values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::TypeMap;
    ///
    /// let type_map = <TypeMap![Send + Sync]>::new();
    /// assert!(type_map.is_empty());
    ///
    /// assert_eq!(type_map.set(1usize), true);
    /// assert!(!type_map.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    fn lock(&self) {
        while self.mutex.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed).is_err() {
            yield_now();
        }
    }

    #[inline(always)]
    fn unlock(&self) {
        assert!(self.mutex.compare_exchange(1, 0, Ordering::AcqRel, Ordering::Relaxed).is_ok());
    }
}

impl Default for TypeMap![Send + Sync] {
    fn default() -> Self {
        <TypeMap![Send + Sync]>::new()
    }
}

impl Default for TypeMap![Send] {
    fn default() -> Self {
        <TypeMap![Send]>::new()
    }
}

impl Default for TypeMap![] {
    fn default() -> Self {
        <TypeMap![]>::new()
    }
}

impl<K: kind::Kind> std::fmt::Debug for TypeMap<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeMap")
            .field("len", &self.len())
            .finish()
    }
}
