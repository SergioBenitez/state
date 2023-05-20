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

/// Defaults to [`InitCell::new()`].
impl<T> Default for InitCell<T> {
    fn default() -> Self {
        InitCell::new()
    }
}

impl<T> InitCell<T> {
    /// Create a new, uninitialized cell.
    ///
    /// To create a cell initializd with a value, use [`InitCell::from()`].
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

    /// Sets this cell's value to `value` if it is not already initialized.
    ///
    /// If there are multiple simultaneous callers, exactly one is guaranteed to
    /// receive `true`, indicating its value was set. All other callers receive
    /// `false`, indicating the value was ignored.
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

    /// Resets the cell to an uninitialized state.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::InitCell;
    ///
    /// let mut cell = InitCell::from(5);
    /// assert_eq!(cell.get(), &5);
    ///
    /// cell.reset();
    /// assert!(cell.try_get().is_none());
    /// ```
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// If the cell is not initialized, it is set `f()`. Returns a borrow to the
    /// value in this cell.
    ///
    /// If `f()` panics during initialization, the cell is left uninitialized.
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
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        if let Some(value) = self.try_get() {
            value
        } else {
            self.set(f());
            self.try_get().expect("cell::get_or_init(): set() => get() ok")
        }
    }

    /// Waits (blocks) until the cell has a value and then borrows it.
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
    pub fn wait(&self) -> &T {
        self.init.wait_until_complete();
        self.try_get().expect("cell::wait(): broken (init await complete w/o value)")
    }

    /// Get a reference to the underlying value, if one is set.
    ///
    /// Returns `Some` if the state has previously been set via methods like
    /// [`InitCell::set()`] or [`InitCell::get_or_init()`]. Otherwise returns
    /// `None`.
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
        if self.init.has_completed() {
            unsafe { self.item.with(|ptr| (*ptr).as_ref()) }
        } else {
            None
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

    /// Borrows the value in this cell, panicking if there is no value.
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
        self.try_get().expect("cell::get(): called get() before set()")
    }

    /// Resets the cell to an uninitialized state and returns the inner value if
    /// any was set.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::InitCell;
    ///
    /// let mut cell = InitCell::from(5);
    /// assert_eq!(cell.get(), &5);
    /// assert_eq!(cell.get(), &5);
    ///
    /// assert_eq!(cell.take(), Some(5));
    /// assert_eq!(cell.take(), None);
    /// ```
    pub fn take(&mut self) -> Option<T> {
        std::mem::replace(self, Self::new()).into_inner()
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

    /// Applies the function `f` to the inner value, if there is any, leaving
    /// the updated value in the cell.
    ///
    /// If `f()` panics during updating, the cell is left uninitialized.
    ///
    /// # Example
    ///
    /// ```rust
    /// use state::InitCell;
    ///
    /// let mut cell = InitCell::from(5);
    /// cell.update(|v| v + 10);
    /// assert_eq!(cell.wait(), &15);
    ///
    /// let mut cell = InitCell::new();
    /// cell.update(|v: u8| v + 10);
    /// assert!(cell.try_get().is_none());
    /// ```
    pub fn update<F: FnOnce(T) -> T>(&mut self, f: F) {
        self.take().map(|v| self.set(f(v)));
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
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> InitCell<U> {
        self.into_inner().map_or_else(InitCell::new, |v| InitCell::from(f(v)))
    }
}

unsafe impl<T: Send> Send for InitCell<T> {  }

unsafe impl<T: Send + Sync> Sync for InitCell<T> {  }

impl<T: fmt::Debug> fmt::Debug for InitCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.try_get() {
            Some(object) => object.fmt(f),
            None => write!(f, "[uninitialized cell]")
        }
    }
}

impl<T> From<T> for InitCell<T> {
    fn from(value: T) -> InitCell<T> {
        let cell = InitCell::new();
        assert!(cell.set(value));
        cell
    }
}

impl<T: Clone> Clone for InitCell<T> {
    fn clone(&self) -> InitCell<T> {
        match self.try_get() {
            Some(val) => InitCell::from(val.clone()),
            None => InitCell::new()
        }
    }
}
