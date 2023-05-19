use std::fmt;

use crate::shim::cell::UnsafeCell;
use crate::init::Init;

/// An init-once cell for global access to a value.
///
/// A `InitCell` instance can hold a single value in a global context. A
/// `InitCell` instance begins without a value and must be initialized via the
/// [`set()`](#method.set) method. Once a value has been set, it can be
/// retrieved at any time and in any thread via the [`get()`](#method.get)
/// method. The [`try_get()`](#method.try_get) can be used to determine whether
/// the `InitCell` has been initialized before attempting to retrieve the value.
///
/// For safety reasons, values stored in `InitCell` must be `Send + Sync`.
///
/// # Example
///
/// The following example uses `InitCell` to hold a global instance of a
/// `HashMap` which can be modified at will:
///
/// ```rust
/// use std::collections::HashMap;
/// use std::sync::Mutex;
/// use std::thread;
///
/// use state::InitCell;
///
/// static GLOBAL_MAP: InitCell<Mutex<HashMap<String, String>>> = InitCell::new();
///
/// fn run_program() {
///     let mut map = GLOBAL_MAP.get().lock().unwrap();
///     map.insert("another_key".into(), "another_value".into());
/// }
///
/// fn main() {
///     // Create the initial map and store it in `GLOBAL_MAP`.
///     let mut initial_map = HashMap::new();
///     initial_map.insert("key".into(), "value".into());
///     GLOBAL_MAP.set(Mutex::new(initial_map));
///
///     // For illustration, we spawn a new thread that modified the map.
///     thread::spawn(|| run_program()).join().expect("thread");
///
///     // Assert that the modification took place.
///     let map = GLOBAL_MAP.get().lock().unwrap();
///     assert_eq!(map.get("another_key").unwrap(), "another_value");
/// }
pub struct InitCell<T> {
    item: UnsafeCell<Option<T>>,
    init: Init
}

impl<T> InitCell<T> {
    /// Create a new, uninitialized cell.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::InitCell;
    ///
    /// static MY_GLOBAL: InitCell<String> = InitCell::new();
    /// ```
    #[cfg(not(loom))]
    pub const fn new() -> InitCell<T> {
        InitCell {
            item: UnsafeCell::new(None),
            init: Init::new()
        }
    }

    /// New, for loom.
    #[cfg(loom)]
    pub fn new() -> InitCell<T> {
        InitCell {
            item: UnsafeCell::new(None),
            init: Init::new()
        }
    }
}

/// Defaults to [`InitCell::new()`].
impl<T> Default for InitCell<T> {
    fn default() -> Self {
        InitCell::new()
    }
}

impl<T: Send + Sync> InitCell<T> {
    /// Sets the value for this cell to `value` if it has not already
    /// been set before.
    ///
    /// If a value has previously been set, `self` is unchanged and `false` is
    /// returned. Otherwise `true` is returned.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use state::InitCell;
    /// static MY_GLOBAL: InitCell<&'static str> = InitCell::new();
    ///
    /// assert_eq!(MY_GLOBAL.set("Hello, world!"), true);
    /// assert_eq!(MY_GLOBAL.set("Goodbye, world!"), false);
    /// ```
    pub fn set(&self, value: T) -> bool {
        if self.init.needed() {
            unsafe { self.item.with_mut(|ptr| *ptr = Some(value)); }
            self.init.mark_complete();
            return true;
        }

        false
    }

    /// Attempts to borrow the value in this cell.
    ///
    /// Returns `Some` if the state has previously been [set](#method.set).
    /// Otherwise returns `None`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use state::InitCell;
    /// static MY_GLOBAL: InitCell<&'static str> = InitCell::new();
    ///
    /// assert_eq!(MY_GLOBAL.try_get(), None);
    ///
    /// MY_GLOBAL.set("Hello, world!");
    ///
    /// assert_eq!(MY_GLOBAL.try_get(), Some(&"Hello, world!"));
    /// ```
    #[inline]
    pub fn try_get(&self) -> Option<&T> {
        if !self.init.has_completed() {
            return None
        }

        unsafe {
            self.item.with(|ptr| (*ptr).as_ref())
        }
    }

    /// Borrows the value in this cell.
    ///
    /// # Panics
    ///
    /// Panics if a value has not previously been [`set()`](#method.set). Use
    /// [`try_get()`](#method.try_get) for a non-panicking version.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use state::InitCell;
    /// static MY_GLOBAL: InitCell<&'static str> = InitCell::new();
    ///
    /// MY_GLOBAL.set("Hello, world!");
    /// assert_eq!(*MY_GLOBAL.get(), "Hello, world!");
    /// ```
    #[inline]
    pub fn get(&self) -> &T {
        self.try_get()
            .expect("cell::get(): called get() before set()")
    }

    /// If the cell has not yet been set, it is set to the return
    /// value of `from`. Returns a borrow to the value in this cell.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use state::InitCell;
    /// static MY_GLOBAL: InitCell<&'static str> = InitCell::new();
    ///
    /// assert_eq!(*MY_GLOBAL.get_or_init(|| "Hello, world!"), "Hello, world!");
    /// ```
    #[inline]
    pub fn get_or_init<F: FnOnce() -> T>(&self, from: F) -> &T {
        if let Some(value) = self.try_get() {
            value
        } else {
            self.set(from());
            self.get()
        }
    }

    /// Returns a mutable reference to the underlying data if any is set.
    ///
    /// This call borrows `InitCell` mutably (at compile-time) so there is no
    /// need for dynamic checks.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::InitCell;
    ///
    /// let mut cell = InitCell::from(5);
    /// *cell.try_get_mut().unwrap() += 1;
    ///
    /// let mut cell: InitCell<usize> = InitCell::new();
    /// assert!(cell.try_get_mut().is_none());
    /// ```
    pub fn try_get_mut(&mut self) -> Option<&mut T> {
        self.item.get_mut().as_mut()
    }

    /// Returns the inner value if any is set.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::InitCell;
    ///
    /// let cell = InitCell::from(5);
    /// assert_eq!(cell.into_inner().unwrap(), 5);
    ///
    /// let cell: InitCell<usize> = InitCell::new();
    /// assert!(cell.into_inner().is_none());
    /// ```
    pub fn into_inner(self) -> Option<T> {
        self.item.into_inner()
    }

    /// Applies the function `f` to the inner value, if there is any, and
    /// returns a new `InitCell` with mapped value.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::InitCell;
    ///
    /// let cell = InitCell::from(5);
    /// assert_eq!(cell.get(), &5);
    ///
    /// let cell = cell.map(|v| v + 10);
    /// assert_eq!(cell.get(), &15);
    /// ```
    pub fn map<U: Send + Sync, F: FnOnce(T) -> U>(self, f: F) -> InitCell<U> {
        self.into_inner().map_or_else(|| InitCell::new(), |v| InitCell::from(f(v)))
    }
}

unsafe impl<T: Send + Sync> Sync for InitCell<T> {  }

unsafe impl<T: Send + Sync> Send for InitCell<T> {  }

impl<T: fmt::Debug + Send + Sync> fmt::Debug for InitCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.try_get() {
            Some(object) => object.fmt(f),
            None => write!(f, "[uninitialized cell]")
        }
    }
}

impl<T: Send + Sync> From<T> for InitCell<T> {
    fn from(value: T) -> InitCell<T> {
        let cell = InitCell::new();
        assert!(cell.set(value));
        cell
    }
}

impl<T: Clone + Send + Sync> Clone for InitCell<T> {
    fn clone(&self) -> InitCell<T> {
        match self.try_get() {
            Some(val) => InitCell::from(val.clone()),
            None => InitCell::new()
        }
    }
}
